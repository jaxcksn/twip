//! The placeholder affordances: field marks and their captions, drawn over the page in its scheme's colours. Sizes
//! are CSS px (`u` points each); nothing here is printed.

use skia_safe::{Canvas, Color, Font, Paint, Point, RRect, Rect, Typeface};

use super::theme::{Scheme, Theme};
use super::{ChromeFace, Renderer, chrome_typeface};
use crate::editing::view::{AffordanceView, Block, BlockState, Caption, CardState, FieldState, Icon, Segment, Tab};

/// A chip: text on a rounded fill, with an optional prefix drawn fainter. Sizes are CSS px.
pub(super) struct Chip<'a> {
    pub text: &'a str,
    pub prefix: Option<&'a str>,
    pub size: f64,
    pub pad: (f64, f64),
    pub radius: f64,
    pub fill: Color,
    pub border: Option<Color>,
    pub ink: Color,
    pub face: ChromeFace,
}

fn paint(c: Color) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color(c);
    p
}

fn stroke(c: Color, width: f64) -> Paint {
    let mut p = paint(c);
    p.set_style(skia_safe::paint::Style::Stroke);
    p.set_stroke_width(width as f32);
    p
}

impl Renderer {
    /// The UI face for `text`, or the document's first face when the UI face lacks one of its characters.
    fn face_for(&self, text: &str, face: ChromeFace) -> Option<Typeface> {
        let mono = chrome_typeface(face);
        match &mono {
            Some(tf) if text.chars().all(|c| tf.unichar_to_glyph(c as i32) != 0) => mono,
            _ => self.typefaces.values().flatten().next().cloned().or(mono),
        }
    }

    /// How wide a chip is, CSS px.
    pub(super) fn chip_width(&self, c: &Chip, u: f64) -> f64 {
        let whole = format!("{}{}", c.prefix.unwrap_or(""), c.text);
        let Some(tf) = self.face_for(&whole, c.face) else { return 0.0 };
        let font = Font::from_typeface(tf, (c.size * u) as f32);
        f64::from(font.measure_str(&whole, None).0) / u + 2.0 * c.pad.0
    }

    /// Draws a chip with its top left at `(x, y)` (page points) and returns its rect.
    pub(super) fn chip(&self, canvas: &Canvas, x: f64, y: f64, c: &Chip, u: f64) -> [f64; 4] {
        let whole = format!("{}{}", c.prefix.unwrap_or(""), c.text);
        let Some(tf) = self.face_for(&whole, c.face) else { return [x, y, x, y] };
        let font = Font::from_typeface(tf, (c.size * u) as f32);
        let w = f64::from(font.measure_str(&whole, None).0) + 2.0 * c.pad.0 * u;
        let h = (c.size + 2.0 * c.pad.1) * u;
        let r = Rect::from_xywh(x as f32, y as f32, w as f32, h as f32);
        let radius = (c.radius * u) as f32;
        canvas.draw_round_rect(r, radius, radius, &paint(c.fill));
        if let Some(b) = c.border {
            canvas.draw_round_rect(r, radius, radius, &stroke(b, u));
        }
        let (mut tx, baseline) = ((x + c.pad.0 * u) as f32, (y + (c.pad.1 + 0.85 * c.size) * u) as f32);
        if let Some(p) = c.prefix {
            canvas.draw_str(p, (tx, baseline), &font, &paint(c.ink.with_a((f32::from(c.ink.a()) * 0.7) as u8)));
            tx += font.measure_str(p, None).0;
        }
        canvas.draw_str(c.text, (tx, baseline), &font, &paint(c.ink));
        [x, y, x + w, y + h]
    }

    /// Page `i`'s affordances, over its content. The canvas is in page points.
    pub(super) fn affordances(&mut self, canvas: &Canvas, views: &[AffordanceView], i: usize, page_size: (f64, f64), u: f64) {
        let theme = self.page_theme();
        // Marks at rest are one fill: a page can hold hundreds.
        let mut rest = skia_safe::PathBuilder::new();
        let plain = !matches!(theme.scheme, Scheme::HighContrast(_));
        for v in views.iter().filter(|v| v.page() == i) {
            match v {
                AffordanceView::Region { bar, tab, rows, state, .. } => {
                    for r in rows {
                        canvas.draw_rect(rect(r), &paint(theme.accent.soft));
                    }
                    canvas.draw_rect(rect(bar), &paint(theme.accent.solid));
                    if let Some(t) = tab {
                        self.tab(canvas, t, *state, &theme, u);
                    }
                }
                AffordanceView::Card { rect: r, name, line, resolution, expand, state, .. } => {
                    self.card(canvas, *r, name, line, *resolution, *expand, *state, &theme, u)
                }
                AffordanceView::Block(b) => self.block(canvas, b, &theme, u),
                AffordanceView::IfRegion { rects, tab, state, .. } => {
                    for r in rects {
                        let rr = RRect::new_rect_xy(rect(r), (3.0 * u) as f32, (3.0 * u) as f32);
                        canvas.draw_rrect(rr, &paint(theme.accent.soft));
                        canvas.draw_rrect(rr, &stroke(theme.accent.mid, u));
                    }
                    if let Some(t) = tab {
                        self.tab(canvas, t, *state, &theme, u);
                    }
                }
                AffordanceView::BodyMark { bar, tab, .. } => {
                    let mut dash = stroke(theme.accent.solid, 2.0 * u);
                    dash.set_path_effect(skia_safe::PathEffect::dash(&[(3.0 * u) as f32, (2.0 * u) as f32], 0.0));
                    let x = (bar[0] + bar[2]) / 2.0;
                    canvas.draw_line((x as f32, bar[1] as f32), (x as f32, bar[3] as f32), &dash);
                    self.tab(canvas, tab, FieldState::Rest, &theme, u);
                }
                AffordanceView::Field { segments, state: FieldState::Rest, on_tint: false, .. } if plain => {
                    for s in segments {
                        rest.add_rrect(segment_shape(s, u), None, None);
                    }
                }
                AffordanceView::Field { segments, state, caption, on_tint, .. } => {
                    for s in segments {
                        if *on_tint {
                            canvas.draw_rrect(segment_shape(s, u), &paint(theme.tokens.paper));
                        }
                        field_segment(canvas, s, *state, &theme, u);
                    }
                    if let (Some(c), Some(last)) = (caption, segments.last()) {
                        self.caption(canvas, c, last.rect, page_size, &theme, u);
                    }
                }
            }
        }
        canvas.draw_path(&rest.detach(), &paint(theme.accent.soft));
    }

    /// A square tab in the accent with its icon and text in the accent's text colour, outlined in the page's
    /// colour while hovered or selected so it reads apart from the bar.
    fn tab(&self, canvas: &Canvas, t: &Tab, state: FieldState, theme: &Theme, u: f64) {
        let a = &theme.accent;
        canvas.draw_rect(rect(&t.rect), &paint(a.solid));
        if state != FieldState::Rest {
            let inset = [t.rect[0] + u, t.rect[1] + u, t.rect[2] - u, t.rect[3] - u];
            canvas.draw_rect(rect(&inset), &stroke(a.on, if state == FieldState::Selected { 1.5 * u } else { u }));
        }
        let mut x = t.rect[0] + 6.0 * u;
        if let Some(Icon::Copy) = t.icon {
            let (cx, cy, s) = (x + 5.0 * u, (t.rect[1] + t.rect[3]) / 2.0, 3.0 * u);
            let pen = stroke(a.on, 1.0 * u);
            canvas.draw_rect(rect(&[cx - s, cy - s + u, cx + s - 2.0 * u, cy + s]), &pen);
            canvas.draw_rect(rect(&[cx - s + 2.0 * u, cy - s - u, cx + s, cy + s - 2.0 * u]), &pen);
            x += 14.0 * u;
        }
        if let Some(Icon::Repeat) = t.icon {
            let (cx, cy, r) = (x + 5.0 * u, (t.rect[1] + t.rect[3]) / 2.0, 3.5 * u);
            let mut arc = skia_safe::PathBuilder::new();
            arc.add_arc(Rect::new((cx - r) as f32, (cy - r) as f32, (cx + r) as f32, (cy + r) as f32), -60.0, 300.0);
            canvas.draw_path(&arc.detach(), &stroke(a.on, 1.25 * u));
            let tip = (cx + r * 0.5, cy - r * 0.87);
            let mut head = skia_safe::PathBuilder::new();
            head.move_to(((tip.0 - 2.5 * u) as f32, (tip.1 - 1.5 * u) as f32))
                .line_to((tip.0 as f32, tip.1 as f32))
                .line_to(((tip.0 - 1.0 * u) as f32, (tip.1 + 2.5 * u) as f32));
            canvas.draw_path(&head.detach(), &stroke(a.on, 1.25 * u));
            x += 14.0 * u;
        }
        if let Some(tf) = self.face_for(&t.text, ChromeFace::Mono500) {
            let font = Font::from_typeface(tf, (11.0 * u) as f32);
            canvas.save();
            canvas.clip_rect(rect(&t.rect), None, true);
            canvas.draw_str(&t.text, (x as f32, (t.rect[3] - 4.5 * u) as f32), &font, &paint(a.on));
            canvas.restore();
        }
    }

    /// A sub-template card: a faint accent tint with a 1 px outline, the name over its line, Expand at the right. A
    /// reference that names nothing (or nearly a name) is outlined faintly and its name struck through.
    #[allow(clippy::too_many_arguments, reason = "the card's parts, as the view gives them")]
    fn card(
        &self,
        canvas: &Canvas,
        r: [f64; 4],
        name: &str,
        line: &str,
        res: CardState,
        expand: Option<[f64; 4]>,
        state: FieldState,
        theme: &Theme,
        u: f64,
    ) {
        let (t, a) = (&theme.tokens, &theme.accent);
        let rr = RRect::new_rect_xy(rect(&r), (6.0 * u) as f32, (6.0 * u) as f32);
        let lost = matches!(res, CardState::Missing | CardState::NearMatch);
        canvas.draw_rrect(rr, &paint(if lost { t.paper } else { a.soft }));
        let width = match state {
            FieldState::Rest | FieldState::Unmapped => u,
            FieldState::Hover => 1.5 * u,
            FieldState::Selected => 2.0 * u,
        };
        canvas.draw_rrect(
            rr,
            &stroke(
                if lost {
                    t.ink3
                } else if state == FieldState::Rest {
                    a.mid
                } else {
                    a.solid
                },
                width,
            ),
        );
        let x = r[0] + 12.0 * u;
        if let Some(tf) = self.face_for(name, ChromeFace::Mono600) {
            let font = Font::from_typeface(tf, (12.0 * u) as f32);
            let y = r[1] + 15.0 * u;
            canvas.draw_str(name, (x as f32, y as f32), &font, &paint(if lost { t.ink3 } else { t.ink }));
            if lost {
                let w = f64::from(font.measure_str(name, None).0);
                canvas.draw_line((x as f32, (y - 4.0 * u) as f32), ((x + w) as f32, (y - 4.0 * u) as f32), &stroke(t.ink3, u));
            }
        }
        if let Some(tf) = self.face_for(line, ChromeFace::Mono500) {
            let font = Font::from_typeface(tf, (10.0 * u) as f32);
            canvas.draw_str(line, (x as f32, (r[1] + 29.0 * u) as f32), &font, &paint(t.ink2));
        }
        if let (Some(e), Some(tf)) = (expand, self.face_for("Expand", ChromeFace::Mono600)) {
            let font = Font::from_typeface(tf, (11.0 * u) as f32);
            canvas.draw_str("Expand", ((e[0] + 6.0 * u) as f32, (e[3] - 5.0 * u) as f32), &font, &paint(a.solid));
        }
    }

    /// An input block in its base colour (the accent by default, lightened on a dark page as the accent is): at rest
    /// a soft tint with 8 px corners and a 20 px paper chip with the icon and label; hovered, a 1 px outline and the
    /// hint under the chip; selected, a 1.5 px outline and the buttons in the chip's place (the primary filled, the
    /// others outlined); filled, the picture itself, outlined with its tab while hovered.
    fn block(&mut self, canvas: &Canvas, b: &Block, theme: &Theme, u: f64) {
        let base = b.base.map_or(theme.accent.base, super::rgb);
        let a = super::theme::Accent::new(base, theme.scheme);
        let t = &theme.tokens;
        let rr = RRect::new_rect_xy(rect(&b.rect), (8.0 * u) as f32, (8.0 * u) as f32);
        let (cx, cy) = ((b.rect[0] + b.rect[2]) / 2.0, (b.rect[1] + b.rect[3]) / 2.0);
        if b.state == BlockState::Done {
            if let Some(tab) = &b.done {
                canvas.draw_rrect(rr, &stroke(a.mid, u));
                self.tab(canvas, tab, FieldState::Rest, theme, u);
            }
            return;
        }
        match &b.image {
            Some(bytes) => {
                let key = std::rc::Rc::as_ptr(bytes) as usize;
                let img = self.look_images.entry(key).or_insert_with(|| skia_safe::Image::from_encoded(skia_safe::Data::new_copy(bytes))).clone();
                if let Some(img) = img {
                    let [x0, y0, x1, y1] = b.rect;
                    let (iw, ih) = (f64::from(img.width()), f64::from(img.height()));
                    let s = ((x1 - x0) / iw).min((y1 - y0) / ih);
                    let (w, h) = (iw * s, ih * s);
                    canvas.draw_image_rect(&img, None, rect(&[cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0]), &Paint::default());
                }
            }
            None => {
                canvas.draw_rrect(rr, &paint(t.paper));
                canvas.draw_rrect(rr, &paint(a.soft));
            }
        }
        match b.state {
            BlockState::Rest | BlockState::Done => {}
            BlockState::Hover => {
                canvas.draw_rrect(rr, &stroke(a.solid, u));
            }
            BlockState::Selected => {
                canvas.draw_rrect(rr, &stroke(a.solid, 1.5 * u));
            }
        }
        if !b.buttons.is_empty() {
            for btn in &b.buttons {
                let r = RRect::new_rect_xy(rect(&btn.rect), (4.0 * u) as f32, (4.0 * u) as f32);
                let ink = if btn.primary {
                    canvas.draw_rrect(r, &paint(a.solid));
                    a.on
                } else {
                    canvas.draw_rrect(r, &paint(t.paper));
                    canvas.draw_rrect(r, &stroke(a.solid, u));
                    a.solid
                };
                if let Some(tf) = self.face_for(&btn.label, ChromeFace::Mono600) {
                    let font = Font::from_typeface(tf, (11.0 * u) as f32);
                    canvas.draw_str(&btn.label, ((btn.rect[0] + 10.0 * u) as f32, (btn.rect[3] - 7.0 * u) as f32), &font, &paint(ink));
                }
            }
            return;
        }
        if b.image.is_none() {
            let icon_w = if b.icon.is_some() { 16.0 } else { 0.0 };
            let w = (crate::editing::view::mono_width(&b.label, 11.0) + 16.0 + icon_w) * u;
            let chip = [cx - w / 2.0, cy - 10.0 * u, cx + w / 2.0, cy + 10.0 * u];
            canvas.draw_rrect(RRect::new_rect_xy(rect(&chip), (10.0 * u) as f32, (10.0 * u) as f32), &paint(t.paper));
            let mut x = chip[0] + 8.0 * u;
            if let Some(icon) = &b.icon {
                self.icon(canvas, icon, x, chip[1] + 4.0 * u, 12.0 * u, a.solid);
                x += icon_w * u;
            }
            if let Some(tf) = self.face_for(&b.label, ChromeFace::Mono600) {
                let font = Font::from_typeface(tf, (11.0 * u) as f32);
                canvas.draw_str(&b.label, (x as f32, (chip[3] - 6.0 * u) as f32), &font, &paint(a.solid));
            }
        }
        if b.state == BlockState::Hover
            && let Some(tf) = self.face_for(&b.hint, ChromeFace::Mono500)
        {
            let font = Font::from_typeface(tf, (10.0 * u) as f32);
            let w = f64::from(font.measure_str(&b.hint, None).0);
            canvas.draw_str(&b.hint, ((cx - w / 2.0) as f32, (cy + 24.0 * u) as f32), &font, &paint(t.ink2));
        }
    }

    /// An icon's vector shapes, `h` tall from `(x, y)`, unfilled parts in `colour`.
    fn icon(&mut self, canvas: &Canvas, icon: &std::rc::Rc<crate::editing::view::SvgIcon>, x: f64, y: f64, h: f64, colour: Color) {
        let paths = self
            .icons
            .entry(std::rc::Rc::as_ptr(icon) as usize)
            .or_insert_with(|| icon.paths.iter().filter_map(|(d, f)| Some((skia_safe::Path::from_svg(d)?, *f))).collect());
        let [vx, vy, _, vh] = icon.view_box;
        let s = (h as f32) / vh.max(0.01);
        canvas.save();
        canvas.translate((x as f32, y as f32));
        canvas.scale((s, s));
        canvas.translate((-vx, -vy));
        for (path, fill) in paths.iter() {
            canvas.draw_path(path, &paint(fill.map_or(colour, Color::new)));
        }
        canvas.restore();
    }

    /// The caption under a field's last segment (above it at the page's bottom), kept on the page and cut to fit it.
    fn caption(&self, canvas: &Canvas, c: &Caption, under: [f64; 4], (pw, ph): (f64, f64), theme: &Theme, u: f64) {
        let t = &theme.tokens;
        let text = if c.type_text.is_empty() { c.key.clone() } else { format!("{} \u{b7} {}", c.key, c.type_text) };
        fn chip<'a>(text: &'a str, t: &super::theme::Tokens) -> Chip<'a> {
            Chip {
                text,
                prefix: Some("{ } "),
                size: 11.0,
                pad: (6.0, 3.0),
                radius: 4.0,
                fill: t.paper,
                border: Some(t.line),
                ink: t.ink2,
                face: ChromeFace::Mono500,
            }
        }
        let room = pw / u;
        let mut shown = text.clone();
        while self.chip_width(&chip(&shown, t), u) > room && shown.chars().count() > 1 {
            let keep = shown.chars().count() - 2;
            shown = shown.chars().take(keep).chain(std::iter::once('\u{2026}')).collect();
        }
        let c = chip(&shown, t);
        let (w, h) = (self.chip_width(&c, u) * u, (c.size + 2.0 * c.pad.1) * u);
        let x = under[0].min(pw - w).max(0.0);
        let below = under[3] + 3.0 * u;
        let y = if below + h > ph { under[1] - 3.0 * u - h } else { below };
        self.chip(canvas, x, y, &c, u);
    }
}

fn rect(r: &[f64; 4]) -> Rect {
    Rect::new(r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32)
}

/// A segment's shape: drawn apart from a touching field's by 0.5 px, its field's ends rounded 3 px.
fn segment_shape(s: &Segment, u: f64) -> RRect {
    let [mut x0, y0, mut x1, y1] = s.rect;
    if s.gap_left {
        x0 += 0.5 * u;
    }
    if s.gap_right {
        x1 -= 0.5 * u;
    }
    let r = (3.0 * u) as f32;
    let (left, right) = (if s.round_left { r } else { 0.0 }, if s.round_right { r } else { 0.0 });
    let corners = [Point::new(left, left), Point::new(right, right), Point::new(right, right), Point::new(left, left)];
    RRect::new_rect_radii(Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32), &corners)
}

/// One line of a field mark: the accent's soft tint (mid when selected), a 1 px outline on hover and 1.5 px when
/// selected, 3 px corners at the field's ends; in high contrast, the line colour's outline and no tint.
fn field_segment(canvas: &Canvas, s: &Segment, state: FieldState, theme: &Theme, u: f64) {
    let rr = segment_shape(s, u);
    let a = &theme.accent;
    if let Scheme::HighContrast(_) = theme.scheme {
        let width = match state {
            FieldState::Rest | FieldState::Hover => u,
            FieldState::Selected => 1.5 * u,
            FieldState::Unmapped => u,
        };
        canvas.draw_rrect(rr, &stroke(theme.tokens.line, width));
        return;
    }
    if state == FieldState::Unmapped {
        let t = &theme.tokens;
        canvas.draw_rrect(rr, &paint(t.ink3.with_a(36)));
        canvas.draw_rrect(rr, &stroke(t.ink3, u));
        let mid = ((s.rect[1] + s.rect[3]) / 2.0) as f32;
        canvas.draw_line((s.rect[0] as f32, mid), (s.rect[2] as f32, mid), &stroke(t.ink2, u));
        return;
    }
    let (fill, outline) = match state {
        FieldState::Rest | FieldState::Unmapped => (a.soft, None),
        FieldState::Hover => (a.soft, Some(u)),
        FieldState::Selected => (a.mid, Some(1.5 * u)),
    };
    canvas.draw_rrect(rr, &paint(fill));
    if let Some(w) = outline {
        canvas.draw_rrect(rr, &stroke(a.solid, w));
    }
}
