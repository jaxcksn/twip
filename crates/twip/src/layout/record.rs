//! What the pass keeps of each committed page: the inputs its bodies were laid out with, where they ended,
//! their fragments and their effects. Snapshots are built from records; page reuse compares them.

use std::rc::Rc;

use crate::docx::NoteKind;

use super::fragment::BoxFragment;
use super::ids::ParaId;
use super::space::{BodyFields, BodySpace, HfHeights};
use super::text::FieldValues;
use super::token::PageToken;

/// What a page body found besides its fragment: the effects the pass applies when it commits the page.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct PageEffects {
    /// Paragraphs placed on the page, in placement order (STYLEREF in headers).
    pub paras: Vec<ParaId>,
    /// Bookmarks whose paragraph starts on the page (PAGEREF).
    pub bookmarks: Vec<String>,
    /// Notes (or note parts) drawn on the page, with their numbers (NOTE-4).
    pub notes_shown: Vec<(NoteKind, i64, String)>,
}

impl PageEffects {
    /// Adds a later body's effects on the same page: a paragraph counts once, and a bookmark belongs to the last
    /// paragraph placed with it.
    pub fn merge(&mut self, o: &PageEffects) {
        for p in &o.paras {
            if !self.paras.contains(p) {
                self.paras.push(*p);
            }
        }
        for b in &o.bookmarks {
            self.bookmarks.retain(|x| x != b);
            self.bookmarks.push(b.clone());
        }
        self.notes_shown.extend(o.notes_shown.iter().cloned());
    }
}

/// One section's body on a page.
#[derive(Clone, Debug)]
pub struct BodyRecord {
    /// Where the body started.
    pub token: PageToken,
    pub space: BodySpace,
    pub fields: BodyFields,
    /// Where the flow is after the body: the next page's start, or past the section's last block.
    pub end: PageToken,
    /// When the section ended on this page: where the next section would start.
    pub section_end: Option<f64>,
    pub effects: PageEffects,
    pub body: Rc<BoxFragment>,
    /// The body's text pushed entirely below the page (HDR-9), by paragraph.
    pub excluded: Vec<(ParaId, std::ops::Range<super::ids::Unit>)>,
    /// The table the body starts in, when it starts at a row of one: page reuse compares it too.
    pub start_table: Option<Rc<super::table::TableLayout>>,
}

/// A header or footer a page shows: the story part, the field values it was composed with, and its
/// composed height (HDR-8 caps it; clipped to the page's reservation in a stabilizing pass).
#[derive(Clone, Debug, PartialEq)]
pub struct HfRecord {
    pub part: String,
    pub fields: FieldValues,
    pub height: f64,
}

/// A committed page. A page holds one body per section on it (continuous section breaks put several on one page);
/// an inserted blank page (HDR-12) holds none.
#[derive(Clone, Debug)]
pub struct PageRecord {
    pub w: f64,
    pub h: f64,
    /// The first section's body top and bottom on the page (HDR-4 to HDR-6).
    pub body_area: Option<(f64, f64)>,
    pub bodies: Vec<BodyRecord>,
    /// The section the page starts in.
    pub section: usize,
    /// The bodies' effects together.
    pub effects: PageEffects,
    /// Header and footer, set when the pass composes them: their records, their boxes, and their heights before any
    /// clipping (what convergence compares).
    pub hf: [Option<HfRecord>; 2],
    pub hf_boxes: [Option<BoxFragment>; 2],
    pub hf_full: HfHeights,
    /// The baseline page this one reuses, if any.
    pub reused: Option<usize>,
    /// Numbering: not reuse inputs, recomputed when a page moves.
    pub number: i64,
    pub blank: bool,
}
