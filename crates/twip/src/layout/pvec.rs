//! Persistent page lists

use std::ops::Range;
use std::rc::Rc;

const BITS: u32 = 5;
const WIDTH: usize = 1 << BITS;
const MASK: usize = WIDTH - 1;

#[derive(Debug)]
enum Node<T> {
    Leaf(Vec<T>),
    Branch(Vec<Rc<Node<T>>>),
}

/// A list that shares what it doesn't change with the list it was made from: a 32-way tree of its items in order, so
/// an item is set or added by copying the nodes on its path, and two lists compare by the subtrees they don't share.
#[derive(Debug)]
pub struct Pvec<T> {
    root: Rc<Node<T>>,
    /// Levels of branches above the leaves.
    height: u32,
    len: usize,
}

impl<T> Clone for Pvec<T> {
    fn clone(&self) -> Self {
        Pvec { root: self.root.clone(), height: self.height, len: self.len }
    }
}

impl<T> Default for Pvec<T> {
    fn default() -> Self {
        Pvec { root: Rc::new(Node::Leaf(Vec::new())), height: 0, len: 0 }
    }
}

impl<T: Clone> FromIterator<T> for Pvec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
        let mut level: Vec<Rc<Node<T>>> = Vec::new();
        let mut leaf = Vec::with_capacity(WIDTH);
        let mut len = 0;
        for item in items {
            leaf.push(item);
            len += 1;
            if leaf.len() == WIDTH {
                level.push(Rc::new(Node::Leaf(std::mem::replace(&mut leaf, Vec::with_capacity(WIDTH)))));
            }
        }
        if !leaf.is_empty() || level.is_empty() {
            level.push(Rc::new(Node::Leaf(leaf)));
        }
        let mut height = 0;
        while level.len() > 1 {
            level = level.chunks(WIDTH).map(|c| Rc::new(Node::Branch(c.to_vec()))).collect();
            height += 1;
        }
        Pvec { root: level.pop().expect("a root"), height, len }
    }
}

impl<T> Pvec<T> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len {
            return None;
        }
        let mut node = &*self.root;
        let mut level = self.height;
        loop {
            match node {
                Node::Branch(children) => {
                    node = &children[(i >> (BITS * level)) & MASK];
                    level -= 1;
                }
                Node::Leaf(items) => return items.get(i & MASK),
            }
        }
    }

    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }

    pub fn last(&self) -> Option<&T> {
        self.len.checked_sub(1).and_then(|i| self.get(i))
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter { list: self, front: 0, back: self.len }
    }

    /// The places whose items may differ from `old`'s: every place in a subtree the lists don't share, and the places
    /// only one of them has. Ascending and not overlapping.
    pub fn changed_from(&self, old: &Pvec<T>) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        if self.height == old.height {
            diff(&self.root, &old.root, self.height, 0, &mut out);
        } else {
            out.push(0..self.len.max(old.len));
        }
        // Clipped to the shorter list, then what the longer one has past it.
        let (short, long) = (self.len.min(old.len), self.len.max(old.len));
        let mut clipped: Vec<Range<usize>> = Vec::new();
        for r in out.into_iter().map(|r| r.start..r.end.min(short)).filter(|r| !r.is_empty()).chain(std::iter::once(short..long)) {
            match clipped.last_mut() {
                Some(l) if l.end >= r.start => l.end = l.end.max(r.end),
                _ if !r.is_empty() => clipped.push(r),
                _ => {}
            }
        }
        clipped
    }
}

fn diff<T>(a: &Rc<Node<T>>, b: &Rc<Node<T>>, level: u32, start: usize, out: &mut Vec<Range<usize>>) {
    if Rc::ptr_eq(a, b) {
        return;
    }
    let span = WIDTH << (BITS * level);
    match (&**a, &**b) {
        (Node::Branch(x), Node::Branch(y)) => {
            let step = span / WIDTH;
            for k in 0..x.len().max(y.len()) {
                match (x.get(k), y.get(k)) {
                    (Some(c), Some(d)) => diff(c, d, level - 1, start + k * step, out),
                    _ => out.push(start + k * step..start + (k + 1) * step),
                }
            }
        }
        _ => out.push(start..start + span),
    }
}

impl<T: Clone> Pvec<T> {
    /// Item `i` set to `item`, or added when `i` is the length.
    pub fn set(&mut self, i: usize, item: T) {
        assert!(i <= self.len, "set {i} past the end {}", self.len);
        if i == self.len && self.len == WIDTH << (BITS * self.height) {
            let old = std::mem::replace(&mut self.root, Rc::new(Node::Branch(Vec::new())));
            self.root = Rc::new(Node::Branch(vec![old]));
            self.height += 1;
        }
        set_in(&mut self.root, self.height, i, item);
        self.len = self.len.max(i + 1);
    }

    pub fn push(&mut self, item: T) {
        self.set(self.len, item);
    }

    /// The first `len` items. What is cut stays in the tree until set again: [`Pvec::get`] and the iterator stop at
    /// the length.
    pub fn truncate(&mut self, len: usize) {
        self.len = self.len.min(len);
    }
}

fn set_in<T: Clone>(node: &mut Rc<Node<T>>, level: u32, i: usize, item: T) {
    if Rc::get_mut(node).is_none() {
        *node = Rc::new(match &**node {
            Node::Leaf(items) => Node::Leaf(items.clone()),
            Node::Branch(children) => Node::Branch(children.clone()),
        });
    }
    match Rc::get_mut(node).expect("made unique") {
        Node::Leaf(items) => {
            let k = i & MASK;
            if k == items.len() {
                items.push(item);
            } else {
                items[k] = item;
            }
        }
        Node::Branch(children) => {
            let k = (i >> (BITS * level)) & MASK;
            if k == children.len() {
                children.push(Rc::new(if level == 1 { Node::Leaf(Vec::new()) } else { Node::Branch(Vec::new()) }));
            }
            set_in(&mut children[k], level - 1, i, item);
        }
    }
}

impl<T> std::ops::Index<usize> for Pvec<T> {
    type Output = T;

    fn index(&self, i: usize) -> &T {
        self.get(i).unwrap_or_else(|| panic!("index {i} out of range for a list of {}", self.len))
    }
}

impl<'a, T> IntoIterator for &'a Pvec<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<T: PartialEq> PartialEq for Pvec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

/// A [`Pvec`]'s items in order.
pub struct Iter<'a, T> {
    list: &'a Pvec<T>,
    front: usize,
    back: usize,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        (self.front < self.back).then(|| {
            self.front += 1;
            &self.list[self.front - 1]
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.back - self.front, Some(self.back - self.front))
    }
}

impl<T> ExactSizeIterator for Iter<'_, T> {}

impl<T> DoubleEndedIterator for Iter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        (self.front < self.back).then(|| {
            self.back -= 1;
            &self.list[self.back]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Pvec;

    #[test]
    fn sets_and_adds_sharing_the_rest() {
        let a: Pvec<usize> = (0..2000).collect();
        let mut b = a.clone();
        b.set(1500, 7);
        for i in 2000..2100 {
            b.push(i);
        }
        assert_eq!(a.iter().copied().collect::<Vec<_>>(), (0..2000).collect::<Vec<_>>());
        let want: Vec<usize> = (0..2100).map(|i| if i == 1500 { 7 } else { i }).collect();
        assert_eq!(b.iter().copied().collect::<Vec<_>>(), want);
        assert_eq!(b.iter().next_back(), Some(&2099));
        assert_eq!(b.changed_from(&a), [1472..1504, 1984..2100]);
        b.truncate(10);
        b.push(99);
        assert_eq!(b.iter().copied().collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 99]);
        let big: Pvec<usize> = (0..40_000).collect();
        assert_eq!((big[0], big[1024], big[39_999], big.len()), (0, 1024, 39_999, 40_000));
    }
}
