//! The definitions a document resolves with: the XML path's resolver and the typed cascade, kept in step
//!
//! The typed side follows the XML path's: an edit changes the resolver (through [`Resolvers`]' mutable access, which
//! marks the typed side stale), and the typed side is brought in step on its next use from what changed: each style,
//! definition, list and picture bullet whose XML differs from the one its holder was read from is read again, the
//! read-order rule is worked out again over the styles, and the cascade is built again over the definitions, the
//! unchanged ones shared.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use super::font_table::{FontRename, rename_char};
use super::list_xml::{ListDefDirect, ListDirect, NumberingXml, PictureDirect};
use super::numbering::{AbstractNum, Num, Numbering};
use super::resolve::Resolver;
use super::style_xml::{DefaultsDef, LatentDef, StyleDef, StylesPart};
use super::styles::{self, StyleOrigin, Styles};
use super::write::props::Cx;
use crate::document::paragraph::ListId;
use crate::format::cascade::{Cascade, Rules};
use crate::format::{ListChoice, ListDef, ListDefId, Lists, Origin, StyleEntry, StyleSheet};

/// The glossary's styles, definitions and lists as the XML path holds their XML, by the ids the document's tables
/// hold them under.
#[derive(Clone, Debug, Default)]
struct GlossaryXml {
    styles: HashMap<Rc<str>, String>,
    defs: HashMap<i64, String>,
    lists: HashMap<i64, String>,
}

/// The typed definitions: the cascade, and the holders of the styles and numbering parts' elements.
#[derive(Clone, Debug)]
pub struct TypedDefs {
    /// The cascade over the definitions.
    pub cascade: Rc<Cascade>,
    /// The styles part's holders; its sheet holds the glossary's styles too.
    pub styles: Rc<StylesPart>,
    /// The numbering part's holders.
    pub numbering: Rc<NumberingXml>,
    glossary: Rc<GlossaryXml>,
}

fn rules(f: &Resolver) -> Rules {
    Rules { word2002_rules: f.styles.word2002_rules, cell_size_rules: f.styles.cell_size_rules, open_type: f.styles.open_type }
}

fn origin(o: StyleOrigin) -> Origin {
    match o {
        StyleOrigin::Document => Origin::Document,
        StyleOrigin::Glossary => Origin::Glossary,
    }
}

/// The cascade over `sheet` and `lists` with what `f` holds beside its definitions.
fn cascade(sheet: Rc<StyleSheet>, lists: Rc<Lists>, f: &Resolver) -> Cascade {
    let mut c = Cascade::new(sheet, lists, f.theme.clone(), f.styles.colours, rules(f));
    (c.categories, c.divisions) = (f.categories.clone(), f.divisions.clone());
    c
}

/// A glossary style the XML path holds as `s`: its XML read, its ids as the document's tables hold them.
fn glossary_style(s: &styles::Style, cx: &Cx) -> StyleEntry {
    let mut style = (**StyleDef::from_xml(&s.raw, cx).get()).clone();
    let id = |v: &Option<String>| v.as_deref().map(Rc::from);
    style.id = s.id.as_str().into();
    style.based_on = id(&s.based_on);
    style.next = id(&s.next);
    style.format.para.style = id(&s.base.ppr.style);
    if let Some(ListChoice::In(n)) = &mut style.format.para.list
        && let Some(v) = s.base.ppr.num_id
    {
        n.0 = v;
    }
    style.format.char.style = id(&s.base.rpr.style);
    StyleEntry { style: Rc::new(style), order: s.order, origin: Origin::Glossary, frozen: s.frozen, word_run_defaults: false }
}

/// A glossary definition the XML path holds as `a` under `id`: its XML read, its ids as the document's tables hold
/// them.
fn glossary_def(id: i64, a: &AbstractNum, cx: &Cx) -> Option<ListDef> {
    let mut d = (**ListDefDirect::from_xml(&a.raw, cx)?.get()).clone();
    d.id = ListDefId(id);
    for (l, f) in d.levels.iter_mut().zip(&a.levels) {
        if let (Some(l), Some(f)) = (l, f) {
            Rc::make_mut(l).style = f.pstyle.as_deref().map(Rc::from);
        }
    }
    d.num_style_link = a.num_style_link.as_deref().map(Rc::from);
    d.origin = Origin::Glossary;
    Some(d)
}

/// A glossary list the XML path holds as `n` under `id`, as [`glossary_def`].
fn glossary_list(id: i64, n: &Num, cx: &Cx) -> Option<crate::format::List> {
    let mut l = (**ListDirect::from_xml(&n.raw, cx)?.get()).clone();
    l.id = ListId(id);
    l.def = l.def.map(|_| ListDefId(n.abstract_id));
    l.origin = Origin::Glossary;
    Some(l)
}

impl TypedDefs {
    /// The typed definitions of the styles part `styles` and numbering part `numbering` (and the glossary's), read
    /// beside `f`, the XML path's resolver of the same parts: the glossary's merged as the reader merges them.
    pub fn read(styles: Option<&str>, numbering: Option<&str>, glossary: Option<(Option<&str>, Option<&str>)>, f: &Resolver) -> TypedDefs {
        let mut part = StylesPart::parse(styles);
        let numbering = NumberingXml::parse(numbering);
        let mut lists = numbering.lists();
        if let Some((gs, gn)) = glossary {
            let g = StylesPart::parse(gs);
            super::glossary::merge_definitions(&mut part.sheet, &mut lists, &g.sheet, &NumberingXml::parse(gn).lists());
        }
        let glossary = glossary_xml(f);
        let sheet = Rc::new(part.sheet.clone());
        let c = cascade(sheet, Rc::new(lists), f);
        TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), glossary: Rc::new(glossary) }
    }

    /// The typed definitions of what `f` holds, read from its elements' XML.
    pub fn of(f: &Resolver) -> TypedDefs {
        let cx = f.styles.cx();
        let mut part = StylesPart { root: f.styles.root.as_str().into(), ..StylesPart::default() };
        for s in f.styles.by_id.values() {
            match s.origin {
                StyleOrigin::Document => {
                    let d = StyleDef::from_xml(&s.raw, &cx);
                    let entry =
                        StyleEntry { style: d.get().clone(), order: s.order, origin: Origin::Document, frozen: false, word_run_defaults: false };
                    part.sheet.styles.insert(s.id.as_str().into(), entry);
                    part.styles.insert(s.id.as_str().into(), d);
                }
                StyleOrigin::Glossary => {
                    part.sheet.styles.insert(s.id.as_str().into(), glossary_style(s, &cx));
                }
            }
        }
        part.sheet.paragraph_order = f.styles.paragraph_order.iter().map(|s| Rc::from(s.as_str())).collect();
        set_defaults(&mut part, &f.styles, &cx);
        part.refinish(&f.styles.colours, None);
        let numbering = numbering_of(&f.numbering, None);
        let lists = lists_of(&numbering, &f.numbering, None);
        let c = cascade(Rc::new(part.sheet.clone()), Rc::new(lists), f);
        let defs = TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), glossary: Rc::new(glossary_xml(f)) };
        defs.check(f);
        defs
    }

    /// These definitions brought in step with `f`, the XML path's resolver after an edit: what changed read again,
    /// the rest shared.
    pub fn follow(&self, f: &Resolver) -> TypedDefs {
        let colours = f.styles.colours;
        let cx = f.styles.cx();
        let mut part = (*self.styles).clone();
        part.root = f.styles.root.as_str().into();
        let mut fresh: Vec<Rc<str>> = Vec::new();
        let mut changed = false;
        for s in f.styles.by_id.values() {
            let id: Rc<str> = s.id.as_str().into();
            match s.origin {
                StyleOrigin::Document => {
                    let same = part.sheet.styles.get(&id).is_some_and(|e| e.origin == Origin::Document)
                        && part.styles.get(&id).and_then(|d| d.origin_xml()).is_some_and(|x| **x == *s.raw);
                    if !same {
                        let d = StyleDef::from_xml(&s.raw, &cx);
                        let entry =
                            StyleEntry { style: d.get().clone(), order: s.order, origin: Origin::Document, frozen: false, word_run_defaults: false };
                        part.sheet.styles.insert(id.clone(), entry);
                        part.styles.insert(id.clone(), d);
                        fresh.push(id);
                    }
                }
                StyleOrigin::Glossary => {
                    let same = self.glossary.styles.get(&id).is_some_and(|x| *x == s.raw)
                        && part.sheet.styles.get(&id).is_some_and(|e| e.origin == Origin::Glossary);
                    if !same {
                        part.styles.remove(&id);
                        part.sheet.styles.insert(id, glossary_style(s, &cx));
                        changed = true;
                    }
                }
            }
        }
        let gone: Vec<Rc<str>> = part.sheet.styles.keys().filter(|id| !f.styles.by_id.contains_key(&***id)).cloned().collect();
        for id in &gone {
            part.sheet.styles.remove(id);
            part.styles.remove(id);
        }
        changed |= !gone.is_empty() || !fresh.is_empty();
        for e in part.sheet.styles.values_mut() {
            if let Some(s) = f.styles.by_id.get(&*e.style.id) {
                e.order = s.order;
            }
        }
        if changed {
            let fresh: Vec<&str> = fresh.iter().map(|s| &**s).collect();
            part.refinish(&colours, Some(&fresh));
        }
        let defaults_were = (part.sheet.defaults.clone(), part.sheet.latent.clone());
        set_defaults(&mut part, &f.styles, &cx);
        changed |= !same_rc(&defaults_were.0, &part.sheet.defaults) || !same_rc(&defaults_were.1, &part.sheet.latent);
        let numbering = numbering_of(&f.numbering, Some(&self.numbering));
        let lists = lists_of(&numbering, &f.numbering, Some((&self.cascade.lists, &self.glossary)));
        let lists_changed = *self.cascade.lists != lists;
        let sheet = if changed { Rc::new(part.sheet.clone()) } else { self.cascade.styles.clone() };
        let lists = if lists_changed { Rc::new(lists) } else { self.cascade.lists.clone() };
        let c = cascade(sheet, lists, f);
        let defs = TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), glossary: Rc::new(glossary_xml(f)) };
        defs.check(f);
        defs
    }

    /// These definitions with `rename` applied as [`super::font_table::rename_tables`] applies it to the XML path's
    /// tables (`f`, the resolver it renamed): in each style's and region's, the defaults' and each definition's
    /// levels' character properties; the holders keep the elements they were read from.
    fn renamed(&self, rename: &FontRename, f: &Resolver) -> TypedDefs {
        let mut part = (*self.styles).clone();
        let style = |s: &crate::format::Style| -> Option<crate::format::Style> {
            let mut s = s.clone();
            let mut n = rename_char(&mut s.format.char, rename);
            for r in &mut s.regions {
                n += rename_char(&mut r.format.char, rename);
            }
            (n > 0).then_some(s)
        };
        let ids: Vec<Rc<str>> = part.sheet.styles.keys().cloned().collect();
        for id in ids {
            let Some(e) = part.sheet.styles.get_mut(&id) else { continue };
            let Some(s) = style(&e.style) else { continue };
            match part.styles.get(&id) {
                Some(d) => {
                    let d = d.edited(s, &[]);
                    e.style = d.get().clone();
                    part.styles.insert(id, d);
                }
                None => e.style = Rc::new(s),
            }
        }
        if let Some(d) = &part.defaults {
            let mut v = (**d.get()).clone();
            if v.char.as_mut().is_some_and(|c| rename_char(c, rename) > 0) {
                let d = d.edited(v, &[]);
                part.sheet.defaults = Some(d.get().clone());
                part.defaults = Some(d);
            }
        }
        let def = |d: &ListDef| -> Option<ListDef> {
            let mut d = d.clone();
            let mut n = 0;
            for l in d.levels.iter_mut().flatten() {
                let mut v = (**l).clone();
                let k = rename_char(&mut v.char, rename);
                if k > 0 {
                    *l = Rc::new(v);
                    n += k;
                }
            }
            (n > 0).then_some(d)
        };
        let mut numbering = (*self.numbering).clone();
        for d in numbering.defs.values_mut() {
            if let Some(v) = def(d.get()) {
                *d = d.edited(v, &[]);
            }
        }
        let mut lists = (*self.cascade.lists).clone();
        for d in lists.defs.values_mut() {
            if let Some(v) = def(d) {
                *d = Rc::new(v);
            }
        }
        let c = cascade(Rc::new(part.sheet.clone()), Rc::new(lists), f);
        TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), glossary: self.glossary.clone() }
    }

    /// In a debug build, that the typed definitions are in step with `f`: the same styles, in the same order and
    /// places, the same frozen table styles and default styles, the same lists and definitions.
    fn check(&self, f: &Resolver) {
        if !cfg!(debug_assertions) {
            return;
        }
        let sheet = &self.cascade.styles;
        assert_eq!(sheet.styles.len(), f.styles.by_id.len(), "the typed styles are the resolver's");
        for s in f.styles.by_id.values() {
            let e = sheet.styles.get(s.id.as_str()).unwrap_or_else(|| panic!("style {} typed", s.id));
            assert!(e.order == s.order && e.origin == origin(s.origin) && e.frozen == s.frozen, "style {}: place and frozen alike", s.id);
        }
        assert!(sheet.paragraph_order.iter().map(|s| &**s).eq(f.styles.paragraph_order.iter().map(String::as_str)), "paragraph styles' order alike");
        assert_eq!(sheet.default_paragraph.as_deref(), f.styles.default_paragraph.as_deref(), "default paragraph style alike");
        assert_eq!(sheet.default_table.as_deref(), f.styles.default_table.as_deref(), "default table style alike");
        let lists = &self.cascade.lists;
        assert!(lists.lists.len() == f.numbering.nums.len() && f.numbering.nums.keys().all(|k| lists.lists.contains_key(&ListId(*k))), "lists alike");
        assert!(
            lists.defs.len() == f.numbering.abstracts.len() && f.numbering.abstracts.keys().all(|k| lists.defs.contains_key(&ListDefId(*k))),
            "definitions alike"
        );
    }
}

fn same_rc<T>(a: &Option<Rc<T>>, b: &Option<Rc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

/// The glossary's elements as `f` holds their XML.
fn glossary_xml(f: &Resolver) -> GlossaryXml {
    let glossary = |o: StyleOrigin| o == StyleOrigin::Glossary;
    GlossaryXml {
        styles: f.styles.by_id.values().filter(|s| glossary(s.origin)).map(|s| (s.id.as_str().into(), s.raw.clone())).collect(),
        defs: f.numbering.abstracts.iter().filter(|(_, a)| glossary(a.origin)).map(|(k, a)| (*k, a.raw.clone())).collect(),
        lists: f.numbering.nums.iter().filter(|(_, n)| glossary(n.origin)).map(|(k, n)| (*k, n.raw.clone())).collect(),
    }
}

/// The defaults and latent styles of `part` as `styles` holds their XML: those that changed read again.
fn set_defaults(part: &mut StylesPart, styles: &Styles, cx: &Cx) {
    let same = |held: Option<&Rc<str>>, raw: Option<&String>| match (held, raw) {
        (Some(h), Some(r)) => **h == **r,
        (None, None) => true,
        _ => false,
    };
    if !same(part.defaults.as_ref().and_then(|d| d.origin_xml()), styles.defaults_raw.as_ref()) {
        part.defaults = styles.defaults_raw.as_deref().map(|x| DefaultsDef::from_xml(x, cx));
        part.sheet.defaults = part.defaults.as_ref().map(|d| d.get().clone());
    }
    if !same(part.latent.as_ref().and_then(|d| d.origin_xml()), styles.latent.raw.as_ref()) {
        part.latent = styles.latent.raw.as_deref().map(|x| LatentDef::from_xml(x, cx));
        part.sheet.latent = part.latent.as_ref().map(|d| d.get().clone());
    }
}

/// The holders of the document's own definitions, lists and picture bullets `n` holds: `was`'s where the XML is the
/// same.
fn numbering_of(n: &Numbering, was: Option<&NumberingXml>) -> NumberingXml {
    let cx = n.cx();
    let mut out = NumberingXml { root: n.root.clone(), ..NumberingXml::default() };
    let doc = |o: StyleOrigin| o == StyleOrigin::Document;
    for (k, a) in n.abstracts.iter().filter(|(_, a)| doc(a.origin)) {
        let held = was.and_then(|w| w.defs.get(&ListDefId(*k))).filter(|d| d.origin_xml().is_some_and(|x| **x == *a.raw));
        if let Some(d) = held.cloned().or_else(|| ListDefDirect::from_xml(&a.raw, &cx)) {
            out.defs.insert(ListDefId(*k), d);
        }
    }
    for (k, m) in n.nums.iter().filter(|(_, m)| doc(m.origin)) {
        let held = was.and_then(|w| w.lists.get(&ListId(*k))).filter(|d| d.origin_xml().is_some_and(|x| **x == *m.raw));
        if let Some(d) = held.cloned().or_else(|| ListDirect::from_xml(&m.raw, &cx)) {
            out.lists.insert(ListId(*k), d);
        }
    }
    for (k, p) in &n.pic_bullets {
        let held = was.and_then(|w| w.pictures.get(k)).filter(|d| d.origin_xml().is_some_and(|x| **x == *p.raw));
        if let Some(d) = held.cloned().or_else(|| PictureDirect::from_xml(&p.raw, &cx)) {
            out.pictures.insert(*k, d);
        }
    }
    out
}

/// The typed lists: the document's own from `numbering`, the glossary's as `n` holds them (`was`'s where the XML is
/// the same), each picture bullet's image as `n` holds it.
fn lists_of(numbering: &NumberingXml, n: &Numbering, was: Option<(&Lists, &GlossaryXml)>) -> Lists {
    let cx = n.cx();
    let mut lists = numbering.lists();
    let glossary = |o: StyleOrigin| o == StyleOrigin::Glossary;
    for (k, a) in n.abstracts.iter().filter(|(_, a)| glossary(a.origin)) {
        let held = was.filter(|(_, g)| g.defs.get(k) == Some(&a.raw)).and_then(|(l, _)| l.defs.get(&ListDefId(*k)).cloned());
        if let Some(d) = held.or_else(|| glossary_def(*k, a, &cx).map(Rc::new)) {
            lists.defs.insert(ListDefId(*k), d);
        }
    }
    for (k, m) in n.nums.iter().filter(|(_, m)| glossary(m.origin)) {
        let held = was.filter(|(_, g)| g.lists.get(k) == Some(&m.raw)).and_then(|(l, _)| l.lists.get(&ListId(*k)).cloned());
        if let Some(l) = held.or_else(|| glossary_list(*k, m, &cx).map(Rc::new)) {
            lists.lists.insert(ListId(*k), l);
        }
    }
    for (k, p) in lists.pictures.iter_mut() {
        let image = n.pic_bullets.get(k).and_then(|b| b.image);
        if p.image != image {
            Rc::make_mut(p).image = image;
        }
    }
    lists
}

/// The XML path's resolver and the typed definitions a document resolves with. Mutable access is to the resolver,
/// and leaves the typed definitions to be brought in step on their next use.
#[derive(Clone)]
pub struct Resolvers {
    frozen: Resolver,
    typed: RefCell<Rc<TypedDefs>>,
    stale: Cell<bool>,
}

impl std::fmt::Debug for Resolvers {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Resolvers({:?})", self.frozen)
    }
}

impl Resolvers {
    /// `frozen` with its typed definitions `typed`, read from the same parts.
    pub fn new(frozen: Resolver, typed: TypedDefs) -> Resolvers {
        Resolvers { frozen, typed: RefCell::new(Rc::new(typed)), stale: Cell::new(true) }
    }

    /// `frozen` with typed definitions read from the XML it holds.
    pub fn of(frozen: Resolver) -> Resolvers {
        let typed = TypedDefs::of(&frozen);
        Resolvers { frozen, typed: RefCell::new(Rc::new(typed)), stale: Cell::new(false) }
    }

    /// The typed definitions, in step with the resolver.
    pub fn typed(&self) -> Rc<TypedDefs> {
        if self.stale.replace(false) {
            let next = self.typed.borrow().follow(&self.frozen);
            *self.typed.borrow_mut() = Rc::new(next);
        }
        self.typed.borrow().clone()
    }

    /// The typed cascade, in step with the resolver.
    pub fn cascade(&self) -> Rc<Cascade> {
        self.typed().cascade.clone()
    }

    /// Renames `rename.from` to `rename.to` in the tables below the text, on both sides; returns the places that
    /// changed, as [`super::font_table::rename_tables`] counts them.
    pub fn rename_fonts(&mut self, rename: &FontRename) -> u32 {
        let typed = self.typed();
        let n = super::font_table::rename_tables(&mut self.frozen, rename);
        *self.typed.borrow_mut() = Rc::new(typed.renamed(rename, &self.frozen));
        self.stale.set(false);
        n
    }

    /// The XML path's resolver.
    pub fn frozen(&self) -> &Resolver {
        &self.frozen
    }
}

impl std::ops::Deref for Resolvers {
    type Target = Resolver;
    fn deref(&self) -> &Resolver {
        &self.frozen
    }
}

impl std::ops::DerefMut for Resolvers {
    fn deref_mut(&mut self) -> &mut Resolver {
        self.stale.set(true);
        &mut self.frozen
    }
}

impl From<Resolver> for Resolvers {
    fn from(frozen: Resolver) -> Resolvers {
        Resolvers::of(frozen)
    }
}
