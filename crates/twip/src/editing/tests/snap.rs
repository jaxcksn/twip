//! Object drags: moves snap to the page's lines and show guides, margin distances and the grid; resizing and turning
//! show the original box and a chip with the new size or angle.

use super::super::Editor;
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::view::{Axis, Measure, Outline, Px, Tone};
use super::fonts;
use crate::api::services::{MeasureDetail, ObjectGrid, RulerUnit};
use crate::layout::FloatId;
use crate::layout::tests::shapes::{anchor, bytes, sp};

/// A letter page (inch margins: x 72 to 540) with a two-column table at the top and a 100 x 60 pt rectangle at
/// (200, 500), the grid off unless a test turns it on.
fn page() -> Editor {
    let cell = |w: u32| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p/></w:tc>"#);
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#,
        cell(2000),
        cell(2000)
    );
    let rect = sp(0, 0, 100, 60, "rect", "", "0000FF");
    let body = format!("{table}<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(200, 500, 100, 60, 1, false, 1, &rect));
    let mut e = Editor::open(&bytes(&body).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    e.view_settings.object_grid = ObjectGrid::Off;
    e
}

fn float(e: &Editor) -> FloatId {
    e.document().sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                crate::docx::Inline::Float(f, _) => Some(FloatId(f.id)),
                _ => None,
            }),
            _ => None,
        })
        .expect("a float")
}

/// The float's box as laid out on page 0.
fn laid_out(e: &mut Editor) -> [f64; 4] {
    let f = float(e);
    e.float_box(f, fonts()).expect("drawn").1
}

/// Page 0's point in document coordinates.
fn doc(e: &Editor, p: [f64; 2]) -> (f64, f64) {
    e.stack().to_doc(0, p[0], p[1]).expect("on the stack")
}

/// Presses at `from` (page 0) and moves by `by` in 8 steps with `mods`, not released.
fn press_and_move(e: &mut Editor, from: [f64; 2], by: [f64; 2], mods: Mods) {
    let (x, y) = doc(e, from);
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    for k in 1..=8 {
        let t = f64::from(k) / 8.0;
        e.pointer_input(PointerKind::Move, x + by[0] * t, y + by[1] * t, 0, mods, fonts());
    }
}

fn release(e: &mut Editor) {
    e.pointer_input(PointerKind::Up, 0.0, 0.0, 0, Mods::default(), fonts());
    e.complete(fonts());
}

/// The float selected, then moved by `by` from its middle.
fn move_float(e: &mut Editor, by: [f64; 2], mods: Mods) {
    let [x0, y0, x1, y1] = laid_out(e);
    let mid = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
    let (x, y) = doc(e, mid);
    e.pointer_input(PointerKind::Down, x, y, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, x, y, 0, Mods::default(), fonts());
    press_and_move(e, mid, by, mods);
}

const ALT: Mods = Mods { ctrl: false, shift: false, alt: true, meta: false };

#[test]
fn a_move_lands_exactly_on_the_margin_with_a_guide_and_alt_moves_freely() {
    // The left edge dragged to 5 px (3.75 pt) right of the left margin.
    let mut e = page();
    move_float(&mut e, [72.0 + 3.75 - 200.0, 0.0], Mods::default());
    let v = e.view(0..1);
    let lines: Vec<_> = v.guides.iter().flat_map(|(_, g)| g.lines.iter().map(|l| (l.axis, l.at.0))).collect();
    assert_eq!(lines, [(Axis::X, 72.0)], "a guide on the margin");
    release(&mut e);
    assert!((laid_out(&mut e)[0] - 72.0).abs() < 1e-6, "on the margin: {:?}", laid_out(&mut e));
    // With Alt: where the pointer put it, no guide.
    let mut e = page();
    move_float(&mut e, [72.0 + 3.75 - 200.0, 0.0], ALT);
    assert!(e.view(0..1).guides.iter().all(|(_, g)| g.lines.is_empty()), "no guide with Alt");
    release(&mut e);
    assert!((laid_out(&mut e)[0] - 75.75).abs() < 1e-6, "not snapped: {:?}", laid_out(&mut e));
}

#[test]
fn a_move_snaps_its_centre_to_the_page_centre_and_its_edge_to_a_table_edge() {
    // The centre 4 pt right of the page's centre (306).
    let mut e = page();
    move_float(&mut e, [306.0 + 4.0 - 250.0, 0.0], Mods::default());
    release(&mut e);
    assert!((laid_out(&mut e)[0] - 256.0).abs() < 1e-6, "{:?}", laid_out(&mut e));
    // The right edge 5 px left of the table's right edge.
    let mut e = page();
    let right = e.pages.page(0, &e.index).expect("page 0").rows.iter().map(|r| r.rect[2]).fold(f64::MIN, f64::max);
    move_float(&mut e, [right - 3.75 - 300.0, 0.0], Mods::default());
    let v = e.view(0..1);
    assert!(v.guides.iter().any(|(_, g)| g.lines.iter().any(|l| l.axis == Axis::X && l.at.0 == right)), "a guide on the table's edge");
    release(&mut e);
    assert!((laid_out(&mut e)[2] - right).abs() < 1e-6, "{:?} vs {right}", laid_out(&mut e));
}

#[test]
fn shift_keeps_a_move_to_one_axis_before_it_snaps() {
    // Mostly left, a little down: the vertical move is dropped, and only x snaps (to the margin).
    let mut e = page();
    move_float(&mut e, [72.0 + 3.0 - 200.0, 20.0], Mods { shift: true, ..Mods::default() });
    release(&mut e);
    let b = laid_out(&mut e);
    assert!((b[0] - 72.0).abs() < 1e-6 && (b[1] - 500.0).abs() < 1e-6, "{b:?}");
}

/// The margin distances shown while the float is moved by `by` with `mods` at `detail`: their labels and tones.
fn readouts(detail: MeasureDetail, by: f64, mods: Mods) -> Vec<(String, Tone)> {
    let mut e = page();
    e.view_settings.detail = detail;
    move_float(&mut e, [by, 0.0], mods);
    e.view(0..1)
        .measures
        .into_iter()
        .filter_map(|(_, m)| match m {
            Measure::Dimension { label, tone, .. } => Some((label, tone)),
            _ => None,
        })
        .collect()
}

#[test]
fn margin_distances_show_in_accent_when_centred() {
    // Centred: 184 pt (2.56 in) each side.
    let centred = vec![("2.56".to_string(), Tone::Accent), ("2.56".to_string(), Tone::Accent)];
    assert_eq!(readouts(MeasureDetail::Standard, 56.0, ALT), centred);
    assert_eq!(readouts(MeasureDetail::Full, 56.0, Mods::default()), centred);
    assert_eq!(readouts(MeasureDetail::Quiet, 56.0, ALT), []);
    // Off centre (x 220: 148 pt and 220 pt): only at the full level (or with Alt held), muted.
    let off = vec![("2.06".to_string(), Tone::Muted), ("3.06".to_string(), Tone::Muted)];
    assert_eq!(readouts(MeasureDetail::Full, 20.0, ALT), off);
    assert_eq!(readouts(MeasureDetail::Standard, 20.0, ALT), off, "Alt: the full set");
    assert_eq!(readouts(MeasureDetail::Standard, 20.0, Mods::default()), []);
}

#[test]
fn the_object_grid_shows_by_its_setting_and_a_move_snaps_to_it() {
    let grid_shown = |e: &mut Editor| e.view(0..1).guides.iter().any(|(_, g)| g.grid.is_some());
    // While dragging: none at rest, then a quarter inch grid from the margins while moved.
    let mut e = page();
    e.view_settings.object_grid = ObjectGrid::WhileDragging;
    assert!(!grid_shown(&mut e));
    move_float(&mut e, [-40.0, 30.0], Mods::default());
    let g = e.view(0..1).guides.iter().find_map(|(_, g)| g.grid).expect("the grid while dragged");
    assert_eq!((g.origin.map(|p| p.0), g.step.0), ([72.0, 72.0], 18.0));
    release(&mut e);
    // Its top left on the nearest grid point, where no line applies: x 160 to 72 + 5 x 18 = 162, y 530 to
    // 72 + 25 x 18 = 522.
    let b = laid_out(&mut e);
    assert!((b[0] - 162.0).abs() < 1e-6 && (b[1] - 522.0).abs() < 1e-6, "{b:?}");
    assert!(!grid_shown(&mut e), "none at rest");
    // Always: at rest too. A metric unit: a quarter centimetre.
    let mut e = page();
    (e.view_settings.object_grid, e.view_settings.unit) = (ObjectGrid::Always, RulerUnit::Centimeters);
    let g = e.view(0..1).guides.iter().find_map(|(_, g)| g.grid).expect("the grid at rest");
    assert!((g.step.0 - 72.0 / 2.54 / 4.0).abs() < 1e-9);
    // Off: never.
    let mut e = page();
    move_float(&mut e, [-40.0, 30.0], Mods::default());
    assert!(!grid_shown(&mut e));
}

#[test]
fn a_move_shows_the_dotted_line_from_the_anchor() {
    let mut e = page();
    move_float(&mut e, [40.0, 0.0], ALT);
    let v = e.view(0..1);
    let (_, [ax, ay]) = v.anchor.expect("the anchor");
    let link = v.guides.iter().find_map(|(_, g)| g.anchor_link).expect("the anchor line");
    // From past the ring (18 px left of the text, 6 px past its centre) to the object's left edge, 8 px below the line's top.
    let ring = (ax - 13.5).max(7.5);
    assert_eq!(link.map(|p| p.0), [ring + 4.5, ay + 6.0, 240.0, ay + 6.0]);
}

#[test]
fn a_resize_shows_the_original_dashed_and_a_size_chip() {
    // The bottom-right handle by (80, 57.2): 180 x 117.2 pt.
    let mut e = page();
    let f = float(&e);
    let [x0, y0, x1, y1] = laid_out(&mut e);
    move_float(&mut e, [0.0, 0.0], Mods::default());
    release(&mut e);
    assert_eq!(e.selected_floats(), [f]);
    press_and_move(&mut e, [x1, y1], [80.0, 57.2], Mods::default());
    let chip = |e: &mut Editor| {
        e.view(0..1)
            .measures
            .into_iter()
            .find_map(|(_, m)| match m {
                Measure::Chip { at, label } => Some((at, label)),
                _ => None,
            })
            .expect("a chip")
    };
    let (at, label) = chip(&mut e);
    assert_eq!(label, "2.50 × 1.63 in");
    // 14 px right of and 18 px below the pointer.
    assert!((at[0].0 - (Px::of(super::super::view::Pt(x1 + 80.0)).0 + 14.0)).abs() < 1e-6, "{at:?}");
    assert!((at[1].0 - (Px::of(super::super::view::Pt(y1 + 57.2)).0 + 18.0)).abs() < 1e-6, "{at:?}");
    assert_eq!(e.view(0..1).original, Some(Outline { page: 0, rect: [x0, y0, x1, y1], rot: 0.0 }));
    e.view_settings.unit = RulerUnit::Centimeters;
    assert_eq!(chip(&mut e).1, "6.35 × 4.13 cm");
    release(&mut e);
    assert_eq!(e.view(0..1).original, None, "gone after the release");
}

/// The shape's own rotation, degrees.
fn rotation(e: &Editor) -> f64 {
    let f = float(e);
    e.float_of(f).and_then(|fl| fl.shape.as_deref()).expect("a shape").xfrm.rot.degrees()
}

/// Selects the float and drags its rotate knob to `to` (page 0) with `mods`; the chip's label before the release.
fn turn(e: &mut Editor, to: [f64; 2], mods: Mods) -> String {
    move_float(e, [0.0, 0.0], Mods::default());
    release(e);
    let before = laid_out(e);
    let knob = e.view(0..1).shapes.first().expect("a shape frame").rotate;
    press_and_move(e, knob, [to[0] - knob[0], to[1] - knob[1]], mods);
    let v = e.view(0..1);
    assert_eq!(v.original, Some(Outline { page: 0, rect: before, rot: 0.0 }), "the original, dashed");
    let label = v
        .measures
        .into_iter()
        .find_map(|(_, m)| match m {
            Measure::Chip { label, .. } => Some(label),
            _ => None,
        })
        .expect("an angle chip");
    release(e);
    label
}

#[test]
fn a_turn_settles_on_a_right_angle_and_shift_snaps_to_fifteen_degrees() {
    // The centre is (250, 530). Towards (350, 525): 87.1°, settled to 90.
    let mut e = page();
    assert_eq!(turn(&mut e, [350.0, 525.0], Mods::default()), "90°");
    assert_eq!(rotation(&e), 90.0);
    // Towards (350, 510): 78.7°, more than 4° off: kept.
    let mut e = page();
    assert_eq!(turn(&mut e, [350.0, 510.0], Mods::default()), "79°");
    assert!((rotation(&e) - 78.690_067_525_979_8).abs() < 1e-3, "{}", rotation(&e));
    // With Shift: the nearest 15°, 75.
    let mut e = page();
    assert_eq!(turn(&mut e, [350.0, 510.0], Mods { shift: true, ..Mods::default() }), "75°");
    assert_eq!(rotation(&e), 75.0);
}

#[test]
fn angle_and_size_labels() {
    use super::super::objects::{angle_label, size_label};
    assert_eq!(angle_label(30.4), "30°");
    assert_eq!(angle_label(300.0), "-60°");
    assert_eq!(angle_label(180.0), "180°");
    assert_eq!(angle_label(-180.0), "180°");
    assert_eq!(size_label(180.0, 117.2, RulerUnit::Inches), "2.50 × 1.63 in");
    assert_eq!(size_label(180.0, 117.2, RulerUnit::Centimeters), "6.35 × 4.13 cm");
}

#[test]
fn a_floating_picture_turns_and_keeps_its_angle_when_saved() {
    let mut r = super::scenarios::Run::open("FX-OBJ.docx", "picture-turn");
    r.step(&serde_json::json!({ "click": { "float": 1 } }));
    let f = r.e.selected_floats()[0];
    assert!(matches!(r.e.float_of(f).and_then(|fl| fl.shape.as_deref()).map(|s| &s.kind), Some(crate::drawing::ShapeKind::Pic(_))), "a picture");
    let frame = r.e.view(0..3).shapes.first().cloned().expect("a rotate knob on the picture");
    let [x0, y0, x1, y1] = frame.rect;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    // 30° clockwise from straight up.
    let to = [cx + 100.0 * 30f64.to_radians().sin(), cy - 100.0 * 30f64.to_radians().cos()];
    let (a, b) = (r.e.stack().to_doc(frame.page, frame.rotate[0], frame.rotate[1]).unwrap(), r.e.stack().to_doc(frame.page, to[0], to[1]).unwrap());
    r.e.pointer_input(PointerKind::Down, a.0, a.1, 1, Mods::default(), fonts());
    for k in 1..=8 {
        let t = f64::from(k) / 8.0;
        r.e.pointer_input(PointerKind::Move, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, 0, Mods::default(), fonts());
    }
    r.e.pointer_input(PointerKind::Up, b.0, b.1, 0, Mods::default(), fonts());
    let rot = |e: &Editor, f: FloatId| e.float_of(f).and_then(|fl| fl.shape.as_deref()).map(|s| s.xfrm.rot.0);
    assert_eq!(rot(&r.e, f), Some(30 * 60000));
    let bytes = crate::docx::write::save(r.e.document()).expect("saves");
    let mut again = Editor::open(&bytes, fonts()).expect("reopens");
    again.complete(fonts());
    let turned: Vec<i32> = again
        .document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| if let crate::docx::Inline::Float(fl, _) = i { fl.shape.as_deref().map(|s| s.xfrm.rot.0) } else { None })
        .collect();
    assert!(turned.contains(&(30 * 60000)), "{turned:?}");
}
