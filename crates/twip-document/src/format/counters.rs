//! List counters on typed definitions: the numbers a story's list paragraphs are at, and their labels' text

use std::collections::{HashMap, HashSet};

use super::cascade::Cascade;
use super::{ListDefId, NumberFormat};
use crate::paragraph::ListId;

/// The numbers lists are at, by definition (lists numbering with the same definition count together), as a story's
/// paragraphs are read in order.
#[derive(Clone, Debug, Default)]
pub struct Counters {
    values: HashMap<ListDefId, [Option<i64>; 9]>,
    /// The lists' levels whose start override has been taken.
    applied: HashSet<(ListId, i64)>,
}

impl Counters {
    /// The value the counter for level `ilvl` of `list` has now.
    pub fn value(&self, c: &Cascade, list: ListId, ilvl: i64) -> Option<i64> {
        self.values.get(&c.abstract_of(list)?)?[ilvl.clamp(0, 8) as usize]
    }

    /// Every level's counter for `list` now.
    pub fn values(&self, c: &Cascade, list: ListId) -> Option<[Option<i64>; 9]> {
        self.values.get(&c.abstract_of(list)?).copied()
    }

    /// Advances the counter for an item at level `ilvl` of `list` and returns its label text: the level's text with
    /// each `%n` the number of level n, in that level's format (in Arabic numerals under legal numbering, a leading
    /// zero kept); a bullet's text as it is.
    pub fn next(&mut self, c: &Cascade, list: ListId, ilvl: i64) -> Option<String> {
        use super::format_name;
        let a = c.abstract_of(list)?;
        let levels = c.levels(list)?;
        let ilvl = ilvl.clamp(0, 8) as usize;
        let level = levels[ilvl]?;
        let vals = self.values.entry(a).or_insert([None; 9]);
        let over = c.lists.lists.get(&list).and_then(|n| n.override_of(ilvl as i64)).and_then(|o| o.start);
        let start = level.start.unwrap_or(0);
        vals[ilvl] = match over {
            Some(s) if self.applied.insert((list, ilvl as i64)) => Some(s),
            _ => Some(vals[ilvl].map_or(start, |v| v.saturating_add(1))),
        };
        for k in ilvl + 1..9 {
            let reset = match levels[k].and_then(|l| l.restart) {
                None => true,
                Some(0) => false,
                Some(n) => (ilvl as i64) < n,
            };
            if reset {
                vals[k] = None;
            }
        }
        for k in 0..ilvl {
            if vals[k].is_none() {
                vals[k] = Some(levels[k].map_or(1, |l| l.start.unwrap_or(0)));
            }
        }
        let vals = *vals;
        if level.format.as_ref().and_then(super::Literal::known) == Some(NumberFormat::Bullet) {
            return Some(level.text.as_deref().unwrap_or_default().to_string());
        }
        let mut out = String::new();
        let mut chars = level.text.as_deref().unwrap_or_default().chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '%'
                && let Some(d) = chars.peek().and_then(|d| d.to_digit(10))
            {
                chars.next();
                let k = (d as usize).saturating_sub(1).min(8);
                let v = vals[k].unwrap_or(1);
                let fmt = levels[k].map_or("decimal", |l| format_name(l.format.as_ref()));
                let fmt = if level.legal == Some(true) && fmt != "decimalZero" { "decimal" } else { fmt };
                out.push_str(&crate::numbers::format_number(v, fmt));
                continue;
            }
            out.push(ch);
        }
        Some(out)
    }
}
