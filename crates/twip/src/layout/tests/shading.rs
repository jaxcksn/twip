//! Shading: patterns, and table shading around spaced cells.

use super::shapes::bytes;
use crate::docx::props::Stripe;
use crate::drawing::paint::Paint;
use crate::layout::fragment::Item;

fn items(body: &str) -> Vec<Item> {
    let doc = crate::docx::read(&bytes(body).expect("a package")).expect("reads");
    let snap = crate::layout::layout(&doc, &super::fonts());
    crate::layout::traverse::paint_items(&snap.pages()[0])
}

fn hatches(items: &[Item]) -> Vec<Stripe> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::Shape(s) => match s.fill {
                Paint::Hatch { stripe, .. } => Some(stripe),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn coloured(items: &[Item], c: u32) -> usize {
    let rgb = |p: &Paint| matches!(p, Paint::Solid(q) if (u32::from(q.r) << 16 | u32::from(q.g) << 8 | u32::from(q.b)) == c);
    items
        .iter()
        .filter(|i| match i {
            Item::Rect { colour, .. } => *colour == c,
            Item::Shape(s) => rgb(&s.fill),
            _ => false,
        })
        .count()
}

fn table(tbl_pr: &str, cell_shd: &str) -> String {
    let tc = format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{cell_shd}</w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#);
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/>{tbl_pr}</w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{tc}{tc}</w:tr><w:tr>{tc}{tc}</w:tr></w:tbl><w:p/>"#
    )
}

#[test]
fn stripes_draw_hatched_and_percentages_and_nil_flat() {
    let shd = |v: &str| format!(r#"<w:shd w:val="{v}" w:color="C00000" w:fill="FFFF00"/>"#);
    let body = format!(
        r#"<w:p><w:r><w:rPr>{}</w:rPr><w:t>Run</w:t></w:r></w:p><w:p><w:pPr>{}</w:pPr><w:r><w:t>Paragraph</w:t></w:r></w:p>{}<w:p><w:pPr>{}</w:pPr><w:r><w:t>Nil</w:t></w:r></w:p><w:p><w:pPr>{}</w:pPr><w:r><w:t>Half</w:t></w:r></w:p>"#,
        shd("horzStripe"),
        shd("thinDiagCross"),
        table("", &shd("vertStripe")),
        shd("nil"),
        shd("pct50"),
    );
    let got = items(&body);
    let mut h = hatches(&got);
    h.sort_by_key(|s| *s as u8);
    assert_eq!(h, [Stripe::Horz, Stripe::Vert, Stripe::Vert, Stripe::Vert, Stripe::Vert, Stripe::ThinDiagCross]);
    assert_eq!(coloured(&got, 0xFFFF00), 0, "a stripe's fill is its tile's ground, nil draws nothing");
    assert_eq!(coloured(&got, 0xE08000), 1, "pct50: half the pattern colour over the fill, flat");
}

/// Word (compat 15) draws a table's shading, its style's and its rows' exceptions' around its cells and never under
/// them: with no cell spacing they don't show.
#[test]
fn table_shading_shows_around_spaced_cells_only() {
    let shd = r#"<w:shd w:val="clear" w:color="auto" w:fill="8EAADB"/>"#;
    assert_eq!(coloured(&items(&table(shd, "")), 0x8EAADB), 0, "unspaced: hidden under the cells");
    let spaced = format!(r#"<w:tblCellSpacing w:w="60" w:type="dxa"/>{shd}"#);
    let got = items(&table(&spaced, r#"<w:shd w:val="clear" w:color="auto" w:fill="E2EFDA"/>"#));
    let (gaps, cells) = (solid_frames(&got, 0x8EAADB), solid_frames(&got, 0xE2EFDA));
    assert_eq!(cells.len(), 4, "each cell's own shading over it");
    // Above, below, beside and between the two cells of each row: none under a cell.
    assert_eq!(gaps.len(), 2 * 5, "{gaps:?}");
    let under = |g: &[f64; 4], c: &[f64; 4]| g[0] < c[2] && c[0] < g[2] && g[1] < c[3] && c[1] < g[3];
    assert!(gaps.iter().all(|g| cells.iter().all(|c| !under(g, c))), "{gaps:?} {cells:?}");
    let striped = r#"<w:tblCellSpacing w:w="60" w:type="dxa"/><w:shd w:val="diagCross" w:color="2E75B6" w:fill="FFFF00"/>"#;
    assert_eq!(hatches(&items(&table(striped, ""))), [Stripe::DiagCross, Stripe::DiagCross]);
}

/// The rects shading draws in `c`, aliased.
fn solid_frames(items: &[Item], c: u32) -> Vec<[f64; 4]> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::Rect { rect, colour, snap: true, .. } if *colour == c => Some(*rect),
            _ => None,
        })
        .collect()
}

/// Word (compat 15): a shaded paragraph's shading covers the space to the next one when that one is shaded too, with
/// the same indents and borders, its fill or not; an unshaded or differently indented next paragraph stops it.
#[test]
fn paragraph_shading_runs_over_the_space_to_a_shaded_neighbour() {
    let p = |fill: &str, ind: u32| {
        let shd = if fill.is_empty() { String::new() } else { format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{fill}"/>"#) };
        format!(
            r#"<w:p><w:pPr>{shd}<w:spacing w:before="120" w:after="240" w:line="240" w:lineRule="exact"/><w:ind w:left="{ind}"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#
        )
    };
    let body = [p("FFD966", 0), p("A9D18E", 0), p("FFD966", 720), p("", 720)].concat();
    let got = items(&body);
    let (yellow, green) = (solid_frames(&got, 0xFFD966), solid_frames(&got, 0xA9D18E));
    assert_eq!((yellow.len(), green.len()), (2, 1), "{yellow:?} {green:?}");
    assert_eq!(yellow[0][3], green[0][1], "the first reaches the second over the space between them");
    assert_eq!(yellow[0][3] - yellow[0][1], 12.0 + 12.0, "its line and the larger of 12 pt after and 6 pt before");
    assert_eq!(green[0][3] - green[0][1], 12.0, "the third is indented: the second stops at its text");
    assert_eq!(yellow[1][3] - yellow[1][1], 12.0, "the last is unshaded: the third stops at its text");
}

/// Tables joined with nothing between them are one table whose rows each show their own table's shading.
#[test]
fn joined_tables_rows_keep_their_own_shading() {
    let one = |fill: &str| {
        let shd = if fill.is_empty() { String::new() } else { format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{fill}"/>"#) };
        let tc = r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#;
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/><w:tblCellSpacing w:w="60" w:type="dxa"/>{shd}</w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{tc}</w:tr></w:tbl>"#
        )
    };
    let body = format!("{}{}{}<w:p/>", one("8EAADB"), one("F4B183"), one(""));
    let got = items(&body);
    let doc = crate::docx::read(&bytes(&body).expect("a package")).expect("reads");
    assert!(matches!(&doc.sections[0].blocks[..], [crate::docx::Block::Table(t), _] if t.rows.len() == 3), "one table");
    let (first, second) = (solid_frames(&got, 0x8EAADB), solid_frames(&got, 0xF4B183));
    assert!(!first.is_empty() && !second.is_empty() && first.iter().all(|a| second.iter().all(|b| a[3] <= b[1])), "{first:?} {second:?}");
    let ground = |r: &[f64; 4]| r[3] - r[1];
    assert!(
        got.iter().all(|i| !matches!(i, Item::Rect { snap: true, rect, colour, .. } if ![0x8EAADB, 0xF4B183].contains(colour) && ground(rect) > 0.0)),
        "the third row has none"
    );
}
