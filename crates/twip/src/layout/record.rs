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
    pub bookmarks: Vec<String>,
    pub notes_shown: Vec<(NoteKind, i64, String)>,
}

impl PageEffects {
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
    pub bodies: Vec<BodyRecord>,
    pub section: usize,
    pub effects: PageEffects,
    pub hf: [Option<HfRecord>; 2],
    pub hf_boxes: [Option<BoxFragment>; 2],
    pub hf_full: HfHeights,
    pub reused: Option<usize>,
    pub number: i64,
    pub blank: bool,
    /// The page's borders, and the rectangle their outer edges are on (page coordinates).
    pub page_borders: Option<(crate::docx::PageBorders, [f64; 4])>,
}
