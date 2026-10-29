//! Style resolution (the STYLE rules): docDefaults, table style regions, paragraph and character style chains, toggles,
//! numbering levels and direct formatting, combined into the model's resolved fields. The reader resolves with it, and
//! the document keeps it (`Document::resolver`) so edits resolve exactly as reading does.

use std::collections::HashMap;
use std::rc::Rc;

use super::model::*;
use super::numbering::{Level, Numbering};
use super::props::*;
use super::styles::{Region, Styles, ThemeFonts};
use super::write::props::Cx;

#[derive(Clone)]
pub struct Resolver {
    pub styles: Styles,
    pub numbering: Numbering,
    pub theme: ThemeFonts,
    /// (abstractNum, ilvl) → the paragraph style linked to that level (STYLE-6).
    pub style_links: HashMap<(i64, i64), String>,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Resolver({} styles)", self.styles.by_id.len())
    }
}

/// A paragraph's resolved properties, and what its runs resolve over.
pub struct ParaResolved {
    pub style: ParaStyle,
    /// docDefaults → table style region → paragraph style chain: each run's base (a toggle level each, STYLE-3).
    pub base: RunProps,
    pub level: Option<Level>,
    pub num_id: Option<i64>,
    pub ilvl: i64,
    /// The list comes from the paragraph style, not direct formatting.
    pub from_style: bool,
    pub style_id: Option<String>,
}

impl Resolver {
    pub fn new(styles: Styles, numbering: Numbering, theme: ThemeFonts) -> Resolver {
        let mut r = Resolver { styles, numbering, theme, style_links: HashMap::new() };
        r.compute_style_links();
        r
    }

    /// STYLE-6: the paragraph style linked to each list level.
    fn compute_style_links(&mut self) {
        for (a, abs) in &self.numbering.abstracts {
            for (i, l) in abs.levels.iter().enumerate() {
                if let Some(ps) = l.as_ref().and_then(|l| l.pstyle.clone()) {
                    self.style_links.insert((*a, i as i64), ps);
                }
            }
        }
        for id in &self.styles.paragraph_order {
            let ppr = &self.styles.by_id[id].base.ppr;
            let Some(num) = ppr.num_id.filter(|n| *n != 0) else { continue };
            let Some(a) = self.numbering.abstract_of(num, &self.styles) else { continue };
            let ilvl = ppr.ilvl.unwrap_or(0);
            self.style_links.entry((a, ilvl)).or_insert_with(|| id.clone());
        }
    }

    pub fn font(&self, f: &Option<FontRef>) -> Option<String> {
        match f {
            Some(FontRef::Theme(t)) => Some(self.theme.resolve(t)),
            Some(FontRef::Name(n)) => Some(n.clone()),
            None => None,
        }
    }

    pub fn run_style(&self, p: &RunProps) -> Run {
        let ascii = self.font(&p.ascii);
        let h_ansi = self.font(&p.h_ansi);
        let fallback = || ascii.clone().or_else(|| h_ansi.clone()).unwrap_or_else(|| self.theme.minor.clone());
        Rc::new(RunStyle {
            ascii: ascii.clone().or_else(|| h_ansi.clone()).unwrap_or_else(fallback),
            h_ansi: h_ansi.clone().or_else(|| ascii.clone()).unwrap_or_else(fallback),
            east_asia: match (&p.east_asia, &p.east_asia_below) {
                (Some(FontRef::Theme(t)), Some(below)) if self.theme.ea_unset(t) => self.font(&Some(below.clone())),
                (e, _) => self.font(e),
            }
            .unwrap_or_else(fallback),
            cs: self.font(&p.cs).unwrap_or_else(fallback),
            // STYLE-8: a named font over an inherited empty slot still leaves the slot empty.
            east_asia_unset: p.east_asia_below.is_none()
                && matches!(p.east_asia.as_ref().filter(|e| matches!(e, FontRef::Theme(_))).or(p.east_asia_theme.as_ref()),
                    Some(FontRef::Theme(t)) if self.theme.ea_unset(t)),
            hint_east_asia: p.hint.as_deref() == Some("eastAsia"),
            size: p.size.unwrap_or(10.0),
            bold: p.toggle(toggle::B),
            italic: p.toggle(toggle::I),
            caps: p.toggle(toggle::CAPS),
            small_caps: p.toggle(toggle::SMALL_CAPS),
            strike: p.toggle(toggle::STRIKE),
            dstrike: p.dstrike.unwrap_or(false),
            hidden: p.toggle(toggle::VANISH),
            underline: p.underline.clone().filter(|u| u != "none"),
            color: p.color.flatten(),
            highlight: p.highlight.flatten(),
            em: p.em.clone().filter(|e| e != "none"),
            toc: false,
            shading: p.shading.flatten(),
            vert_align: p.vert_align.unwrap_or(VertAlign::Baseline),
            position: p.position.unwrap_or(0.0),
            spacing: p.spacing.unwrap_or(0.0),
            scale: p.scale.unwrap_or(100.0),
            kern: p.kern.unwrap_or(0.0),
            ligatures: p.ligatures.unwrap_or(0),
            lang: p.lang.clone().unwrap_or_default(),
            style_id: p.style.clone(),
            direct: Raw::default(),
            rev: Default::default(),
        })
    }

    pub fn para_style(&self, p: &ParaProps) -> ParaStyle {
        let line_rule = p.line_rule.unwrap_or(LineRule::Auto);
        let line = match (p.line, line_rule) {
            (Some(l), LineRule::Auto) => l / 240.0,
            (Some(l), _) => l / 20.0,
            (None, _) => 1.0,
        };
        let bdr = |b: &Option<Border>| b.clone().filter(|b| b.visible());
        ParaStyle {
            style_id: p.style.clone(),
            jc: p.jc.unwrap_or(Jc::Left),
            ind_left: p.ind_left.unwrap_or(0.0),
            ind_right: p.ind_right.unwrap_or(0.0),
            ind_first: p.ind_first.unwrap_or(0.0),
            space_before: p.space_before.unwrap_or(0.0),
            space_after: p.space_after.unwrap_or(0.0),
            before_auto: p.before_auto.unwrap_or(false),
            after_auto: p.after_auto.unwrap_or(false),
            line,
            line_rule: if p.line.is_some() { line_rule } else { LineRule::Auto },
            contextual: p.contextual.unwrap_or(false),
            keep_next: p.keep_next.unwrap_or(false),
            keep_lines: p.keep_lines.unwrap_or(false),
            page_break_before: p.page_break_before.unwrap_or(false),
            // Absent = on: a document with no widowControl anywhere keeps widow control (PAGE-3).
            widow_control: p.widow_control.unwrap_or(true),
            suppress_auto_hyphens: p.suppress_auto_hyphens.unwrap_or(false),
            suppress_line_numbers: p.suppress_line_numbers.unwrap_or(false),
            tabs: p.resolved_tabs(),
            border_top: bdr(&p.border_top),
            border_bottom: bdr(&p.border_bottom),
            border_left: bdr(&p.border_left),
            border_right: bdr(&p.border_right),
            border_between: bdr(&p.border_between),
            shading: p.shading.flatten(),
            frame: p.frame.clone(),
            text_alignment: p.text_alignment.clone().unwrap_or_else(|| "auto".into()),
            outline_lvl: p.outline_lvl,
            list_jc: "left".into(),
            num: None,
            list_fmt: None,
            list: None,
            direct: Raw::default(),
            change: None,
        }
    }

    /// A run's properties over a base: its character style chain (a toggle level), then direct formatting.
    pub fn run_props(&self, base: &RunProps, direct: &RunProps) -> RunProps {
        let mut p = base.clone();
        if let Some(cs) = direct.style.as_deref() {
            p.merge_level(&self.styles.chain_rpr(Some(cs)));
        }
        p.merge(direct);
        p
    }

    /// A paragraph's properties (STYLE-6: which list applies, and whether its indent sits above or below the style's),
    /// in a table cell's region when it is in one.
    pub fn paragraph(&self, direct: &ParaProps, region: Option<&Region>) -> ParaResolved {
        let style_id = self.styles.paragraph_style(direct.style.as_deref()).map(str::to_string);
        let style_ppr = self.styles.chain_ppr(style_id.as_deref());
        let (num_id, ilvl, from_style) = match direct.num_id {
            Some(id) => (Some(id), direct.ilvl.or(style_ppr.ilvl).unwrap_or(0), false),
            None => match style_ppr.num_id {
                Some(id) => (Some(id), direct.ilvl.or(style_ppr.ilvl).unwrap_or(0), true),
                None => (None, 0, false),
            },
        };
        let num_id = num_id.filter(|&id| id != 0);
        let level = num_id.and_then(|id| self.numbering.level(id, ilvl, &self.styles)).cloned();

        let mut ppr = self.styles.doc_ppr.clone();
        if let Some(t) = region {
            ppr.merge(&t.ppr);
        }
        if let (Some(l), true) = (&level, from_style) {
            ppr.merge(&l.ppr);
            ppr.merge(&style_ppr);
        } else {
            ppr.merge(&style_ppr);
            if let Some(l) = &level {
                ppr.merge(&l.ppr);
            }
        }
        ppr.merge(direct);
        ppr.style = style_id.clone();
        ppr.num_id = num_id;
        let mut style = self.para_style(&ppr);
        if let Some(l) = &level {
            style.list_jc = l.jc.clone();
            style.num = num_id.map(|id| (id, ilvl));
            style.list_fmt = Some(l.fmt.clone());
            style.list = num_id.and_then(|id| self.numbering.abstract_of(id, &self.styles));
        }

        let mut base = self.styles.doc_rpr.clone();
        if let Some(t) = region {
            base.merge_level(&t.rpr);
        }
        let style_rpr = self.styles.chain_rpr(style_id.as_deref());
        base.merge_level(&style_rpr);
        // STYLE-4: a 12pt size that comes from the default paragraph style itself gives way to the table style's.
        if let Some(size) = region.and_then(|t| t.size_over_normal) {
            let sizer = self.styles.chain(style_id.as_deref()).into_iter().rev().find(|s| s.base.rpr.size.is_some());
            if sizer.is_some_and(|s| Some(&s.id) == self.styles.default_paragraph.as_ref() && s.base.rpr.size == Some(12.0)) {
                base.size = Some(size);
            }
        }
        ParaResolved { style, base, level, num_id, ilvl, from_style, style_id }
    }

    /// Whether a paragraph in list `num_id` shows its label: always for direct numbering; for a style's, only in the
    /// style linked to that level (STYLE-6).
    pub fn label_shows(&self, num_id: i64, ilvl: i64, from_style: bool, style_id: Option<&str>) -> bool {
        !from_style || {
            let a = self.numbering.abstract_of(num_id, &self.styles).unwrap_or(-1);
            self.style_links.get(&(a, ilvl)).map(|s| Some(s.as_str()) == style_id).unwrap_or(true)
        }
    }

    /// The region of cell (`row`, `cell`) of `t` as the reader computed it (the same look default).
    pub fn region_of(&self, t: &Table, row: usize, cell: usize) -> Region {
        let look = t.look.unwrap_or(Look { first_row: true, last_row: false, first_col: true, last_col: false, no_h_band: false, no_v_band: true });
        let ncols = t.rows.get(row).map_or(0, |r| r.cells.len());
        self.cell_region(t.style_id.as_deref(), look, t.bands, (row, cell), (t.rows.len(), ncols))
    }

    /// A run resolved from its direct `w:rPr` as written (namespaces as `cx` names them) over `base`: what an edit of
    /// a run's direct formatting gives.
    pub fn run_from_xml(&self, base: &RunProps, rpr: Option<&str>, cx: &Cx) -> Run {
        let direct = rpr.and_then(|x| cx.parse(x, |_, el| RunProps::parse(Some(el)))).unwrap_or_default();
        let mut s = (*self.run_style(&self.run_props(base, &direct))).clone();
        s.direct = Raw(rpr.map(Rc::from));
        Rc::new(s)
    }

    /// `run` resolved again from its direct `w:rPr` over `base`, where it is: in a table of contents' result without the
    /// Hyperlink character style, as the reader resolved it.
    pub fn rerun(&self, base: &RunProps, run: &RunStyle, cx: &Cx) -> Run {
        let rpr = run.direct.0.as_deref();
        let mut direct = rpr.and_then(|x| cx.parse(x, |_, el| RunProps::parse(Some(el)))).unwrap_or_default();
        if run.toc && direct.style.as_deref() == Some("Hyperlink") {
            direct.style = None;
        }
        let mut s = (*self.run_style(&self.run_props(base, &direct))).clone();
        s.direct = Raw(rpr.map(Rc::from));
        s.toc = run.toc;
        Rc::new(s)
    }

    /// A paragraph resolved from its direct `w:pPr` as written, in `region` when it is in a cell.
    pub fn para_from_xml(&self, ppr: Option<&str>, region: Option<&Region>, cx: &Cx) -> ParaResolved {
        let direct = ppr.and_then(|x| cx.parse(x, |_, el| ParaProps::parse(Some(el)))).unwrap_or_default();
        let mut r = self.paragraph(&direct, region);
        r.style.direct = Raw(ppr.map(Rc::from));
        r
    }

    /// The table-style region of cell (`ri`, `ci`) of a table with `size` (rows, cells in its row): the conditional
    /// formats `look` turns on, banding included (STYLE-4).
    pub fn cell_region(
        &self,
        style_id: Option<&str>,
        look: Look,
        bands: (i64, i64),
        (ri, ci): (usize, usize),
        (nrows, ncols): (usize, usize),
    ) -> Region {
        let defines = |r: &str| self.styles.table_defines(style_id, r);
        let first_row = look.first_row && defines("firstRow");
        let last_row = look.last_row && defines("lastRow");
        let first_col = look.first_col && defines("firstCol");
        let last_col = look.last_col && defines("lastCol");
        let (row_band, col_band) = bands;
        let is_first_row = ri == 0 && first_row;
        let is_last_row = ri + 1 == nrows && last_row;
        let is_first_col = ci == 0 && first_col;
        let is_last_col = ci + 1 == ncols && last_col;
        let mut active: Vec<&str> = Vec::new();
        if row_band > 0 && !look.no_h_band && !is_first_row && !is_last_row {
            let r = ri as i64 - first_row as i64;
            active.push(if (r / row_band) % 2 == 0 { "band1Horz" } else { "band2Horz" });
        }
        if col_band > 0 && !look.no_v_band && !is_first_col && !is_last_col {
            let c = ci as i64 - first_col as i64;
            active.push(if (c / col_band) % 2 == 0 { "band1Vert" } else { "band2Vert" });
        }
        if is_first_col {
            active.push("firstCol");
        }
        if is_last_col {
            active.push("lastCol");
        }
        if is_first_row {
            active.push("firstRow");
        }
        if is_last_row {
            active.push("lastRow");
        }
        if ri == 0 && ci == 0 && look.first_row && look.first_col {
            active.push("nwCell");
        }
        if ri == 0 && ci + 1 == ncols && look.first_row && look.last_col {
            active.push("neCell");
        }
        if ri + 1 == nrows && ci == 0 && look.last_row && look.first_col {
            active.push("swCell");
        }
        if ri + 1 == nrows && ci + 1 == ncols && look.last_row && look.last_col {
            active.push("seCell");
        }
        self.styles.table_region(style_id, &active)
    }
}

#[cfg(test)]
mod tests {
    //! Re-resolving every body paragraph, its mark and its text runs from their direct XML gives what the reader
    //! resolved, on every repository fixture and the corpus.

    use std::path::{Path, PathBuf};

    use super::*;

    fn docs() -> Vec<PathBuf> {
        fn walk(d: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "docx") {
                    out.push(p);
                }
            }
        }
        let mut out = Vec::new();
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"), &mut out);
        out
    }

    fn check(res: &Resolver, cx: &Cx, blocks: &[Block], region: Option<&Region>, what: &str, checked: &mut usize) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    // A paragraph joined to the one before by a hidden mark (STYLE-7) holds runs resolved in that paragraph.
                    if p.src.as_ref().is_some_and(|s| s.ranges.len() > 1) {
                        continue;
                    }
                    // Revisions aren't formatting: they come from the elements around the direct XML, so they're
                    // carried over before comparing.
                    let mut r = res.para_from_xml(p.style.direct.0.as_deref(), region, cx);
                    r.style.change = p.style.change.clone();
                    assert_eq!(*p.style, r.style, "{what}: paragraph {:?}", p.id);
                    let with_rev = |again: Run, read: &Run| RunStyle { rev: read.rev.clone(), ..(*again).clone() };
                    let mark = with_rev(res.run_from_xml(&r.base, p.mark.direct.0.as_deref(), cx), &p.mark);
                    assert_eq!(*p.mark, mark, "{what}: the mark of {:?}", p.id);
                    for i in &p.inlines {
                        if let Inline::Text(t, run) = i {
                            assert_eq!(**run, with_rev(res.rerun(&r.base, run, cx), run), "{what}: run {t:?} of {:?}", p.id);
                            *checked += 1;
                        }
                    }
                }
                Block::Table(t) => {
                    for (ri, row) in t.rows.iter().enumerate() {
                        for (ci, c) in row.cells.iter().enumerate() {
                            let region = res.region_of(t, ri, ci);
                            check(res, cx, &c.blocks, Some(&region), what, checked);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn re_resolving_gives_what_reading_resolved() {
        let mut checked = 0;
        for f in docs() {
            let Ok(doc) = super::super::read_editable(&std::fs::read(&f).expect("readable")) else { continue };
            let (Some(res), Some(src)) = (&doc.resolver, &doc.source) else { panic!("{}: no resolver", f.display()) };
            let cx = Cx::of_xml(&src.xml);
            for s in &doc.sections {
                check(res, &cx, &s.blocks, None, &f.display().to_string(), &mut checked);
            }
        }
        assert!(checked > 10_000, "only {checked} runs checked");
    }

    #[test]
    fn a_12pt_normal_gives_way_to_a_table_style_that_sets_text_properties() {
        // STYLE-4: (docDefaults, Normal) sizes in half-points, the table style T1's contents, the cell paragraph's
        // style, compat 15 → the cell text's size in a T1 table and in a table with the default table style.
        let size = |dd: u32, normal: u32, table: &str, p_style: Option<&str>, compat15: bool| {
            let sz = |v: u32| format!(r#"<w:rPr><w:sz w:val="{v}"/></w:rPr>"#);
            let xml = format!(
                r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault>{}</w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="P0">{}</w:style><w:style w:type="paragraph" w:styleId="P1"><w:basedOn w:val="P0"/></w:style><w:style w:type="paragraph" w:styleId="P2"><w:basedOn w:val="P0"/>{}</w:style><w:style w:type="table" w:default="1" w:styleId="T0"/><w:style w:type="table" w:styleId="T1"><w:basedOn w:val="T0"/>{table}<w:tblPr/></w:style></w:styles>"#,
                sz(dd),
                sz(normal),
                sz(normal)
            );
            let mut styles = Styles::parse(Some(&xml));
            styles.compat15 = compat15;
            let res = Resolver::new(styles, Numbering::default(), ThemeFonts::default());
            let look = Look { first_row: false, last_row: false, first_col: false, last_col: false, no_h_band: true, no_v_band: true };
            let direct = ParaProps { style: p_style.map(str::to_string), ..ParaProps::default() };
            let in_table = |id| res.paragraph(&direct, Some(&res.cell_region(Some(id), look, (0, 0), (0, 0), (1, 1)))).base.size;
            (in_table("T1"), in_table("T0"))
        };
        let fonts = r#"<w:rPr><w:rFonts w:ascii="Calibri"/></w:rPr>"#;
        assert_eq!(size(22, 24, fonts, None, false), (Some(11.0), Some(12.0)));
        assert_eq!(size(22, 24, fonts, Some("P1"), false), (Some(11.0), Some(12.0)));
        assert_eq!(size(22, 24, r#"<w:rPr><w:sz w:val="28"/></w:rPr>"#, None, false), (Some(14.0), Some(12.0)));
        // A table style without paragraph or run properties, an empty rPr, a resolved 10pt, a style that sets 12pt
        // itself, an 11pt Normal, compat 15: Normal's size stays.
        assert_eq!(size(22, 24, "", None, false), (Some(12.0), Some(12.0)));
        assert_eq!(size(22, 24, "<w:rPr/>", None, false), (Some(12.0), Some(12.0)));
        assert_eq!(size(20, 24, fonts, None, false), (Some(12.0), Some(12.0)));
        assert_eq!(size(22, 24, r#"<w:rPr><w:sz w:val="20"/></w:rPr>"#, None, false), (Some(12.0), Some(12.0)));
        assert_eq!(size(22, 24, fonts, Some("P2"), false), (Some(12.0), Some(12.0)));
        assert_eq!(size(24, 22, fonts, None, false), (Some(11.0), Some(11.0)));
        assert_eq!(size(22, 24, fonts, None, true), (Some(12.0), Some(12.0)));
    }

    #[test]
    fn a_named_east_asian_font_over_an_empty_theme_slot_leaves_it_empty() {
        // STYLE-8: (docDefaults rFonts, Normal's rFonts) under a run naming eastAsia="Times New Roman", with an empty
        // minor East Asian theme font → whether the run's East Asian slot is the empty one.
        let unset = |dd: &str, normal: &str| {
            let xml = format!(
                r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts {dd}/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="P0"><w:rPr><w:rFonts {normal}/></w:rPr></w:style></w:styles>"#
            );
            let res = Resolver::new(Styles::parse(Some(&xml)), Numbering::default(), ThemeFonts::default());
            let base = res.paragraph(&ParaProps::default(), None).base;
            let direct = RunProps { east_asia: Some(FontRef::Name("Times New Roman".into())), ..RunProps::default() };
            res.run_style(&res.run_props(&base, &direct)).east_asia_unset
        };
        assert!(unset(r#"w:eastAsiaTheme="minorEastAsia""#, ""));
        // A named font under the slot fills it, so the run's name stands.
        assert!(!unset(r#"w:eastAsia="Calibri""#, r#"w:eastAsiaTheme="minorEastAsia""#));
        assert!(!unset(r#"w:eastAsia="Calibri""#, ""));
    }
}
