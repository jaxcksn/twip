//! The list galleries: the built-in libraries, the lists recently used, the document's lists and list styles; applying an
//! entry, and its preview.

use std::rc::Rc;

use crate::api::command::{ListApplyTo, ListDefinitionChange, ListLevel, NumberFormat};
use crate::api::services::{ListEntry, ListEntryId, ListGalleryKind, ListGallerySection, ListGallerySectionKind};
use crate::docx::{Block, Inline, Paragraph, RunStyle, next_para_id};
use crate::font::Fonts;
use crate::layout::ParaId;
use crate::services::lists;

use super::Editor;
use super::commands::Outcome;
use crate::docx::run_xml;

/// Recent entries kept per gallery.
const RECENT: usize = 8;

fn kind_name(k: ListGalleryKind) -> &'static str {
    match k {
        ListGalleryKind::Bullet => "bullet",
        ListGalleryKind::Numbering => "numbering",
        ListGalleryKind::Multilevel => "multilevel",
    }
}

fn kind_of_name(s: &str) -> Option<ListGalleryKind> {
    [ListGalleryKind::Bullet, ListGalleryKind::Numbering, ListGalleryKind::Multilevel].into_iter().find(|k| kind_name(*k) == s)
}

/// The library of a gallery.
fn library(k: ListGalleryKind) -> Vec<Vec<ListLevel>> {
    let table: &[&[lists::L]] = match k {
        ListGalleryKind::Bullet => &lists::BULLETS,
        ListGalleryKind::Numbering => &lists::NUMBERING,
        ListGalleryKind::Multilevel => &lists::MULTILEVEL,
    };
    table.iter().map(|e| lists::levels(e)).collect()
}

/// The gallery a list's levels belong in: a bullet, one level of numbers, or several.
pub fn gallery_of(levels: &[ListLevel], multilevel: bool) -> ListGalleryKind {
    match levels.first() {
        _ if multilevel => ListGalleryKind::Multilevel,
        Some(l) if l.format == NumberFormat::Bullet => ListGalleryKind::Bullet,
        _ => ListGalleryKind::Numbering,
    }
}

/// An entry's short label: a bullet's character, or its first three levels' numbers.
fn label(levels: &[ListLevel], k: ListGalleryKind) -> String {
    let one = |l: &ListLevel| match l.format {
        NumberFormat::Bullet => l.text.clone(),
        NumberFormat::None => String::new(),
        _ => {
            let mut out = String::new();
            let mut chars = l.text.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '%' && chars.peek().is_some_and(|d| d.is_ascii_digit()) {
                    let d = chars.next().and_then(|d| d.to_digit(10)).unwrap_or(1) as usize;
                    let fmt = levels.get(d.saturating_sub(1)).map_or(NumberFormat::Decimal, |x| x.format.clone());
                    let fmt = if l.legal && fmt != NumberFormat::DecimalZero { NumberFormat::Decimal } else { fmt };
                    out.push_str(&crate::docx::numbering::format_number(1, fmt.val()));
                } else {
                    out.push(c);
                }
            }
            out
        }
    };
    let shown = if k == ListGalleryKind::Multilevel { 3 } else { 1 };
    levels.iter().take(shown).map(one).collect::<Vec<_>>().join(" ")
}

impl Editor {
    /// A gallery's sections: the library, the recently used, the document's lists of its kind, and (for multilevel
    /// lists) the document's list styles.
    pub fn list_galleries(&self, k: ListGalleryKind) -> Vec<ListGallerySection> {
        let entry = |id: String, levels: Vec<ListLevel>| ListEntry { id: ListEntryId(id), label: label(&levels, k), levels };
        let library = library(k).into_iter().enumerate().map(|(i, l)| entry(format!("library:{}:{}", kind_name(k), i + 1), l)).collect();
        let recent =
            self.recent_lists.iter().enumerate().filter(|(_, (rk, _))| *rk == k).map(|(i, (_, l))| entry(format!("recent:{i}"), l.clone())).collect();
        let mut out = vec![
            ListGallerySection { kind: ListGallerySectionKind::Library, entries: library },
            ListGallerySection { kind: ListGallerySectionKind::RecentlyUsed, entries: recent },
        ];
        let Some(res) = self.doc.resolver.as_ref() else { return out };
        let c = res.cascade();
        let mut nums: Vec<i64> = self.list_paras().into_iter().map(|(_, n, _)| n).collect();
        nums.dedup();
        let mut seen = std::collections::HashSet::new();
        let document = nums
            .into_iter()
            .filter(|n| seen.insert(*n))
            .filter_map(|n| {
                let levels = self.list_levels(n)?;
                let multi = c
                    .abstract_of(crate::document::paragraph::ListId(n))
                    .and_then(|a| c.lists.defs.get(&a))
                    .is_some_and(|a| crate::docx::list_xml::multi_level_name(a.multi_level_type.as_ref()) == Some("multilevel"));
                (gallery_of(&levels, multi) == k).then(|| entry(format!("document:{n}"), levels))
            })
            .collect();
        out.push(ListGallerySection { kind: ListGallerySectionKind::Document, entries: document });
        if k == ListGalleryKind::Multilevel {
            let name = |s: &crate::format::Style| s.name.as_deref().unwrap_or_default().to_string();
            let mut styles: Vec<_> = c.styles.styles.values().map(|e| &*e.style).filter(|s| s.kind == crate::format::StyleKind::Numbering).collect();
            styles.sort_by_key(|s| name(s));
            let entries = styles
                .into_iter()
                .filter_map(|s| {
                    let num = crate::docx::para_props::num_id(c.chain_para(Some(&s.id)).list)?;
                    let levels = self.list_levels(num)?;
                    Some(ListEntry { id: ListEntryId(format!("style:{}", s.id)), label: name(s), levels })
                })
                .collect();
            out.push(ListGallerySection { kind: ListGallerySectionKind::ListStyles, entries });
        }
        out
    }

    /// An entry's gallery and levels, and the list it continues (a document list or list style).
    pub(super) fn list_entry(&self, id: &ListEntryId) -> Option<(ListGalleryKind, Vec<ListLevel>, Option<i64>)> {
        let mut parts = id.0.split(':');
        match (parts.next()?, parts.next()?, parts.next()) {
            ("library", k, Some(n)) => {
                let k = kind_of_name(k)?;
                Some((k, library(k).into_iter().nth(n.parse::<usize>().ok()?.checked_sub(1)?)?, None))
            }
            ("recent", n, None) => self.recent_lists.get(n.parse::<usize>().ok()?).map(|(k, l)| (*k, l.clone(), None)),
            ("document", n, None) => {
                let n: i64 = n.parse().ok()?;
                let levels = self.list_levels(n)?;
                Some((gallery_of(&levels, false), levels, Some(n)))
            }
            ("style", s, None) => {
                let num = crate::docx::para_props::num_id(self.doc.resolver.as_ref()?.cascade().chain_para(Some(s)).list)?;
                Some((ListGalleryKind::Multilevel, self.list_levels(num)?, Some(num)))
            }
            _ => None,
        }
    }

    /// Remembers a list's levels as recently used in its gallery.
    pub(super) fn remember_list(&mut self, k: ListGalleryKind, levels: Vec<ListLevel>) {
        self.recent_lists.retain(|(rk, l)| !(*rk == k && *l == levels));
        self.recent_lists.insert(0, (k, levels));
        let mut kept = 0;
        self.recent_lists.retain(|(rk, _)| {
            kept += usize::from(*rk == k);
            *rk != k || kept <= RECENT
        });
    }

    pub(super) fn apply_list_entry(&mut self, id: &ListEntryId, fonts: &Fonts) -> Outcome {
        let Some((k, levels, continues)) = self.list_entry(id) else {
            return Outcome::Refused(crate::api::error::Error::invalid("no such list entry"));
        };
        if let Some(num) = continues {
            let paras = self.selected_paras();
            let assign: Vec<(ParaId, i64)> = paras.iter().map(|&p| (p, num)).collect();
            let fresh = self.selection_list().is_none();
            return self.edit_numbering("Bullets and Numbering", |_, _, _| Some(()), &assign, fresh, fonts);
        }
        let levels_now = self.selection_list().and_then(|(_, num, ilvl)| Some((ilvl, self.list_levels(num)?)));
        let change = match (k, levels_now) {
            // A bullet or a number: the current level takes the entry's first one, keeping where it is.
            (ListGalleryKind::Bullet | ListGalleryKind::Numbering, Some((ilvl, mut now))) => {
                let k_ = ilvl.clamp(0, 8) as usize;
                let first = levels[0].clone();
                let l = &mut now[k_];
                (l.format, l.text, l.font, l.alignment, l.picture) = (first.format, first.text, first.font, first.alignment, first.picture);
                now.truncate(k_ + 1);
                ListDefinitionChange { levels: now, apply_to: ListApplyTo::WholeList, name: None }
            }
            // The list's style links where the entry has none
            (_, now) => {
                let mut levels = levels.clone();
                for (l, was) in levels.iter_mut().zip(now.iter().flat_map(|(_, n)| n)) {
                    if l.linked_style.is_none() {
                        l.linked_style.clone_from(&was.linked_style);
                    }
                }
                ListDefinitionChange { levels, apply_to: ListApplyTo::WholeList, name: None }
            }
        };
        let out = self.set_list_definition(change, fonts);
        if !matches!(out, Outcome::Unchanged(_) | Outcome::Refused(_)) {
            self.remember_list(k, levels);
        }
        out
    }

    /// What an entry's preview shows: three items (three levels for a multilevel list), each a grey line.
    pub fn list_entry_preview(&self, id: &ListEntryId) -> Option<Vec<Block>> {
        let (k, levels, _) = self.list_entry(id)?;
        self.list_preview(&levels, k == ListGalleryKind::Multilevel)
    }

    /// A list's preview: three items at its first level, or at its first three levels.
    pub fn list_preview(&self, levels: &[ListLevel], multilevel: bool) -> Option<Vec<Block>> {
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        let typed = next.typed();
        let (abstract_id, num_id) = typed.cascade.lists.free_ids();
        let (abstract_id, num_id) = (abstract_id.0, num_id.0);
        let colours = typed.cascade.colours;
        let levels = super::list_defs::complete(levels);
        next.set_list_def(super::list_defs::definition(abstract_id, None, super::list_defs::Levels::Shown(&levels), None, &[], &[], &colours));
        next.set_list(super::list_defs::new_list(num_id, abstract_id, &[]));
        let c = next.cascade();
        let mut counters = crate::format::counters::Counters::default();
        let ilvls: [i64; 3] = if multilevel { [0, 1, 2] } else { [0, 0, 0] };
        let mut out = Vec::new();
        for ilvl in ilvls {
            let ppr = crate::format::ParaProps {
                list: Some(crate::format::ListChoice::In(crate::document::paragraph::ListId(num_id))),
                list_level: Some(ilvl),
                ..Default::default()
            };
            let para = crate::docx::para_xml::resolve(&next, &ppr, None, None);
            let list = crate::document::paragraph::ListId(num_id);
            let text = counters.next(&c, list, ilvl)?;
            let level = c.level(list, ilvl)?.clone();
            let under = run_xml::folded(&next, &para.base, None, false);
            let label = crate::docx::para_xml::label(&next, text, &under, &level);
            let base = Rc::new(run_xml::resolve(&next, &para.base, Default::default(), false));
            let grey = Rc::new(RunStyle { color: Some(0x80_8080), ..(*base).clone() });
            out.push(Block::Para(Paragraph {
                style: Rc::new(para.style),
                mark: grey.clone(),
                list: Some(label),
                inlines: vec![Inline::Text("\u{2014}".repeat(10), grey)],
                src: None,
                id: next_para_id(),
                div: None,
                controls: Vec::new(),
            }));
        }
        Some(out)
    }
}
