//! The definitions a document resolves with: its styles and lists, typed, beside the elements they were read from
//!
//! An edit sets typed definitions; the cascade is built again over them, the unchanged ones shared, and each element
//! is written as read while its definition is the one read from it.

use std::collections::HashMap;
use std::rc::Rc;

use super::font_table::{FontRename, rename_char};
use super::list_xml::{ListDefDirect, ListDirect, NumberingXml, PictureDirect};
use super::style_xml::{DefaultsDef, LatentDef, StyleDef, StylesPart, alone};
use super::write::props::Cx;
use crate::document::paragraph::ListId;
use crate::docx::list_xml::ListXml;
use crate::docx::style_xml::StyleXml;
use crate::format::cascade::{Cascade, Rules};
use crate::format::{ListDefId, Lists, Origin, StyleSheet};

/// The glossary's styles, definitions and lists as read, by the ids the document's definitions hold them under.
#[derive(Clone, Debug, Default, PartialEq)]
struct GlossaryXml {
    styles: HashMap<Rc<str>, String>,
    defs: HashMap<i64, String>,
    lists: HashMap<i64, String>,
}

impl GlossaryXml {
    /// The glossary's elements as read, under the ids `map` gives them among the document's: each style that joined
    /// `sheet` (one named as a document style is that style), each definition and list.
    fn merged(sheet: &StyleSheet, styles: &StylesPart, numbering: &NumberingXml, map: &super::glossary::GlossaryMap) -> GlossaryXml {
        let xml = |x: Option<&Rc<str>>| x.map(|x| x.to_string());
        let mut out = GlossaryXml::default();
        for (id, d) in &styles.styles {
            let view: Rc<str> = map.styles.view(&id.to_string()).into();
            if sheet.styles.get(&view).is_some_and(|e| e.origin == Origin::Glossary)
                && let Some(x) = xml(d.origin_xml())
            {
                out.styles.insert(view, x);
            }
        }
        for (id, d) in &numbering.defs {
            out.defs.extend(xml(d.origin_xml()).map(|x| (map.abstracts.view(&id.0), x)));
        }
        for (id, d) in &numbering.lists {
            out.lists.extend(xml(d.origin_xml()).map(|x| (map.nums.view(&id.0), x)));
        }
        out
    }
}

/// What the definitions resolve with beside them: the theme's fonts and colours, the settings' rules, the fonts'
/// categories and the HTML divisions.
#[derive(Default)]
pub struct Surroundings {
    /// The theme's fonts.
    pub theme: super::styles::ThemeFonts,
    /// The theme's colours.
    pub colours: super::props::ThemeColours,
    /// The settings the cascade follows.
    pub rules: Rules,
    /// The generic category of each family the font table describes, by lowercase name.
    pub categories: Rc<HashMap<String, super::font_table::GenericFamily>>,
    /// The HTML divisions, by id.
    pub divisions: Rc<HashMap<i64, crate::docx::Division>>,
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

/// The cascade over `sheet` and `lists` with what `c` resolves with beside its definitions.
fn cascade_like(sheet: Rc<StyleSheet>, lists: Rc<Lists>, c: &Cascade) -> Cascade {
    let mut next = Cascade::new(sheet, lists, c.theme.clone(), c.colours, c.rules);
    (next.categories, next.divisions) = (c.categories.clone(), c.divisions.clone());
    next
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

    /// The typed definitions of the styles part `styles` and numbering part `numbering`, with the glossary's parts
    /// (`glossary`) merged in, resolving within `env`; and how the glossary's ids map onto the document's.
    pub fn read(
        styles: Option<&str>,
        numbering: Option<&str>,
        glossary: Option<(Option<&str>, Option<&str>)>,
        env: Surroundings,
    ) -> (TypedDefs, Option<super::glossary::GlossaryMap>) {
        let mut part = StylesPart::parse(styles);
        let numbering = NumberingXml::parse(numbering);
        let mut lists = numbering.lists();
        let mut held = GlossaryXml::default();
        let map = glossary.map(|(gs, gn)| {
            let (g, gn) = (StylesPart::parse(gs), NumberingXml::parse(gn));
            let map = super::glossary::merge_definitions(&mut part.sheet, &mut lists, &g.sheet, &gn.lists());
            held = GlossaryXml::merged(&part.sheet, &g, &gn, &map);
            map
        });
        let mut c = Cascade::new(Rc::new(part.sheet.clone()), Rc::new(lists), env.theme, env.colours, env.rules);
        (c.categories, c.divisions) = (env.categories, env.divisions);
        (TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), glossary: Rc::new(held) }, map)
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
}

/// The typed definitions a document resolves with, to read and to edit.
#[derive(Clone, Debug)]
pub struct Resolvers {
    typed: Rc<TypedDefs>,
}

impl Resolvers {
    /// The definitions `typed`.
    pub fn new(typed: TypedDefs) -> Resolvers {
        Resolvers { typed: Rc::new(typed) }
    }

    /// The typed definitions.
    pub fn typed(&self) -> Rc<TypedDefs> {
        self.typed.clone()
    }

    /// The typed cascade.
    pub fn cascade(&self) -> Rc<Cascade> {
        self.typed.cascade.clone()
    }

    /// Renames `rename.from` to `rename.to` in the definitions below the text. Returns the places that changed:
    /// styles, document defaults, numbering levels and theme slots.
    pub fn rename_fonts(&mut self, rename: &FontRename) -> u32 {
        let (typed, n) = self.typed.renamed(rename);
        self.typed = Rc::new(typed);
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

    /// The holder of style `id`, to edit: a document style's (a glossary style has none).
    pub fn style_def(&self, id: &str) -> Option<StyleDef> {
        self.typed().styles.styles.get(id).cloned()
    }

    /// The holder of the document defaults, to edit.
    pub fn defaults_def(&self) -> Option<DefaultsDef> {
        self.typed().styles.defaults.clone()
    }

    /// The holder of the latent styles, to edit.
    pub fn latent_def(&self) -> Option<LatentDef> {
        self.typed().styles.latent.clone()
    }

    /// The holder of list definition `id`, to edit: a document definition's.
    pub fn list_def(&self, id: ListDefId) -> Option<ListDefDirect> {
        self.typed().numbering.defs.get(&id).cloned()
    }

    /// The holder of list `id`, to edit: a document list's.
    pub fn list(&self, id: ListId) -> Option<ListDirect> {
        self.typed().numbering.lists.get(&id).cloned()
    }

    /// The `w:docDefaults` as it is written.
    pub fn defaults_xml(&self) -> Option<String> {
        let t = self.typed();
        t.styles.defaults.as_ref().map(|d| d.xml(&t.cascade.colours, &t.styles.cx()).into_owned())
    }
}

// Edits

impl Resolvers {
    /// The typed definitions as `edit` leaves a copy of them, the cascade built again over them.
    fn edit_typed<T>(&mut self, edit: impl FnOnce(&mut StylesPart, &mut NumberingXml, &mut Lists) -> T) -> T {
        let typed = &self.typed;
        let (mut part, mut numbering, mut lists) = ((*typed.styles).clone(), (*typed.numbering).clone(), (*typed.cascade.lists).clone());
        let out = edit(&mut part, &mut numbering, &mut lists);
        let c = cascade_like(Rc::new(part.sheet.clone()), Rc::new(lists), &typed.cascade);
        let defs = TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), glossary: typed.glossary.clone() };
        self.typed = Rc::new(defs);
        out
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

    /// Adds or replaces a style from its `w:style` XML, as [`Self::set_style`] sets it.
    pub fn set_style_xml(&mut self, raw: String) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        let cx = self.typed.styles.cx();
        let style = cx.parse(&raw, |_, el| (alone(el) && super::xml::is_w(el, "style")).then(|| super::style_xml::parse(el)).flatten()).flatten();
        let style = style.ok_or(crate::docx::style_xml::StyleEditError::Malformed)?;
        self.set_style(StyleDef::read(style, raw.into()))
    }

    /// Adds or replaces style `def`, refused when it would be based on itself. Returns the styles whose properties
    /// changed.
    pub fn set_style(&mut self, def: StyleDef) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        let id = def.get().id.clone();
        let colours = self.colours();
        let before = self.cascade();
        let existed = before.styles.styles.contains_key(&id);
        self.edit_typed(|part, _, _| part.set(def, &colours))?;
        Ok(Self::changed_with(&before, &self.cascade(), &id, existed))
    }

    /// Removes style `id`: the styles based on it take its base, references to it from a next style or a link go.
    /// Refused when a style it rebases would then be based on itself. Returns the styles whose properties changed.
    pub fn delete_style(&mut self, id: &str) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        let before = self.cascade();
        if !before.styles.styles.contains_key(id) {
            return Err(crate::docx::style_xml::StyleEditError::Missing);
        }
        let rebased: Vec<Rc<str>> = before
            .styles
            .styles
            .values()
            .filter(|e| &*e.style.id != id && e.style.based_on.as_deref() == Some(id) && !e.style.word_default_table())
            .map(|e| e.style.id.clone())
            .collect();
        let colours = self.colours();
        let mut part = (*self.typed.styles).clone();
        part.delete(id, &colours);
        if rebased.iter().any(|r| part.sheet.styles.get(r).is_some_and(|e| part.loops(&e.style))) {
            return Err(crate::docx::style_xml::StyleEditError::Loop);
        }
        let changed: Vec<String> = before.dependants(id).iter().map(|s| s.to_string()).collect();
        self.edit_typed(|p, _, _| *p = part);
        Ok(changed)
    }

    /// Replaces the document defaults from their `w:docDefaults` XML: a level it doesn't write keeps the one before.
    /// Returns every style, by id.
    #[cfg(test)]
    pub fn set_defaults_xml(&mut self, raw: String) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        let cx = self.typed.styles.cx();
        let parsed = cx.parse(&raw, |_, el| super::style_xml::parse_defaults(el)).ok_or(crate::docx::style_xml::StyleEditError::Malformed)?;
        let was = self.cascade().styles.defaults.clone();
        let merged = crate::format::Defaults {
            char: parsed.char.clone().or_else(|| was.as_ref().and_then(|d| d.char.clone())),
            para: parsed.para.clone().or_else(|| was.as_ref().and_then(|d| d.para.clone())),
            foreign: parsed.foreign.clone(),
        };
        self.set_defaults(DefaultsDef::read(merged, raw.into()))
    }

    /// Replaces the document defaults with `def`. Returns every style, by id.
    pub fn set_defaults(&mut self, def: DefaultsDef) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.edit_typed(|part, _, _| {
            part.sheet.defaults = Some(def.get().clone());
            part.defaults = Some(def);
        });
        let mut all: Vec<String> = self.typed.cascade.styles.styles.keys().map(|k| k.to_string()).collect();
        all.sort();
        Ok(all)
    }

    /// Replaces the latent styles from their `w:latentStyles` XML.
    #[cfg(test)]
    pub fn set_latent_xml(&mut self, raw: String) -> Result<(), crate::docx::style_xml::StyleEditError> {
        let cx = self.typed.styles.cx();
        let parsed =
            cx.parse(&raw, |_, el| (alone(el) && super::xml::is_w(el, "latentStyles")).then(|| super::style_xml::parse_latent(el))).flatten();
        self.set_latent(LatentDef::read(parsed.ok_or(crate::docx::style_xml::StyleEditError::Malformed)?, raw.into()))
    }

    /// Replaces the latent styles with `def`.
    pub fn set_latent(&mut self, def: LatentDef) -> Result<(), crate::docx::style_xml::StyleEditError> {
        self.edit_typed(|part, _, _| {
            part.sheet.latent = Some(def.get().clone());
            part.latent = Some(def);
        });
        Ok(())
    }

    /// Adds or replaces a list definition from its `w:abstractNum` XML. Its id, or `None` when it isn't one.
    #[cfg(test)]
    pub fn set_abstract_xml(&mut self, raw: String) -> Option<i64> {
        let d = ListDefDirect::from_xml(&raw, &self.typed.numbering.cx())?;
        Some(self.set_list_def(d).0)
    }

    /// Adds or replaces list definition `d`. Its id.
    pub fn set_list_def(&mut self, d: ListDefDirect) -> ListDefId {
        let id = d.get().id;
        self.edit_typed(|_, numbering, lists| {
            lists.defs.insert(id, d.get().clone());
            numbering.defs.insert(id, d);
        });
        id
    }

    /// Adds or replaces a list from its `w:num` XML, as [`Self::set_abstract_xml`].
    #[cfg(test)]
    pub fn set_num_xml(&mut self, raw: String) -> Option<i64> {
        let d = ListDirect::from_xml(&raw, &self.typed.numbering.cx())?;
        Some(self.set_list(d).0)
    }

    /// Adds or replaces list `d`. Its id.
    pub fn set_list(&mut self, d: ListDirect) -> ListId {
        let id = d.get().id;
        self.edit_typed(|_, numbering, lists| {
            lists.lists.insert(id, d.get().clone());
            numbering.lists.insert(id, d);
        });
        id
    }

    /// Adds picture bullet `id` from its `w:numPicBullet` XML, its image read through relationship `rel` at `image`.
    pub fn add_picture(&mut self, id: i64, raw: String, rel: String, image: crate::docx::ResourceId) {
        let _ = rel;
        if let Some(d) = PictureDirect::from_xml(&raw, &self.typed.numbering.cx()) {
            let mut v = (**d.get()).clone();
            v.image = Some(image);
            let d = PictureDirect::read(v, raw.as_str().into());
            self.edit_typed(|_, numbering, lists| {
                lists.pictures.insert(id, d.get().clone());
                numbering.pictures.insert(id, d);
            });
        }
    }

    /// Gives picture bullets their images as read: each id with its image in the document's images.
    pub fn set_picture_images(&mut self, images: &[(i64, crate::docx::ResourceId)]) {
        if images.is_empty() {
            return;
        }
        let cx = self.typed.numbering.cx();
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
    }

    /// These definitions with the style `raw` (a `w:style`) holds set among them, and its id: `None` when it isn't
    /// one, or would be based on itself.
    pub fn with_style_xml(&self, raw: &str) -> Option<(Resolvers, Rc<str>)> {
        let id = self.typed.styles.style_from_xml(raw)?.id;
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
    }

    /// Replaces the numbering part whole with `xml`'s definitions, lists and picture bullets.
    #[cfg(test)]
    pub fn set_numbering_part(&mut self, xml: &str) {
        let part = NumberingXml::parse(Some(xml));
        self.edit_typed(|_, numbering, lists| {
            *lists = part.lists();
            *numbering = part;
        });
    }

    /// Follows the document settings `rules`, as the reader takes them from the settings part.
    #[cfg(test)]
    pub fn set_rules(&mut self, rules: Rules) {
        let mut c = (*self.typed.cascade).clone();
        c.rules = rules;
        let c = cascade_like(c.styles.clone(), c.lists.clone(), &c);
        self.typed = Rc::new(TypedDefs { cascade: Rc::new(c), ..(*self.typed).clone() });
    }

    /// These definitions with nothing as read: each style, the defaults, the latent styles, each definition, list and
    /// picture bullet written from its typed value, in parts whose roots are `styles_root` and `numbering_root`.
    pub fn unanchored(&self, styles_root: Rc<str>, numbering_root: String) -> Resolvers {
        let t = &self.typed;
        let mut part = (*t.styles).clone();
        part.root = styles_root;
        for d in part.styles.values_mut() {
            *d = StyleDef::typed(d.get().clone());
        }
        part.defaults = part.defaults.as_ref().map(|d| DefaultsDef::typed(d.get().clone()));
        part.latent = part.latent.as_ref().map(|d| LatentDef::typed(d.get().clone()));
        let mut numbering = (*t.numbering).clone();
        numbering.root = numbering_root;
        for d in numbering.defs.values_mut() {
            *d = ListDefDirect::typed(d.get().clone());
        }
        for d in numbering.lists.values_mut() {
            *d = ListDirect::typed(d.get().clone());
        }
        for d in numbering.pictures.values_mut() {
            *d = PictureDirect::typed(d.get().clone());
        }
        let defs = TypedDefs { styles: Rc::new(part), numbering: Rc::new(numbering), ..(**t).clone() };
        Resolvers { typed: Rc::new(defs) }
    }

    /// The styles, to edit.
    pub fn style_edits(&mut self) -> StyleEdits<'_> {
        StyleEdits(self)
    }

    /// The numbering, to edit.
    pub fn numbering_edits(&mut self) -> NumberingEdits<'_> {
        NumberingEdits(self)
    }
}

/// A document's styles being edited.
pub struct StyleEdits<'a>(&'a mut Resolvers);

impl StyleEdits<'_> {
    /// [`Resolvers::set_style`].
    pub fn set(&mut self, def: StyleDef) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.set_style(def)
    }

    /// [`Resolvers::set_defaults`].
    pub fn set_defaults(&mut self, def: DefaultsDef) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.set_defaults(def)
    }

    /// [`Resolvers::set_latent`].
    pub fn set_latent(&mut self, def: LatentDef) -> Result<(), crate::docx::style_xml::StyleEditError> {
        self.0.set_latent(def)
    }

    /// The typed definitions as the edits so far leave them.
    pub fn typed(&self) -> Rc<TypedDefs> {
        self.0.typed()
    }

    /// [`Resolvers::set_style_xml`].
    #[cfg(test)]
    pub fn set_xml(&mut self, raw: String) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.set_style_xml(raw)
    }

    /// [`Resolvers::delete_style`].
    pub fn delete(&mut self, id: &str) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.delete_style(id)
    }

    /// [`Resolvers::set_defaults_xml`].
    #[cfg(test)]
    pub fn set_defaults_xml(&mut self, raw: String) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.set_defaults_xml(raw)
    }

    /// [`Resolvers::set_latent_xml`].
    #[cfg(test)]
    pub fn set_latent_xml(&mut self, raw: String) -> Result<(), crate::docx::style_xml::StyleEditError> {
        self.0.set_latent_xml(raw)
    }
}

/// A document's numbering being edited, as [`StyleEdits`].
pub struct NumberingEdits<'a>(&'a mut Resolvers);

impl NumberingEdits<'_> {
    /// [`Resolvers::set_list_def`].
    pub fn set_list_def(&mut self, d: ListDefDirect) -> ListDefId {
        self.0.set_list_def(d)
    }

    /// [`Resolvers::set_list`].
    pub fn set_list(&mut self, d: ListDirect) -> ListId {
        self.0.set_list(d)
    }

    /// The typed definitions as the edits so far leave them.
    pub fn typed(&self) -> Rc<TypedDefs> {
        self.0.typed()
    }

    /// [`Resolvers::set_abstract_xml`].
    #[cfg(test)]
    pub fn set_abstract_xml(&mut self, raw: String, _colours: &super::props::ThemeColours) -> Option<i64> {
        self.0.set_abstract_xml(raw)
    }

    /// [`Resolvers::set_num_xml`].
    #[cfg(test)]
    pub fn set_num_xml(&mut self, raw: String, _colours: &super::props::ThemeColours) -> Option<i64> {
        self.0.set_num_xml(raw)
    }

    /// [`Resolvers::add_picture`].
    pub fn add_picture(&mut self, id: i64, raw: String, rel: String, image: crate::docx::ResourceId) {
        self.0.add_picture(id, raw, rel, image);
    }
}
