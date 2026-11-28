//! Resources

use std::collections::BTreeMap;

use crate::ident::ResourceId;

/// A document's resources: each image's bytes, by id, in the order they were added (ids grow).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resources(BTreeMap<ResourceId, Vec<u8>>);

impl Resources {
    /// Resource `id`'s bytes; `None` for an id that names none.
    pub fn get(&self, id: ResourceId) -> Option<&[u8]> {
        self.0.get(&id).map(Vec::as_slice)
    }

    /// Resource `id`'s bytes, to change.
    pub fn get_mut(&mut self, id: ResourceId) -> Option<&mut Vec<u8>> {
        self.0.get_mut(&id)
    }

    /// Adds `data` under a new id, and returns it.
    pub fn add(&mut self, data: Vec<u8>) -> ResourceId {
        let id = ResourceId::next();
        self.0.insert(id, data);
        id
    }

    /// Puts `data` under `id`, returning what was there.
    pub fn insert(&mut self, id: ResourceId, data: Vec<u8>) -> Option<Vec<u8>> {
        self.0.insert(id, data)
    }

    /// Takes resource `id` out.
    pub fn remove(&mut self, id: ResourceId) -> Option<Vec<u8>> {
        self.0.remove(&id)
    }

    /// Whether `id` names a resource.
    pub fn contains(&self, id: ResourceId) -> bool {
        self.0.contains_key(&id)
    }

    /// The first resource (in id order) whose bytes are `data`.
    pub fn find(&self, data: &[u8]) -> Option<ResourceId> {
        self.iter().find_map(|(id, d)| (d == data).then_some(id))
    }

    /// Each resource, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (ResourceId, &[u8])> {
        self.0.iter().map(|(id, d)| (*id, d.as_slice()))
    }

    /// The last resource added (the greatest id).
    pub fn last(&self) -> Option<(ResourceId, &[u8])> {
        self.0.last_key_value().map(|(id, d)| (*id, d.as_slice()))
    }

    /// How many there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// There are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<(ResourceId, Vec<u8>)> for Resources {
    fn from_iter<I: IntoIterator<Item = (ResourceId, Vec<u8>)>>(iter: I) -> Resources {
        Resources(iter.into_iter().collect())
    }
}

impl IntoIterator for Resources {
    type Item = (ResourceId, Vec<u8>);
    type IntoIter = std::collections::btree_map::IntoIter<ResourceId, Vec<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Extend<(ResourceId, Vec<u8>)> for Resources {
    fn extend<I: IntoIterator<Item = (ResourceId, Vec<u8>)>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

#[cfg(test)]
mod tests {
    use super::Resources;

    #[test]
    fn ids_order_resources_as_added_and_an_unknown_id_names_none() {
        let mut r = Resources::default();
        let (a, b) = (r.add(vec![1]), r.add(vec![2]));
        assert!(a < b);
        assert_eq!(r.iter().map(|(_, d)| d[0]).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(r.find(&[2]), Some(b));
        assert_eq!(r.remove(a), Some(vec![1]));
        assert_eq!(r.get(a), None);
    }
}
