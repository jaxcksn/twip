//! Placeholders: picture content controls still showing their placeholder, filled by the application
//! after merge (signatures). Activating one reports it to the host; filling puts an image in it.

use crate::docx::Inline;
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Editor;
use super::commands::Outcome;
use super::input::Request;
use super::selection::{ObjectRef, Selection};
use super::step::Step;

/// What the host learns about a placeholder: its key (`Placeholder:<key>` or `MergeImage:<key>`), its label (the
/// control's alias, else the key), its box in points, and whether it is a merge image.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaceholderInfo {
    pub key: String,
    pub label: String,
    pub width: f64,
    pub height: f64,
    pub merge: bool,
}

impl PlaceholderInfo {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "key": self.key, "label": self.label, "width": self.width, "height": self.height, "merge": self.merge })
    }
}

impl Editor {
    /// The unfilled placeholder an inline picture shows, if it shows one.
    pub fn placeholder_of(&self, image: Option<usize>, w: f64, h: f64) -> Option<PlaceholderInfo> {
        let i = image?;
        if self.doc.filled.contains(&i) {
            return None;
        }
        let (merge, label) = self.doc.placeholders.get(&i)?;
        let key = self.doc.placeholder_keys.get(&i).cloned().unwrap_or_default();
        Some(PlaceholderInfo { key, label: label.clone(), width: w, height: h, merge: *merge })
    }

    /// The selected unfilled placeholder.
    pub fn selected_placeholder(&self) -> Option<PlaceholderInfo> {
        let Selection::Object(ObjectRef::Inline { para, offset }) = self.selection else { return None };
        match self.index.para(&self.doc, para).and_then(|p| super::stories::inline_at(p, offset))? {
            Inline::Picture(p, _) => self.placeholder_of(p.image, p.w, p.h),
            _ => None,
        }
    }

    /// The unfilled placeholders drawn on `pages` with a look the host set: theirs, else the default.
    pub(super) fn placeholder_views(&mut self, pages: &std::ops::Range<usize>) -> Vec<super::view::PlaceholderView> {
        if self.placeholder_looks.is_empty() || self.doc.placeholders.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for page in pages.clone() {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for l in &geo.lines {
                for w in l.stops.windows(2).filter(|w| w[1].para == w[0].para && w[1].unit.0 == w[0].unit.0 + 1) {
                    let Some(p) = self.index.para(&self.doc, w[0].para) else { continue };
                    let Some(Inline::Picture(pic, _)) = super::stories::inline_at(p, w[0].unit) else { continue };
                    let Some(info) = self.placeholder_of(pic.image, pic.w, pic.h) else { continue };
                    let Some(look) = self.placeholder_looks.get(&info.key).or_else(|| self.placeholder_looks.get("")) else { continue };
                    let Some(rect) = self.inline_box(&geo, w[0].para, w[0].unit) else { continue };
                    out.push(super::view::PlaceholderView {
                        page,
                        rect,
                        color: look.color.unwrap_or(0x40_00_78_D4),
                        label: look.label.clone().unwrap_or(info.label.clone()),
                        icon: look.icon.clone(),
                    });
                }
            }
        }
        out
    }

    /// Activation of the selected placeholder: reported to the host. True when there was one.
    pub(super) fn activate_placeholder(&mut self) -> bool {
        // Design mode: placeholders don't ask to be filled; activation goes to the host instead.
        if self.mode == super::Mode::Design {
            return false;
        }
        match self.selected_placeholder() {
            Some(info) => {
                self.requests.push(Request::Placeholder(info));
                true
            }
            None => false,
        }
    }

    /// Fill Placeholder: the selected placeholder (`key` None) or every unfilled one with `key` takes the
    /// image, fitted into its box keeping the image's aspect ratio. One step; `Unchanged` when nothing matches or the
    /// data isn't an image.
    pub(super) fn fill_placeholders(&mut self, key: Option<&str>, data: Vec<u8>, fonts: &Fonts) -> Outcome {
        let Some((pw, ph, dx, dy)) = crate::docx::image::image_info(&data).filter(|_| crate::docx::image::image_type(&data).is_some()) else {
            return Outcome::Unchanged("not a picture");
        };
        let (iw, ih) = (pw * 72.0 / if dx > 1.0 { dx } else { 96.0 }, ph * 72.0 / if dy > 1.0 { dy } else { 96.0 });
        let images: Vec<usize> = match key {
            None => {
                let Selection::Object(ObjectRef::Inline { para, offset }) = self.selection else {
                    return Outcome::Unchanged("no placeholder selected");
                };
                match self.index.para(&self.doc, para).and_then(|p| super::stories::inline_at(p, offset)) {
                    Some(Inline::Picture(p, _)) if self.placeholder_of(p.image, p.w, p.h).is_some() => p.image.into_iter().collect(),
                    _ => return Outcome::Unchanged("no placeholder selected"),
                }
            }
            Some(k) => {
                let mut v: Vec<usize> =
                    self.doc.placeholder_keys.iter().filter(|(i, kk)| kk.as_str() == k && !self.doc.filled.contains(i)).map(|(i, _)| *i).collect();
                v.sort_unstable();
                v
            }
        };
        if images.is_empty() {
            return Outcome::Unchanged("no such placeholder");
        }
        self.fill_images(&images, data, (iw, ih), "Fill Placeholder", fonts)
    }

    /// Pictures with image indexes `images` take `data` (its size in points `size`), fitted into their boxes keeping
    /// the image's aspect ratio. One step.
    pub(super) fn fill_images(&mut self, images: &[usize], data: Vec<u8>, size: (f64, f64), label: &'static str, fonts: &Fonts) -> Outcome {
        let (iw, ih) = size;
        // Every paragraph showing one of them, with its pictures resized to fit their boxes.
        let mut restyles: Vec<(ParaId, Vec<Inline>)> = Vec::new();
        for para in self.index.para_ids().collect::<Vec<_>>() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if !p.inlines.iter().any(|i| matches!(i, Inline::Picture(pic, _) if pic.image.is_some_and(|k| images.contains(&k)))) {
                continue;
            }
            let mut inlines = p.inlines.clone();
            for i in inlines.iter_mut() {
                if let Inline::Picture(pic, _) = i
                    && pic.image.is_some_and(|k| images.contains(&k))
                    && iw > 0.0
                    && ih > 0.0
                {
                    let s = (pic.w / iw).min(pic.h / ih);
                    (pic.w, pic.h) = (((iw * s) * 20.0).round() / 20.0, ((ih * s) * 20.0).round() / 20.0);
                }
            }
            restyles.push((para, inlines));
        }
        let mut tx = self.begin(label);
        for &index in images {
            if self.step(&mut tx, Step::ImageData { index, data: data.clone(), filled: true }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("fill failed");
            }
        }
        for (para, inlines) in restyles {
            if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("fill failed");
            }
        }
        self.commit(tx, None, fonts)
    }
}
