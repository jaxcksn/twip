//! The definitions a document resolves with: the typed cascade and the XML path's resolver, kept in step
//!
//! Edits change the typed definitions first and the XML path's tables are derived from them. Where the XML path's
//! tables are read apart (the reader's), the typed side is brought in step on its next use from what
//! changed: each style, definition, list and picture bullet whose XML differs from the one its holder was read from
//! is read again, the read-order rule is worked out again over the styles, and the cascade is built again over the
//! definitions, the unchanged ones shared.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use super::font_table::{FontRename, rename_char};
use super::list_xml::{ListDefDirect, ListDirect, NumberingXml, PictureDirect};
use super::numbering::{AbstractNum, Num, Numbering};
use super::resolve::Resolver;
use super::style_xml::{DefaultsDef, LatentDef, StyleDef, StylesPart, alone};
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
    Rules {
        compat: f.styles.compat,
        word2002_rules: f.styles.word2002_rules,
        cell_size_rules: f.styles.cell_size_rules,
        open_type: f.styles.open_type,
    }
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
    /// The `w:style` of style `id` as it is written: a glossary style's as the glossary holds it.
    pub fn style_xml(&self, id: &str) -> Option<std::borrow::Cow<'_, str>> {
        match self.styles.styles.get(id) {
            Some(d) => Some(d.xml(&self.cascade.colours, &self.styles.cx())),
            None => self.glossary.styles.get(id).map(|x| std::borrow::Cow::Borrowed(x.as_str())),
        }
    }

    /// The `w:abstractNum` of list definition `id` as it is written: a glossary one's as the glossary holds it.
    pub fn def_xml(&self, id: i64) -> Option<std::borrow::Cow<'_, str>> {
        match self.numbering.defs.get(&ListDefId(id)) {
            Some(d) => Some(d.xml(&self.cascade.colours, &self.numbering.cx())),
            None => self.glossary.defs.get(&id).map(|x| std::borrow::Cow::Borrowed(x.as_str())),
        }
    }

    /// The `w:num` of list `id` as it is written, as [`Self::def_xml`].
    pub fn list_xml(&self, id: i64) -> Option<std::borrow::Cow<'_, str>> {
        match self.numbering.lists.get(&ListId(id)) {
            Some(d) => Some(d.xml(&self.cascade.colours, &self.numbering.cx())),
            None => self.glossary.lists.get(&id).map(|x| std::borrow::Cow::Borrowed(x.as_str())),
        }
    }

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
        let numbering = numbering_of(&f.numbering, None, &f.styles.colours);
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
                        && part
                            .styles
                            .get(&id)
                            .is_some_and(|d| d.origin_xml().is_some_and(|x| **x == *s.raw) || d.xml(&colours, &cx) == s.raw.as_str());
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
        let numbering = numbering_of(&f.numbering, Some(&self.numbering), &f.styles.colours);
        let lists = lists_of(&numbering, &f.numbering, Some((&self.cascade.lists, &self.glossary, &self.numbering)));
        let lists_changed = *self.cascade.lists != lists;
        let sheet = if changed { Rc::new(part.sheet.clone()) } else { self.cascade.styles.clone() };
        let lists = if lists_changed { Rc::new(lists) } else { self.cascade.lists.clone() };
        let c = cascade(sheet, lists, f);
        let defs = TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), glossary: Rc::new(glossary_xml(f)) };
        defs.check(f);
        defs
    }

    /// These definitions with `rename` applied: in each style's and region's, the defaults' (Word's where the part
    /// writes none) and each definition's levels' character properties, the theme's fonts and the fonts' categories;
    /// and the places that changed. The holders keep the elements as they are: saving renames the family in them.
    fn renamed(&self, rename: &FontRename) -> (TypedDefs, u32) {
        let c = &*self.cascade;
        let mut n = 0;
        let mut place = |k: u32| n += u32::from(k > 0);
        let mut part = (*self.styles).clone();
        // The document defaults
        let defaults = part.sheet.defaults.as_deref().cloned().unwrap_or_default();
        let mut char = defaults.char.clone().unwrap_or_else(crate::format::Defaults::word_char);
        let k = rename_char(&mut char, rename);
        place(k);
        if k > 0 {
            part.sheet.defaults = Some(Rc::new(crate::format::Defaults { char: Some(char), ..defaults }));
        }
        // Styles: Word's built-in default table style holds no character properties
        let ids: Vec<Rc<str>> = part.sheet.styles.keys().cloned().collect();
        for id in ids {
            let Some(e) = part.sheet.styles.get_mut(&id) else { continue };
            let mut s = (*e.style).clone();
            let mut k = rename_char(&mut s.format.char, rename);
            for r in &mut s.regions {
                k += rename_char(&mut r.format.char, rename);
            }
            let others: u32 = super::style_xml::other_region_chars(&s).iter_mut().map(|c| rename_char(c, rename)).sum();
            place(if s.word_default_table() { 0 } else { k + others });
            if k > 0 {
                e.style = Rc::new(s);
            }
        }
        // Definitions' levels
        let mut lists = (*c.lists).clone();
        for d in lists.defs.values_mut() {
            let mut v = (**d).clone();
            let mut any = false;
            for l in v.levels.iter_mut().flatten() {
                let mut x = (**l).clone();
                let k = rename_char(&mut x.char, rename);
                place(k);
                if k > 0 {
                    *l = Rc::new(x);
                    any = true;
                }
            }
            if any {
                *d = Rc::new(v);
            }
        }
        // Theme fonts and categories
        let mut theme = c.theme.clone();
        for slot in [&mut theme.major, &mut theme.minor] {
            if slot.eq_ignore_ascii_case(&rename.from) {
                *slot = rename.to.clone();
                place(1);
            }
        }
        for slot in [&mut theme.major_ea, &mut theme.minor_ea, &mut theme.major_cs, &mut theme.minor_cs].into_iter().flatten() {
            if slot.eq_ignore_ascii_case(&rename.from) {
                *slot = rename.to.clone();
                place(1);
            }
        }
        let from = rename.from.to_lowercase();
        let mut categories = c.categories.clone();
        if let Some(category) = categories.get(&from).copied() {
            let mut m = (*categories).clone();
            m.remove(&from);
            m.entry(rename.to.to_lowercase()).or_insert(category);
            categories = Rc::new(m);
        }
        let mut next = Cascade::new(Rc::new(part.sheet.clone()), Rc::new(lists), theme, c.colours, c.rules);
        (next.categories, next.divisions) = (categories, c.divisions.clone());
        let defs = TypedDefs { cascade: Rc::new(next), styles: Rc::new(part), numbering: self.numbering.clone(), glossary: self.glossary.clone() };
        (defs, n)
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
    let colours = &styles.colours;
    let held = |o: Option<&Rc<str>>, x: std::borrow::Cow<'_, str>, raw: &String| o.is_some_and(|o| **o == **raw) || *x == **raw;
    let defaults_same = match (&part.defaults, &styles.defaults_raw) {
        (Some(d), Some(r)) => held(d.origin_xml(), d.xml(colours, cx), r),
        (None, None) => true,
        _ => false,
    };
    if !defaults_same {
        part.defaults = styles.defaults_raw.as_deref().map(|x| DefaultsDef::from_xml(x, cx));
        part.sheet.defaults = part.defaults.as_ref().map(|d| d.get().clone());
    }
    let latent_same = match (&part.latent, &styles.latent.raw) {
        (Some(d), Some(r)) => held(d.origin_xml(), d.xml(colours, cx), r),
        (None, None) => true,
        _ => false,
    };
    if !latent_same {
        part.latent = styles.latent.raw.as_deref().map(|x| LatentDef::from_xml(x, cx));
        part.sheet.latent = part.latent.as_ref().map(|d| d.get().clone());
    }
}

/// The holders of the document's own definitions, lists and picture bullets `n` holds: `was`'s where the XML is the
/// same.
fn numbering_of(n: &Numbering, was: Option<&NumberingXml>, colours: &super::props::ThemeColours) -> NumberingXml {
    let cx = n.cx();
    let mut out = NumberingXml { root: n.root.clone(), ..NumberingXml::default() };
    let doc = |o: StyleOrigin| o == StyleOrigin::Document;
    for (k, a) in n.abstracts.iter().filter(|(_, a)| doc(a.origin)) {
        let held = was
            .and_then(|w| w.defs.get(&ListDefId(*k)))
            .filter(|d| d.origin_xml().is_some_and(|x| **x == *a.raw) || d.xml(colours, &cx) == a.raw.as_str());
        if let Some(d) = held.cloned().or_else(|| ListDefDirect::from_xml(&a.raw, &cx)) {
            out.defs.insert(ListDefId(*k), d);
        }
    }
    for (k, m) in n.nums.iter().filter(|(_, m)| doc(m.origin)) {
        let held = was
            .and_then(|w| w.lists.get(&ListId(*k)))
            .filter(|d| d.origin_xml().is_some_and(|x| **x == *m.raw) || d.xml(colours, &cx) == m.raw.as_str());
        if let Some(d) = held.cloned().or_else(|| ListDirect::from_xml(&m.raw, &cx)) {
            out.lists.insert(ListId(*k), d);
        }
    }
    for (k, p) in &n.pic_bullets {
        let held =
            was.and_then(|w| w.pictures.get(k)).filter(|d| d.origin_xml().is_some_and(|x| **x == *p.raw) || d.xml(colours, &cx) == p.raw.as_str());
        if let Some(d) = held.cloned().or_else(|| PictureDirect::from_xml(&p.raw, &cx)) {
            out.pictures.insert(*k, d);
        }
    }
    out
}

/// The typed lists: the document's own from `numbering`, the glossary's as `n` holds them (`was`'s where the XML is
/// the same), each picture bullet's image as `n` holds it.
fn lists_of(numbering: &NumberingXml, n: &Numbering, was: Option<(&Lists, &GlossaryXml, &NumberingXml)>) -> Lists {
    let cx = n.cx();
    let mut lists = numbering.lists();
    // The document's own, as they were while their holders are
    if let Some((l, _, held)) = was {
        for (k, d) in &numbering.defs {
            if held.defs.get(k).is_some_and(|h| Rc::ptr_eq(h.get(), d.get()))
                && let Some(v) = l.defs.get(k)
            {
                lists.defs.insert(*k, v.clone());
            }
        }
        for (k, d) in &numbering.lists {
            if held.lists.get(k).is_some_and(|h| Rc::ptr_eq(h.get(), d.get()))
                && let Some(v) = l.lists.get(k)
            {
                lists.lists.insert(*k, v.clone());
            }
        }
    }
    let glossary = |o: StyleOrigin| o == StyleOrigin::Glossary;
    for (k, a) in n.abstracts.iter().filter(|(_, a)| glossary(a.origin)) {
        let held = was.filter(|(_, g, _)| g.defs.get(k) == Some(&a.raw)).and_then(|(l, _, _)| l.defs.get(&ListDefId(*k)).cloned());
        if let Some(d) = held.or_else(|| glossary_def(*k, a, &cx).map(Rc::new)) {
            lists.defs.insert(ListDefId(*k), d);
        }
    }
    for (k, m) in n.nums.iter().filter(|(_, m)| glossary(m.origin)) {
        let held = was.filter(|(_, g, _)| g.lists.get(k) == Some(&m.raw)).and_then(|(l, _, _)| l.lists.get(&ListId(*k)).cloned());
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

/// The XML path's resolver and the typed definitions a document resolves with. Edits go to the typed definitions
/// first, the resolver's tables derived from them.
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

    /// Renames `rename.from` to `rename.to` in the definitions below the text: the typed ones, then the XML path's
    /// tables alike. Returns the places that changed: styles, document defaults, numbering levels and theme slots.
    pub fn rename_fonts(&mut self, rename: &FontRename) -> u32 {
        let (typed, n) = self.typed().renamed(rename);
        *self.typed.borrow_mut() = Rc::new(typed);
        let frozen = super::font_table::rename_tables(&mut self.frozen, rename);
        debug_assert_eq!(frozen, n, "the XML path's tables renamed alike");
        self.stale.set(false);
        self.edited();
        n
    }

    /// Whether the document defines style `id`.
    pub fn has_style(&self, id: &str) -> bool {
        self.cascade().styles.styles.contains_key(id)
    }

    /// The kind of style `id`, when the document defines it.
    pub fn style_kind(&self, id: &str) -> Option<crate::format::StyleKind> {
        self.cascade().styles.get(id).map(|s| s.kind)
    }

    /// The theme colours the definitions resolve with.
    pub fn colours(&self) -> super::props::ThemeColours {
        self.cascade().colours
    }

    /// The styles part's namespaces, as its elements are written.
    pub fn cx(&self) -> Cx {
        self.typed().styles.cx()
    }

    /// The `w:style` of style `id` as it is written.
    pub fn style_xml(&self, id: &str) -> Option<String> {
        self.typed().style_xml(id).map(std::borrow::Cow::into_owned)
    }

    /// The `w:docDefaults` as it is written.
    pub fn defaults_xml(&self) -> Option<String> {
        let t = self.typed();
        t.styles.defaults.as_ref().map(|d| d.xml(&t.cascade.colours, &t.styles.cx()).into_owned())
    }

    /// The XML path's resolver.
    pub fn frozen(&self) -> &Resolver {
        &self.frozen
    }
}

#[cfg(test)]
impl std::ops::Deref for Resolvers {
    type Target = Resolver;
    fn deref(&self) -> &Resolver {
        &self.frozen
    }
}

impl From<Resolver> for Resolvers {
    fn from(frozen: Resolver) -> Resolvers {
        Resolvers::of(frozen)
    }
}

// Edits: the typed definitions first, the XML path's derived from them

impl Resolvers {
    /// The typed definitions as an edit leaves them, `edit` given a copy of them (in step) to change; the XML path's
    /// then changed by `derive` to hold what they hold.
    fn edit_typed<T>(&mut self, edit: impl FnOnce(&mut StylesPart, &mut NumberingXml, &mut Lists) -> T) -> T {
        let typed = self.typed();
        let (mut part, mut numbering, mut lists) = ((*typed.styles).clone(), (*typed.numbering).clone(), (*typed.cascade.lists).clone());
        let out = edit(&mut part, &mut numbering, &mut lists);
        let c = cascade(Rc::new(part.sheet.clone()), Rc::new(lists), &self.frozen);
        let defs = TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), glossary: typed.glossary.clone() };
        *self.typed.borrow_mut() = Rc::new(defs);
        self.stale.set(false);
        out
    }

    /// After an edit, the glossary's XML as the XML path holds it, and in a debug build the two sides checked.
    fn edited(&mut self) {
        let typed = self.typed.borrow().clone();
        let defs = TypedDefs { glossary: Rc::new(glossary_xml(&self.frozen)), ..(*typed).clone() };
        defs.check(&self.frozen);
        *self.typed.borrow_mut() = Rc::new(defs);
    }

    /// The styles that changed with style `id`: its dependants before and after, and the default styles when they
    /// moved, as [`Styles::set_xml`] gives them.
    fn changed_with(before: &Cascade, after: &Cascade, id: &str, existed: bool) -> Vec<String> {
        let mut changed: Vec<String> = if existed { before.dependants(id).iter().map(|s| s.to_string()).collect() } else { Vec::new() };
        changed.extend(after.dependants(id).iter().map(|s| s.to_string()));
        let defaults = |c: &Cascade| (c.styles.default_paragraph.clone(), c.styles.default_table.clone());
        let (was, now) = (defaults(before), defaults(after));
        if was != now {
            changed.extend([was.0, was.1, now.0, now.1].into_iter().flatten().map(|s| s.to_string()));
        }
        changed.sort();
        changed.dedup();
        changed
    }

    /// Adds or replaces a style from its `w:style` XML: the typed style set (refused when it would be based on
    /// itself), then the XML path's. Returns the styles whose properties changed.
    pub fn set_style_xml(&mut self, raw: String) -> Result<Vec<String>, styles::StyleEditError> {
        let cx = self.frozen.styles.cx();
        let style = cx.parse(&raw, |_, el| (alone(el) && super::xml::is_w(el, "style")).then(|| super::style_xml::parse(el)).flatten()).flatten();
        let style = style.ok_or(styles::StyleEditError::Malformed)?;
        let id = style.id.clone();
        let colours = self.frozen.styles.colours;
        let before = self.cascade();
        let existed = before.styles.styles.contains_key(&id);
        let def = StyleDef::read(style, raw.as_str().into());
        self.edit_typed(|part, _, _| part.set(def, &colours))?;
        let changed = Self::changed_with(&before, &self.cascade(), &id, existed);
        let frozen = self.frozen.styles.set_xml(raw);
        debug_assert_eq!(frozen.as_ref(), Ok(&changed), "style {id}: the XML path set alike");
        self.edited();
        Ok(changed)
    }

    /// Removes style `id`: the styles based on it take its base, references to it from a next style or a link go;
    /// the typed styles first, then the XML path's, each style it rewrote written as the typed one is. Returns the
    /// styles whose properties changed.
    pub fn delete_style(&mut self, id: &str) -> Result<Vec<String>, styles::StyleEditError> {
        let before = self.cascade();
        if !before.styles.styles.contains_key(id) {
            return Err(styles::StyleEditError::Missing);
        }
        let mut frozen = self.frozen.styles.clone();
        frozen.delete(id)?;
        let changed: Vec<String> = before.dependants(id).iter().map(|s| s.to_string()).collect();
        let colours = self.frozen.styles.colours;
        self.edit_typed(|part, _, _| part.delete(id, &colours));
        let typed = self.typed();
        let cx = typed.styles.cx();
        for (rid, d) in &typed.styles.styles {
            if let Some(s) = frozen.by_id.get_mut(&**rid)
                && d.as_read().is_none()
            {
                let xml = d.xml(&colours, &cx).into_owned();
                debug_assert_eq!(xml, s.raw, "style {rid}: rewritten alike");
                s.raw = xml;
            }
        }
        self.frozen.styles = frozen;
        self.edited();
        Ok(changed)
    }

    /// Replaces the document defaults from their `w:docDefaults` XML: a level it doesn't write keeps the one before,
    /// as [`Styles::set_defaults_xml`] keeps it. Returns every style, by id.
    pub fn set_defaults_xml(&mut self, raw: String) -> Result<Vec<String>, styles::StyleEditError> {
        let cx = self.frozen.styles.cx();
        let parsed = cx.parse(&raw, |_, el| super::style_xml::parse_defaults(el)).ok_or(styles::StyleEditError::Malformed)?;
        let was = self.cascade().styles.defaults.clone();
        let merged = crate::format::Defaults {
            char: parsed.char.clone().or_else(|| was.as_ref().and_then(|d| d.char.clone())),
            para: parsed.para.clone().or_else(|| was.as_ref().and_then(|d| d.para.clone())),
            foreign: parsed.foreign.clone(),
        };
        let def = DefaultsDef::read(merged, raw.as_str().into());
        self.edit_typed(|part, _, _| {
            part.sheet.defaults = Some(def.get().clone());
            part.defaults = Some(def);
        });
        let all = self.frozen.styles.set_defaults_xml(raw);
        self.edited();
        all
    }

    /// Replaces the latent styles from their `w:latentStyles` XML.
    pub fn set_latent_xml(&mut self, raw: String) -> Result<(), styles::StyleEditError> {
        let cx = self.frozen.styles.cx();
        let parsed =
            cx.parse(&raw, |_, el| (alone(el) && super::xml::is_w(el, "latentStyles")).then(|| super::style_xml::parse_latent(el))).flatten();
        let def = LatentDef::read(parsed.ok_or(styles::StyleEditError::Malformed)?, raw.as_str().into());
        self.edit_typed(|part, _, _| {
            part.sheet.latent = Some(def.get().clone());
            part.latent = Some(def);
        });
        let out = self.frozen.styles.set_latent_xml(raw);
        self.edited();
        out
    }

    /// Adds or replaces a list definition from its `w:abstractNum` XML: the typed one, then the XML path's. Its id,
    /// or `None` when it isn't one.
    pub fn set_abstract_xml(&mut self, raw: String) -> Option<i64> {
        let d = ListDefDirect::from_xml(&raw, &self.frozen.numbering.cx())?;
        let id = d.get().id;
        self.edit_typed(|_, numbering, lists| {
            lists.defs.insert(id, d.get().clone());
            numbering.defs.insert(id, d);
        });
        let colours = self.frozen.styles.colours;
        let frozen = self.frozen.numbering.set_abstract_xml(raw, &colours);
        debug_assert_eq!(frozen, Some(id.0), "definition {}: the XML path set alike", id.0);
        self.edited();
        Some(id.0)
    }

    /// Adds or replaces a list from its `w:num` XML, as [`Self::set_abstract_xml`].
    pub fn set_num_xml(&mut self, raw: String) -> Option<i64> {
        let d = ListDirect::from_xml(&raw, &self.frozen.numbering.cx())?;
        let id = d.get().id;
        self.edit_typed(|_, numbering, lists| {
            lists.lists.insert(id, d.get().clone());
            numbering.lists.insert(id, d);
        });
        let colours = self.frozen.styles.colours;
        let frozen = self.frozen.numbering.set_num_xml(raw, &colours);
        debug_assert_eq!(frozen, Some(id.0), "list {}: the XML path set alike", id.0);
        self.edited();
        Some(id.0)
    }

    /// Adds picture bullet `id` from its `w:numPicBullet` XML, its image read through relationship `rel` at `image`.
    pub fn add_picture(&mut self, id: i64, raw: String, rel: String, image: usize) {
        if let Some(d) = PictureDirect::from_xml(&raw, &self.frozen.numbering.cx()) {
            let mut v = (**d.get()).clone();
            v.image = Some(image);
            let d = PictureDirect::read(v, raw.as_str().into());
            self.edit_typed(|_, numbering, lists| {
                lists.pictures.insert(id, d.get().clone());
                numbering.pictures.insert(id, d);
            });
        }
        self.frozen.numbering.pic_bullets.insert(id, super::numbering::PicBullet { raw, rel: Some(rel), image: Some(image) });
        self.edited();
    }

    /// Gives picture bullets their images as read: each id with its image's index in the document's images.
    pub fn set_picture_images(&mut self, images: &[(i64, usize)]) {
        if images.is_empty() {
            return;
        }
        let cx = self.frozen.numbering.cx();
        self.edit_typed(|_, numbering, lists| {
            for &(id, image) in images {
                let Some(d) = numbering.pictures.get(&id) else { continue };
                let mut v = (**d.get()).clone();
                v.image = Some(image);
                let xml = d.origin_xml().cloned().unwrap_or_else(|| d.xml(&Default::default(), &cx).into());
                let d = PictureDirect::read(v, xml);
                lists.pictures.insert(id, d.get().clone());
                numbering.pictures.insert(id, d);
            }
        });
        for &(id, image) in images {
            if let Some(p) = self.frozen.numbering.pic_bullets.get_mut(&id) {
                p.image = Some(image);
            }
        }
        self.edited();
    }

    /// These definitions with the style `raw` (a `w:style`) holds set among them, and its id: `None` when it isn't
    /// one, or would be based on itself.
    pub fn with_style_xml(&self, raw: &str) -> Option<(Resolvers, Rc<str>)> {
        let id = self.typed().styles.style_from_xml(raw)?.id;
        let mut next = self.clone();
        next.set_style_xml(raw.to_string()).ok()?;
        Some((next, id))
    }

    /// Takes the document defaults out from under the styles: text resolves over no defaults at all.
    pub fn without_defaults(&mut self) {
        let none = Rc::new(crate::format::Defaults {
            char: Some(crate::format::CharProps::default()),
            para: Some(crate::format::ParaProps::default()),
            foreign: None,
        });
        self.edit_typed(|part, _, _| part.sheet.defaults = Some(none));
        (self.frozen.styles.doc_rpr, self.frozen.styles.doc_ppr) = Default::default();
        self.edited();
    }

    /// Replaces the numbering part whole with `xml`'s definitions, lists and picture bullets.
    #[cfg(test)]
    pub fn set_numbering_part(&mut self, xml: &str) {
        let part = NumberingXml::parse(Some(xml));
        self.edit_typed(|_, numbering, lists| {
            *lists = part.lists();
            *numbering = part;
        });
        self.frozen.numbering = Numbering::parse(Some(xml), &self.frozen.styles.colours);
        self.frozen.compute_style_links();
        self.edited();
    }

    /// Follows the document settings `rules`, as the reader takes them from the settings part.
    #[cfg(test)]
    pub fn set_rules(&mut self, rules: Rules) {
        let s = &mut self.frozen.styles;
        (s.compat, s.word2002_rules, s.cell_size_rules, s.open_type) = (rules.compat, rules.word2002_rules, rules.cell_size_rules, rules.open_type);
        self.edit_typed(|_, _, _| ());
        self.edited();
    }

    /// The styles, to edit: reads are the XML path's, edits the typed side's first.
    pub fn style_edits(&mut self) -> StyleEdits<'_> {
        StyleEdits(self)
    }

    /// The numbering, to edit: reads are the XML path's, edits the typed side's first.
    pub fn numbering_edits(&mut self) -> NumberingEdits<'_> {
        NumberingEdits(self)
    }
}

/// A document's styles being edited: reads are the XML path's tables, edits go to the typed side first and the XML
/// path's follows.
pub struct StyleEdits<'a>(&'a mut Resolvers);

#[cfg(test)]
impl std::ops::Deref for StyleEdits<'_> {
    type Target = Styles;
    fn deref(&self) -> &Styles {
        &self.0.frozen.styles
    }
}

impl StyleEdits<'_> {
    /// [`Resolvers::set_style_xml`].
    pub fn set_xml(&mut self, raw: String) -> Result<Vec<String>, styles::StyleEditError> {
        self.0.set_style_xml(raw)
    }

    /// [`Resolvers::delete_style`].
    pub fn delete(&mut self, id: &str) -> Result<Vec<String>, styles::StyleEditError> {
        self.0.delete_style(id)
    }

    /// [`Resolvers::set_defaults_xml`].
    pub fn set_defaults_xml(&mut self, raw: String) -> Result<Vec<String>, styles::StyleEditError> {
        self.0.set_defaults_xml(raw)
    }

    /// [`Resolvers::set_latent_xml`].
    pub fn set_latent_xml(&mut self, raw: String) -> Result<(), styles::StyleEditError> {
        self.0.set_latent_xml(raw)
    }
}

/// A document's numbering being edited, as [`StyleEdits`].
pub struct NumberingEdits<'a>(&'a mut Resolvers);

#[cfg(test)]
impl std::ops::Deref for NumberingEdits<'_> {
    type Target = Numbering;
    fn deref(&self) -> &Numbering {
        &self.0.frozen.numbering
    }
}

impl NumberingEdits<'_> {
    /// [`Resolvers::set_abstract_xml`].
    pub fn set_abstract_xml(&mut self, raw: String, _colours: &super::props::ThemeColours) -> Option<i64> {
        self.0.set_abstract_xml(raw)
    }

    /// [`Resolvers::set_num_xml`].
    pub fn set_num_xml(&mut self, raw: String, _colours: &super::props::ThemeColours) -> Option<i64> {
        self.0.set_num_xml(raw)
    }

    /// [`Resolvers::add_picture`].
    pub fn add_picture(&mut self, id: i64, raw: String, rel: String, image: usize) {
        self.0.add_picture(id, raw, rel, image);
    }
}
