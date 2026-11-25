//! Picture tools: crop and change picture

use crate::docx::Inline;
use crate::drawing::ShapeKind;
use crate::drawing::units::Pct;
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::selection::{ObjectRef, Selection};
use super::step::Step;

/// The selected picture's parts a picture tool changes.
pub(super) struct PicEdit<'a> {
    pub w: &'a mut f64,
    pub h: &'a mut f64,
    pub image: &'a mut Option<usize>,
    /// Its effect extents (`wp:effectExtent`).
    pub effect: &'a mut [f64; 4],
    pub shape: Option<&'a mut crate::drawing::Shape>,
    /// A float's width and height relative to a box.
    pub relative: Option<&'a mut [Option<(String, i64)>; 2]>,
}

impl Editor {
    /// The selected picture's DrawingML picture.
    fn selected_pic(&self) -> Option<&crate::drawing::PicShape> {
        match self.selection {
            Selection::Object(ObjectRef::Inline { para, offset }) => match super::stories::inline_at(self.index.para(&self.doc, para)?, offset)? {
                Inline::Picture(p, _) => p.shape.as_deref()?.pic(),
                _ => None,
            },
            Selection::Object(ObjectRef::Floating(f)) => self.float_of(f)?.shape.as_deref()?.pic(),
            _ => None,
        }
    }

    pub(super) fn selected_picture_crop(&self) -> Option<crate::api::command::PictureCrop> {
        let s = self.selected_pic()?.blip.src.map(Pct::fraction);
        s.iter().any(|v| *v != 0.0).then_some(crate::api::command::PictureCrop {
            left: Some(s[0]),
            top: Some(s[1]),
            right: Some(s[2]),
            bottom: Some(s[3]),
        })
    }

    pub(super) fn selected_transparent_color(&self) -> Option<crate::api::value::Color> {
        self.selected_pic()?.blip.adjust.iter().find_map(|a| match a {
            crate::drawing::paint::BlipAdjust::ClrChange { from, .. } => match from.base {
                crate::drawing::colour::ColourBase::Srgb(v) => Some(crate::api::value::Color(v)),
                _ => None,
            },
            _ => None,
        })
    }

    /// The selected float's width (`k` 0) or height (1) as a share of a box on the page.
    pub(super) fn selected_relative_size(&self, k: usize) -> Option<crate::api::command::RelativeSize> {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return None };
        let (from, t) = self.float_of(f)?.relative.size[k].as_ref()?;
        Some(crate::api::command::RelativeSize {
            percent: *t as f64 / 10.0,
            relative_to: serde_json::from_value(serde_json::Value::String(from.clone())).unwrap_or_default(),
        })
    }

    /// Applies `edit` to the selected inline object (a picture or a shape); `false` from it changes nothing.
    pub(super) fn edit_inline(&mut self, label: &'static str, edit: impl FnOnce(&mut crate::docx::Picture) -> bool, fonts: &Fonts) -> Outcome {
        let Selection::Object(ObjectRef::Inline { para, offset }) = self.selection else { return Outcome::unavailable("no inline object selected") };
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let mut inlines = p.inlines.clone();
        let mut at = 0;
        let Some(pic) = inlines.iter_mut().find_map(|i| {
            let here = at;
            at += i.units();
            match i {
                Inline::Picture(pic, _) if here == offset.0 as usize => Some(pic),
                _ => None,
            }
        }) else {
            return Outcome::Unchanged("no picture");
        };
        let before = pic.clone();
        if !edit(pic) || *pic == before {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin(label);
        if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale object");
        }
        self.selection = Selection::Object(ObjectRef::Inline { para, offset });
        self.commit(tx, None, fonts)
    }

    /// Applies `edit` to the selected picture, inline or floating; `false` from it changes nothing.
    pub(super) fn edit_picture(&mut self, label: &'static str, edit: impl FnOnce(&mut PicEdit) -> bool, fonts: &Fonts) -> Outcome {
        match self.selection {
            Selection::Object(ObjectRef::Inline { .. }) => self.edit_inline(
                label,
                |pic| {
                    let (was, effect) = ((pic.w, pic.h, pic.shape.as_deref().map_or(0.0, |s| s.xfrm.rot.degrees())), pic.effect);
                    if !edit(&mut PicEdit {
                        w: &mut pic.w,
                        h: &mut pic.h,
                        image: &mut pic.image,
                        effect: &mut pic.effect,
                        shape: pic.shape.as_deref_mut(),
                        relative: None,
                    }) {
                        return false;
                    }
                    let deg = pic.shape.as_deref().map_or(0.0, |s| s.xfrm.rot.degrees());
                    if pic.effect != effect {
                        // The edit set the effects' own reach: the turn's goes on it.
                        super::shapes::refit_turn(pic, (pic.w, pic.h, 0.0));
                    } else if (was.2 != 0.0 || deg != 0.0) && (was.0, was.1, was.2) != (pic.w, pic.h, deg) {
                        super::shapes::refit_turn(pic, was);
                    }
                    true
                },
                fonts,
            ),
            Selection::Object(ObjectRef::Floating(f)) => {
                let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
                if n.text_box.is_some() {
                    return Outcome::unavailable("not a picture");
                }
                // A quarter-turned float's box is the picture's own width and height swapped.
                let (w, h) = if n.rot_swap { (&mut n.h, &mut n.w) } else { (&mut n.w, &mut n.h) };
                let rot = n.shape.as_deref().map(|s| s.xfrm.rot);
                let mut e =
                    PicEdit { w, h, image: &mut n.image, effect: &mut n.effect, shape: n.shape.as_deref_mut(), relative: Some(&mut n.relative.size) };
                if !edit(&mut e) {
                    return Outcome::Unchanged("already so");
                }
                // A changed turn swaps the box back when it leaves or enters a quarter turn.
                if let Some(now) = n.shape.as_deref().map(|s| s.xfrm.rot)
                    && Some(now) != rot
                {
                    super::shapes::rotate_to(&mut n, &[], now.degrees(), self.doc.settings.policy.mode);
                }
                self.set_float(n, label, fonts)
            }
            _ => Outcome::unavailable("no picture selected"),
        }
    }

    /// Crops the selected picture by fractions of its image from each edge: the box grows or shrinks with the part of
    /// the image shown, at the scale it is drawn.
    pub(in crate::editing) fn set_picture_crop(&mut self, c: crate::api::command::PictureCrop, fonts: &Fonts) -> Outcome {
        self.edit_picture(
            "Crop",
            |e| {
                let Some(ShapeKind::Pic(p)) = e.shape.as_mut().map(|s| &mut s.kind) else { return false };
                let was = p.blip.src.map(Pct::fraction);
                let mut now = was;
                for (k, v) in [c.left, c.top, c.right, c.bottom].into_iter().enumerate() {
                    if let Some(v) = v {
                        now[k] = v;
                    }
                }
                // The shown share of the image across and down.
                let shown = |s: [f64; 4]| ((1.0 - s[0] - s[2]).max(0.001), (1.0 - s[1] - s[3]).max(0.001));
                if now == was || shown(now).0 <= 0.001 || shown(now).1 <= 0.001 {
                    return false;
                }
                let ((wa, ha), (wb, hb)) = (shown(was), shown(now));
                p.blip.src = now.map(|v| Pct((v * 100_000.0).round() as i32));
                *e.w = ((*e.w * wb / wa).max(1.0) * 20.0).round() / 20.0;
                *e.h = ((*e.h * hb / ha).max(1.0) * 20.0).round() / 20.0;
                true
            },
            fonts,
        )
    }

    /// Replaces the selected picture's image, keeping its width, effects and place: its height follows the new image's
    /// proportions and the crop is cleared.
    pub(in crate::editing) fn change_picture(&mut self, data: Vec<u8>, fonts: &Fonts) -> Outcome {
        let Some((pw, ph, ..)) = crate::docx::image::image_info(&data) else { return Outcome::invalid("not a picture") };
        let k = self.pending_image(data);
        let out = self.edit_picture(
            "Change Picture",
            |e| {
                *e.image = Some(k);
                if let Some(ShapeKind::Pic(p)) = e.shape.as_mut().map(|s| &mut s.kind) {
                    p.blip.image = Some(k);
                    p.blip.src = [Pct(0); 4];
                }
                if pw > 0.0 {
                    *e.h = ((*e.w * ph / pw).max(1.0) * 20.0).round() / 20.0;
                }
                true
            },
            fonts,
        );
        // Refused before a transaction began: the image isn't left for the next one.
        if !matches!(out, Outcome::Changed(_)) {
            self.pending_images.clear();
        }
        out
    }
}

/// A picture's recolouring (Word's Picture Color > Recolor).
#[derive(Clone, Debug, PartialEq)]
pub enum Recolor {
    /// Its own colours.
    None,
    /// Shades of grey.
    Grayscale,
    /// Black and white: pixels lighter than the threshold (0 to 1) white, the rest black.
    BlackAndWhite(f64),
    /// Two colours, its darkest drawn in the first and its lightest in the second (Sepia, the accent variations).
    Duotone(crate::drawing::colour::Colour, crate::drawing::colour::Colour),
}

impl Recolor {
    /// The recolouring a picture's adjustments make.
    pub fn of(adjust: &[crate::drawing::paint::BlipAdjust]) -> Recolor {
        use crate::drawing::paint::BlipAdjust as A;
        adjust
            .iter()
            .find_map(|a| match a {
                A::Grayscale => Some(Recolor::Grayscale),
                A::BiLevel(t) => Some(Recolor::BlackAndWhite(t.fraction())),
                A::Duotone([a, b]) => Some(Recolor::Duotone(a.clone(), b.clone())),
                _ => None,
            })
            .unwrap_or(Recolor::None)
    }
}

/// A picture's adjustments (Picture Corrections, Color and Transparency), read from its adjustments.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureAdjust {
    /// Lighter (positive, to 1) or darker (to −1).
    pub brightness: f64,
    /// More (positive, to 1) or less contrast (to −1).
    pub contrast: f64,
    /// How see-through it is (0 opaque, 1 invisible).
    pub transparency: f64,
    /// Its recolouring.
    pub recolor: Recolor,
}

impl PictureAdjust {
    /// A picture's adjustments as `adjust` holds them.
    pub fn of(adjust: &[crate::drawing::paint::BlipAdjust]) -> PictureAdjust {
        use crate::drawing::paint::BlipAdjust as A;
        let (brightness, contrast) = adjust
            .iter()
            .find_map(|a| if let A::Lum { bright, contrast } = a { Some((bright.fraction(), contrast.fraction())) } else { None })
            .unwrap_or((0.0, 0.0));
        let transparency = adjust.iter().find_map(|a| if let A::AlphaModFix(p) = a { Some(1.0 - p.fraction()) } else { None }).unwrap_or(0.0);
        PictureAdjust { brightness, contrast, transparency, recolor: Recolor::of(adjust) }
    }
}

impl Editor {
    /// The selected picture's adjustments (a group child's when one is selected).
    pub fn picture_adjust(&self) -> Option<PictureAdjust> {
        Some(PictureAdjust::of(&self.object_shape()?.pic()?.blip.adjust))
    }

    /// Replaces the selected pictures' adjustments (every selected picture, or the selected group child).
    pub fn set_picture_adjust(&mut self, adjust: &[crate::drawing::paint::BlipAdjust], fonts: &Fonts) -> Outcome {
        self.edit_shapes(
            "Picture Adjustments",
            |s| match &mut s.kind {
                ShapeKind::Pic(p) => {
                    p.blip.adjust = adjust.to_vec();
                    true
                }
                _ => false,
            },
            fonts,
        )
    }

    /// Recolours the selected pictures, keeping their other adjustments.
    pub fn set_picture_recolor(&mut self, recolor: Recolor, fonts: &Fonts) -> Outcome {
        use crate::drawing::paint::BlipAdjust as A;
        if let Recolor::BlackAndWhite(t) = recolor
            && !(0.0..=1.0).contains(&t)
        {
            return Outcome::invalid("a threshold is 0 to 1");
        }
        self.edit_shapes(
            "Recolor",
            |s| match &mut s.kind {
                ShapeKind::Pic(p) => {
                    p.blip.adjust.retain(|a| !matches!(a, A::Grayscale | A::BiLevel(_) | A::Duotone(_)));
                    match &recolor {
                        Recolor::None => {}
                        Recolor::Grayscale => p.blip.adjust.push(A::Grayscale),
                        Recolor::BlackAndWhite(t) => p.blip.adjust.push(A::BiLevel(Pct((t * 100_000.0).round() as i32))),
                        Recolor::Duotone(a, b) => p.blip.adjust.push(A::Duotone([a.clone(), b.clone()])),
                    }
                    true
                }
                _ => false,
            },
            fonts,
        )
    }

    /// Word's Reset Picture: the selected picture's adjustments, effects, outline and shape gone (its crop kept); with
    /// `and_size` (Reset Picture & Size) its crop gone too and its image at its own size. One step.
    pub fn reset_picture(&mut self, and_size: bool, fonts: &Fonts) -> Outcome {
        let natural = self.original_object_size();
        if and_size && natural.is_none() {
            return Outcome::unavailable("no image size");
        }
        self.edit_picture(
            if and_size { "Reset Picture & Size" } else { "Reset Picture" },
            |e| {
                let Some(ShapeKind::Pic(p)) = e.shape.as_mut().map(|s| &mut s.kind) else { return false };
                p.blip.adjust.clear();
                p.effects = None;
                p.line = None;
                // Without effects or an outline, nothing but a turn reaches past its box.
                *e.effect = [0.0; 4];
                p.geom = crate::drawing::geom::Geometry::Preset { preset: crate::drawing::geom::Preset::Rect, adj: Vec::new() };
                if let Some(s) = e.shape.as_deref_mut() {
                    (s.xfrm.rot, s.xfrm.flip_h, s.xfrm.flip_v) = (Default::default(), false, false);
                }
                if and_size && let (Some((w, h)), Some(ShapeKind::Pic(p))) = (natural, e.shape.as_mut().map(|s| &mut s.kind)) {
                    p.blip.src = [Pct(0); 4];
                    *e.w = (w.max(1.0) * 20.0).round() / 20.0;
                    *e.h = (h.max(1.0) * 20.0).round() / 20.0;
                    if let Some(r) = e.relative.as_deref_mut() {
                        *r = [None, None];
                    }
                }
                true
            },
            fonts,
        )
    }
}

impl Editor {
    /// Aligns or distributes the selected floating objects across their page, its margins, or the box around them
    /// (the default for two or more).
    pub(in crate::editing) fn align_objects(&mut self, a: crate::api::command::AlignObjects, fonts: &Fonts) -> Outcome {
        use crate::api::command::{AlignReference as R, ObjectAlign as A};
        let ids = self.selected_floats();
        if ids.is_empty() {
            return Outcome::unavailable("no floating object selected");
        }
        let boxes: Vec<(crate::layout::FloatId, usize, [f64; 4])> =
            ids.iter().filter_map(|&f| self.float_box(f, fonts).map(|(p, r)| (f, p, r))).collect();
        let Some(&(_, page, _)) = boxes.first() else { return Outcome::Unchanged("not laid out") };
        let boxes: Vec<_> = boxes.into_iter().filter(|b| b.1 == page).collect();
        let reference = a.relative_to.unwrap_or(if boxes.len() > 1 { R::SelectedObjects } else { R::Margin });
        let Some(geo) = self.pages.page(page, &self.index) else { return Outcome::Unchanged("not laid out") };
        let (pw, ph) = (geo.w, geo.h);
        let area = match reference {
            R::Page => [0.0, 0.0, pw, ph],
            R::Margin => match self.float_frames(page, None, false) {
                Some((fr, _)) => [fr.margin[0], fr.margin[2], pw - fr.margin[1], ph - fr.margin[3]],
                None => [0.0, 0.0, pw, ph],
            },
            R::SelectedObjects => boxes
                .iter()
                .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |u, (_, _, r)| [u[0].min(r[0]), u[1].min(r[1]), u[2].max(r[2]), u[3].max(r[3])]),
        };
        let mut targets: Vec<(crate::layout::FloatId, [f64; 4], [f64; 4])> = Vec::new();
        let place = |r: [f64; 4], x: Option<f64>, y: Option<f64>| {
            let (w, h) = (r[2] - r[0], r[3] - r[1]);
            let (x, y) = (x.unwrap_or(r[0]), y.unwrap_or(r[1]));
            [x, y, x + w, y + h]
        };
        match a.align {
            A::DistributeHorizontally | A::DistributeVertically => {
                let k = usize::from(a.align == A::DistributeVertically);
                let mut sorted = boxes.clone();
                sorted.sort_by(|p, q| p.2[k].total_cmp(&q.2[k]));
                let total: f64 = sorted.iter().map(|b| b.2[k + 2] - b.2[k]).sum();
                let n = sorted.len();
                // Even gaps between the objects, the outer ones at the area's edges (one object: centred).
                let gap = if n > 1 { (area[k + 2] - area[k] - total) / (n - 1) as f64 } else { 0.0 };
                let mut at = if n > 1 { area[k] } else { (area[k] + area[k + 2] - total) / 2.0 };
                for (f, _, r) in sorted {
                    let to = if k == 0 { place(r, Some(at), None) } else { place(r, None, Some(at)) };
                    at += r[k + 2] - r[k] + gap;
                    targets.push((f, r, to));
                }
            }
            align => {
                for &(f, _, r) in &boxes {
                    let (w, h) = (r[2] - r[0], r[3] - r[1]);
                    let to = match align {
                        A::Left => place(r, Some(area[0]), None),
                        A::Center => place(r, Some((area[0] + area[2] - w) / 2.0), None),
                        A::Right => place(r, Some(area[2] - w), None),
                        A::Top => place(r, None, Some(area[1])),
                        A::Middle => place(r, None, Some((area[1] + area[3] - h) / 2.0)),
                        _ => place(r, None, Some(area[3] - h)),
                    };
                    targets.push((f, r, to));
                }
            }
        }
        let moved: Vec<crate::docx::Float> = targets
            .into_iter()
            .filter(|(_, r, to)| r.iter().zip(to).any(|(a, b)| (a - b).abs() > 0.01))
            .filter_map(|(f, r, to)| self.moved_float(f, page, r, to))
            .collect();
        if moved.is_empty() {
            return Outcome::Unchanged("already so");
        }
        let label = if matches!(a.align, A::DistributeHorizontally | A::DistributeVertically) { "Distribute" } else { "Align" };
        let (sel, also) = (self.selection, self.object_also.clone());
        let mut tx = self.begin(label);
        for n in moved {
            let f = crate::layout::FloatId(n.id);
            let Some(anchor) = self.index.float_anchor(f) else { continue };
            let Some(p) = self.index.para(&self.doc, anchor) else { continue };
            let mut inlines = p.inlines.clone();
            for i in &mut inlines {
                if let Inline::Float(fl, _) = i
                    && fl.id == n.id
                {
                    **fl = n.clone();
                }
            }
            if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale object");
            }
        }
        (self.selection, self.object_also) = (sel, also);
        self.commit(tx, None, fonts)
    }
}

#[cfg(test)]
mod tests {
    use crate::editing::commands::{Command, Outcome};
    use crate::editing::selection::{ObjectRef, Selection};
    use crate::editing::tests::{fonts, open};

    fn run(e: &mut crate::editing::Editor, v: serde_json::Value) {
        let c: Command = serde_json::from_value(v.clone()).expect("a command");
        let out = e.execute(c, fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{v}: {out:?}");
    }

    #[test]
    fn picture_tools_save_and_read_back() {
        let picture = |e: &crate::editing::Editor| {
            crate::editing::tests::texts_ids(e)
                .into_iter()
                .filter_map(|p| e.index().para(e.document(), p))
                .flat_map(|p| p.inlines.iter())
                .find_map(|i| match i {
                    crate::docx::Inline::Float(f, _) if f.text_box.is_none() && f.shape.as_deref().and_then(|s| s.pic()).is_some() => Some(f.id),
                    _ => None,
                })
                .expect("a floating picture")
        };
        let mut e = open("FX-OBJ.docx");
        let data = e.document().images.last().cloned().expect("an image");
        let float = picture(&e);
        e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(crate::layout::FloatId(float))));
        let w0 = e.object_selection().expect("selected").width.0;
        run(&mut e, serde_json::json!({ "type": "setPictureCrop", "left": 0.25 }));
        let o = e.object_selection().expect("selected");
        assert!((o.width.0 - w0 * 0.75).abs() < 0.1, "the box follows the part shown: {} of {w0}", o.width.0);
        assert_eq!(o.crop.and_then(|c| c.left), Some(0.25));
        run(&mut e, serde_json::json!({ "type": "setPictureAdjust", "transparentColor": "#FFFFFF" }));
        assert_eq!(e.object_selection().and_then(|o| o.transparent_color), Some(crate::api::value::Color(0xFFFFFF)));
        run(&mut e, serde_json::json!({ "type": "setObjectSize", "relativeWidth": { "percent": 40, "relativeTo": "margin" } }));
        run(&mut e, serde_json::json!({ "type": "setObjectPosition", "horizontal": { "relativeTo": "page", "percent": 10 } }));
        run(&mut e, serde_json::json!({ "type": "alignObjects", "align": "bottom", "relativeTo": "page" }));
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
        for el in ["<a:srcRect l=\"25000\"", "<a:clrChange>", "<wp14:sizeRelH", "<wp14:pctWidth>40000</wp14:pctWidth>", "<wp14:pctPosHOffset"] {
            assert!(xml.contains(el), "{el}");
        }
        crate::editing::Editor::open(&saved, fonts()).expect("reopens");
        // Change Picture clears the crop.
        let mut e2 = open("FX-OBJ.docx");
        let float = picture(&e2);
        e2.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(crate::layout::FloatId(float))));
        run(&mut e2, serde_json::json!({ "type": "setPictureCrop", "top": 0.5 }));
        let out = e2.execute(Command::ChangePicture(crate::api::value::Bytes(data)), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
        assert!(e2.object_selection().is_some_and(|o| o.crop.is_none()));
    }
}

#[cfg(test)]
mod inline_tests {
    use crate::editing::commands::{Command, Outcome};
    use crate::editing::selection::{ObjectRef, Selection};
    use crate::editing::tests::{fonts, open};
    use crate::layout::Unit;

    #[test]
    fn an_inline_picture_keeps_its_crop() {
        let mut e = open("FX-OBJ.docx");
        let at = crate::editing::tests::texts_ids(&e).into_iter().find_map(|p| {
            let para = e.index().para(e.document(), p)?;
            let mut u = 0u32;
            for i in &para.inlines {
                if matches!(i, crate::docx::Inline::Picture(pic, _) if pic.shape.as_deref().and_then(|s| s.pic()).is_some()) {
                    return Some((p, u));
                }
                u += i.units() as u32;
            }
            None
        });
        let (para, u) = at.expect("an inline picture with DrawingML properties");
        e.set_caret_for_benchmark(Selection::Object(ObjectRef::Inline { para, offset: Unit(u) }));
        let c: Command = serde_json::from_value(serde_json::json!({ "type": "setPictureCrop", "right": 0.5 })).expect("a command");
        assert!(matches!(e.execute(c, fonts()), Outcome::Changed(_)));
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
        assert!(xml.contains("r=\"50000\""), "the crop is saved");
    }
}
