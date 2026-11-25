//! References: tables of contents, captions and cross-references

use crate::api::command::{
    AutoCaption, CaptionChapter, CaptionPosition, ChapterSeparator, CrossReferenceContent, CrossReferenceKind, InsertCaption, InsertCrossReference,
    InsertTableOfContents, InsertTableOfFigures, MarkTableOfContentsEntry, NumberFormat, TableOfContentsOptions, TocCaptions, TocChapterNumbers,
    TocEntryFields, TocLayout, TocLevels, TocPageNumbers, TocStyleLevel, TocUpdateMode,
};
use crate::api::value::{StyleId, TabLeader};
use crate::docx::props::Leader;
use crate::font::Fonts;
use std::cell::OnceCell;
use std::collections::HashMap;

use super::Outcome;
use crate::editing::Editor;
use crate::editing::captions::{self, CaptionOptions};
use crate::editing::toc::{Caption, TocField, TocFrame, TocLook, TocMark, TocOptions, TocUpdate};
use crate::editing::xref::{CrossReference, RefContent, RefKind, RefTarget};

/// Why a caption label is refused.
const BAD_LABEL: &str = "a label is a letter, then letters, digits, spaces and underscores";

/// A level range's ends, checked: 1 to 9, the first no more than the last.
fn levels(l: TocLevels) -> Result<(u8, u8), Outcome> {
    if (1..=9).contains(&l.from) && (l.from..=9).contains(&l.to) {
        Ok((l.from, l.to))
    } else {
        Err(Outcome::invalid("the levels are 1…9, from no more than to"))
    }
}

fn level(l: u8) -> Result<u8, Outcome> {
    if (1..=9).contains(&l) { Ok(l) } else { Err(Outcome::invalid("the level is 1…9")) }
}

fn api_levels((from, to): (u8, u8)) -> TocLevels {
    TocLevels { from, to }
}

/// What updating a table rewrites, as the engine names it.
pub fn toc_update(m: TocUpdateMode) -> TocUpdate {
    match m {
        TocUpdateMode::PageNumbers => TocUpdate::Pages,
        TocUpdateMode::EntireTable => TocUpdate::Entire,
    }
}

/// A cross-reference kind as the engine names it.
pub fn ref_kind(k: &CrossReferenceKind) -> RefKind {
    match k {
        CrossReferenceKind::NumberedItem => RefKind::NumberedItem,
        CrossReferenceKind::Heading => RefKind::Heading,
        CrossReferenceKind::Bookmark => RefKind::Bookmark,
        CrossReferenceKind::Footnote => RefKind::Footnote,
        CrossReferenceKind::Endnote => RefKind::Endnote,
        CrossReferenceKind::Caption(label) => RefKind::Caption(captions::sequence_of(label)),
    }
}

fn ref_content(c: CrossReferenceContent) -> RefContent {
    use CrossReferenceContent as C;
    match c {
        C::Text => RefContent::Text,
        C::PageNumber => RefContent::PageNumber,
        C::Number => RefContent::Number,
        C::NumberNoContext => RefContent::NumberNoContext,
        C::NumberFullContext => RefContent::NumberFullContext,
        C::AboveBelow => RefContent::AboveBelow,
        C::NoteNumber => RefContent::NoteNumber,
        C::NoteNumberFormatted => RefContent::NoteNumberFormatted,
        C::LabelAndNumber => RefContent::LabelAndNumber,
        C::CaptionText => RefContent::CaptionText,
    }
}

/// A target as the API lists it.
pub fn ref_target(t: &RefTarget) -> crate::api::services::CrossReferenceTarget {
    crate::api::services::CrossReferenceTarget {
        text: t.text.clone(),
        number: t.number.clone(),
        level: t.level,
        bookmark: t.bookmark.clone(),
        note: t.note,
    }
}

/// A caption's options as the engine takes them.
fn caption_options(
    label: &str,
    text: Option<&str>,
    position: Option<CaptionPosition>,
    exclude: Option<bool>,
    format: Option<&NumberFormat>,
    chapter: Option<CaptionChapter>,
) -> Result<CaptionOptions, Outcome> {
    use captions::CaptionFormat as F;
    let format = match format {
        None | Some(NumberFormat::Decimal) => F::Arabic,
        Some(NumberFormat::LowerLetter) => F::LowerLetter,
        Some(NumberFormat::UpperLetter) => F::UpperLetter,
        Some(NumberFormat::LowerRoman) => F::LowerRoman,
        Some(NumberFormat::UpperRoman) => F::UpperRoman,
        Some(_) => return Err(Outcome::invalid("a caption numbers in decimal, letters or roman numerals")),
    };
    let chapter = chapter
        .map(|c| {
            let separator = match c.separator {
                ChapterSeparator::Hyphen => captions::ChapterSeparator::Hyphen,
                ChapterSeparator::Period => captions::ChapterSeparator::Period,
                ChapterSeparator::Colon => captions::ChapterSeparator::Colon,
                ChapterSeparator::EmDash => captions::ChapterSeparator::EmDash,
                ChapterSeparator::EnDash => captions::ChapterSeparator::EnDash,
            };
            level(c.level).map_err(|_| Outcome::invalid("the chapter level is 1…9")).map(|level| captions::ChapterNumber { level, separator })
        })
        .transpose()?;
    Ok(CaptionOptions {
        label: label.to_string(),
        text: text.unwrap_or_default().to_string(),
        position: position.map(|p| match p {
            CaptionPosition::Above => captions::CaptionPosition::Above,
            CaptionPosition::Below => captions::CaptionPosition::Below,
        }),
        exclude_label: exclude.unwrap_or(false),
        format,
        chapter,
    })
}

/// The AutoCaption setting as the API reads it back.
pub fn auto_caption_api(c: &CaptionOptions) -> AutoCaption {
    use captions::CaptionFormat as F;
    AutoCaption {
        label: c.label.clone(),
        position: c.position.map(|p| match p {
            captions::CaptionPosition::Above => CaptionPosition::Above,
            captions::CaptionPosition::Below => CaptionPosition::Below,
        }),
        exclude_label: c.exclude_label.then_some(true),
        format: match c.format {
            F::Arabic => None,
            F::LowerLetter => Some(NumberFormat::LowerLetter),
            F::UpperLetter => Some(NumberFormat::UpperLetter),
            F::LowerRoman => Some(NumberFormat::LowerRoman),
            F::UpperRoman => Some(NumberFormat::UpperRoman),
        },
        chapter: c.chapter.map(|ch| CaptionChapter {
            level: ch.level,
            separator: match ch.separator {
                captions::ChapterSeparator::Hyphen => ChapterSeparator::Hyphen,
                captions::ChapterSeparator::Period => ChapterSeparator::Period,
                captions::ChapterSeparator::Colon => ChapterSeparator::Colon,
                captions::ChapterSeparator::EmDash => ChapterSeparator::EmDash,
                captions::ChapterSeparator::EnDash => ChapterSeparator::EnDash,
            },
        }),
    }
}

fn look(l: Option<&TocLayout>) -> TocLook {
    let d = TocLook::default();
    let Some(l) = l else { return d };
    TocLook {
        right_align: l.right_align_page_numbers.unwrap_or(d.right_align),
        leader: match l.tab_leader {
            None => d.leader,
            Some(TabLeader::None) => Leader::None,
            Some(TabLeader::Dots) => Leader::Dot,
            Some(TabLeader::Dashes) => Leader::Hyphen,
            Some(TabLeader::Underline) => Leader::Underscore,
            Some(TabLeader::Heavy) => Leader::Heavy,
            Some(TabLeader::MiddleDot) => Leader::MiddleDot,
        },
    }
}

fn frame(f: Option<&crate::api::command::TocFrame>) -> TocFrame {
    f.map_or_else(TocFrame::default, |f| TocFrame { title: f.title.clone(), control: f.building_block.unwrap_or(false) })
}

impl Editor {
    /// The name the document (or Word's built-ins) gives style `id`, as a table of contents' code names it.
    fn style_name(&self, id: &StyleId) -> Option<String> {
        if let Some(s) = self.doc.resolver.as_ref().and_then(|r| r.styles.by_id.get(&id.0)) {
            return Some(s.name.clone());
        }
        crate::services::styles::builtin::BUILT_IN.iter().find(|b| b.id == id.0).map(|b| b.name.to_string())
    }

    /// The document's style ids by name and by alias, ASCII lower-cased; a name wins over another style's alias, and
    /// of styles sharing a name, the first by id.
    pub fn style_ids_by_name(&self) -> HashMap<String, StyleId> {
        let mut ids = HashMap::new();
        let Some(r) = self.doc.resolver.as_ref() else { return ids };
        let mut styles: Vec<&crate::docx::styles::Style> = r.styles.by_id.values().collect();
        styles.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        for s in &styles {
            ids.entry(s.name.to_ascii_lowercase()).or_insert_with(|| StyleId(s.id.clone()));
        }
        for s in &styles {
            for a in &s.aliases {
                ids.entry(a.to_ascii_lowercase()).or_insert_with(|| StyleId(s.id.clone()));
            }
        }
        ids
    }

    /// The engine's options of an API table of contents' options: `None` fields off; invalid levels and unknown
    /// styles refused.
    pub fn toc_options(&self, o: &TableOfContentsOptions) -> Result<TocOptions, Outcome> {
        let styles = o
            .style_levels
            .iter()
            .map(|s: &TocStyleLevel| {
                let name = self.style_name(&s.style).ok_or_else(|| Outcome::invalid("no such style"))?;
                Ok((name, level(s.level)?))
            })
            .collect::<Result<Vec<_>, Outcome>>()?;
        if o.captions.as_ref().is_some_and(|c| !captions::valid_label(&c.label)) {
            return Err(Outcome::invalid(BAD_LABEL));
        }
        let on = |v: Option<bool>| v.unwrap_or(false);
        let (entry_fields, entry_levels) = match &o.entry_fields {
            Some(f) => (Some(f.identifier.clone().unwrap_or_else(|| "C".into())), f.levels.map(levels).transpose()?),
            None => (None, None),
        };
        let omit_pages = match o.page_numbers {
            None | Some(TocPageNumbers::All) => None,
            Some(TocPageNumbers::None) => Some((1, 9)),
            Some(TocPageNumbers::OmitLevels(l)) => Some(levels(l)?),
        };
        Ok(TocOptions {
            headings: o.heading_levels.map(levels).transpose()?,
            outline_levels: on(o.outline_levels),
            styles,
            entry_fields,
            entry_levels,
            captions: o.captions.as_ref().map(|c| Caption { label: captions::sequence_of(&c.label), include_label: c.include_label_and_number }),
            bookmark: o.bookmark.clone(),
            hyperlinks: on(o.hyperlinks),
            omit_pages,
            separator: o.separator.clone(),
            sequence: o.chapter_numbers.as_ref().map(|c| (c.sequence.clone(), c.separator.clone())),
            hide_in_web: on(o.hide_in_web_view),
            keep_tabs: on(o.keep_tabs),
            keep_breaks: on(o.keep_line_breaks),
            other: Vec::new(),
        })
    }

    /// A table's options as the API reads them back: `\t`'s style names as the document's style ids (a name it
    /// doesn't define left out). `names` caches [`Editor::style_ids_by_name`] across tables, built on first need.
    pub fn toc_options_api(&self, o: &TocOptions, names: &OnceCell<HashMap<String, StyleId>>) -> TableOfContentsOptions {
        let flag = |b: bool| b.then_some(true);
        let style_levels = if o.styles.is_empty() {
            Vec::new()
        } else {
            let names = names.get_or_init(|| self.style_ids_by_name());
            o.styles
                .iter()
                .filter_map(|(name, level)| Some(TocStyleLevel { style: names.get(&name.to_ascii_lowercase())?.clone(), level: *level }))
                .collect()
        };
        TableOfContentsOptions {
            heading_levels: o.headings.map(api_levels),
            outline_levels: flag(o.outline_levels),
            style_levels,
            entry_fields: o
                .entry_fields
                .as_ref()
                .map(|id| TocEntryFields { identifier: (!id.eq_ignore_ascii_case("C")).then(|| id.clone()), levels: o.entry_levels.map(api_levels) }),
            captions: o.captions.as_ref().map(|c| TocCaptions { label: c.label.replace('_', " "), include_label_and_number: c.include_label }),
            bookmark: o.bookmark.clone(),
            hyperlinks: flag(o.hyperlinks),
            page_numbers: o.omit_pages.map(|l| if l == (1, 9) { TocPageNumbers::None } else { TocPageNumbers::OmitLevels(api_levels(l)) }),
            separator: o.separator.clone(),
            chapter_numbers: o
                .sequence
                .as_ref()
                .map(|(sequence, separator)| TocChapterNumbers { sequence: sequence.clone(), separator: separator.clone() }),
            hide_in_web_view: flag(o.hide_in_web),
            keep_tabs: flag(o.keep_tabs),
            keep_line_breaks: flag(o.keep_breaks),
        }
    }

    /// The document's list separator, which a table's `\t` style list uses.
    pub fn list_separator(&self) -> char {
        self.doc.settings.list_separator.unwrap_or(',')
    }

    /// `insertTableOfContents`: `{}` (or no options) is Word's default table.
    pub(super) fn insert_toc_api(&mut self, c: &InsertTableOfContents, fonts: &Fonts) -> Outcome {
        let o = match &c.options {
            None => TocOptions::word_default(),
            Some(o) if *o == TableOfContentsOptions::default() => TocOptions::word_default(),
            Some(o) => match self.toc_options(o) {
                Ok(o) => o,
                Err(out) => return out,
            },
        };
        self.insert_table_of_contents(&o, look(c.layout.as_ref()), &frame(c.frame.as_ref()), fonts)
    }

    /// `insertTableOfFigures`: the captions of its label, with its other options (Word's when none are given).
    pub(super) fn insert_tof_api(&mut self, c: &InsertTableOfFigures, fonts: &Fonts) -> Outcome {
        if !captions::valid_label(&c.label) {
            return Outcome::invalid(BAD_LABEL);
        }
        let mut o = match &c.options {
            None => TocOptions::figures(&c.label),
            Some(o) => match self.toc_options(o) {
                Ok(o) => o,
                Err(out) => return out,
            },
        };
        o.captions = Some(Caption { label: captions::sequence_of(&c.label), include_label: c.include_label_and_number.unwrap_or(true) });
        self.insert_table_of_contents(&o, look(c.layout.as_ref()), &frame(c.frame.as_ref()), fonts)
    }

    /// The table `index` names among [`Editor::tables_of_contents`], else the one at the selection (or the first).
    pub fn toc_by_index(&self, index: Option<u32>) -> Result<TocField, Outcome> {
        let t = match index {
            Some(k) => self.tables_of_contents().into_iter().nth(k as usize),
            None => self.toc_here().map(|(_, t)| t),
        };
        t.ok_or_else(|| Outcome::unavailable("no table of contents"))
    }

    /// `markTableOfContentsEntry`.
    pub(super) fn mark_toc_entry_api(&mut self, m: &MarkTableOfContentsEntry, fonts: &Fonts) -> Outcome {
        let mark = TocMark { text: m.text.clone(), level: m.level, identifier: m.identifier.clone(), page_number: m.page_number.unwrap_or(true) };
        self.mark_toc_entry(&mark, fonts)
    }

    /// `insertCaption`.
    pub(super) fn insert_caption_api(&mut self, c: &InsertCaption, fonts: &Fonts) -> Outcome {
        match caption_options(&c.label, c.text.as_deref(), c.position, c.exclude_label, c.format.as_ref(), c.chapter) {
            Ok(o) => self.insert_caption(&o, fonts),
            Err(out) => out,
        }
    }

    /// `setAutoCaption`: a session setting, the label checked as a caption's.
    pub fn set_auto_caption_api(&mut self, c: Option<&AutoCaption>) -> Outcome {
        let Some(c) = c else {
            self.set_auto_caption(None);
            return Outcome::Moved;
        };
        if !captions::valid_label(&c.label) {
            return Outcome::invalid(BAD_LABEL);
        }
        match caption_options(&c.label, None, c.position, c.exclude_label, c.format.as_ref(), c.chapter) {
            Ok(o) => {
                self.set_auto_caption(Some(o));
                Outcome::Moved
            }
            Err(out) => out,
        }
    }

    /// `insertCrossReference`: the target listed now at its index.
    pub(super) fn insert_cross_reference_api(&mut self, c: &InsertCrossReference, fonts: &Fonts) -> Outcome {
        let targets = self.cross_reference_targets(&ref_kind(&c.target.kind));
        let Some(target) = targets.get(c.target.index as usize) else { return Outcome::unavailable("no such target") };
        let r =
            CrossReference { content: ref_content(c.content), hyperlink: c.hyperlink.unwrap_or(true), above_below: c.above_below.unwrap_or(false) };
        self.insert_cross_reference(target, &r, fonts)
    }
}
