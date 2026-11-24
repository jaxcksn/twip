//! Page records

use std::rc::Rc;

use crate::docx::NoteKind;

use super::fragment::BoxFragment;
use super::ids::ParaId;
use super::space::{BodyFields, BodySpace, HfHeights};
use super::text::FieldValues;
use super::token::PageToken;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct PageEffects {
    pub paras: Vec<ParaId>,
    /// The bookmarks starting on the page and the paragraphs they start in.
    pub bookmarks: Vec<(String, ParaId)>,
    pub notes_shown: Vec<(NoteKind, i64, String)>,
    /// How many reference fields showing where their bookmark is (`\p`) the page's paragraphs hold.
    pub relative: usize,
    /// The top of the highest footnote area drawn at the bottom of its column.
    pub notes_top: Option<f64>,
    /// The wrapping floats placed on the page, which a continuous section going on on it wraps too.
    pub floats: Vec<super::float::Placed>,
}

impl PageEffects {
    pub fn merge(&mut self, o: &PageEffects) {
        for p in &o.paras {
            if !self.paras.contains(p) {
                self.paras.push(*p);
            }
        }
        for b in &o.bookmarks {
            self.bookmarks.retain(|x| x.0 != b.0);
            self.bookmarks.push(b.clone());
        }
        self.notes_shown.extend(o.notes_shown.iter().cloned());
        self.relative += o.relative;
        self.notes_top = self.notes_top.into_iter().chain(o.notes_top).reduce(f64::min);
    }
}

#[derive(Clone, Debug)]
pub struct BodyRecord {
    pub token: PageToken,
    pub space: BodySpace,
    pub fields: BodyFields,
    pub end: PageToken,
    pub section_end: Option<f64>,
    pub effects: PageEffects,
    pub body: Rc<BoxFragment>,
    pub excluded: Vec<(ParaId, std::ops::Range<super::ids::Unit>)>,
    pub start_table: Option<Rc<super::table::TableLayout>>,
    pub held: super::flow::Held,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HfRecord {
    pub part: String,
    pub fields: FieldValues,
    pub height: f64,
}

#[derive(Clone, Debug)]
pub struct PageRecord {
    pub w: f64,
    pub h: f64,
    pub body_area: Option<(f64, f64)>,
    /// Shared with the layout a page is reused from, as is `effects`.
    pub bodies: std::rc::Rc<Vec<BodyRecord>>,
    pub section: usize,
    pub effects: std::rc::Rc<PageEffects>,
    pub hf: [Option<HfRecord>; 2],
    pub hf_boxes: [Option<BoxFragment>; 2],
    pub hf_full: HfHeights,
    /// Its headers and footers were composed, though it may have none.
    pub composed: bool,
    pub reused: Option<usize>,
    pub number: i64,
    pub blank: bool,
    /// Its section's pages so far that aren't blank, this one included.
    pub in_section: usize,
    /// The page's borders, and the rectangle their outer edges are on (page coordinates).
    pub page_borders: Option<(Rc<crate::docx::PageBorders>, [f64; 4])>,
}
