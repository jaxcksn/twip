//! Preview paragraphs

use std::rc::Rc;

use crate::docx::styles::StyleKind;
use crate::docx::write::props::esc;
use crate::docx::{Block, Inline, Paragraph, RunStyle, next_para_id};

use super::Editor;
use crate::docx::run_xml;

/// The style dialog's preview texts.
const PREVIOUS: &str = "Previous Paragraph ";
const SAMPLE: &str = "Sample Text ";
const FOLLOWING: &str = "Following Paragraph ";
/// A gallery tile's text.
const TILE: &str = "AaBbCcDd";

fn paragraph(style: Rc<crate::docx::ParaStyle>, run: Rc<RunStyle>, text: String) -> Block {
    Block::Para(Paragraph {
        style,
        mark: run.clone(),
        list: None,
        inlines: vec![Inline::Text(text, run)],
        src: None,
        id: next_para_id(),
        div: None,
        controls: Vec::new(),
    })
}

/// The table a table style's preview shows.
const TABLE_SAMPLE: [[&str; 5]; 5] = [
    ["", "Jan", "Feb", "Mar", "Total"],
    ["East", "7", "7", "5", "19"],
    ["West", "6", "4", "7", "17"],
    ["South", "8", "7", "9", "24"],
    ["Total", "21", "18", "21", "60"],
];

impl Editor {
    /// A table style's preview: a 5 by 5 sample table in the style, with every part on.
    pub fn table_style_preview(&self, raw: &str) -> Option<Vec<Block>> {
        use crate::docx::model::{Cell, Row, Table, VMerge};
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        next.styles.set_xml(raw.to_string()).ok()?;
        let id = next.styles.by_id.values().find(|s| s.raw == raw)?.id.clone();
        let cx = next.styles.cx();
        let look = crate::docx::props::Look { first_row: true, last_row: true, first_col: true, last_col: true, no_h_band: false, no_v_band: false };
        let whole = next.styles.table_region(Some(&id), &[]);
        let bands = (whole.tbl.row_band.unwrap_or(1).max(1), whole.tbl.col_band.unwrap_or(1).max(1));
        let width = 54.0;
        let rows = TABLE_SAMPLE
            .iter()
            .enumerate()
            .map(|(ri, texts)| Row {
                height: None,
                cant_split: false,
                header: ri == 0,
                grid_before: 0,
                grid_after: 0,
                w_before: None,
                w_after: None,
                ind: None,
                jc: None,
                div: None,
                borders: None,
                shading: None,
                hatch: None,
                table_shading: None,
                hidden: false,
                vanished: false,
                cell_spacing: None,
                nudge: None,
                run: None,
                cells: texts
                    .iter()
                    .enumerate()
                    .map(|(ci, t)| {
                        let region = next.cell_region(Some(&id), look, bands, (ri, ci), (5, 5), 1);
                        let para = next.para_from_xml(None, Some(&region), &cx);
                        let run = run_xml::plain(&next, &para.base);
                        let tc = &region.tc;
                        Cell {
                            width: Some(crate::docx::props::Width::Dxa(width)),
                            span: 1,
                            v_merge: VMerge::None,
                            top: tc.borders.top.clone(),
                            left: tc.borders.left.clone(),
                            bottom: tc.borders.bottom.clone(),
                            right: tc.borders.right.clone(),
                            shading: tc.shading.flatten(),
                            hatch: tc.shd.as_ref().and_then(crate::docx::props::Shd::hatch),
                            margins: [0.0, 5.4, 0.0, 5.4],
                            v_align: tc.v_align.unwrap_or(crate::docx::VerticalAlignment::Top),
                            text_direction: "lrTb".into(),
                            no_wrap: false,
                            fit_text: false,
                            hide_mark: false,
                            diagonals: None,
                            direct: Default::default(),
                            blocks: vec![paragraph(Rc::new(para.style), run, t.to_string())],
                            controls: Vec::new(),
                            ids: None,
                            rev: Default::default(),
                        }
                    })
                    .collect(),
                src: None,
                direct: Default::default(),
                exceptions: Default::default(),
                controls: Vec::new(),
                rev: Default::default(),
            })
            .collect();
        let b = &whole.tbl.borders;
        let table = Table {
            style_id: Some(id),
            width: crate::docx::props::Width::Auto,
            jc: crate::docx::props::Jc::Left,
            own_jc: None,
            ind: 0.0,
            ind_pct: None,
            ind_set: true,
            borders: [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()],
            fixed: true,
            cell_spacing: 0.0,
            shd: whole.tbl.shd.clone(),
            look: Some(look),
            bands,
            alt: None,
            floating: None,
            bidi_visual: false,
            grid: vec![width; 5],
            rows,
            src: None,
            id: next_para_id(),
            direct: Default::default(),
            controls: Vec::new(),
            change: None,
            grid_change: None,
        };
        let plain = next.para_from_xml(None, None, &cx);
        let base = run_xml::plain(&next, &plain.base);
        // A table is followed by a paragraph.
        Some(vec![Block::Table(table), paragraph(Rc::new(plain.style), base, String::new())])
    }

    /// A character in a font, for a symbol picker's tile.
    pub fn character_preview(&self, family: &str, c: char) -> Option<Vec<Block>> {
        let res = self.doc.resolver.as_ref()?;
        let cx = res.styles.cx();
        let para = res.para_from_xml(None, None, &cx);
        let base = run_xml::plain(res, &para.base);
        let run = Rc::new(RunStyle { ascii: family.to_string(), h_ansi: family.to_string(), cs: family.to_string(), ..(*base).clone() });
        Some(vec![paragraph(Rc::new(para.style), run, c.to_string())])
    }

    /// What a style's preview shows: its gallery tile (`tile`), or the style dialog's preview box: a greyed previous
    /// paragraph, text in the style, a greyed following paragraph.
    pub fn style_preview(&self, raw: &str, tile: bool) -> Option<Vec<Block>> {
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        next.styles.set_xml(raw.to_string()).ok()?;
        let style = next.styles.by_id.values().find(|s| s.raw == raw)?;
        let cx = next.styles.cx();
        let w = &cx.w;
        let id = esc(&style.id);
        let (ppr, direct) = match style.kind {
            StyleKind::Character => {
                let mut e = crate::format::CharEdit::new(crate::format::CharProps { style: Some(style.id.as_str().into()), ..Default::default() });
                e.whole(crate::format::Prop::Style);
                (None, crate::docx::char_props::RunDirect::default().edit(e))
            }
            _ => (Some(format!("<{w}pPr><{w}pStyle {w}val=\"{id}\"/></{w}pPr>")), Default::default()),
        };
        let para = next.para_from_xml(ppr.as_deref(), None, &cx);
        let run = run_xml::run_of(&next, &para.base, direct);
        if tile {
            // A tile shows the character formatting on a plain paragraph.
            let plain = next.para_from_xml(None, None, &cx);
            return Some(vec![paragraph(Rc::new(plain.style), run, TILE.to_string())]);
        }
        let around = next.para_from_xml(None, None, &cx);
        let grey = {
            let base = run_xml::plain(&next, &around.base);
            Rc::new(RunStyle { color: Some(0x80_8080), ..(*base).clone() })
        };
        let around = Rc::new(around.style);
        Some(vec![
            paragraph(around.clone(), grey.clone(), PREVIOUS.repeat(8).trim_end().to_string()),
            paragraph(Rc::new(para.style), run, SAMPLE.repeat(12).trim_end().to_string()),
            paragraph(around, grey, FOLLOWING.repeat(8).trim_end().to_string()),
        ])
    }
}
