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
    pub style_links: HashMap<(i64, i64), String>,
    /// The generic category of each family the document's font table describes, by lowercase name.
    pub categories: Rc<HashMap<String, super::font_table::GenericFamily>>,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Resolver({} styles)", self.styles.by_id.len())
    }
}

/// A paragraph's resolved properties, and what its runs resolve over.
pub struct ParaResolved {
    pub style: ParaStyle,
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
        let mut r = Resolver { styles, numbering, theme, style_links: HashMap::new(), categories: Rc::default() };
        r.compute_style_links();
        r
    }

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
        let east_asia = match (&p.east_asia, &p.east_asia_below) {
            (Some(FontRef::Theme(t)), Some(below)) if self.theme.ea_unset(t) => self.font(&Some(below.clone())),
            (e, _) => self.font(e),
        };
        let east_asia_unset = p.east_asia_below.is_none()
            && matches!(p.east_asia.as_ref().filter(|e| matches!(e, FontRef::Theme(_))).or(p.east_asia_theme.as_ref()),
                Some(FontRef::Theme(t)) if self.theme.ea_unset(t));
        let ascii_name = ascii.clone().or_else(|| h_ansi.clone()).unwrap_or_else(fallback);
        let h_ansi_name = h_ansi.clone().or_else(|| ascii.clone()).unwrap_or_else(fallback);
        let east_asia_name = east_asia.clone().unwrap_or_else(fallback);
        let category = |family: &str| self.categories.get(&family.to_lowercase()).copied();
        let categories =
            if self.categories.is_empty() { [None; 3] } else { [category(&ascii_name), category(&h_ansi_name), category(&east_asia_name)] };
        Rc::new(RunStyle {
            ascii: ascii_name,
            h_ansi: h_ansi_name,
            east_asia_named: !east_asia_unset && east_asia.as_deref().is_some_and(|n| !n.is_empty() && !n.eq_ignore_ascii_case("Times New Roman")),
            east_asia: east_asia_name,
            cs: self.font(&p.cs).unwrap_or_else(fallback),
            east_asia_unset,
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
            shd: p.shd.clone().filter(|s| s.pattern != "nil"),
            border: p.border.clone().filter(|b| b.visible()),
            vert_align: p.vert_align.unwrap_or(VertAlign::Baseline),
            position: p.position.unwrap_or(0.0),
            spacing: p.spacing.unwrap_or(0.0),
            scale: p.scale.unwrap_or(100.0),
            kern: p.kern.unwrap_or(0.0),
            ligatures: p.ligatures.unwrap_or(0) | if p.cntxt_alts == Some(true) { 16 } else { 0 },
            lang: p.lang.clone().unwrap_or_default(),
            style_id: p.style.clone(),
            direct: Raw::default(),
            rev: Default::default(),
            categories,
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
            bidi: p.bidi.unwrap_or(false),
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
            snap_to_grid: p.snap_to_grid.unwrap_or(true),
            keep_next: p.keep_next.unwrap_or(false),
            keep_lines: p.keep_lines.unwrap_or(false),
            page_break_before: p.page_break_before.unwrap_or(false),
            widow_control: p.widow_control.unwrap_or(true),
            suppress_auto_hyphens: p.suppress_auto_hyphens.unwrap_or(false),
            suppress_line_numbers: p.suppress_line_numbers.unwrap_or(false),
            div: p.div_id,
            div_boxes: Rc::from([]),
            tabs: p.resolved_tabs(),
            border_top: bdr(&p.border_top),
            border_bottom: bdr(&p.border_bottom),
            border_left: bdr(&p.border_left),
            border_right: bdr(&p.border_right),
            border_between: bdr(&p.border_between),
            shading: p.shading.flatten(),
            shd: p.shd.clone().filter(|s| s.pattern != "nil"),
            frame: p.frame.clone(),
            text_alignment: p.text_alignment.clone().unwrap_or_else(|| "auto".into()),
            outline_lvl: p.outline_lvl,
            list_jc: "left".into(),
            num: None,
            list_fmt: None,
            list: None,
            mark_size: None,
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
        if let Some(size) = region.and_then(|t| t.size_over_normal) {
            let sizer = self.styles.chain(style_id.as_deref()).into_iter().rev().find(|s| s.base.rpr.size.is_some());
            if sizer.is_some_and(|s| Some(&s.id) == self.styles.default_paragraph.as_ref() && s.base.rpr.size == Some(12.0)) {
                base.size = Some(size);
            }
        }
        if region.is_some_and(|t| t.mark_at_style) && direct.mark_size == Some(11.0) {
            style.mark_size = base.size;
        }
        ParaResolved { style, base, level, num_id, ilvl, from_style, style_id }
    }

    pub fn label_shows(&self, num_id: i64, ilvl: i64, from_style: bool, style_id: Option<&str>) -> bool {
        !from_style || {
            let a = self.numbering.abstract_of(num_id, &self.styles).unwrap_or(-1);
            self.style_links.get(&(a, ilvl)).is_none_or(|linked| {
                // A style based on the linked one that doesn't set numbering itself inherits the linked style's list.
                let chain = self.styles.chain(style_id);
                chain.iter().rposition(|s| s.id == *linked).is_some_and(|i| chain[i + 1..].iter().all(|s| s.base.ppr.num_id.is_none()))
            })
        }
    }

    /// The region of cell (`row`, `cell`) of `t` as the reader computed it (the same look default).
    pub fn region_of(&self, t: &Table, row: usize, cell: usize) -> Region {
        let look = t.look.unwrap_or(Look { first_row: true, last_row: false, first_col: true, last_col: false, no_h_band: false, no_v_band: true });
        let ncols = t.rows.get(row).map_or(0, |r| r.cells.len());
        let headers = t.rows.iter().enumerate().take_while(|(i, r)| r.header || *i == 0 && look.first_row).count();
        self.cell_region(t.style_id.as_deref(), look, t.bands, (row, cell), (t.rows.len(), ncols), headers)
    }

    /// A run resolved from its direct `w:rPr` as written (namespaces as `cx` names them) over `base`: what an edit of
    /// a run's direct formatting gives.
    pub fn run_from_xml(&self, base: &RunProps, rpr: Option<&str>, cx: &Cx) -> Run {
        let direct = rpr.and_then(|x| cx.parse(x, |_, el| RunProps::parse(Some(el), &self.styles.colours))).unwrap_or_default();
        let mut s = (*self.run_style(&self.run_props(base, &direct))).clone();
        s.direct = Raw(rpr.map(Rc::from));
        Rc::new(s)
    }

    /// `run` resolved again from its direct `w:rPr` over `base`, where it is: in a table of contents' result without the
    /// Hyperlink character style, as the reader resolved it.
    pub fn rerun(&self, base: &RunProps, run: &RunStyle, cx: &Cx) -> Run {
        let rpr = run.direct.0.as_deref();
        let mut direct = rpr.and_then(|x| cx.parse(x, |_, el| RunProps::parse(Some(el), &self.styles.colours))).unwrap_or_default();
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
        let direct = ppr.and_then(|x| cx.parse(x, |_, el| ParaProps::parse(Some(el), &self.styles.colours))).unwrap_or_default();
        let mut r = self.paragraph(&direct, region);
        r.style.direct = Raw(ppr.map(Rc::from));
        r
    }

    pub fn cell_region(
        &self,
        style_id: Option<&str>,
        look: Look,
        bands: (i64, i64),
        (ri, ci): (usize, usize),
        (nrows, ncols): (usize, usize),
        headers: usize,
    ) -> Region {
        let defines = |r: &str| self.styles.table_defines(style_id, r);
        // A look bit counts only when the style formats one of its regions; with word2002 rules, not a corner.
        let corner = |r: &str| !self.styles.word2002_rules && defines(r);
        let first_row = look.first_row && (defines("firstRow") || look.first_col && corner("nwCell") || look.last_col && corner("neCell"));
        let last_row = look.last_row && (defines("lastRow") || look.first_col && corner("swCell") || look.last_col && corner("seCell"));
        let first_col = look.first_col && (defines("firstCol") || look.first_row && corner("nwCell") || look.last_row && corner("swCell"));
        let last_col = look.last_col && (defines("lastCol") || look.first_row && corner("neCell") || look.last_row && corner("seCell"));
        // The first-row run: the leading header rows, row 0 with the firstRow bit.
        let run = if first_row || defines("firstRow") { headers.max(usize::from(first_row)) } else { 0 };
        let in_run = ri < run;
        let (last, end) = (ri + 1 == nrows, ci + 1 == ncols);
        let (row_band, col_band) = bands;
        let band = |i: i64, size: i64, one: &'static str, two: &'static str| if i >= 0 && (i / size) % 2 == 0 { one } else { two };
        let mut active: Vec<&str> = Vec::new();
        // One row region, one column region, at most one corner.
        if in_run && first_row {
            active.push("firstRow");
        } else if last && last_row {
            active.push("lastRow");
        } else if row_band > 0 && !look.no_h_band {
            active.push(band(ri as i64 - run as i64, row_band, "band1Horz", "band2Horz"));
        }
        if ci == 0 && first_col {
            active.push("firstCol");
        } else if end && last_col {
            active.push("lastCol");
        } else if col_band > 0 && !look.no_v_band {
            active.push(band(ci as i64 - i64::from(first_col), col_band, "band1Vert", "band2Vert"));
        }
        if let Some(c) = [
            (ci == 0 && in_run && first_row && first_col, "nwCell"),
            (end && in_run && first_row && last_col, "neCell"),
            (ci == 0 && last && last_row && first_col, "swCell"),
            (end && last && last_row && last_col, "seCell"),
        ]
        .into_iter()
        .find_map(|(on, c)| on.then_some(c))
        {
            active.push(c);
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
        let size = |dd: u32, normal: u32, table: &str, p_style: Option<&str>, compat15: bool| {
            let sz = |v: u32| format!(r#"<w:rPr><w:sz w:val="{v}"/></w:rPr>"#);
            let xml = format!(
                r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault>{}</w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="P0">{}</w:style><w:style w:type="paragraph" w:styleId="P1"><w:basedOn w:val="P0"/></w:style><w:style w:type="paragraph" w:styleId="P2"><w:basedOn w:val="P0"/>{}</w:style><w:style w:type="table" w:default="1" w:styleId="T0"/><w:style w:type="table" w:styleId="T1"><w:basedOn w:val="T0"/>{table}<w:tblPr/></w:style></w:styles>"#,
                sz(dd),
                sz(normal),
                sz(normal)
            );
            let mut styles = Styles::parse(Some(&xml), Default::default());
            styles.compat = if compat15 { Compat::Generic } else { Compat::Legacy };
            let res = Resolver::new(styles, Numbering::default(), ThemeFonts::default());
            let look = Look { first_row: false, last_row: false, first_col: false, last_col: false, no_h_band: true, no_v_band: true };
            let direct = ParaProps { style: p_style.map(str::to_string), ..ParaProps::default() };
            let in_table = |id| res.paragraph(&direct, Some(&res.cell_region(Some(id), look, (0, 0), (0, 0), (1, 1), 0))).base.size;
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
    fn an_11pt_cell_mark_is_laid_out_at_its_paragraphs_size_in_compat_14() {
        let mark = |dd: u32, normal: u32, own: f64, table: Option<&str>, compat15: bool| {
            let xml = format!(
                r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="{dd}"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="P0"><w:rPr><w:sz w:val="{normal}"/></w:rPr></w:style><w:style w:type="table" w:default="1" w:styleId="T0"/><w:style w:type="table" w:styleId="T1"><w:basedOn w:val="T0"/></w:style></w:styles>"#
            );
            let mut styles = Styles::parse(Some(&xml), Default::default());
            styles.compat = if compat15 { Compat::Generic } else { Compat::Legacy };
            let res = Resolver::new(styles, Numbering::default(), ThemeFonts::default());
            let look = Look { first_row: false, last_row: false, first_col: false, last_col: false, no_h_band: true, no_v_band: true };
            let direct = ParaProps { mark_size: Some(own), ..ParaProps::default() };
            res.paragraph(&direct, Some(&res.cell_region(table, look, (0, 0), (0, 0), (1, 1), 0))).style.mark_size
        };
        assert_eq!(mark(22, 24, 11.0, None, false), Some(12.0));
        assert_eq!(mark(22, 24, 11.0, Some("T0"), false), Some(12.0));
        // A named table style, compat 15, another mark size, docDefaults or Normal: the mark's own.
        assert_eq!(mark(22, 24, 11.0, Some("T1"), false), None);
        assert_eq!(mark(22, 24, 11.0, None, true), None);
        assert_eq!(mark(22, 24, 10.0, None, false), None);
        assert_eq!(mark(20, 24, 11.0, None, false), None);
        assert_eq!(mark(22, 28, 11.0, None, false), None);
    }

    #[test]
    fn a_named_east_asian_font_over_an_empty_theme_slot_leaves_it_empty() {
        let unset = |dd: &str, normal: &str| {
            let xml = format!(
                r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts {dd}/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="P0"><w:rPr><w:rFonts {normal}/></w:rPr></w:style></w:styles>"#
            );
            let res = Resolver::new(Styles::parse(Some(&xml), Default::default()), Numbering::default(), ThemeFonts::default());
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
