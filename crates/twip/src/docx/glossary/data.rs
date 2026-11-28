//! How the glossary's style, list and list definition ids map onto the document's tables

use std::collections::HashMap;
use std::hash::Hash;

/// Ids as the glossary's parts write them, and the ids the document's tables hold them under. Ids that are the same
/// in both aren't stored.
#[derive(Clone, Debug, PartialEq)]
pub struct IdMap<K: Eq + Hash> {
    to_view: HashMap<K, K>,
    to_glossary: HashMap<K, K>,
}

impl<K: Eq + Hash> Default for IdMap<K> {
    fn default() -> Self {
        IdMap { to_view: HashMap::new(), to_glossary: HashMap::new() }
    }
}

impl<K: Eq + Hash + Clone> IdMap<K> {
    /// Maps `glossary`, an id as the glossary's parts write it, to `view`, the document table's.
    pub fn insert(&mut self, glossary: K, view: K) {
        if glossary != view {
            self.to_glossary.insert(view.clone(), glossary.clone());
            self.to_view.insert(glossary, view);
        }
    }

    /// The document table's id for a glossary id.
    pub fn view(&self, glossary: &K) -> K {
        self.to_view.get(glossary).unwrap_or(glossary).clone()
    }

    /// The glossary's id for a document table id.
    pub fn glossary(&self, view: &K) -> K {
        self.to_glossary.get(view).unwrap_or(view).clone()
    }
}

/// How the glossary's style, list and list definition ids map onto the document's tables.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossaryMap {
    pub styles: IdMap<String>,
    pub nums: IdMap<i64>,
    pub abstracts: IdMap<i64>,
}
