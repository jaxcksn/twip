//! Watermarks and the page background

use crate::api::command::{PictureWatermark, TextWatermark, Watermark, WatermarkLayout};
use crate::api::drawing::{ColourValue, FillValue, TransformKind};
use crate::api::value::{Bytes, Color, Points};
use crate::docx::watermark::{self as wm, PictureMark, TextMark};
use crate::docx::{Block, Float, Inline, ThemeColourRef};
use crate::drawing::paint::{BlipAdjust, BlipMode, Fill};
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Editor;
use super::commands::Outcome;
use super::step::{PageBackground, Step};

/// The most stops a page gradient has.
const MAX_STOPS: usize = 64;
/// The largest picture watermark scale, percent.
const MAX_SCALE: f64 = 5000.0;

impl Editor {
    /// Sets the page colour, or its fill.
    pub(super) fn set_page_color(&mut self, v: FillValue, fonts: &Fonts) -> Outcome {
        let was = PageBackground::of(&self.doc);
        let now = match self.background_of(&v, &was) {
            Ok(b) => b,
            Err(why) => {
                self.pending_images.clear();
                return Outcome::Refused(crate::api::error::Error::invalid(why));
            }
        };
        if now == was {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin("Page Color");
        if self.step(&mut tx, Step::PageBackground(Box::new(now))).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the page colour can't change");
        }
        self.commit(tx, None, fonts)
    }

    /// The page background `v` sets: no colour, a colour (a theme colour's tint or shade as Word applies one), or a
    /// fill over the colour of its first colour, which shows.
    fn background_of(&mut self, v: &FillValue, was: &PageBackground) -> Result<PageBackground, String> {
        let palette = crate::drawing::paint::Palette { theme: &self.doc.theme, map: &self.doc.colour_map };
        let rgb = |c: &crate::drawing::colour::Colour| c.resolve(palette.theme, palette.map, None).to_rgb();
        Ok(match v {
            FillValue::None => PageBackground { colour: None, theme: None, fill: None, shown: was.shown },
            FillValue::Solid { colour } => {
                let (colour, theme) = self.page_colour_of(colour)?;
                PageBackground { colour: Some(colour), theme, fill: None, shown: true }
            }
            FillValue::FromStyle => return Err("the page has no style to take a fill from".into()),
            FillValue::Gradient { stops, .. } if stops.len() > MAX_STOPS => return Err(format!("a page gradient has at most {MAX_STOPS} stops")),
            FillValue::Pattern { preset, .. } if !crate::drawing::patterns::WORD.iter().any(|(p, _)| p.name() == preset) => {
                return Err(format!("{preset:?} isn't a pattern Word offers for the page"));
            }
            _ => {
                let mut added = None;
                // No picture keeps the page's own.
                let kept = match &was.fill {
                    Some(Fill::Blip(b)) => b.image,
                    _ => None,
                };
                let fill = v
                    .to_model(&mut |bytes| {
                        if bytes.is_empty() {
                            return kept;
                        }
                        crate::docx::image::image_type(bytes)?;
                        added = Some(bytes.to_vec());
                        Some(self.doc.images.len() + self.pending_images.len())
                    })?
                    .ok_or("no fill")?;
                let fill = one_colour(fill, palette);
                self.pending_images.extend(added);
                let colour = match &fill {
                    Fill::Gradient(g) => g.stops.first().map(|(_, c)| rgb(c)),
                    Fill::Pattern { bg, .. } => Some(rgb(bg)),
                    _ => was.colour.or(Some(0xFF_FFFF)),
                };
                PageBackground { colour, theme: None, fill: Some(fill), shown: true }
            }
        })
    }

    /// A page colour and the theme colour it is: a theme colour takes one tint or shade.
    fn page_colour_of(&self, c: &ColourValue) -> Result<(u32, Option<ThemeColourRef>), String> {
        let Some(t) = c.theme else {
            let model = c.to_model()?;
            return Ok((model.resolve(&self.doc.theme, &self.doc.colour_map, None).to_rgb(), None));
        };
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let (tint, shade) = match c.transforms.as_slice() {
            [] => (None, None),
            [x] if x.kind == TransformKind::Tint => (Some(byte(x.value)), None),
            [x] if x.kind == TransformKind::Shade => (None, Some(byte(x.value))),
            _ => return Err("a theme page colour takes one tint or shade".into()),
        };
        let r = ThemeColourRef { slot: t.slot(), tint, shade };
        Ok((crate::docx::props::ThemeColours::new(&self.doc.theme, &self.doc.colour_map).resolve(r), Some(r)))
    }

    /// The page background as the state reports it: a pattern's tile picture as its pattern.
    pub(super) fn background_state(&self) -> Option<FillValue> {
        if let Some(f) = &self.doc.page_fill {
            if let Fill::Blip(b) = f
                && matches!(b.mode, BlipMode::Tile { .. })
                && let Some(BlipAdjust::Duotone([fg, bg])) = b.adjust.iter().find(|a| matches!(a, BlipAdjust::Duotone(_)))
                && let Some(p) = b.image.and_then(|i| self.doc.images.get(i)).and_then(|d| crate::drawing::patterns::of_picture(d))
            {
                return Some(FillValue::Pattern {
                    preset: p.name().into(),
                    foreground: ColourValue::of(fg, None),
                    background: ColourValue::of(bg, None),
                });
            }
            return Some(FillValue::of(f, None, &[]));
        }
        let colour = self.doc.page_colour?;
        let value = match self.doc.page_colour_theme.and_then(|t| crate::api::drawing::ThemeColour::of(t.slot).map(|s| (s, t))) {
            Some((slot, t)) => {
                let mut v = ColourValue::theme(slot);
                let f = |b: u8| f64::from(b) / 255.0;
                match (t.tint, t.shade) {
                    (Some(b), _) => v.transforms.push(crate::api::drawing::ColourTransform { kind: TransformKind::Tint, value: f(b) }),
                    (None, Some(b)) => v.transforms.push(crate::api::drawing::ColourTransform { kind: TransformKind::Shade, value: f(b) }),
                    _ => {}
                }
                v
            }
            None => ColourValue::rgb(Color(colour)),
        };
        Some(FillValue::Solid { colour: value })
    }

    /// The watermark section `section`'s pages show: the first in its headers, else in any header.
    pub(super) fn watermark_state(&self, section: usize) -> Option<Watermark> {
        let sections = &self.doc.sections;
        let shown: Vec<&String> = (0..3)
            .filter_map(|slot| (0..=section.min(sections.len().saturating_sub(1))).rev().find_map(|k| sections[k].headers[slot].as_ref()))
            .collect();
        let names = shown.into_iter().chain(sections.iter().flat_map(|s| s.headers.iter().flatten()));
        let (f, run) = names.filter_map(|n| self.doc.stories.get(n)).find_map(|b| first_watermark(b))?;
        Some(match wm::of(f)? {
            wm::Watermark::Text(t) => Watermark::Text(TextWatermark {
                text: t.text,
                font: Some(t.family),
                language: Some(run.lang.clone()).filter(|l| !l.is_empty()),
                size: t.size.map(Points),
                color: Some(Color(t.colour)),
                transparency: Some(t.transparency),
                layout: Some(if t.diagonal { WatermarkLayout::Diagonal } else { WatermarkLayout::Horizontal }),
                bold: Some(t.bold),
                italic: Some(t.italic),
            }),
            wm::Watermark::Picture(p) => {
                let natural = p.image.and_then(|i| self.doc.images.get(i)).and_then(|d| natural_size(d));
                // Auto: the size that fits the section's margins.
                let auto = natural.map(|(w, h)| picture_size((w, h), None, self.margin_area(section)));
                let scale = match (natural, auto) {
                    (_, Some(a)) if (a[0] - f.w).abs() < 0.5 && (a[1] - f.h).abs() < 0.5 => None,
                    (Some(n), _) => Some((f.w / n.0 * 100.0).round()),
                    _ => None,
                };
                Watermark::Picture(PictureWatermark { image: Bytes(Vec::new()), scale, washout: Some(p.washout) })
            }
        })
    }

    /// Puts watermark `w` in every header of every section in place of the one there is: in each header part a
    /// section shows, and in new ones for the first section's missing headers (the sections after it show those).
    pub(super) fn set_watermark(&mut self, w: Watermark, fonts: &Fonts) -> Outcome {
        let language = match &w {
            Watermark::Text(t) => t.language.clone(),
            Watermark::Picture(_) => None,
        };
        let (mark, natural, data) = match self.mark_of(w) {
            Ok(m) => m,
            Err(why) => return Outcome::Refused(crate::api::error::Error::invalid(why)),
        };
        let image = match data {
            Some(Ok(d)) => Some(self.pending_image(d)),
            Some(Err(kept)) => Some(kept),
            None => None,
        };
        let mark = match mark {
            wm::Watermark::Picture(p) => wm::Watermark::Picture(PictureMark { image, ..p }),
            m => m,
        };
        let used = self.shape_numbers();
        // Past the highest number in use; numbers from Word's first again when none are left past it.
        let mut spid = used.iter().max().copied().unwrap_or(0).max(2048);
        if spid.checked_add(4096).is_none() {
            spid = 2048;
        }
        let ink = match &mark {
            wm::Watermark::Text(t) => ink_em(&t.text, &t.family, t.bold, t.italic, fonts),
            wm::Watermark::Picture(_) => (1.0, 1.0),
        };
        let mut make = |e: &Editor, section: usize| {
            loop {
                spid = spid.checked_add(1).unwrap_or(2049);
                if !used.contains(&spid) {
                    break;
                }
            }
            let area = e.margin_area(section);
            let size = match (&mark, natural) {
                (wm::Watermark::Text(t), _) => text_size(t, ink, area),
                (wm::Watermark::Picture(_), Some((w, h, scale))) => picture_size((w, h), scale, area),
                _ => area,
            };
            wm::float(&mark, size, spid, image)
        };
        let mut tx = self.begin("Watermark");
        let mut changed = false;
        for (name, section) in self.header_parts() {
            let Some(f) = make(self, section) else { continue };
            let Some(steps) = self.restyled(&name, Some(f), language.as_deref()) else { continue };
            for s in steps {
                if self.step(&mut tx, s).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("the header can't change");
                }
            }
            changed = true;
        }
        for slot in 0..3 {
            if self.doc.sections.first().is_none_or(|s| s.headers[slot].is_some()) {
                continue;
            }
            let (Some(f), Some(mut para)) = (make(self, 0), self.empty_story_para(false)) else { continue };
            let mut run = para.mark.clone();
            if let Some(l) = &language {
                std::rc::Rc::make_mut(&mut run).lang.clone_from(l);
            }
            para.inlines.push(Inline::Float(Box::new(f), run));
            let step = Step::HeaderPart { name: self.new_story_name(false), section: 0, footer: false, slot, blocks: Some(vec![Block::Para(para)]) };
            if self.step(&mut tx, step).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("the header can't change");
            }
            changed = true;
        }
        if !changed {
            self.abort(tx);
            return Outcome::Unchanged("no header takes a watermark");
        }
        self.commit(tx, None, fonts)
    }

    /// Takes the watermarks out of every header.
    pub(super) fn remove_watermark(&mut self, fonts: &Fonts) -> Outcome {
        let mut tx = self.begin("Remove Watermark");
        let names: Vec<String> = self.doc.stories.keys().cloned().collect();
        for name in names {
            for s in self.restyled(&name, None, None).unwrap_or_default() {
                if self.step(&mut tx, s).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("the header can't change");
                }
            }
        }
        if tx.undo.is_empty() {
            self.abort(tx);
            return Outcome::Unchanged("no watermark");
        }
        self.commit(tx, None, fonts)
    }

    /// The watermark `w` describes, with a picture's own size and scale and its image (new, or the index of the one
    /// the watermark keeps); checked.
    #[allow(clippy::type_complexity)]
    fn mark_of(&self, w: Watermark) -> Result<(wm::Watermark, Option<(f64, f64, Option<f64>)>, Option<Result<Vec<u8>, usize>>), String> {
        Ok(match w {
            Watermark::Text(t) => {
                let text: String = t.text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
                if text.trim().is_empty() || text.chars().count() > wm::MAX_TEXT {
                    return Err(format!("a text watermark has 1…{} characters", wm::MAX_TEXT));
                }
                let family = match t.font {
                    Some(f) if f.trim().is_empty() || f.chars().count() > 64 => return Err("a font name has 1…64 characters".into()),
                    Some(f) => f,
                    None => self.doc.resolver.as_ref().map_or_else(|| "Calibri".to_string(), |r| r.theme.minor.clone()),
                };
                if t.language.as_deref().is_some_and(|l| l.is_empty() || l.len() > 35 || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) {
                    return Err("a language is a tag such as en-US".into());
                }
                if t.transparency.is_some_and(|v| !(0.0..=100.0).contains(&v)) {
                    return Err("a watermark's transparency is 0…100%".into());
                }
                if t.size.is_some_and(|s| !(1.0..=1584.0).contains(&s.0)) {
                    return Err("a watermark's font size is 1…1584 pt".into());
                }
                let mark = TextMark {
                    text,
                    family,
                    size: t.size.map(|s| s.0),
                    colour: t.color.map_or(wm::SILVER, |c| c.0 & 0xFF_FFFF),
                    transparency: t.transparency.unwrap_or(50.0),
                    diagonal: t.layout.unwrap_or_default() == WatermarkLayout::Diagonal,
                    bold: t.bold.unwrap_or(false),
                    italic: t.italic.unwrap_or(false),
                };
                (wm::Watermark::Text(mark), None, None)
            }
            Watermark::Picture(p) => {
                if p.scale.is_some_and(|s| !(1.0..=MAX_SCALE).contains(&s)) {
                    return Err(format!("a picture watermark's scale is 1…{MAX_SCALE}%"));
                }
                // No picture keeps the watermark's own.
                let image = match p.image.0.is_empty() {
                    true => Err(self.watermark_picture().ok_or("a picture watermark needs a picture")?),
                    false => Ok(p.image.0),
                };
                let bytes = match &image {
                    Ok(d) => d.as_slice(),
                    Err(k) => self.doc.images.get(*k).map_or(&[][..], Vec::as_slice),
                };
                let (w, h) = natural_size(bytes).ok_or("not a picture")?;
                let mark = PictureMark { image: None, washout: p.washout.unwrap_or(true) };
                (wm::Watermark::Picture(mark), Some((w, h, p.scale)), Some(image))
            }
        })
    }

    /// The image of the picture watermark there is.
    fn watermark_picture(&self) -> Option<usize> {
        self.doc.stories.values().filter_map(|b| first_watermark(b)).find_map(|(f, _)| match wm::of(f)? {
            wm::Watermark::Picture(p) => p.image,
            wm::Watermark::Text(_) => None,
        })
    }

    /// Each header part a section shows, once, with the first section that has it as its own.
    fn header_parts(&self) -> Vec<(String, usize)> {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<(String, usize)> = Vec::new();
        for (k, s) in self.doc.sections.iter().enumerate() {
            for name in s.headers.iter().flatten() {
                if self.doc.stories.contains_key(name) && seen.insert(name.as_str()) {
                    out.push((name.clone(), k));
                }
            }
        }
        out
    }

    /// A section's text area (between its margins), points.
    fn margin_area(&self, section: usize) -> [f64; 2] {
        self.doc
            .sections
            .get(section)
            .map_or([468.0, 648.0], |s| [(s.page_w - s.left - s.right - s.gutter).max(36.0), (s.page_h - s.top - s.bottom).max(36.0)])
    }

    /// The steps that take the watermarks out of header part `name`, putting `with` at the start of its first
    /// paragraph; `None` when nothing changes.
    fn restyled(&self, name: &str, with: Option<Float>, language: Option<&str>) -> Option<Vec<Step>> {
        fn paras<'b>(blocks: &'b [Block], out: &mut Vec<&'b crate::docx::Paragraph>) {
            for b in blocks {
                match b {
                    Block::Para(p) => out.push(p),
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| paras(&c.blocks, out)),
                }
            }
        }
        let blocks = self.doc.stories.get(name)?;
        let mut all = Vec::new();
        paras(blocks, &mut all);
        let first = blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(p.id) } else { None }).or(all.first().map(|p| p.id));
        let mut with = with;
        let mut steps = Vec::new();
        for p in all {
            let marked = p.inlines.iter().any(|i| matches!(i, Inline::Float(f, _) if wm::is(f)));
            if !marked && (Some(p.id) != first || with.is_none()) {
                continue;
            }
            let mut inlines: Vec<Inline> = p.inlines.iter().filter(|i| !matches!(i, Inline::Float(f, _) if wm::is(f))).cloned().collect();
            if Some(p.id) == first
                && let Some(f) = with.take()
            {
                let mut run = p.inlines.iter().find_map(|i| i.run().cloned()).unwrap_or_else(|| p.mark.clone());
                if let Some(l) = language {
                    std::rc::Rc::make_mut(&mut run).lang = l.to_string();
                }
                inlines.insert(0, Inline::Float(Box::new(f), run));
            }
            steps.push(Step::Restyle { para: ParaId(p.id), inlines });
        }
        (!steps.is_empty()).then_some(steps)
    }

    /// The VML shape numbers the headers use (`o:spid`'s `_x0000_s` numbers as read, and the shapes made since).
    fn shape_numbers(&self) -> std::collections::HashSet<u32> {
        let mut used: std::collections::HashSet<u32> =
            self.doc.source.as_ref().map_or_else(Default::default, |s| s.stories.values().flat_map(|(xml, _)| spids(xml)).collect());
        used.extend(
            self.doc
                .stories
                .values()
                .flat_map(|b| b.iter())
                .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
                .flat_map(|p| p.inlines.iter())
                .filter_map(|i| if let Inline::Float(f, _) = i { f.shape.as_ref().map(|s| s.nv.id).filter(|n| *n > 0) } else { None }),
        );
        used
    }
}

/// A linear gradient from a colour to its own shade or tint (Fill Effects' One colour) as Word makes it: the second
/// colour the first's VML `darken` or `lighten`, the two blended as Word blends two colours.
fn one_colour(f: Fill, palette: crate::drawing::paint::Palette) -> Fill {
    use crate::drawing::colour::{Colour, ColourMod};
    let Fill::Gradient(mut g) = f else { return f };
    let [(_, a), (_, b)] = g.stops.as_slice() else { return Fill::Gradient(g) };
    let derived = match b.mods.split_last() {
        Some((ColourMod::Shade(p), rest)) if b.base == a.base && rest == a.mods.as_slice() => Some((false, *p)),
        Some((ColourMod::Tint(p), rest)) if b.base == a.base && rest == a.mods.as_slice() => Some((true, *p)),
        _ => None,
    };
    let (Some((light, p)), crate::drawing::paint::GradientKind::Linear { .. }) = (derived, g.kind) else { return Fill::Gradient(g) };
    let first = a.resolve(palette.theme, palette.map, None).to_rgb();
    let n = (p.fraction().clamp(0.0, 1.0) * 255.0).round() as u8;
    let second = if light { crate::drawing::vml::lighten(first, n) } else { crate::drawing::vml::darken(first, n) };
    g.stops = crate::drawing::vml::two_colour(&Colour::srgb(first), &Colour::srgb(second));
    Fill::Gradient(g)
}

/// The first watermark in a story's paragraphs.
fn first_watermark(blocks: &[Block]) -> Option<(&Float, &crate::docx::Run)> {
    blocks.iter().find_map(|b| match b {
        Block::Para(p) => p.inlines.iter().find_map(|i| match i {
            Inline::Float(f, run) if wm::is(f) => Some((&**f, run)),
            _ => None,
        }),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| first_watermark(&c.blocks)),
    })
}

/// The `_x0000_s` shape numbers in a part's XML.
fn spids(xml: &str) -> impl Iterator<Item = u32> + '_ {
    xml.match_indices("_x0000_s").filter_map(|(k, m)| xml[k + m.len()..].split(|c: char| !c.is_ascii_digit()).next()?.parse::<u32>().ok())
}

/// A picture watermark's size: its own (`natural`) at `scale` percent, or Auto: fitted to the margins (`area`).
fn picture_size((w, h): (f64, f64), scale: Option<f64>, area: [f64; 2]) -> [f64; 2] {
    let k = scale.map_or_else(|| (area[0] / w).min(area[1] / h), |s| s / 100.0);
    [(w * k).clamp(1.0, 1584.0), (h * k).clamp(1.0, 1584.0)]
}

/// A picture's own size in points (at its resolution, else 96 dpi).
fn natural_size(data: &[u8]) -> Option<(f64, f64)> {
    crate::docx::image::image_type(data)?;
    let (pw, ph, dx, dy) = crate::docx::image::image_info(data)?;
    let at = |px: f64, dpi: f64| px * 72.0 / if dpi > 1.0 { dpi } else { 96.0 };
    Some((at(pw, dx), at(ph, dy))).filter(|(w, h)| *w > 0.0 && *h > 0.0)
}

/// A text watermark's box: its ink at its size with the insets Word keeps around a text path (7.2 pt across, 3.6
/// down); an Auto size fills the margins' width, the box turned when diagonal.
fn text_size(t: &TextMark, (iw, ih): (f64, f64), area: [f64; 2]) -> [f64; 2] {
    let size = t.size.unwrap_or_else(|| {
        let fit =
            if t.diagonal { (area[0] * std::f64::consts::SQRT_2 - 21.6) / (iw + ih) } else { ((area[0] - 14.4) / iw).min((area[1] - 7.2) / ih) };
        fit.max(1.0)
    });
    [(iw * size + 14.4).min(1584.0), (ih * size + 7.2).min(1584.0)]
}

/// The ink box of `text` in its face, in ems: width and height; a plain estimate without the face.
fn ink_em(text: &str, family: &str, bold: bool, italic: bool, fonts: &Fonts) -> (f64, f64) {
    let estimate = (0.6 * text.chars().count().max(1) as f64, 0.7);
    let Some(face) = fonts.face(family, None, bold, italic) else { return estimate };
    let em = 1.0 / face.upem;
    let (mut pen, mut x0, mut x1, mut top, mut low) = (0.0, f64::MAX, f64::MIN, f64::MIN, f64::MAX);
    for g in face.shape(text, true, 0, 0) {
        if let Some((left, w)) = face.ink(g.id) {
            x0 = x0.min(pen + g.dx + left);
            x1 = x1.max(pen + g.dx + left + w);
        }
        if let (Some(t), Some(b)) = (face.ink_top(g.id), face.ink_bottom(g.id)) {
            top = top.max(t);
            low = low.min(-b);
        }
        pen += g.advance;
    }
    if x1 <= x0 || top <= low {
        return estimate;
    }
    (((x1 - x0) * em).max(0.05), ((top - low) * em).max(0.05))
}
