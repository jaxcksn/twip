//! The definitions a document resolves with: its styles and lists, typed, beside the elements they were read from
//!
//! An edit sets typed definitions; the cascade is built again over them, the unchanged ones shared, and each element
//! is written as read while its definition is the one read from it.

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

pub use crate::document::definitions::*;

use super::font_table::{FontRename, rename_char};
use super::list_xml::{ListDefDirect, ListDirect, NumberingXml, PictureDirect};
use super::style_xml::{DefaultsDef, LatentDef, StyleDef, StylesPart, alone};
use super::write::props::Cx;
use crate::document::origin::Origin;
use crate::document::paragraph::ListId;
use crate::docx::list_xml::ListXml;
use crate::docx::style_xml::StyleXml;
use crate::format::cascade::{Cascade, Rules};
use crate::format::{DefinedIn, ListDefId, StyleSheet};

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
    fn merged(sheet: &StyleSheet, styles: &StyleHolders, numbering: &ListHolders, map: &super::glossary::GlossaryMap) -> GlossaryXml {
        let xml = |x: Option<&Rc<str>>| x.map(|x| x.to_string());
        let mut out = GlossaryXml::default();
        for (id, d) in &styles.styles {
            let view: Rc<str> = map.styles.view(&id.to_string()).into();
            if sheet.styles.get(&view).is_some_and(|e| e.origin == DefinedIn::Glossary)
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

/// What the WordprocessingML adapter adds to typed definitions: reading them from the styles and numbering parts,
/// and the elements they are written as.
pub trait TypedDefsXml: Sized {
    /// The `w:style` of style `id` as it is written: a glossary style's as the glossary holds it.
    fn style_xml(&self, id: &str) -> Option<Cow<'_, str>>;

    /// The `w:abstractNum` of list definition `id` as it is written: a glossary one's as the glossary holds it.
    fn def_xml(&self, id: i64) -> Option<Cow<'_, str>>;

    /// The `w:num` of list `id` as it is written, as [`Self::def_xml`].
    fn list_xml(&self, id: i64) -> Option<Cow<'_, str>>;

    /// The typed definitions of the styles part `styles` and numbering part `numbering`, with the glossary's parts
    /// (`glossary`) merged in, resolving within `env`; and how the glossary's ids map onto the document's.
    fn read(
        styles: Option<&str>,
        numbering: Option<&str>,
        glossary: Option<(Option<&str>, Option<&str>)>,
        env: Surroundings,
    ) -> (Self, Option<super::glossary::GlossaryMap>);
}

/// The glossary's elements as typed definitions hold them.
fn glossary(t: &TypedDefs) -> Option<&GlossaryXml> {
    t.origin.get()
}

impl TypedDefsXml for TypedDefs {
    fn style_xml(&self, id: &str) -> Option<Cow<'_, str>> {
        match self.styles.styles.get(id) {
            Some(d) => Some(d.xml(&self.cascade.colours, &self.styles.cx())),
            None => glossary(self)?.styles.get(id).map(|x| Cow::Borrowed(x.as_str())),
        }
    }

    fn def_xml(&self, id: i64) -> Option<Cow<'_, str>> {
        match self.numbering.defs.get(&ListDefId(id)) {
            Some(d) => Some(d.xml(&self.cascade.colours, &self.numbering.cx())),
            None => glossary(self)?.defs.get(&id).map(|x| Cow::Borrowed(x.as_str())),
        }
    }

    fn list_xml(&self, id: i64) -> Option<Cow<'_, str>> {
        match self.numbering.lists.get(&ListId(id)) {
            Some(d) => Some(d.xml(&self.cascade.colours, &self.numbering.cx())),
            None => glossary(self)?.lists.get(&id).map(|x| Cow::Borrowed(x.as_str())),
        }
    }

    fn read(
        styles: Option<&str>,
        numbering: Option<&str>,
        glossary: Option<(Option<&str>, Option<&str>)>,
        env: Surroundings,
    ) -> (TypedDefs, Option<super::glossary::GlossaryMap>) {
        let mut part = StyleHolders::parse(styles);
        let numbering = ListHolders::parse(numbering);
        let mut lists = numbering.lists();
        let mut held = GlossaryXml::default();
        let map = glossary.map(|(gs, gn)| {
            let (g, gn) = (StyleHolders::parse(gs), ListHolders::parse(gn));
            let map = super::glossary::merge_definitions(&mut part.sheet, &mut lists, &g.sheet, &gn.lists());
            held = GlossaryXml::merged(&part.sheet, &g, &gn, &map);
            map
        });
        let mut c = Cascade::new(Rc::new(part.sheet.clone()), Rc::new(lists), env.theme, env.colours, env.rules, super::style_xml::other_regions);
        (c.categories, c.divisions) = (env.categories, env.divisions);
        let typed = TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), origin: Origin::new(held) };
        (typed, map)
    }
}

/// The definitions `t` with `rename` applied: in each style's and region's, the defaults' (Word's where the part
/// writes none) and each definition's levels' character properties, the theme's fonts and the fonts' categories;
/// and the places that changed. The holders keep the elements as they are: saving renames the family in them.
fn renamed(t: &TypedDefs, rename: &FontRename) -> (TypedDefs, u32) {
    let c = &*t.cascade;
    let mut n = 0;
    let mut place = |k: u32| n += u32::from(k > 0);
    let mut part = (*t.styles).clone();
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
    let mut next = Cascade::new(Rc::new(part.sheet.clone()), Rc::new(lists), theme, c.colours, c.rules, super::style_xml::other_regions);
    (next.categories, next.divisions) = (categories, c.divisions.clone());
    let defs = TypedDefs { cascade: Rc::new(next), styles: Rc::new(part), numbering: t.numbering.clone(), origin: t.origin.clone() };
    (defs, n)
}

/// What the WordprocessingML adapter adds to a document's definitions: the elements they are written as, edits that
/// read or write them, and renaming a font through them.
pub trait DefinitionsXml {
    /// Renames `rename.from` to `rename.to` in the definitions below the text. Returns the places that changed:
    /// styles, document defaults, numbering levels and theme slots.
    fn rename_fonts(&mut self, rename: &FontRename) -> u32;

    /// The styles part's namespaces, as its elements are written.
    fn cx(&self) -> Cx;

    /// The `w:style` of style `id` as it is written.
    fn style_xml(&self, id: &str) -> Option<String>;

    /// The `w:docDefaults` as it is written.
    fn defaults_xml(&self) -> Option<String>;

    /// Adds or replaces a style from its `w:style` XML, as [`Self::set_style`] sets it.
    fn set_style_xml(&mut self, raw: String) -> Result<Vec<String>, StyleEditError>;

    /// Adds or replaces style `def`, refused when it would be based on itself. Returns the styles whose properties
    /// changed.
    fn set_style(&mut self, def: StyleDef) -> Result<Vec<String>, StyleEditError>;

    /// Removes style `id`: the styles based on it take its base, references to it from a next style or a link go.
    /// Refused when a style it rebases would then be based on itself. Returns the styles whose properties changed.
    fn delete_style(&mut self, id: &str) -> Result<Vec<String>, StyleEditError>;

    /// Replaces the document defaults from their `w:docDefaults` XML: a level it doesn't write keeps the one before.
    /// Returns every style, by id.
    #[cfg(test)]
    fn set_defaults_xml(&mut self, raw: String) -> Result<Vec<String>, StyleEditError>;

    /// Replaces the latent styles from their `w:latentStyles` XML.
    #[cfg(test)]
    fn set_latent_xml(&mut self, raw: String) -> Result<(), StyleEditError>;

    /// Adds or replaces a list definition from its `w:abstractNum` XML. Its id, or `None` when it isn't one.
    #[cfg(test)]
    fn set_abstract_xml(&mut self, raw: String) -> Option<i64>;

    /// Adds or replaces a list from its `w:num` XML, as [`Self::set_abstract_xml`].
    #[cfg(test)]
    fn set_num_xml(&mut self, raw: String) -> Option<i64>;

    /// Adds picture bullet `id` from its `w:numPicBullet` XML, its image read through relationship `rel` at `image`.
    fn add_picture(&mut self, id: i64, raw: String, rel: String, image: crate::docx::ResourceId);

    /// Gives picture bullets their images as read: each id with its image in the document's images.
    fn set_picture_images(&mut self, images: &[(i64, crate::docx::ResourceId)]);

    /// These definitions with the style `raw` (a `w:style`) holds set among them, and its id: `None` when it isn't
    /// one, or would be based on itself.
    fn with_style_xml(&self, raw: &str) -> Option<(Definitions, Rc<str>)>;

    /// Replaces the numbering part whole with `xml`'s definitions, lists and picture bullets.
    #[cfg(test)]
    fn set_numbering_part(&mut self, xml: &str);

    /// Follows the document settings `rules`, as the reader takes them from the settings part.
    #[cfg(test)]
    fn set_rules(&mut self, rules: Rules);

    /// These definitions with nothing as read: each style, the defaults, the latent styles, each definition, list and
    /// picture bullet written from its typed value, in parts whose roots are `styles_root` and `numbering_root`.
    fn unanchored(&self, styles_root: Rc<str>, numbering_root: String) -> Definitions;

    /// The styles, to edit.
    fn style_edits(&mut self) -> StyleEdits<'_>;

    /// The numbering, to edit.
    fn numbering_edits(&mut self) -> NumberingEdits<'_>;
}

/// The styles that changed with style `id`: its dependants before and after, and the default styles when they moved,
/// as [`Styles::set_xml`] gives them.
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

impl DefinitionsXml for Definitions {
    fn rename_fonts(&mut self, rename: &FontRename) -> u32 {
        let (typed, n) = renamed(&self.typed(), rename);
        *self = Definitions::new(typed);
        n
    }

    fn cx(&self) -> Cx {
        self.typed().styles.cx()
    }

    fn style_xml(&self, id: &str) -> Option<String> {
        self.typed().style_xml(id).map(Cow::into_owned)
    }

    fn defaults_xml(&self) -> Option<String> {
        let t = self.typed();
        t.styles.defaults.as_ref().map(|d| d.xml(&t.cascade.colours, &t.styles.cx()).into_owned())
    }

    fn set_style_xml(&mut self, raw: String) -> Result<Vec<String>, StyleEditError> {
        let cx = self.typed().styles.cx();
        let style = cx.parse(&raw, |_, el| (alone(el) && super::xml::is_w(el, "style")).then(|| super::style_xml::parse(el)).flatten()).flatten();
        let style = style.ok_or(StyleEditError::Malformed)?;
        self.set_style(StyleDef::read(style, raw.into()))
    }

    fn set_style(&mut self, def: StyleDef) -> Result<Vec<String>, StyleEditError> {
        let id = def.get().id.clone();
        let colours = self.colours();
        let before = self.cascade();
        let existed = before.styles.styles.contains_key(&id);
        self.edit(|part, _, _| part.set(def, &colours))?;
        Ok(changed_with(&before, &self.cascade(), &id, existed))
    }

    fn delete_style(&mut self, id: &str) -> Result<Vec<String>, StyleEditError> {
        let before = self.cascade();
        if !before.styles.styles.contains_key(id) {
            return Err(StyleEditError::Missing);
        }
        let rebased: Vec<Rc<str>> = before
            .styles
            .styles
            .values()
            .filter(|e| &*e.style.id != id && e.style.based_on.as_deref() == Some(id) && !e.style.word_default_table())
            .map(|e| e.style.id.clone())
            .collect();
        let colours = self.colours();
        let mut part = (*self.typed().styles).clone();
        part.delete(id, &colours);
        if rebased.iter().any(|r| part.sheet.styles.get(r).is_some_and(|e| part.loops(&e.style))) {
            return Err(StyleEditError::Loop);
        }
        let changed: Vec<String> = before.dependants(id).iter().map(|s| s.to_string()).collect();
        self.edit(|p, _, _| *p = part);
        Ok(changed)
    }

    #[cfg(test)]
    fn set_defaults_xml(&mut self, raw: String) -> Result<Vec<String>, StyleEditError> {
        let cx = self.typed().styles.cx();
        let parsed = cx.parse(&raw, |_, el| super::style_xml::parse_defaults(el)).ok_or(StyleEditError::Malformed)?;
        let was = self.cascade().styles.defaults.clone();
        let merged = crate::format::Defaults {
            char: parsed.char.clone().or_else(|| was.as_ref().and_then(|d| d.char.clone())),
            para: parsed.para.clone().or_else(|| was.as_ref().and_then(|d| d.para.clone())),
            foreign: parsed.foreign.clone(),
        };
        self.set_defaults(DefaultsDef::read(merged, raw.into()))
    }

    #[cfg(test)]
    fn set_latent_xml(&mut self, raw: String) -> Result<(), StyleEditError> {
        let cx = self.typed().styles.cx();
        let parsed =
            cx.parse(&raw, |_, el| (alone(el) && super::xml::is_w(el, "latentStyles")).then(|| super::style_xml::parse_latent(el))).flatten();
        self.set_latent(LatentDef::read(parsed.ok_or(StyleEditError::Malformed)?, raw.into()))
    }

    #[cfg(test)]
    fn set_abstract_xml(&mut self, raw: String) -> Option<i64> {
        let d = ListDefDirect::from_xml(&raw, &self.typed().numbering.cx())?;
        Some(self.set_list_def(d).0)
    }

    #[cfg(test)]
    fn set_num_xml(&mut self, raw: String) -> Option<i64> {
        let d = ListDirect::from_xml(&raw, &self.typed().numbering.cx())?;
        Some(self.set_list(d).0)
    }

    fn add_picture(&mut self, id: i64, raw: String, rel: String, image: crate::docx::ResourceId) {
        let _ = rel;
        if let Some(d) = PictureDirect::from_xml(&raw, &self.typed().numbering.cx()) {
            let mut v = (**d.get()).clone();
            v.image = Some(image);
            let d = PictureDirect::read(v, raw.as_str().into());
            self.edit(|_, numbering, lists| {
                lists.pictures.insert(id, d.get().clone());
                numbering.pictures.insert(id, d);
            });
        }
    }

    fn set_picture_images(&mut self, images: &[(i64, crate::docx::ResourceId)]) {
        if images.is_empty() {
            return;
        }
        let cx = self.typed().numbering.cx();
        self.edit(|_, numbering, lists| {
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

    fn with_style_xml(&self, raw: &str) -> Option<(Definitions, Rc<str>)> {
        let id = self.typed().styles.style_from_xml(raw)?.id;
        let mut next = self.clone();
        next.set_style_xml(raw.to_string()).ok()?;
        Some((next, id))
    }

    #[cfg(test)]
    fn set_numbering_part(&mut self, xml: &str) {
        let part = ListHolders::parse(Some(xml));
        self.edit(|_, numbering, lists| {
            *lists = part.lists();
            *numbering = part;
        });
    }

    #[cfg(test)]
    fn set_rules(&mut self, rules: Rules) {
        let t = self.typed();
        let mut c = (*t.cascade).clone();
        c.rules = rules;
        let c = c.over(c.styles.clone(), c.lists.clone());
        *self = Definitions::new(TypedDefs { cascade: Rc::new(c), ..(*t).clone() });
    }

    fn unanchored(&self, styles_root: Rc<str>, numbering_root: String) -> Definitions {
        let t = self.typed();
        let mut part = (*t.styles).clone().with_root(styles_root);
        for d in part.styles.values_mut() {
            *d = StyleDef::typed(d.get().clone());
        }
        part.defaults = part.defaults.as_ref().map(|d| DefaultsDef::typed(d.get().clone()));
        part.latent = part.latent.as_ref().map(|d| LatentDef::typed(d.get().clone()));
        let mut numbering = (*t.numbering).clone().with_root(numbering_root);
        for d in numbering.defs.values_mut() {
            *d = ListDefDirect::typed(d.get().clone());
        }
        for d in numbering.lists.values_mut() {
            *d = ListDirect::typed(d.get().clone());
        }
        for d in numbering.pictures.values_mut() {
            *d = PictureDirect::typed(d.get().clone());
        }
        Definitions::new(TypedDefs { styles: Rc::new(part), numbering: Rc::new(numbering), ..(*t).clone() })
    }

    fn style_edits(&mut self) -> StyleEdits<'_> {
        StyleEdits(self)
    }

    fn numbering_edits(&mut self) -> NumberingEdits<'_> {
        NumberingEdits(self)
    }
}

/// A document's styles being edited.
pub struct StyleEdits<'a>(&'a mut Definitions);

impl StyleEdits<'_> {
    /// [`Definitions::set_style`].
    pub fn set(&mut self, def: StyleDef) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.set_style(def)
    }

    /// [`Definitions::set_defaults`].
    pub fn set_defaults(&mut self, def: DefaultsDef) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.set_defaults(def)
    }

    /// [`Definitions::set_latent`].
    pub fn set_latent(&mut self, def: LatentDef) -> Result<(), crate::docx::style_xml::StyleEditError> {
        self.0.set_latent(def)
    }

    /// The typed definitions as the edits so far leave them.
    pub fn typed(&self) -> Rc<TypedDefs> {
        self.0.typed()
    }

    /// [`Definitions::set_style_xml`].
    #[cfg(test)]
    pub fn set_xml(&mut self, raw: String) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.set_style_xml(raw)
    }

    /// [`Definitions::delete_style`].
    pub fn delete(&mut self, id: &str) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.delete_style(id)
    }

    /// [`Definitions::set_defaults_xml`].
    #[cfg(test)]
    pub fn set_defaults_xml(&mut self, raw: String) -> Result<Vec<String>, crate::docx::style_xml::StyleEditError> {
        self.0.set_defaults_xml(raw)
    }

    /// [`Definitions::set_latent_xml`].
    #[cfg(test)]
    pub fn set_latent_xml(&mut self, raw: String) -> Result<(), crate::docx::style_xml::StyleEditError> {
        self.0.set_latent_xml(raw)
    }
}

/// A document's numbering being edited, as [`StyleEdits`].
pub struct NumberingEdits<'a>(&'a mut Definitions);

impl NumberingEdits<'_> {
    /// [`Definitions::set_list_def`].
    pub fn set_list_def(&mut self, d: ListDefDirect) -> ListDefId {
        self.0.set_list_def(d)
    }

    /// [`Definitions::set_list`].
    pub fn set_list(&mut self, d: ListDirect) -> ListId {
        self.0.set_list(d)
    }

    /// The typed definitions as the edits so far leave them.
    pub fn typed(&self) -> Rc<TypedDefs> {
        self.0.typed()
    }

    /// [`Definitions::set_abstract_xml`].
    #[cfg(test)]
    pub fn set_abstract_xml(&mut self, raw: String, _colours: &super::props::ThemeColours) -> Option<i64> {
        self.0.set_abstract_xml(raw)
    }

    /// [`Definitions::set_num_xml`].
    #[cfg(test)]
    pub fn set_num_xml(&mut self, raw: String, _colours: &super::props::ThemeColours) -> Option<i64> {
        self.0.set_num_xml(raw)
    }

    /// [`Definitions::add_picture`].
    pub fn add_picture(&mut self, id: i64, raw: String, rel: String, image: crate::docx::ResourceId) {
        self.0.add_picture(id, raw, rel, image);
    }
}
