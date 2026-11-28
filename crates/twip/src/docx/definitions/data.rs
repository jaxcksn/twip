//! The definitions a document resolves with: its styles and lists, typed, each with where it was read from

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::document::origin::Origin;
use crate::document::paragraph::ListId;
use crate::format::cascade::Cascade;
use crate::format::{
    CharProps, Defaults, DefaultsDef, LatentDef, ListDefDirect, ListDefId, ListDirect, Lists, ParaProps, PictureDirect, Style, StyleDef, StyleKind,
    StyleSheet, ThemeColours,
};

/// Why a style edit is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleEditError {
    /// What was given isn't one style with an id.
    Malformed,
    /// It would be based on itself, through its chain.
    Loop,
    /// No such style.
    Missing,
}

/// A document's styles: the definitions, and each style's, the defaults' and the latent styles' holders.
#[derive(Clone, Debug, Default)]
pub struct StyleHolders {
    /// The definitions.
    pub sheet: StyleSheet,
    /// Each style's holder, by id: the one the sheet holds.
    pub styles: HashMap<Rc<str>, StyleDef>,
    /// The defaults' holder.
    pub defaults: Option<DefaultsDef>,
    /// The latent styles' holder.
    pub latent: Option<LatentDef>,
    /// Where the styles were read from, as the adapter that read them keeps it.
    pub origin: Origin,
}

impl StyleHolders {
    /// Whether `s` would be based on itself: its bases through the sheet's styles (Word's built-in default table
    /// style without one, as the last working out left it) reach its id, or go past 32 of them.
    pub fn loops(&self, s: &Style) -> bool {
        let mut base = s.based_on.as_deref();
        let mut hops = 0;
        while let Some(b) = base {
            if b == &*s.id || hops > 32 {
                return true;
            }
            base = self.sheet.styles.get(b).and_then(|e| e.style.based_on.as_deref().filter(|_| !e.style.word_default_table()));
            hops += 1;
        }
        false
    }
}

/// A document's list definitions, lists and picture bullets, each with its holder.
#[derive(Clone, Debug, Default)]
pub struct ListHolders {
    /// The definitions, by id.
    pub defs: HashMap<ListDefId, ListDefDirect>,
    /// The lists, by id.
    pub lists: HashMap<ListId, ListDirect>,
    /// The picture bullets, by id.
    pub pictures: BTreeMap<i64, PictureDirect>,
    /// Where they were read from, as the adapter that read them keeps it.
    pub origin: Origin,
}

impl ListHolders {
    /// The typed definitions, lists and picture bullets.
    pub fn lists(&self) -> Lists {
        Lists {
            defs: self.defs.iter().map(|(k, d)| (*k, d.get().clone())).collect(),
            lists: self.lists.iter().map(|(k, l)| (*k, l.get().clone())).collect(),
            pictures: self.pictures.iter().map(|(k, p)| (*k, p.get().clone())).collect(),
        }
    }
}

/// The typed definitions: the cascade, and the holders of the styles and lists.
#[derive(Clone, Debug)]
pub struct TypedDefs {
    /// The cascade over the definitions.
    pub cascade: Rc<Cascade>,
    /// The styles' holders; the sheet holds the glossary's styles too.
    pub styles: Rc<StyleHolders>,
    /// The lists' holders.
    pub numbering: Rc<ListHolders>,
    /// What the adapter that read them keeps beside them.
    pub origin: Origin,
}

/// The typed definitions a document resolves with, to read and to edit.
#[derive(Clone, Debug)]
pub struct Definitions {
    typed: Rc<TypedDefs>,
}

impl Definitions {
    /// The definitions `typed`.
    pub fn new(typed: TypedDefs) -> Definitions {
        Definitions { typed: Rc::new(typed) }
    }

    /// The typed definitions.
    pub fn typed(&self) -> Rc<TypedDefs> {
        self.typed.clone()
    }

    /// The typed cascade.
    pub fn cascade(&self) -> Rc<Cascade> {
        self.typed.cascade.clone()
    }

    /// Whether the document defines style `id`.
    pub fn has_style(&self, id: &str) -> bool {
        self.cascade().styles.styles.contains_key(id)
    }

    /// The kind of style `id`, when the document defines it.
    pub fn style_kind(&self, id: &str) -> Option<StyleKind> {
        self.cascade().styles.get(id).map(|s| s.kind)
    }

    /// The theme colours the definitions resolve with.
    pub fn colours(&self) -> ThemeColours {
        self.cascade().colours
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
}

// Edits

impl Definitions {
    /// The typed definitions as `edit` leaves a copy of them, the cascade built again over them.
    pub fn edit<T>(&mut self, edit: impl FnOnce(&mut StyleHolders, &mut ListHolders, &mut Lists) -> T) -> T {
        let typed = &self.typed;
        let (mut part, mut numbering, mut lists) = ((*typed.styles).clone(), (*typed.numbering).clone(), (*typed.cascade.lists).clone());
        let out = edit(&mut part, &mut numbering, &mut lists);
        let c = typed.cascade.over(Rc::new(part.sheet.clone()), Rc::new(lists));
        let defs = TypedDefs { cascade: Rc::new(c), styles: Rc::new(part), numbering: Rc::new(numbering), origin: typed.origin.clone() };
        self.typed = Rc::new(defs);
        out
    }

    /// Replaces the document defaults with `def`. Returns every style, by id.
    pub fn set_defaults(&mut self, def: DefaultsDef) -> Result<Vec<String>, StyleEditError> {
        self.edit(|part, _, _| {
            part.sheet.defaults = Some(def.get().clone());
            part.defaults = Some(def);
        });
        let mut all: Vec<String> = self.typed.cascade.styles.styles.keys().map(|k| k.to_string()).collect();
        all.sort();
        Ok(all)
    }

    /// Replaces the latent styles with `def`.
    pub fn set_latent(&mut self, def: LatentDef) -> Result<(), StyleEditError> {
        self.edit(|part, _, _| {
            part.sheet.latent = Some(def.get().clone());
            part.latent = Some(def);
        });
        Ok(())
    }

    /// Adds or replaces list definition `d`. Its id.
    pub fn set_list_def(&mut self, d: ListDefDirect) -> ListDefId {
        let id = d.get().id;
        self.edit(|_, numbering, lists| {
            lists.defs.insert(id, d.get().clone());
            numbering.defs.insert(id, d);
        });
        id
    }

    /// Adds or replaces list `d`. Its id.
    pub fn set_list(&mut self, d: ListDirect) -> ListId {
        let id = d.get().id;
        self.edit(|_, numbering, lists| {
            lists.lists.insert(id, d.get().clone());
            numbering.lists.insert(id, d);
        });
        id
    }

    /// Takes the document defaults out from under the styles: text resolves over no defaults at all.
    pub fn without_defaults(&mut self) {
        let none = Rc::new(Defaults { char: Some(CharProps::default()), para: Some(ParaProps::default()), foreign: None });
        self.edit(|part, _, _| part.sheet.defaults = Some(none));
    }
}
