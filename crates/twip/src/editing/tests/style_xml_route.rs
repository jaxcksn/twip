//! The style commands' former XML route against their typed route: over every fixture's styles, document defaults
//! and latent styles, with representative inputs, the element the typed route writes is the one patching the XML
//! wrote, byte for byte, or one that reads as the same definition. Each difference is listed with `--no-capture`.

use std::collections::BTreeMap;
use std::rc::Rc;

use super::super::Editor;
use super::super::style_defs::{char_half, created_style, latent_edited, table_style_edit};
use super::super::style_dialog::{dialog_fields, numbered, retyped, table_format_edits, table_style_edits};
use super::super::styles::{StyleTarget, defaults_edited, edit_chars, edit_paras, edit_style, meta_edited, selection_props, updated_to};
use super::fonts;
use crate::api::command::{Border, BorderEdges, BorderStyle, CellBorders, CellMargins, Command, StyleMetaChange};
use crate::api::error::Error;
use crate::api::services::{StyleDialog, StyleDialogType, StyleOpen, StyleVisibility, TableStylePart};
use crate::api::styles::{StyleChange, StyleList, TablePartFormatting};
use crate::api::value::{AutoOr, CellAlignment, HorizontalAlignment, Points, StyleId};
use crate::docx::char_props::RunXml;
use crate::docx::props::ThemeColours;
use crate::docx::style_xml::StyleXml;
use crate::docx::style_xml::{DefaultsDef, StyleDef};
use crate::docx::write::props::{Cx, Props, STYLE, TBLPR, TC_BORDERS, TCPR, TRPR, esc};
use crate::docx::{Document, Session};
use crate::format::{
    CharEdit, Color, FontRef, ParaEdit, ParaFlag, ParaProp, Prop, RegionKind, ScriptClass, Shading, StyleKind, TabStop, TextBorder, Toggle,
    UnderlineProps,
};

// The fixtures

/// `check` run on every fixture read for editing (with its name), in parallel, the differences it finds together.
fn over_fixtures(test: &str, check: impl Fn(&str, &Document, &Session, &mut Differences) + Sync) {
    let mut paths = Vec::new();
    crate::layout::tests::output::fixtures(&crate::layout::tests::output::root().join("fixtures"), &mut paths);
    let found = super::par_map(&paths, |path| {
        let mut diffs = Differences::default();
        if let Ok((d, dx)) = crate::docx::read_editable(&std::fs::read(path).expect("reads"))
            && d.resolver.is_some()
            && dx.source.is_some()
        {
            let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            check(&name, &d, &dx, &mut diffs);
        }
        diffs
    });
    let mut all = Differences::default();
    for d in found {
        all.merge(d);
    }
    all.finish(test);
}

/// Each fixture's styles, in order, at most `n` of them.
fn styles_of(d: &Document, n: usize) -> Vec<StyleDef> {
    let typed = d.resolver.as_ref().expect("a resolver").typed();
    let mut ids: Vec<&Rc<str>> = typed.styles.styles.keys().collect();
    ids.sort();
    ids.into_iter().take(n).map(|id| typed.styles.styles[id].clone()).collect()
}

// Writing the two routes compare

/// The differences found: each group's by the text that differs (the two writings without what they share at
/// either end), with the number of times it was seen and an example; those that read otherwise by group.
#[derive(Default)]
struct Differences {
    compared: usize,
    seen: BTreeMap<(String, String, String), (usize, String)>,
    unlike: BTreeMap<String, (usize, Vec<String>)>,
}

/// The text `a` and `b` differ in: each without the longest start and end they share.
fn differing<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    let start = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let mut start = start;
    while !a.is_char_boundary(start) || !b.is_char_boundary(start) {
        start -= 1;
    }
    let (a, b) = (&a[start..], &b[start..]);
    let end = a.bytes().rev().zip(b.bytes().rev()).take_while(|(x, y)| x == y).count().min(a.len()).min(b.len());
    let (mut i, mut j) = (a.len() - end, b.len() - end);
    while !a.is_char_boundary(i) || !b.is_char_boundary(j) {
        (i, j) = (i + 1, j + 1);
    }
    (&a[..i], &b[..j])
}

impl Differences {
    fn seen(&mut self, group: &str, what: &str, old: &str, new: &str) {
        let (x, y) = differing(old, new);
        let cut = |s: &str| s.chars().take(160).collect::<String>();
        let e = self.seen.entry((group.to_string(), cut(x), cut(y))).or_insert_with(|| (0, format!("{what}\n      xml   {old}\n      typed {new}")));
        e.0 += 1;
    }

    fn unlike(&mut self, group: &str, what: String) {
        let e = self.unlike.entry(group.to_string()).or_default();
        e.0 += 1;
        if e.1.len() < 4 {
            e.1.push(what);
        }
    }

    /// `old` (the XML route's) against `new` (the typed route's): equal, or each read by `read` as the same.
    fn check<T: PartialEq + std::fmt::Debug>(&mut self, group: &str, what: &str, old: &str, new: &str, read: impl Fn(&str) -> Option<T>) {
        self.compared += 1;
        if old == new {
            return;
        }
        let (a, b) = (read(old), read(new));
        if a.is_none() || a != b {
            self.unlike(group, format!("{what}\n    xml   {old}\n    typed {new}\n    read {a:?}\n    vs   {b:?}"));
            return;
        }
        self.seen(group, what, old, new);
    }

    /// [`Self::check`], but that where the XML route lost what it copied (`lost`), what reads otherwise is listed in
    /// `lost_group`.
    #[allow(clippy::too_many_arguments)]
    fn check_or_lost<T: PartialEq + std::fmt::Debug>(
        &mut self,
        group: &str,
        lost_group: &str,
        lost: bool,
        what: &str,
        old: &str,
        new: &str,
        read: impl Fn(&str) -> Option<T>,
    ) {
        if lost && old != new && (read(old).is_none() || read(old) != read(new)) {
            self.compared += 1;
            self.seen(lost_group, what, old, new);
            return;
        }
        self.check(group, what, old, new, read);
    }

    /// Whether each route wrote anything, the XML route's `old` and the typed route's `new`, the same.
    fn same_outcome(&mut self, group: &str, what: &str, old: bool, new: bool) {
        self.compared += 1;
        if old != new {
            self.unlike(group, format!("{what}: written by the XML route {old}, by the typed route {new}"));
        }
    }

    /// Whether each route left `was` alone the same, but that the XML route wrote it again only respelt (`old` reads
    /// by `read` as `was` does), where the typed route leaves it as it is.
    fn same_or_respelt<T: PartialEq>(&mut self, group: &str, what: &str, was: &str, old: &str, new: &str, read: impl Fn(&str) -> Option<T>) {
        self.compared += 1;
        let (wrote, typed) = (old != was, new != was);
        if wrote == typed {
            return;
        }
        if wrote && !typed && read(old).is_some() && read(old) == read(was) {
            self.seen(&format!("{group}: only respelt by the XML route"), what, old, new);
            return;
        }
        self.unlike(
            group,
            format!("{what}: written by the XML route {wrote}, by the typed route {typed}\n    was   {was}\n    xml   {old}\n    typed {new}"),
        );
    }

    fn merge(&mut self, o: Differences) {
        self.compared += o.compared;
        for (k, (n, example)) in o.seen {
            self.seen.entry(k).or_insert((0, example)).0 += n;
        }
        for (k, (n, examples)) in o.unlike {
            let e = self.unlike.entry(k).or_default();
            e.0 += n;
            e.1.extend(examples.into_iter().take(4usize.saturating_sub(e.1.len())));
        }
    }

    /// Fails on any difference that doesn't read alike, and lists those that do.
    fn finish(self, test: &str) {
        println!("{test}: {} compared", self.compared);
        for ((group, x, y), (n, example)) in &self.seen {
            println!("  {group}: {n} times {x:?} written {y:?}, reading alike; e.g. {example}");
        }
        assert!(self.compared > 0, "{test}: nothing compared");
        let unlike: Vec<String> = self.unlike.iter().map(|(g, (n, e))| format!("{g}: {n} read otherwise, e.g.\n  {}", e.join("\n  "))).collect();
        assert!(unlike.is_empty(), "{test}:\n{}", unlike.join("\n"));
    }
}

/// `f` without the spellings of modelled attributes it keeps (a value written otherwise than the writer writes it,
/// which reads as the same value).
fn spelling_aside(f: &mut Option<Rc<crate::format::ForeignProps>>) {
    if let Some(p) = f.as_mut() {
        let p = Rc::make_mut(p);
        p.attributes.retain(|a| a.stands_for.is_none());
        if *p == crate::format::ForeignProps::default() {
            *f = None;
        }
    }
}

fn format_spelling_aside(f: &mut crate::format::StyleFormat) {
    spelling_aside(&mut f.para.foreign);
    spelling_aside(&mut f.char.foreign);
    spelling_aside(&mut f.table.foreign);
    spelling_aside(&mut f.row.foreign);
    spelling_aside(&mut f.cell.foreign);
}

/// A style's XML as the typed definition it reads as, its attributes' spellings aside.
fn read_style(cx: &Cx) -> impl Fn(&str) -> Option<crate::format::Style> + '_ {
    |x| {
        let mut s = cx.parse(x, |_, el| crate::docx::style_xml::parse(el)).flatten()?;
        spelling_aside(&mut s.foreign);
        format_spelling_aside(&mut s.format);
        for r in &mut s.regions {
            spelling_aside(&mut r.foreign);
            format_spelling_aside(&mut r.format);
        }
        Some(s)
    }
}

/// Document defaults' XML as the typed defaults they read as, their attributes' spellings aside.
fn read_defaults(cx: &Cx) -> impl Fn(&str) -> Option<crate::format::Defaults> + '_ {
    |x| {
        let mut d = cx.parse(x, |_, el| crate::docx::style_xml::parse_defaults(el))?;
        spelling_aside(&mut d.foreign);
        if let Some(c) = d.char.as_mut() {
            spelling_aside(&mut c.foreign);
        }
        if let Some(p) = d.para.as_mut() {
            spelling_aside(&mut p.foreign);
        }
        Some(d)
    }
}

/// Latent styles' XML as the typed latent styles they read as, their attributes' spellings aside.
fn read_latent(cx: &Cx) -> impl Fn(&str) -> Option<crate::format::LatentStyles> + '_ {
    |x| {
        let mut l = cx.parse(x, |_, el| crate::docx::style_xml::parse_latent(el))?;
        spelling_aside(&mut l.foreign);
        for e in &mut l.exceptions {
            spelling_aside(&mut e.foreign);
        }
        Some(l)
    }
}

// The XML route: a style's XML patched

/// CT_TblStylePr's children in schema order.
const TBL_STYLE_PR: &[&str] = &["pPr", "rPr", "tblPr", "trPr", "tcPr"];

fn edit_table_part(raw: &str, region: &str, child: &str, cx: &Cx, edit: impl FnOnce(&mut Props)) -> String {
    with_table_part_child(raw, region, child, cx, |x| {
        let mut props = Props::parse(x, child, cx);
        match child {
            "tcPr" => props.reorder(TCPR),
            "tblPr" => props.reorder(TBLPR),
            _ => {}
        }
        edit(&mut props);
        props.xml()
    })
}

fn with_table_part_child(raw: &str, region: &str, child: &str, cx: &Cx, edit: impl FnOnce(Option<&str>) -> Option<String>) -> String {
    let w = &cx.w;
    let mut el = Props::parse(Some(raw), "style", cx);
    let at =
        el.kids.iter().position(|(n, x)| n == "tblStylePr" && cx.parse(x, |_, p| crate::docx::xml::attr(p, "type") == Some(region)) == Some(true));
    let blank = format!("<{w}tblStylePr {w}type=\"{region}\"><{w}pPr/></{w}tblStylePr>");
    let mut part = Props::parse(Some(at.map_or(blank.as_str(), |i| el.kids[i].1.as_str())), "tblStylePr", cx);
    part.reorder(TBL_STYLE_PR);
    if at.is_none() {
        part.remove("pPr");
    }
    match edit(part.get(child)) {
        Some(x) => part.set(child, x),
        None => part.remove(child),
    }
    let xml = part.xml().unwrap_or_else(|| format!("<{w}tblStylePr {w}type=\"{region}\"/>"));
    match at {
        Some(i) => el.kids[i].1 = xml,
        None => el.add("tblStylePr", xml),
    }
    el.reorder(STYLE);
    el.xml().unwrap_or_else(|| raw.to_string())
}

fn edit_child(raw: &str, child: &str, cx: &Cx, edit: impl FnOnce(&mut Props)) -> String {
    with_child(raw, child, cx, |x| {
        let mut props = Props::parse(x, child, cx);
        edit(&mut props);
        props.xml()
    })
}

fn with_child(raw: &str, child: &str, cx: &Cx, edit: impl FnOnce(Option<&str>) -> Option<String>) -> String {
    let mut el = Props::parse(Some(raw), "style", cx);
    match edit(el.get(child)) {
        Some(x) => el.set(child, x),
        None => el.remove(child),
    }
    el.reorder(STYLE);
    el.xml().unwrap_or_else(|| raw.to_string())
}

fn set_child(raw: &str, cx: &Cx, name: &str, xml: Option<String>) -> String {
    let mut el = Props::parse(Some(raw), "style", cx);
    match xml {
        Some(x) => el.set(name, x),
        None => el.remove(name),
    }
    el.reorder(STYLE);
    el.xml().unwrap_or_else(|| raw.to_string())
}

fn with_start(raw: &str, start: &str) -> String {
    raw.find('>').map_or_else(|| raw.to_string(), |e| format!("{start}{}", &raw[e + 1..]))
}

fn literal(r: Option<RegionKind>) -> Option<&'static str> {
    r.map(crate::docx::style_xml::region_literal)
}

/// A style's `w:rPr` (its own, or its region's) as a typed edit of it patched it.
fn xml_rpr(raw: &str, region: Option<RegionKind>, cx: &Cx, colours: &ThemeColours, edit: &dyn Fn(&mut CharEdit)) -> String {
    let typed = |x: Option<&str>| {
        super::super::format::edit_rpr_xml(x, colours, cx, |e| {
            edit(e);
            true
        })
    };
    match literal(region) {
        Some(r) => with_table_part_child(raw, r, "rPr", cx, typed),
        None => with_child(raw, "rPr", cx, typed),
    }
}

/// A style's `w:pPr` (its own, or its region's) as a typed edit of it patched it.
fn xml_ppr(raw: &str, region: Option<RegionKind>, cx: &Cx, colours: &ThemeColours, edit: &dyn Fn(&mut ParaEdit)) -> String {
    let typed = |x: Option<&str>| {
        super::super::format::edit_ppr_xml(x, colours, cx, |e| {
            edit(e);
            true
        })
    };
    match literal(region) {
        Some(r) => with_table_part_child(raw, r, "pPr", cx, typed),
        None => with_child(raw, "pPr", cx, typed),
    }
}

// Representative edits

type CharEdits = Vec<(&'static str, Box<dyn Fn(&mut CharEdit)>)>;
type ParaEdits = Vec<(&'static str, Box<dyn Fn(&mut ParaEdit)>)>;

fn char_edits() -> CharEdits {
    vec![
        ("bold", Box::new(|e: &mut CharEdit| e.toggle(Toggle::BOLD, true))),
        ("not italic", Box::new(|e: &mut CharEdit| e.toggle(Toggle::ITALIC, false))),
        (
            "size",
            Box::new(|e: &mut CharEdit| {
                (e.size, e.complex_size) = (Some(14.0), Some(14.0));
                e.whole(Prop::Size);
                e.whole(Prop::ComplexSize);
            }),
        ),
        (
            "no size",
            Box::new(|e: &mut CharEdit| {
                e.size = None;
                e.whole(Prop::Size);
            }),
        ),
        (
            "colour",
            Box::new(|e: &mut CharEdit| {
                e.color = Some(Color::Rgb(0x112233));
                e.whole(Prop::Color);
            }),
        ),
        (
            "font",
            Box::new(|e: &mut CharEdit| {
                let arial = Some(FontRef::Family("Arial".into()));
                (e.fonts.latin, e.fonts.latin_extended) = (arial.clone(), arial);
                e.whole(Prop::Font(ScriptClass::Latin));
                e.whole(Prop::Font(ScriptClass::LatinExtended));
            }),
        ),
        (
            "underline",
            Box::new(|e: &mut CharEdit| {
                e.underline = Some(UnderlineProps { style: crate::document::line::Underline::Single, color: None });
                e.whole(Prop::Underline);
            }),
        ),
        (
            "spacing",
            Box::new(|e: &mut CharEdit| {
                e.spacing = Some(1.5);
                e.whole(Prop::Spacing);
            }),
        ),
    ]
}

fn para_edits() -> ParaEdits {
    use crate::format::BorderSide;
    vec![
        (
            "centred",
            Box::new(|e: &mut ParaEdit| {
                e.align = Some(crate::document::format::Jc::Center);
                e.whole(ParaProp::Align);
            }),
        ),
        (
            "space before",
            Box::new(|e: &mut ParaEdit| {
                e.space_before = Some(6.0);
                e.part(ParaProp::SpaceBefore);
            }),
        ),
        (
            "left indent",
            Box::new(|e: &mut ParaEdit| {
                e.indent_left = Some(36.0);
                e.part(ParaProp::IndentLeft);
            }),
        ),
        ("keep with next", Box::new(|e: &mut ParaEdit| e.flag(ParaFlag::KEEP_NEXT, true))),
        (
            "line spacing",
            Box::new(|e: &mut ParaEdit| {
                (e.line, e.line_rule) = (Some(360.0), Some(crate::document::format::LineRule::Auto));
                e.part(ParaProp::Line);
                e.part(ParaProp::LineRule);
            }),
        ),
        (
            "tabs",
            Box::new(|e: &mut ParaEdit| {
                e.tabs = vec![TabStop { pos: 72.0, kind: crate::document::format::TabKind::Left, leader: crate::document::format::Leader::None }];
                e.whole(ParaProp::Tabs);
            }),
        ),
        (
            "top border",
            Box::new(|e: &mut ParaEdit| {
                e.borders.top = Some(TextBorder {
                    style: crate::document::line::BorderStyle::Single,
                    width: 0.5,
                    space: 1.0,
                    shadow: false,
                    color: Some(Color::Auto),
                });
                e.part(ParaProp::Border(BorderSide::Top));
            }),
        ),
        (
            "shading",
            Box::new(|e: &mut ParaEdit| {
                e.shading = Some(Shading {
                    pattern: crate::document::format::ShadingPattern::Clear,
                    color: Some(Color::Auto),
                    fill: Some(Color::Rgb(0xFFEE00)),
                });
                e.whole(ParaProp::Shading);
            }),
        ),
        (
            "outline level",
            Box::new(|e: &mut ParaEdit| {
                e.outline_level = Some(1);
                e.whole(ParaProp::OutlineLevel);
            }),
        ),
    ]
}

/// The regions a table style's formatting is tried at: its base, a header row and each it defines.
fn regions(def: &StyleDef) -> Vec<Option<RegionKind>> {
    let s = def.get();
    let mut out = vec![None];
    if s.kind == StyleKind::Table {
        out.push(Some(RegionKind::FirstRow));
        out.extend(s.regions.iter().map(|r| Some(r.kind)).filter(|r| *r != Some(RegionKind::FirstRow)));
    }
    out
}

// Formatting a style

/// Every fixture style formatted by each representative character and paragraph edit, at its base and at its table
/// regions, one edit and two in turn, writes as patching its XML wrote.
#[test]
fn formatting_a_style_writes_as_its_xml_patch_did() {
    over_fixtures("formatting a style", |path, d, _, diffs| {
        let (chars, paras) = (char_edits(), para_edits());
        let typed = d.resolver.as_ref().expect("a resolver").typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        for def in styles_of(d, 60) {
            let raw = def.xml(&colours, &cx).into_owned();
            for region in regions(&def) {
                let what = |e: &str| format!("{path}: {} {e} at {region:?}", def.get().id);
                for (name, edit) in &chars {
                    let old = xml_rpr(&raw, region, &cx, &colours, edit);
                    let new = edit_style(&def, |s, set| edit_chars(s, set, region, edit));
                    diffs.check("formatting characters", &what(name), &old, &new.xml(&colours, &cx), read_style(&cx));
                }
                for (name, edit) in &paras {
                    let old = xml_ppr(&raw, region, &cx, &colours, edit);
                    let new = edit_style(&def, |s, set| edit_paras(s, set, region, edit));
                    diffs.check("formatting paragraphs", &what(name), &old, &new.xml(&colours, &cx), read_style(&cx));
                }
                // Commands in turn
                let old = xml_ppr(&xml_rpr(&raw, region, &cx, &colours, &chars[2].1), region, &cx, &colours, &paras[1].1);
                let old = xml_rpr(&old, region, &cx, &colours, &chars[0].1);
                let mut t = StyleTarget::of(def.clone());
                t.edit(|s, set| edit_chars(s, set, region, &chars[2].1));
                t.edit(|s, set| edit_paras(s, set, region, &paras[1].1));
                t.edit(|s, set| edit_chars(s, set, region, &chars[0].1));
                diffs.check("formatting in turn", &what("size, space before, bold"), &old, &t.def().xml(&colours, &cx), read_style(&cx));
            }
        }
    });
}

// A table style's table commands

/// A table style edit by a table command, for the part `region` (none: the whole table), as its XML was patched.
fn xml_table_style_edit(e: &Editor, raw: &str, region: Option<&'static str>, c: &Command, cx: &Cx) -> Result<String, Error> {
    use super::super::table_format::{Edges, border_xml};
    let w = cx.w.clone();
    let root = |raw: &str, child: &str, order: &'static [&'static str], edit: &dyn Fn(&mut Props)| {
        edit_child(raw, child, cx, |p| {
            p.reorder(order);
            edit(p);
        })
    };
    let cell = |raw: &str, edit: &dyn Fn(&mut Props)| match region {
        Some(r) => edit_table_part(raw, r, "tcPr", cx, |p| {
            p.reorder(TCPR);
            edit(p);
        }),
        None => root(raw, "tcPr", TCPR, edit),
    };
    let tw = |p: Points| (p.0 * 20.0).round() as i64;
    let whole_only = |what: &str| if region.is_some() { Err(Error::invalid(format!("{what} is the whole table's"))) } else { Ok(()) };
    Ok(match c {
        Command::SetCellShading(fill) => {
            let fill = fill.as_ref().map(|c| e.word_colour(c)).transpose().map_err(Error::invalid)?;
            cell(raw, &|p| match fill {
                Some(c) => {
                    let tag = format!("<{w}shd {w}val=\"clear\" {w}color=\"auto\"/>");
                    p.set("shd", super::super::commands::colour_attrs(&tag, &w, "fill", super::super::commands::THEME_FILL, Some(c)))
                }
                None => p.remove("shd"),
            })
        }
        Command::SetCellAlignment(a) => {
            let v = super::super::table_format::v_align_xml(&w, super::super::table_format::cell_alignment(*a));
            cell(raw, &|p| p.set("vAlign", v.clone()))
        }
        Command::SetCellMargins(m) => {
            let sides = [("top", m.top), ("left", m.left), ("bottom", m.bottom), ("right", m.right)];
            let edit = |p: &mut Props, name: &str| {
                let mut mar = Props::parse(p.get(name), name, cx);
                mar.reorder(&["top", "start", "left", "bottom", "end", "right"]);
                for (side, v) in sides {
                    if let Some(v) = v {
                        mar.set(side, format!("<{w}{side} {w}w=\"{}\" {w}type=\"dxa\"/>", tw(v)));
                    }
                }
                match mar.xml() {
                    Some(x) => p.set(name, x),
                    None => p.remove(name),
                }
            };
            match region {
                Some(_) => cell(raw, &|p| edit(p, "tcMar")),
                None => root(raw, "tblPr", TBLPR, &|p| edit(p, "tblCellMar")),
            }
        }
        Command::SetRowCanBreak(on) => match region {
            Some(r) => edit_table_part(raw, r, "trPr", cx, |p| {
                p.reorder(TRPR);
                p.set("cantSplit", if *on { format!("<{w}cantSplit {w}val=\"0\"/>") } else { format!("<{w}cantSplit/>") });
            }),
            None => root(raw, "trPr", TRPR, &|p| {
                p.set("cantSplit", if *on { format!("<{w}cantSplit {w}val=\"0\"/>") } else { format!("<{w}cantSplit/>") })
            }),
        },
        Command::SetTableAlignment(a) => {
            whole_only("the alignment")?;
            let v = match a {
                HorizontalAlignment::Left => "left",
                HorizontalAlignment::Center => "center",
                HorizontalAlignment::Right => "right",
            };
            root(raw, "tblPr", TBLPR, &|p| p.set("jc", format!("<{w}jc {w}val=\"{v}\"/>")))
        }
        Command::SetTableIndent(i) => {
            whole_only("the indent")?;
            root(raw, "tblPr", TBLPR, &|p| p.set("tblInd", format!("<{w}tblInd {w}w=\"{}\" {w}type=\"dxa\"/>", tw(*i))))
        }
        Command::SetCellBorders(b) => {
            let line = b.border.as_ref().map(|l| super::super::readback::BorderLine::of_api(l, 0.0, false));
            let (edges, line) = e.border_lines(b.edges, line.as_ref()).map_err(Error::invalid)?;
            let nil = Edges::None == edges;
            let sides: &[&str] = match edges {
                Edges::All | Edges::None => &["top", "left", "bottom", "right", "insideH", "insideV"],
                Edges::Outside => &["top", "left", "bottom", "right"],
                Edges::Inside => &["insideH", "insideV"],
                Edges::InsideH => &["insideH"],
                Edges::InsideV => &["insideV"],
                Edges::Top => &["top"],
                Edges::Bottom => &["bottom"],
                Edges::Left => &["left"],
                Edges::Right => &["right"],
                Edges::DiagonalDown | Edges::DiagonalUp => &[],
            };
            let diagonals: &[&str] = match edges {
                Edges::DiagonalDown => &["tl2br"],
                Edges::DiagonalUp => &["tr2bl"],
                Edges::None => &["tl2br", "tr2bl"],
                _ => &[],
            };
            let xml = |side: &str| if nil { format!("<{w}{side} {w}val=\"nil\"/>") } else { border_xml(&w, side, &line) };
            let edit = |p: &mut Props, name: &str, names: &[&str]| {
                let mut bd = Props::parse(p.get(name), name, cx);
                bd.reorder(TC_BORDERS);
                for side in names {
                    bd.set(side, xml(side));
                }
                match bd.xml() {
                    Some(x) => p.set(name, x),
                    None => p.remove(name),
                }
            };
            let mut raw = raw.to_string();
            match region {
                Some(_) => {
                    let all: Vec<&str> = sides.iter().chain(diagonals).copied().collect();
                    raw = cell(&raw, &|p| edit(p, "tcBorders", &all));
                }
                None => {
                    if !sides.is_empty() {
                        raw = root(&raw, "tblPr", TBLPR, &|p| edit(p, "tblBorders", sides));
                    }
                    if !diagonals.is_empty() {
                        raw = cell(&raw, &|p| edit(p, "tcBorders", diagonals));
                    }
                }
            }
            raw
        }
        _ => return Err(Error::invalid(format!("{:?} doesn't format a table style", c.id()))),
    })
}

fn line(width: f64) -> Border {
    Border { style: BorderStyle::Single, width: Points(width), color: AutoOr::Auto, space: None, shadow: None }
}

fn table_commands() -> Vec<Command> {
    let rgb = |v: u32| crate::api::drawing::ColourValue { rgb: Some(crate::api::value::Color(v)), ..Default::default() };
    vec![
        Command::SetCellShading(Some(rgb(0x00B050))),
        Command::SetCellShading(None),
        Command::SetCellAlignment(CellAlignment::Center),
        Command::SetCellMargins(CellMargins { top: Some(Points(2.0)), left: Some(Points(3.3)), bottom: None, right: None }),
        Command::SetRowCanBreak(true),
        Command::SetRowCanBreak(false),
        Command::SetTableAlignment(HorizontalAlignment::Center),
        Command::SetTableIndent(Points(10.0)),
        Command::SetCellBorders(CellBorders { edges: BorderEdges::All, border: Some(line(1.0)) }),
        Command::SetCellBorders(CellBorders { edges: BorderEdges::Outside, border: Some(line(0.5)) }),
        Command::SetCellBorders(CellBorders { edges: BorderEdges::InsideHorizontal, border: None }),
        Command::SetCellBorders(CellBorders { edges: BorderEdges::DiagonalDown, border: Some(line(0.75)) }),
        Command::SetCellBorders(CellBorders { edges: BorderEdges::None, border: None }),
    ]
}

/// Every fixture table style edited by each table command, at its base and at its regions, writes as patching its
/// XML wrote, and refuses alike.
#[test]
fn a_table_style_s_table_commands_write_as_its_xml_patch_did() {
    over_fixtures("table commands on a table style", |path, d, dx, diffs| {
        let e = Editor::with_session(d.clone(), dx.clone(), fonts());
        let typed = d.resolver.as_ref().expect("a resolver").typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        for def in styles_of(d, usize::MAX).into_iter().filter(|s| s.get().kind == StyleKind::Table).take(30) {
            let raw = def.xml(&colours, &cx).into_owned();
            for region in regions(&def) {
                for c in table_commands() {
                    let what = format!("{path}: {} {c:?} at {region:?}", def.get().id);
                    let old = xml_table_style_edit(&e, &raw, literal(region), &c, &cx);
                    let new = table_style_edit(&e, &def, region, &c);
                    match (old, new) {
                        (Ok(old), Ok(new)) => diffs.check("table commands", &what, &old, &new.xml(&colours, &cx), read_style(&cx)),
                        (Err(a), Err(b)) => diffs.same_outcome("table commands refused", &format!("{what}: {a:?} {b:?}"), a == b, true),
                        (a, b) => diffs.same_outcome("table commands", &what, a.is_ok(), b.is_ok()),
                    }
                }
            }
        }
    });
}

// A style's list flags

fn xml_meta(was: &str, m: &StyleMetaChange, cx: &Cx) -> Option<String> {
    let w = cx.w.clone();
    let mut el = Props::parse(Some(was), "style", cx);
    let flag = |el: &mut Props, name: &str, on: bool| if on { el.set(name, format!("<{w}{name}/>")) } else { el.remove(name) };
    if let Some(v) = m.visibility {
        let (semi, unhide) = match v {
            StyleVisibility::Show => (false, false),
            StyleVisibility::HideUntilUsed => (true, true),
            StyleVisibility::Hide => (true, false),
        };
        flag(&mut el, "semiHidden", semi);
        flag(&mut el, "unhideWhenUsed", unhide);
    }
    if let Some(p) = m.priority {
        el.set("uiPriority", format!("<{w}uiPriority {w}val=\"{p}\"/>"));
    }
    if let Some(q) = m.quick {
        flag(&mut el, "qFormat", q);
    }
    if let Some(l) = m.locked {
        flag(&mut el, "locked", l);
    }
    el.reorder(STYLE);
    el.xml()
}

fn meta_changes() -> Vec<StyleMetaChange> {
    let m = |visibility, priority, quick, locked| StyleMetaChange { id: StyleId::default(), visibility, priority, quick, locked };
    vec![
        m(Some(StyleVisibility::Show), None, None, None),
        m(Some(StyleVisibility::HideUntilUsed), None, None, None),
        m(Some(StyleVisibility::Hide), Some(5), None, None),
        m(None, Some(99), Some(true), None),
        m(None, None, Some(false), Some(true)),
        m(None, None, None, Some(false)),
        m(Some(StyleVisibility::Show), Some(1), Some(true), Some(false)),
    ]
}

/// Every fixture style given each representative set of list flags writes as patching its XML wrote, and is left
/// alone alike.
#[test]
fn a_style_s_list_flags_write_as_their_xml_patch_did() {
    over_fixtures("a style's list flags", |path, d, _, diffs| {
        let typed = d.resolver.as_ref().expect("a resolver").typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        for def in styles_of(d, usize::MAX) {
            let raw = def.xml(&colours, &cx).into_owned();
            for m in meta_changes() {
                let what = format!("{path}: {} {m:?}", def.get().id);
                let old = xml_meta(&raw, &m, &cx).expect("a style");
                let new = meta_edited(&def, &m).xml(&colours, &cx).into_owned();
                diffs.check("list flags", &what, &old, &new, read_style(&cx));
                diffs.same_or_respelt("list flags left alone", &what, &raw, &old, &new, read_style(&cx));
            }
        }
    });
}

// A style updated to the selection

/// The selection's direct formatting as the XML route took it: its paragraph's `w:pPr` (written in the main part) and
/// its run's `w:rPr` (in the styles part), without the style, list, section break and tracked changes.
fn xml_selection(
    p: &crate::docx::Paragraph,
    run: Option<&crate::docx::Run>,
    res: &crate::docx::resolvers::Resolvers,
    main: &Cx,
) -> (Option<String>, Option<String>) {
    let cx = res.cx();
    let strip = |x: &str, part: &str, drop: &[&str]| {
        let mut p = Props::parse(Some(x), part, &cx);
        for k in drop {
            p.remove(k);
        }
        p.xml()
    };
    let ppr = crate::docx::para_xml::ppr(Some(res), &p.style.direct, main)
        .and_then(|x| strip(&x, "pPr", &["pStyle", "numPr", "sectPr", "rPr", "pPrChange"]));
    let colours = res.colours();
    let rpr = run.and_then(|r| {
        let mut e = CharEdit::new((**r.direct.props()?).clone());
        e.style = None;
        e.whole(Prop::Style);
        r.direct.edit(e).xml(&colours, &cx).map(std::borrow::Cow::into_owned)
    });
    (ppr, rpr)
}

/// What the XML route lost of a selection's formatting it copied into a style.
const LOST: &str = "copied with names the styles part doesn't declare: lost by the XML route";

/// Whether the selection's formatting as the XML route took it (`ppr`, `rpr`) names what the styles part doesn't
/// declare (written with its own declarations), so that its copy there lost it: it doesn't parse there, or reads
/// as nothing.
fn lost_in_styles(ppr: &Option<String>, rpr: &Option<String>, cx: &Cx) -> bool {
    let lost = |x: &Option<String>, part: &str| x.as_deref().is_some_and(|x| x.contains("xmlns") || Props::parse(Some(x), part, cx).kids.is_empty());
    lost(ppr, "pPr") || lost(rpr, "rPr")
}

fn xml_updated(was: &str, ppr: Option<String>, rpr: Option<String>, character: bool, cx: &Cx) -> String {
    let mut raw = was.to_string();
    for (part, own) in [("pPr", ppr), ("rPr", rpr)] {
        let Some(own) = own else { continue };
        if part == "pPr" && character {
            continue;
        }
        let theirs = Props::parse(Some(&own), part, cx);
        raw = edit_child(&raw, part, cx, |p| {
            for (name, xml) in &theirs.kids {
                p.set(name, xml.clone());
            }
        });
    }
    raw
}

/// Every fixture body paragraph's style (and a run's character style) updated to the paragraph's and its first run's
/// direct formatting writes as patching its XML wrote, and is left alone alike.
#[test]
fn a_style_updated_to_the_selection_writes_as_its_xml_patch_did() {
    over_fixtures("a style updated to the selection", |path, d, dx, diffs| {
        let res = d.resolver.as_deref().expect("a resolver");
        let typed = res.typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        let main = Cx::of_xml(&dx.source.as_ref().expect("a source").xml);
        let default = typed.cascade.styles.default_paragraph.clone();
        let paras = d.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None });
        for (k, p) in paras.take(400).enumerate() {
            let run = p.inlines.iter().find_map(|i| i.run());
            let (ppr, rpr) = xml_selection(p, run, res, &main);
            let lost = lost_in_styles(&ppr, &rpr, &cx);
            let (tp, tr) = selection_props(Some(p.style.direct.get()), run.and_then(|r| r.direct.props()).map(|r| &**r));
            let mut targets: Vec<(Rc<str>, bool)> = Vec::new();
            if let Some(id) = p.style.style_id.as_deref().map(Rc::from).or_else(|| default.clone()) {
                targets.push((id, false));
            }
            if let Some(c) = run.and_then(|r| r.style_id.as_deref()) {
                targets.push((c.into(), true));
            }
            for (id, character) in targets {
                let Some(def) = typed.styles.styles.get(&id) else { continue };
                let raw = def.xml(&colours, &cx).into_owned();
                let what = format!("{path}: paragraph {k} into {id}");
                let old = xml_updated(&raw, ppr.clone(), rpr.clone(), character, &cx);
                let new = updated_to(def, tp.clone(), tr.clone(), character).xml(&colours, &cx).into_owned();
                diffs.check_or_lost("updated to the selection", LOST, lost, &what, &old, &new, read_style(&cx));
                if lost && read_style(&cx)(&old) != read_style(&cx)(&new) {
                    continue;
                }
                diffs.same_or_respelt("left alone when it matches", &what, &raw, &old, &new, read_style(&cx));
            }
        }
    });
}

// The document defaults

fn xml_stand_in(defaults: &str, cx: &Cx) -> String {
    let w = &cx.w;
    let parts = Props::parse(Some(defaults), "docDefaults", cx);
    let inner = |wrapper: &str, part: &str| {
        parts.get(wrapper).and_then(|x| Props::parse(Some(x), wrapper, cx).get(part).map(str::to_string)).unwrap_or_default()
    };
    format!(
        "<{w}style {w}type=\"paragraph\" {w}styleId=\"DocumentDefaults\"><{w}name {w}val=\"Document Defaults\"/>{}{}</{w}style>",
        inner("pPrDefault", "pPr"),
        inner("rPrDefault", "rPr")
    )
}

/// The document defaults `defaults` with their stand-in style formatted as `edited`, as the XML route wrote them:
/// `None` when they didn't change.
fn xml_defaults(defaults: &str, edited: &str, cx: &Cx) -> Option<String> {
    let w = &cx.w;
    let parts = Props::parse(Some(defaults), "docDefaults", cx);
    let inner = |wrapper: &str, part: &str| {
        parts.get(wrapper).and_then(|x| Props::parse(Some(x), wrapper, cx).get(part).map(str::to_string)).unwrap_or_default()
    };
    let el = Props::parse(Some(edited), "style", cx);
    if [("pPrDefault", "pPr"), ("rPrDefault", "rPr")].iter().all(|&(wrapper, part)| el.get(part).unwrap_or_default() == inner(wrapper, part)) {
        return None;
    }
    let mut out = Props::parse(Some(defaults), "docDefaults", cx);
    for (wrapper, part) in [("rPrDefault", "rPr"), ("pPrDefault", "pPr")] {
        let mut wrap = Props::parse(parts.get(wrapper), wrapper, cx);
        match el.get(part) {
            Some(x) => wrap.set(part, x.to_string()),
            None => wrap.remove(part),
        }
        if parts.get(wrapper).is_some() || el.get(part).is_some() {
            out.set(wrapper, wrap.xml().unwrap_or_else(|| format!("<{w}{wrapper}/>")));
        }
    }
    out.reorder(&["rPrDefault", "pPrDefault"]);
    Some(out.xml().unwrap_or_else(|| format!("<{w}docDefaults/>")))
}

/// Every fixture's document defaults (and none) formatted by each representative character and paragraph edit write
/// as patching their XML wrote, and are left alone alike.
#[test]
fn the_document_defaults_write_as_their_xml_patch_did() {
    over_fixtures("the document defaults", |path, d, _, diffs| {
        let (chars, paras) = (char_edits(), para_edits());
        let typed = d.resolver.as_ref().expect("a resolver").typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        let empty = DefaultsDef::typed(Rc::default());
        for was in [typed.styles.defaults.as_ref(), None] {
            let defaults = was.map_or_else(|| format!("<{}docDefaults/>", cx.w), |w| w.xml(&colours, &cx).into_owned());
            let stand_in = xml_stand_in(&defaults, &cx);
            let def = was.unwrap_or(&empty).get();
            let style = crate::format::Style {
                id: "DocumentDefaults".into(),
                name: Some("Document Defaults".into()),
                format: crate::format::StyleFormat {
                    para: def.para.clone().unwrap_or_default(),
                    char: def.char.clone().unwrap_or_default(),
                    ..Default::default()
                },
                ..Default::default()
            };
            diffs.check(
                "the defaults as a style",
                &format!("{path}: stand-in"),
                &stand_in,
                &crate::docx::style_xml::serialize(&style, &colours, &cx),
                read_style(&cx),
            );
            let mut check = |what: String, old: String, edit: &dyn Fn(&mut StyleTarget)| {
                let old = xml_defaults(&defaults, &old, &cx);
                let mut t = StyleTarget::of(StyleDef::typed(Rc::new(style.clone())));
                edit(&mut t);
                let new = defaults_edited(&typed, was, &t);
                diffs.same_outcome("defaults left alone", &what, old.is_some(), new.is_some());
                if let (Some(old), Some(new)) = (old, new) {
                    diffs.check("defaults", &what, &old, &new.xml(&colours, &cx), read_defaults(&cx));
                }
            };
            for (name, e) in &chars {
                check(format!("{path}: {name} over {}", was.is_some()), xml_rpr(&stand_in, None, &cx, &colours, e), &|t| {
                    t.edit(|s, set| edit_chars(s, set, None, e))
                });
            }
            for (name, e) in &paras {
                check(format!("{path}: {name} over {}", was.is_some()), xml_ppr(&stand_in, None, &cx, &colours, e), &|t| {
                    t.edit(|s, set| edit_paras(s, set, None, e))
                });
            }
        }
        // The application's defaults, written empty
        if typed.styles.defaults.is_none() {
            let new = crate::docx::style_xml::serialize_defaults(&Default::default(), &colours, &cx);
            diffs.check("empty defaults", path, &format!("<{}docDefaults/>", cx.w), &new, read_defaults(&cx));
        }
    });
}

// The latent styles

fn xml_latent(held: Option<String>, b: &crate::services::styles::builtin::BuiltIn, m: &StyleMetaChange, cx: &Cx) -> Option<String> {
    use crate::services::styles::builtin;
    let w = cx.w.clone();
    let raw = held.clone().unwrap_or_else(|| {
        let exceptions: String = builtin::BUILT_IN
            .iter()
            .map(|b| {
                let mut x = format!("<{w}lsdException {w}name=\"{}\"", esc(b.name));
                for (on, a) in [(b.semi_hidden, "semiHidden"), (b.unhide_when_used, "unhideWhenUsed")] {
                    if on {
                        x.push_str(&format!(" {w}{a}=\"1\""));
                    }
                }
                if b.priority != 99 {
                    x.push_str(&format!(" {w}uiPriority=\"{}\"", b.priority));
                }
                if b.quick {
                    x.push_str(&format!(" {w}qFormat=\"1\""));
                }
                x + "/>"
            })
            .collect();
        format!(
            "<{w}latentStyles {w}defLockedState=\"0\" {w}defUIPriority=\"99\" {w}defSemiHidden=\"0\" {w}defUnhideWhenUsed=\"0\" {w}defQFormat=\"0\" {w}count=\"{}\">{exceptions}</{w}latentStyles>",
            builtin::BUILT_IN.len()
        )
    });
    let mut el = Props::parse(Some(&raw), "latentStyles", cx);
    let named = |x: &str| cx.parse(x, |_, n| crate::docx::xml::attr(n, "name").is_some_and(|v| v.eq_ignore_ascii_case(b.name))).unwrap_or(false);
    let at = el.kids.iter().position(|(n, x)| n == "lsdException" && named(x));
    let was = at.map_or_else(|| format!("<{w}lsdException {w}name=\"{}\"/>", esc(b.name)), |i| el.kids[i].1.clone());
    let mut attrs: Vec<(String, String)> =
        cx.parse(&was, |_, n| n.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect::<Vec<_>>()).unwrap_or_default();
    let mut put = |name: &str, v: Option<String>| {
        attrs.retain(|(a, _)| a != name);
        if let Some(v) = v {
            attrs.push((name.to_string(), v));
        }
    };
    if let Some(v) = m.visibility {
        let (semi, unhide) = match v {
            StyleVisibility::Show => (false, false),
            StyleVisibility::HideUntilUsed => (true, true),
            StyleVisibility::Hide => (true, false),
        };
        put("semiHidden", Some(if semi { "1" } else { "0" }.into()));
        put("unhideWhenUsed", Some(if unhide { "1" } else { "0" }.into()));
    }
    if let Some(p) = m.priority {
        put("uiPriority", Some(p.to_string()));
    }
    if let Some(q) = m.quick {
        put("qFormat", Some(if q { "1" } else { "0" }.into()));
    }
    if let Some(l) = m.locked {
        put("locked", Some(if l { "1" } else { "0" }.into()));
    }
    let order = ["name", "locked", "uiPriority", "semiHidden", "unhideWhenUsed", "qFormat"];
    attrs.sort_by_key(|(a, _)| order.iter().position(|o| *o == a.as_str()).unwrap_or(order.len()));
    let x = format!("<{w}lsdException{}/>", attrs.iter().map(|(a, v)| format!(" {w}{a}=\"{}\"", esc(v))).collect::<String>());
    match at {
        Some(i) => el.kids[i].1 = x,
        None => el.add("lsdException", x),
    }
    el.xml()
}

/// Every fixture's latent styles (and Word's, for a document without them) given each representative set of list
/// flags for built-in styles, among their exceptions or not, write as patching their XML wrote.
#[test]
fn the_latent_styles_write_as_their_xml_patch_did() {
    use crate::services::styles::builtin::BUILT_IN;
    over_fixtures("the latent styles", |path, d, _, diffs| {
        let typed = d.resolver.as_ref().expect("a resolver").typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        let held = typed.styles.latent.as_ref();
        for b in [&BUILT_IN[0], &BUILT_IN[2], &BUILT_IN[150], &BUILT_IN[BUILT_IN.len() - 1]] {
            for m in meta_changes() {
                for latent in [held, None] {
                    let what = format!("{path}: {} {m:?} over {}", b.name, latent.is_some());
                    let old = xml_latent(latent.map(|l| l.xml(&colours, &cx).into_owned()), b, &m, &cx).expect("latent styles");
                    let new = latent_edited(latent, b, &m).xml(&colours, &cx).into_owned();
                    diffs.check(if latent.is_some() { "latent styles" } else { "Word's latent styles" }, &what, &old, &new, read_latent(&cx));
                }
            }
        }
    });
}

// Built-in styles

const XML_MAJOR: &str = r#"<w:rFonts w:asciiTheme="majorHAnsi" w:eastAsiaTheme="majorEastAsia" w:hAnsiTheme="majorHAnsi" w:cstheme="majorBidi"/>"#;

fn xml_formatting(id: &str) -> (String, String) {
    let heading = |n: u8, before: u32, after: u32, rpr: &str| {
        (
            format!(r#"<w:keepNext/><w:keepLines/><w:spacing w:before="{before}" w:after="{after}"/><w:outlineLvl w:val="{}"/>"#, n - 1),
            rpr.to_string(),
        )
    };
    let toc = |n: u32| (format!(r#"<w:spacing w:after="100"/><w:ind w:left="{}"/>"#, 240 * (n - 1)), String::new());
    let s = |p: &str, r: &str| (p.to_string(), r.to_string());
    match id {
        "Heading1" => heading(1, 360, 80, &format!(r#"{XML_MAJOR}<w:color w:val="0F4761"/><w:sz w:val="40"/><w:szCs w:val="40"/>"#)),
        "Heading2" => heading(2, 160, 80, &format!(r#"{XML_MAJOR}<w:color w:val="0F4761"/><w:sz w:val="32"/><w:szCs w:val="32"/>"#)),
        "Heading3" => heading(3, 160, 80, r#"<w:color w:val="0F4761"/><w:sz w:val="28"/><w:szCs w:val="28"/>"#),
        "Heading4" => heading(4, 80, 40, r#"<w:i/><w:iCs/><w:color w:val="0F4761"/>"#),
        "Heading5" => heading(5, 80, 40, r#"<w:color w:val="0F4761"/>"#),
        "Heading6" => heading(6, 40, 0, r#"<w:i/><w:iCs/><w:color w:val="595959"/>"#),
        "Heading7" => heading(7, 40, 0, r#"<w:color w:val="595959"/>"#),
        "Heading8" => heading(8, 0, 0, r#"<w:i/><w:iCs/><w:color w:val="272727"/>"#),
        "Heading9" => heading(9, 0, 0, r#"<w:color w:val="272727"/>"#),
        "Title" => s(
            r#"<w:spacing w:after="80" w:line="240" w:lineRule="auto"/><w:contextualSpacing/>"#,
            &format!(r#"{XML_MAJOR}<w:spacing w:val="-10"/><w:kern w:val="28"/><w:sz w:val="56"/><w:szCs w:val="56"/>"#),
        ),
        "Subtitle" => s(r#"<w:spacing w:after="160"/>"#, r#"<w:color w:val="595959"/><w:spacing w:val="15"/><w:sz w:val="28"/><w:szCs w:val="28"/>"#),
        "Quote" => s(r#"<w:spacing w:before="160"/><w:jc w:val="center"/>"#, r#"<w:i/><w:iCs/><w:color w:val="404040"/>"#),
        "IntenseQuote" => s(
            r#"<w:pBdr><w:top w:val="single" w:sz="4" w:space="10" w:color="0F4761"/><w:bottom w:val="single" w:sz="4" w:space="10" w:color="0F4761"/></w:pBdr><w:spacing w:before="360" w:after="360"/><w:ind w:left="864" w:right="864"/><w:jc w:val="center"/>"#,
            r#"<w:i/><w:iCs/><w:color w:val="0F4761"/>"#,
        ),
        "ListParagraph" => s(r#"<w:ind w:left="720"/><w:contextualSpacing/>"#, ""),
        "NoSpacing" => s(r#"<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"#, ""),
        "Caption" => s(
            r#"<w:spacing w:after="200" w:line="240" w:lineRule="auto"/>"#,
            r#"<w:i/><w:iCs/><w:color w:val="0E2841"/><w:sz w:val="18"/><w:szCs w:val="18"/>"#,
        ),
        "TOCHeading" => (r#"<w:numPr><w:numId w:val="0"/></w:numPr><w:outlineLvl w:val="9"/>"#.to_string(), String::new()),
        "TableofFigures" => s(r#"<w:spacing w:after="0"/>"#, ""),
        "Header" | "Footer" => s(
            r#"<w:tabs><w:tab w:val="center" w:pos="4680"/><w:tab w:val="right" w:pos="9360"/></w:tabs><w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"#,
            "",
        ),
        "FootnoteText" | "EndnoteText" | "CommentText" => {
            s(r#"<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>"#, r#"<w:sz w:val="20"/><w:szCs w:val="20"/>"#)
        }
        "FootnoteReference" | "EndnoteReference" => s("", r#"<w:vertAlign w:val="superscript"/>"#),
        "CommentReference" => s("", r#"<w:sz w:val="16"/><w:szCs w:val="16"/>"#),
        "Hyperlink" => s("", r#"<w:color w:val="467886"/><w:u w:val="single"/>"#),
        "FollowedHyperlink" => s("", r#"<w:color w:val="96607D"/><w:u w:val="single"/>"#),
        "Strong" => s("", "<w:b/><w:bCs/>"),
        "Emphasis" => s("", "<w:i/><w:iCs/>"),
        "SubtleEmphasis" => s("", r#"<w:i/><w:iCs/><w:color w:val="404040"/>"#),
        "IntenseEmphasis" => s("", r#"<w:i/><w:iCs/><w:color w:val="0F4761"/>"#),
        "SubtleReference" => s("", r#"<w:smallCaps/><w:color w:val="5A5A5A"/>"#),
        "IntenseReference" => s("", r#"<w:b/><w:bCs/><w:smallCaps/><w:color w:val="0F4761"/><w:spacing w:val="5"/>"#),
        "BookTitle" => s("", r#"<w:b/><w:bCs/><w:i/><w:iCs/><w:spacing w:val="5"/>"#),
        id if id.len() == 4 && id.starts_with("TOC") => id[3..].parse().map_or_else(|_| s("", ""), toc),
        _ => s("", ""),
    }
}

/// `b`'s `w:style` as the XML route wrote it.
fn xml_definition(b: &crate::services::styles::builtin::BuiltIn) -> String {
    use crate::format::StyleKind as K;
    let base = match (b.kind, b.id) {
        (_, "Normal" | "DefaultParagraphFont" | "TableNormal" | "NoList") => None,
        (_, "TOCHeading") => Some("Heading1"),
        (K::Paragraph, _) => Some("Normal"),
        (K::Character, _) => Some("DefaultParagraphFont"),
        (K::Table, _) => Some("TableNormal"),
        (K::Numbering, _) => None,
    };
    let kind = match b.kind {
        K::Paragraph => "paragraph",
        K::Character => "character",
        K::Table => "table",
        K::Numbering => "numbering",
    };
    let mut x = format!(r#"<w:style w:type="{kind}" w:styleId="{}"><w:name w:val="{}"/>"#, b.id, b.name);
    if let Some(base) = base {
        x.push_str(&format!(r#"<w:basedOn w:val="{base}"/>"#));
    }
    if b.kind == K::Paragraph && (b.id.starts_with("Heading") || matches!(b.id, "TOCHeading" | "TableofFigures")) {
        x.push_str(r#"<w:next w:val="Normal"/>"#);
    }
    x.push_str(&format!(r#"<w:uiPriority w:val="{}"/>"#, b.priority));
    for (on, flag) in [(b.semi_hidden, "<w:semiHidden/>"), (b.unhide_when_used, "<w:unhideWhenUsed/>"), (b.quick, "<w:qFormat/>")] {
        if on {
            x.push_str(flag);
        }
    }
    let (ppr, rpr) = xml_formatting(b.id);
    let ppr = if b.id == "TableGrid" { String::new() } else { ppr };
    if !ppr.is_empty() && b.kind == K::Paragraph {
        x.push_str(&format!("<w:pPr>{ppr}</w:pPr>"));
    }
    if !rpr.is_empty() {
        x.push_str(&format!("<w:rPr>{rpr}</w:rPr>"));
    }
    if b.id == "TableGrid" {
        let line = |side: &str| format!(r#"<w:{side} w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#);
        let sides: String = ["top", "left", "bottom", "right", "insideH", "insideV"].into_iter().map(line).collect();
        x.push_str(&format!(
            r#"<w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:tblPr><w:tblBorders>{sides}</w:tblBorders></w:tblPr>"#
        ));
    }
    x.push_str("</w:style>");
    x
}

/// Every built-in style's definition is the one its XML read as, and writes as that XML; List Bullet's in its own
/// list too.
#[test]
fn built_in_styles_are_their_xml() {
    use crate::services::styles::builtin::{BUILT_IN, definition};
    let cx = Cx::of_xml(&format!("<w:styles xmlns:w=\"{}\">", crate::docx::write::props::W_NS));
    let colours = ThemeColours::default();
    let mut diffs = Differences::default();
    for b in &BUILT_IN {
        let old = xml_definition(b);
        let typed = definition(b);
        // The values the XML read as: but for what reading kept of how it was written beside them
        let values = |mut s: crate::format::Style| {
            let f = &mut s.format;
            (f.para.foreign, f.char.foreign, f.table.foreign) = (None, None, None);
            s
        };
        assert_eq!(read_style(&cx)(&old).map(values), Some(values(typed.clone())), "{}: {old}", b.id);
        diffs.check("built-in styles", b.id, &old, &crate::docx::style_xml::serialize(&typed, &colours, &cx), read_style(&cx));
        if b.id == "ListBullet" {
            let w = &cx.w;
            let mut style = Props::parse(Some(&old), "style", &cx);
            let mut ppr = Props::parse(style.get("pPr"), "pPr", &cx);
            ppr.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"7\"/></{w}numPr>"));
            ppr.set("contextualSpacing", format!("<{w}contextualSpacing/>"));
            style.set("pPr", ppr.xml().expect("a pPr"));
            let mut typed = typed;
            super::super::styles::bulleted(&mut typed, 7);
            diffs.check(
                "List Bullet in its list",
                b.id,
                &style.xml().expect("a style"),
                &crate::docx::style_xml::serialize(&typed, &colours, &cx),
                read_style(&cx),
            );
        }
    }
    diffs.finish("built-in styles");
}

// A style's changes through the API

/// Formatting commands run on a style as the XML route ran them: its table commands; the character and paragraph
/// formatting commands are [`formatting_a_style_writes_as_its_xml_patch_did`]'s.
fn xml_format_style_with(e: &Editor, raw: String, kind: StyleKind, region: Option<&'static str>, commands: &[Command]) -> Result<String, Error> {
    let cx = e.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?.typed().styles.cx();
    for c in commands {
        super::super::commands::bounds::bounds(c)?;
        if kind != StyleKind::Table {
            return Err(Error::invalid(format!("only a table style takes {:?}", c.id())));
        }
    }
    let mut raw = raw;
    for c in commands {
        raw = xml_table_style_edit(e, &raw, region, c, &cx)?;
    }
    Ok(raw)
}

fn xml_referable(e: &Editor, id: &StyleId, kind: StyleKind, what: &str) -> Result<Option<String>, Error> {
    use crate::services::styles::builtin;
    let typed = e.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?.typed();
    match typed.cascade.styles.get(&id.0) {
        Some(s) if s.kind == kind => Ok(None),
        Some(_) => Err(Error::invalid(format!("the {what} is a style of another type"))),
        None => match builtin::BUILT_IN.iter().find(|b| b.id == id.0 && b.kind == kind) {
            Some(b) => Ok(Some(xml_definition(b))),
            None => Err(Error::invalid(format!("the {what} doesn't exist"))),
        },
    }
}

/// A style's XML with `change` written into it, as the XML route wrote it, with the other styles to write with it.
fn xml_changed_style(e: &Editor, raw: String, kind: StyleKind, change: &StyleChange) -> Result<(String, Vec<String>), Error> {
    use crate::services::styles::{check_name, describe};
    let typed = e.doc.resolver.as_ref().ok_or_else(|| Error::unavailable("no styles"))?.typed();
    let (part, styles) = (&*typed.styles, &*typed.cascade.styles);
    let xml = |id: &str| typed.style_xml(id).map(std::borrow::Cow::into_owned).unwrap_or_default();
    let cx = part.cx();
    let w = cx.w.clone();
    let mut also: Vec<String> = Vec::new();
    let para = kind == StyleKind::Paragraph;
    if !para && (change.next.is_some() || change.auto_update.is_some() || change.list.is_some()) {
        return Err(Error::invalid("the following style, automatic updating and a list is a paragraph style's"));
    }
    if kind != StyleKind::Table && (!change.parts.is_empty() || change.row_band.is_some() || change.column_band.is_some()) {
        return Err(Error::invalid("table parts and bands are a table style's"));
    }
    if kind != StyleKind::Numbering && change.list_levels.is_some() {
        return Err(Error::invalid("levels are a list style's"));
    }
    let mut raw = raw;
    if change.clear {
        raw = edit_child(&raw, "pPr", &cx, |p| p.kids.retain(|(n, _)| n == "numPr"));
        raw = set_child(&raw, &cx, "rPr", None);
        if kind == StyleKind::Table {
            for child in ["tblPr", "trPr", "tcPr"] {
                raw = set_child(&raw, &cx, child, None);
            }
        }
    }
    raw = xml_format_style_with(e, raw, kind, None, &change.formatting)?;
    for part in &change.parts {
        let region = part.part.region();
        if part.clear {
            let mut el = Props::parse(Some(&raw), "style", &cx);
            match region {
                Some(r) => {
                    el.kids.retain(|(n, x)| n != "tblStylePr" || cx.parse(x, |_, p| crate::docx::xml::attr(p, "type") != Some(r)).unwrap_or(true))
                }
                None => el.kids.retain(|(n, _)| !["pPr", "rPr", "tblPr", "trPr", "tcPr"].contains(&n.as_str())),
            }
            raw = el.xml().unwrap_or(raw);
        }
        raw = xml_format_style_with(e, raw, kind, region, &part.formatting)?;
    }
    if change.row_band.is_some() || change.column_band.is_some() {
        for (n, name) in [(change.row_band, "tblStyleRowBandSize"), (change.column_band, "tblStyleColBandSize")] {
            let Some(n) = n else { continue };
            super::super::commands::bounds::within("the band size", n as f64, 1.0, 32767.0)?;
            raw = edit_child(&raw, "tblPr", &cx, |p| {
                p.reorder(TBLPR);
                p.set(name, format!("<{w}{name} {w}val=\"{n}\"/>"));
            });
        }
    }
    let id = part.style_from_xml(&raw).map(|s| s.id.to_string()).ok_or_else(|| Error::invalid("not a style"))?;
    let val = |name: &str, v: &str| format!("<{w}{name} {w}val=\"{}\"/>", esc(v));
    if let Some(b) = &change.based_on {
        if let Some(b) = b {
            if b.0 == id {
                return Err(Error::invalid("a style can't be based on itself"));
            }
            also.extend(xml_referable(e, b, kind, "base")?);
        }
        raw = set_child(&raw, &cx, "basedOn", b.as_ref().map(|b| val("basedOn", &b.0)));
    }
    if let Some(n) = &change.next {
        if let Some(n) = n.as_ref().filter(|n| n.0 != id) {
            also.extend(xml_referable(e, n, StyleKind::Paragraph, "following style")?);
        }
        raw = set_child(&raw, &cx, "next", n.as_ref().map(|n| val("next", &n.0)));
    }
    if let Some(on) = change.auto_update {
        raw = set_child(&raw, &cx, "autoRedefine", on.then(|| format!("<{w}autoRedefine/>")));
    }
    if let Some(on) = change.quick {
        raw = set_child(&raw, &cx, "qFormat", on.then(|| format!("<{w}qFormat/>")));
    }
    if let Some(l) = &change.list {
        if let Some(l) = l {
            if !typed.cascade.lists.lists.contains_key(&crate::document::paragraph::ListId(l.list)) || l.list == 0 {
                return Err(Error::invalid("no such list"));
            }
            if l.level > 8 {
                return Err(Error::invalid("a list level is 0 to 8"));
            }
        }
        raw = edit_child(&raw, "pPr", &cx, |p| match l {
            Some(StyleList { list, level: 0 }) => p.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"{list}\"/></{w}numPr>")),
            Some(StyleList { list, level }) => {
                p.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"{level}\"/><{w}numId {w}val=\"{list}\"/></{w}numPr>"))
            }
            None => p.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"0\"/></{w}numPr>")),
        });
    }
    if let Some(name) = &change.name {
        let first = name.trim();
        let cur = part.style_from_xml(&raw).ok_or_else(|| Error::invalid("not a style"))?;
        if !describe::display_name(&cur).eq_ignore_ascii_case(first) {
            check_name(styles, first, Some(&id)).map_err(|_| Error::invalid("the name is refused"))?;
            let aliases = cur.aliases.iter().map(|a| a.to_string());
            let names: Vec<String> = if cur.custom.unwrap_or(false) {
                std::iter::once(first.to_string()).chain(aliases).collect()
            } else {
                [cur.name.as_deref().unwrap_or_default().to_string(), first.to_string()].into_iter().chain(aliases).collect()
            };
            raw = set_child(&raw, &cx, "name", Some(val("name", &names.join(","))));
        }
    }
    let link_of = |id: &str| styles.get(id).and_then(|s| s.link.as_deref().map(str::to_string));
    let was_link = link_of(&id);
    let mut link = was_link.clone();
    if let Some(l) = &change.link {
        let other = match kind {
            StyleKind::Paragraph => StyleKind::Character,
            StyleKind::Character => StyleKind::Paragraph,
            _ => return Err(Error::invalid("only paragraph and character styles link")),
        };
        if let Some(l) = l {
            match styles.get(&l.0) {
                Some(s) if s.kind == other => {}
                Some(_) => return Err(Error::invalid("a style links to one of the other type")),
                None => return Err(Error::invalid("the linked style doesn't exist")),
            }
        }
        link = l.as_ref().map(|l| l.0.clone());
        raw = set_child(&raw, &cx, "link", link.as_ref().map(|l| val("link", l)));
        let unlink = |raw: &str| set_child(raw, &cx, "link", None);
        if let Some(old) = was_link.as_ref().filter(|o| Some(*o) != link.as_ref())
            && styles.get(old).is_some_and(|s| s.link.as_deref() == Some(&*id))
        {
            also.push(unlink(&xml(old)));
        }
        if let Some(new) = &link
            && let Some(s) = styles.get(new)
            && s.link.as_deref() != Some(&*id)
        {
            if let Some(partner) = s.link.as_deref().filter(|p| **p != *id)
                && styles.get(partner).is_some_and(|p| p.link.as_deref() == Some(new.as_str()))
            {
                also.push(unlink(&xml(partner)));
            }
            also.push(set_child(&xml(new), &cx, "link", Some(val("link", &id))));
        }
    }
    let rpr = |x: &str| Props::parse(Some(x), "style", &cx).get("rPr").map(str::to_string);
    let before = typed.style_xml(&id).map(|x| rpr(&x)).unwrap_or_default();
    if let Some(l) = &link
        && rpr(&raw) != before
        && matches!(kind, StyleKind::Paragraph | StyleKind::Character)
    {
        let half = also.iter().position(|x| part.style_from_xml(x).is_some_and(|s| *s.id == **l));
        let base = match half {
            Some(i) => Some(also.remove(i)),
            None => styles.get(l).filter(|s| s.link.as_deref() == Some(&*id)).map(|_| xml(l)),
        };
        if let Some(base) = base {
            also.push(set_child(&base, &cx, "rPr", rpr(&raw)));
        }
    }
    Ok((raw, also))
}

/// The representative changes a style of `kind` is given, its document's styles `others` to refer to.
fn style_changes(kind: StyleKind, others: &crate::format::StyleSheet, list: Option<i64>) -> Vec<StyleChange> {
    let of = |k: StyleKind| others.in_order().into_iter().map(|e| e.style.clone()).rfind(|s| s.kind == k).map(|s| StyleId(s.id.to_string()));
    let mut out = vec![
        StyleChange { name: Some("Renamed, Again".into()), ..Default::default() },
        StyleChange { based_on: Some(None), ..Default::default() },
        StyleChange { quick: Some(true), ..Default::default() },
        StyleChange { quick: Some(false), ..Default::default() },
        StyleChange { clear: true, ..Default::default() },
    ];
    if let Some(b) = of(kind) {
        out.push(StyleChange { based_on: Some(Some(b)), ..Default::default() });
    }
    match kind {
        StyleKind::Paragraph => {
            out.push(StyleChange { next: Some(None), ..Default::default() });
            out.push(StyleChange { next: Some(Some(StyleId("Heading1".into()))), ..Default::default() });
            out.push(StyleChange { auto_update: Some(true), ..Default::default() });
            out.push(StyleChange { auto_update: Some(false), quick: Some(true), ..Default::default() });
            out.push(StyleChange { list: Some(None), ..Default::default() });
            out.push(StyleChange { based_on: Some(Some(StyleId("Title".into()))), ..Default::default() });
            if let Some(n) = list {
                out.push(StyleChange { list: Some(Some(StyleList { list: n, level: 0 })), ..Default::default() });
                out.push(StyleChange { list: Some(Some(StyleList { list: n, level: 3 })), ..Default::default() });
            }
            out.push(StyleChange { link: Some(None), ..Default::default() });
            out.extend(of(StyleKind::Character).map(|c| StyleChange { link: Some(Some(c)), ..Default::default() }));
        }
        StyleKind::Character => {
            out.push(StyleChange { link: Some(None), ..Default::default() });
            out.extend(of(StyleKind::Paragraph).map(|c| StyleChange { link: Some(Some(c)), ..Default::default() }));
        }
        StyleKind::Table => {
            out.push(StyleChange { row_band: Some(2), column_band: Some(3), ..Default::default() });
            out.push(StyleChange { formatting: table_commands(), ..Default::default() });
            for part in [TableStylePart::WholeTable, TableStylePart::HeaderRow, TableStylePart::OddBandedRows] {
                out.push(StyleChange { parts: vec![TablePartFormatting { part, clear: true, formatting: Vec::new() }], ..Default::default() });
                let formatting =
                    table_commands().into_iter().filter(|c| !matches!(c, Command::SetTableAlignment(_) | Command::SetTableIndent(_))).collect();
                out.push(StyleChange { parts: vec![TablePartFormatting { part, clear: false, formatting }], ..Default::default() });
            }
        }
        StyleKind::Numbering => {}
    }
    out
}

/// Every fixture style given each representative change through the API (but character and paragraph formatting)
/// writes, with the styles it writes with it, as patching their XML wrote, and is refused alike.
#[test]
fn a_style_s_changes_write_as_their_xml_patch_did() {
    over_fixtures("a style's changes", |path, d, dx, diffs| {
        let mut e = Editor::with_session(d.clone(), dx.clone(), fonts());
        let typed = d.resolver.as_ref().expect("a resolver").typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        let list = typed.cascade.lists.lists.keys().map(|l| l.0).filter(|n| *n != 0).min();
        for def in styles_of(d, 40) {
            let raw = def.xml(&colours, &cx).into_owned();
            let kind = def.get().kind;
            for change in style_changes(kind, &typed.cascade.styles, list) {
                let what = format!("{path}: {} {change:?}", def.get().id);
                let old = xml_changed_style(&e, raw.clone(), kind, &change);
                let new = e.changed_style(def.clone(), kind, &change, fonts());
                match (old, new) {
                    (Ok((old, old_also)), Ok((new, new_also))) => {
                        // A region emptied, then edited again: the XML route wrote it without its type
                        let untyped = old.contains(&format!("<{}tblStylePr>", cx.w));
                        let lost = "a region emptied and edited again: its type lost by the XML route";
                        diffs.check_or_lost("changed", lost, untyped, &what, &old, &new.xml(&colours, &cx), read_style(&cx));
                        diffs.same_outcome("styles written with it", &what, old_also.len() == new_also.len(), true);
                        for (a, b) in old_also.iter().zip(&new_also) {
                            diffs.check("written with it", &what, a, &b.xml(&colours, &cx), read_style(&cx));
                        }
                    }
                    (Err(_), Err(_)) => diffs.compared += 1,
                    (a, b) => {
                        diffs.same_outcome("changes refused", &format!("{what}: {:?} {:?}", a.as_ref().err(), b.as_ref().err()), a.is_ok(), b.is_ok())
                    }
                }
            }
        }
    });
}

/// A new style made from formatting (every fixture body paragraph's, and none), and a linked style's character half,
/// write as the XML route wrote them.
#[test]
fn a_new_style_writes_as_its_xml_did() {
    over_fixtures("a new style", |path, d, dx, diffs| {
        let res = d.resolver.as_deref().expect("a resolver");
        let typed = res.typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        let main = Cx::of_xml(&dx.source.as_ref().expect("a source").xml);
        let w = cx.w.clone();
        let paras = d.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None });
        for (k, p) in paras.take(100).enumerate() {
            let run = p.inlines.iter().find_map(|i| i.run());
            let (ppr, rpr) = xml_selection(p, run, res, &main);
            let (tp, tr) = selection_props(Some(p.style.direct.get()), run.and_then(|r| r.direct.props()).map(|r| &**r));
            for (kind, xml_type, name, base) in [
                (StyleKind::Paragraph, "paragraph", "Style 1", Some("Normal")),
                (StyleKind::Character, "character", "Mine, Also", None),
                (StyleKind::Table, "table", "T & <A>", Some("TableGrid")),
            ] {
                let id = format!("Made{k}");
                let (ppr, rpr, tp, tr) = match kind {
                    StyleKind::Paragraph => (ppr.clone(), rpr.clone(), tp.clone(), tr.clone()),
                    StyleKind::Character => (None, rpr.clone(), None, tr.clone()),
                    _ => (None, None, None, None),
                };
                let (ppr_copy, rpr_copy) = (ppr.clone(), rpr.clone());
                let based = base.map_or(String::new(), |b| format!("<{w}basedOn {w}val=\"{}\"/>", esc(b)));
                let next = if kind == StyleKind::Paragraph { format!("<{w}next {w}val=\"{}\"/>", esc(&id)) } else { String::new() };
                let old = format!(
                    "<{w}style {w}type=\"{xml_type}\" {w}customStyle=\"1\" {w}styleId=\"{i}\"><{w}name {w}val=\"{n}\"/>{based}{next}<{w}qFormat/>{}{}</{w}style>",
                    ppr.unwrap_or_default(),
                    rpr.unwrap_or_default(),
                    i = esc(&id),
                    n = esc(name)
                );
                let new = created_style(&id, kind, name, base, tp, tr);
                let what = format!("{path}: paragraph {k} as a {xml_type} style");
                diffs.check_or_lost(
                    "made from formatting",
                    LOST,
                    lost_in_styles(&ppr_copy, &rpr_copy, &cx),
                    &what,
                    &old,
                    &crate::docx::style_xml::serialize(&new, &colours, &cx),
                    read_style(&cx),
                );
                if kind == StyleKind::Paragraph {
                    let rpr = Props::parse(Some(&old), "style", &cx).get("rPr").map(str::to_string).unwrap_or_default();
                    let half = format!(
                        "<{w}style {w}type=\"character\" {w}customStyle=\"1\" {w}styleId=\"{id}Char\"><{w}name {w}val=\"{n} Char\"/><{w}basedOn {w}val=\"DefaultParagraphFont\"/><{w}link {w}val=\"{id}\"/>{rpr}</{w}style>",
                        n = esc(name)
                    );
                    let typed_half = char_half(&format!("{id}Char"), name, &id, new.format.char.clone());
                    diffs.check_or_lost(
                        "a linked style's half",
                        LOST,
                        lost_in_styles(&None, &rpr_copy, &cx),
                        &what,
                        &half,
                        &crate::docx::style_xml::serialize(&typed_half, &colours, &cx),
                        read_style(&cx),
                    );
                }
            }
        }
    });
}

// The style dialog

/// The dialog's fill, cell alignment, border and band sizes written into a table style, as the XML route wrote them.
fn xml_table_style_edits(raw: &str, was: &StyleDialog, now: &StyleDialog, region: Option<&'static str>, cx: &Cx) -> String {
    let w = cx.w.clone();
    let mut raw = raw.to_string();
    let root = |raw: &str, child: &str, order: &'static [&'static str], edit: &dyn Fn(&mut Props)| {
        edit_child(raw, child, cx, |p| {
            p.reorder(order);
            edit(p);
        })
    };
    let cell = |raw: &str, edit: &dyn Fn(&mut Props)| match region {
        Some(r) => edit_table_part(raw, r, "tcPr", cx, edit),
        None => root(raw, "tcPr", TCPR, edit),
    };
    if now.fill != was.fill {
        raw = cell(&raw, &|p| match now.fill {
            Some(c) => p.set("shd", format!("<{w}shd {w}val=\"clear\" {w}color=\"auto\" {w}fill=\"{:06X}\"/>", c.0)),
            None => p.remove("shd"),
        });
    }
    if now.cell_alignment != was.cell_alignment {
        let v = super::super::table_format::v_align_xml(&w, super::super::table_format::cell_alignment(now.cell_alignment));
        raw = cell(&raw, &|p| p.set("vAlign", v.clone()));
    }
    if now.border != was.border {
        let edges = |name: &str| -> String {
            let one = |side: &str| xml_dialog_line(&w, side, &now.border);
            let sides: String = ["top", "left", "bottom", "right", "insideH", "insideV"].iter().map(|s| one(s)).collect();
            format!("<{w}{name}>{sides}</{w}{name}>")
        };
        raw = match region {
            Some(r) => edit_table_part(&raw, r, "tcPr", cx, |p| p.set("tcBorders", edges("tcBorders"))),
            None => root(&raw, "tblPr", TBLPR, &|p| p.set("tblBorders", edges("tblBorders"))),
        };
    }
    if now.row_band != was.row_band || now.column_band != was.column_band {
        let (rows, cols) = (now.row_band.max(1), now.column_band.max(1));
        raw = root(&raw, "tblPr", TBLPR, &|p| {
            p.set("tblStyleRowBandSize", format!("<{w}tblStyleRowBandSize {w}val=\"{rows}\"/>"));
            p.set("tblStyleColBandSize", format!("<{w}tblStyleColBandSize {w}val=\"{cols}\"/>"));
        });
    }
    raw
}

fn xml_dialog_line(w: &str, side: &str, b: &Option<Border>) -> String {
    match b {
        Some(b) => {
            let color = match super::super::readback::rgb_only(&b.color) {
                AutoOr::Auto => "auto".to_string(),
                AutoOr::Value(c) => format!("{:06X}", c.0),
            };
            format!(
                "<{w}{side} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"0\" {w}color=\"{color}\"/>",
                crate::docx::line_style::border_val(super::super::readback::border_style(b.style)),
                (b.width.0 * 8.0).round() as i64
            )
        }
        None => format!("<{w}{side} {w}val=\"nil\"/>"),
    }
}

/// The dialog's base, following style, flags and name written into a style, as the XML route wrote them.
fn xml_dialog_fields(raw: &str, was: &StyleDialog, now: &StyleDialog, cx: &Cx) -> String {
    let w = cx.w.clone();
    let mut pending = raw.to_string();
    let id_ref = |id: &StyleId| esc(&id.0).to_string();
    if now.based_on != was.based_on {
        pending = set_child(&pending, cx, "basedOn", now.based_on.as_ref().map(|b| format!("<{w}basedOn {w}val=\"{}\"/>", id_ref(b))));
    }
    if now.next != was.next {
        pending = set_child(&pending, cx, "next", now.next.as_ref().map(|n| format!("<{w}next {w}val=\"{}\"/>", id_ref(n))));
    }
    if now.quick != was.quick {
        pending = set_child(&pending, cx, "qFormat", now.quick.then(|| format!("<{w}qFormat/>")));
    }
    if now.auto_update != was.auto_update {
        pending = set_child(&pending, cx, "autoRedefine", now.auto_update.then(|| format!("<{w}autoRedefine/>")));
    }
    if now.name != was.name {
        pending = set_child(&pending, cx, "name", Some(format!("<{w}name {w}val=\"{}\"/>", esc(now.name.trim()))));
    }
    pending
}

/// A new style made a style of another dialog type, as the XML route wrote it.
fn xml_retyped(raw: &str, kind: StyleDialogType, id: &str, cx: &Cx) -> String {
    let w = cx.w.clone();
    let xml_type = match kind {
        StyleDialogType::Character => "character",
        StyleDialogType::Table => "table",
        StyleDialogType::List => "numbering",
        _ => "paragraph",
    };
    let mut pending = with_start(raw, &format!("<{w}style {w}type=\"{xml_type}\" {w}customStyle=\"1\" {w}styleId=\"{}\">", esc(id)));
    if kind != StyleDialogType::Paragraph && kind != StyleDialogType::Linked {
        pending = set_child(&pending, cx, "next", None);
    }
    if kind == StyleDialogType::Character {
        pending = set_child(&pending, cx, "pPr", None);
    }
    pending
}

/// The dialog changed from `v` in each representative way.
fn dialog_changes(v: &StyleDialog) -> Vec<StyleDialog> {
    let rgb = crate::api::value::Color;
    let mut out = vec![
        StyleDialog { based_on: None, ..v.clone() },
        StyleDialog { based_on: Some(StyleId("Normal".into())), ..v.clone() },
        StyleDialog { next: None, ..v.clone() },
        StyleDialog { next: Some(StyleId("Heading1".into())), ..v.clone() },
        StyleDialog { quick: !v.quick, ..v.clone() },
        StyleDialog { auto_update: !v.auto_update, ..v.clone() },
        StyleDialog { name: " Renamed & More ".into(), ..v.clone() },
        StyleDialog { name: "A, B,, C".into(), quick: !v.quick, ..v.clone() },
    ];
    if v.kind == StyleDialogType::Table {
        out.push(StyleDialog { fill: Some(rgb(0x00FF00)), ..v.clone() });
        out.push(StyleDialog { fill: None, cell_alignment: CellAlignment::Bottom, ..v.clone() });
        out.push(StyleDialog { border: Some(line(1.5)), ..v.clone() });
        out.push(StyleDialog { border: None, row_band: 4, ..v.clone() });
        out.push(StyleDialog { column_band: 2, ..v.clone() });
    }
    out
}

/// Every fixture style's dialog (and a new style's) changed in each representative way, its type changed, its
/// numbering set, and a table style's Format entries, write as patching its XML wrote.
#[test]
fn the_style_dialog_writes_as_its_xml_patch_did() {
    over_fixtures("the style dialog", |path, d, dx, diffs| {
        let e = Editor::with_session(d.clone(), dx.clone(), fonts());
        let typed = d.resolver.as_ref().expect("a resolver").typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        let mut opened: Vec<StyleDialog> =
            styles_of(d, 25).iter().filter_map(|s| e.style_dialog_open(StyleOpen::Modify(StyleId(s.get().id.to_string()))).ok()).collect();
        opened.extend(e.style_dialog_open(StyleOpen::Create).ok());
        for was in &opened {
            let start = e.style_of_xml(&was.pending).expect("the dialog's style");
            let what = |w: &str| format!("{path}: {} {w}", start.get().id);
            // Its style as opened: the built-in one's as the XML route read it
            if let StyleOpen::Modify(id) = &was.open
                && !typed.cascade.styles.styles.contains_key(id.0.as_str())
                && let Some(b) = crate::services::styles::builtin::built_in(&id.0)
            {
                diffs.check("opened on a built-in style", &what("opened"), &xml_definition(b), &was.pending, read_style(&cx));
            }
            for now in dialog_changes(was) {
                let region = if was.kind == StyleDialogType::Table { was.table_part.region() } else { None };
                let mut old = was.pending.clone();
                if was.kind == StyleDialogType::Table {
                    old = xml_table_style_edits(&old, was, &now, region, &cx);
                }
                let old = xml_dialog_fields(&old, was, &now, &cx);
                let mut new = start.clone();
                if was.kind == StyleDialogType::Table {
                    new = table_style_edits(&new, was, &now, region.and_then(|r| crate::docx::style_xml::region_kind(Some(r))));
                }
                let new = dialog_fields(&new, was, &now);
                diffs.check("updated", &what(&format!("{now:?}")), &old, &new.xml(&colours, &cx), read_style(&cx));
            }
            for num in [None, typed.cascade.lists.lists.keys().map(|l| l.0).find(|n| *n != 0)] {
                let w = cx.w.clone();
                let old = edit_child(&was.pending, "pPr", &cx, |p| match num {
                    Some(n) => p.set("numPr", format!("<{w}numPr><{w}numId {w}val=\"{n}\"/></{w}numPr>")),
                    None => p.remove("numPr"),
                });
                diffs.check("numbering", &what(&format!("{num:?}")), &old, &numbered(&start, num).xml(&colours, &cx), read_style(&cx));
            }
            if was.open == StyleOpen::Create {
                for kind in [StyleDialogType::Character, StyleDialogType::Table, StyleDialogType::List, StyleDialogType::Linked] {
                    let old = xml_retyped(&was.pending, kind, &start.get().id, &cx);
                    diffs.check("retyped", &what(&format!("{kind:?}")), &old, &retyped(&start, kind).xml(&colours, &cx), read_style(&cx));
                }
            }
            if was.kind == StyleDialogType::Table {
                for part in [TableStylePart::WholeTable, TableStylePart::HeaderRow, TableStylePart::TotalRow] {
                    let v = StyleDialog { table_part: part, ..was.clone() };
                    table_formats(&e, &v, &start, &cx, &colours, path, diffs);
                }
            }
        }
    });
}

/// A table style's Format entries, as the XML route wrote them against the typed route.
fn table_formats(e: &Editor, v: &StyleDialog, start: &StyleDef, cx: &Cx, colours: &ThemeColours, path: &str, diffs: &mut Differences) {
    use crate::api::services::DialogValues;
    let Some(bs) = e.borders_shading_values(Some(&v.pending), false) else { return };
    let Some(tp) = e.style_table_properties(&v.pending) else { return };
    let Some(band) = e.style_banding(&v.pending) else { return };
    let rgb = crate::api::value::Color;
    let changes = [
        (
            DialogValues::BordersShading(Box::new(bs.clone())),
            DialogValues::BordersShading(Box::new(crate::api::services::BordersShadingDialog { top: Some(line(2.25)), ..bs.clone() })),
        ),
        (
            DialogValues::BordersShading(Box::new(bs.clone())),
            DialogValues::BordersShading(Box::new(crate::api::services::BordersShadingDialog { diagonal_up: Some(line(0.5)), ..bs.clone() })),
        ),
        (
            DialogValues::BordersShading(Box::new(bs.clone())),
            DialogValues::BordersShading(Box::new(crate::api::services::BordersShadingDialog {
                fill: crate::api::value::NoneOr::Value(rgb(0x123456)),
                ..bs.clone()
            })),
        ),
        (
            DialogValues::BordersShading(Box::new(bs.clone())),
            DialogValues::BordersShading(Box::new(crate::api::services::BordersShadingDialog {
                fill: crate::api::value::NoneOr::None,
                pattern: crate::api::command::ShadingPattern::Clear,
                pattern_color: AutoOr::Auto,
                ..bs.clone()
            })),
        ),
        (
            DialogValues::TableProperties(tp.clone()),
            DialogValues::TableProperties(crate::api::services::TablePropertiesDialog {
                alignment: HorizontalAlignment::Right,
                indent: Points(7.5),
                ..tp.clone()
            }),
        ),
        (
            DialogValues::TableProperties(tp.clone()),
            DialogValues::TableProperties(crate::api::services::TablePropertiesDialog {
                row_can_break: crate::api::state::Tri::Uniform(!matches!(tp.row_can_break, crate::api::state::Tri::Uniform(true))),
                cell_alignment: crate::api::state::Tri::Uniform(CellAlignment::Center),
                ..tp.clone()
            }),
        ),
        (DialogValues::Banding(band.clone()), DialogValues::Banding(crate::api::services::BandingDialog { row_band: 3, column_band: 5 })),
    ];
    for (a, b) in changes {
        let what = format!("{path}: {} {:?} {b:?}", start.get().id, v.table_part);
        let old = xml_table_format(&v.pending, v.table_part.region(), &a, &b, cx);
        let new = table_format_edits(start.clone(), v.table_part.region().and_then(|r| crate::docx::style_xml::region_kind(Some(r))), &a, &b);
        match (old, new) {
            (Ok(old), Ok(new)) => diffs.check("table formats", &what, &old, &new.xml(colours, cx), read_style(cx)),
            (a, b) => diffs.same_outcome("table formats refused", &what, a.is_ok(), b.is_ok()),
        }
    }
}

fn xml_table_format(
    raw: &str,
    region: Option<&'static str>,
    start: &crate::api::services::DialogValues,
    now: &crate::api::services::DialogValues,
    cx: &Cx,
) -> Result<String, Error> {
    use crate::api::services::DialogValues;
    let w = cx.w.clone();
    let root = |raw: &str, child: &str, order: &'static [&'static str], edit: &dyn Fn(&mut Props)| {
        edit_child(raw, child, cx, |p| {
            p.reorder(order);
            edit(p);
        })
    };
    let cell = |raw: &str, edit: &dyn Fn(&mut Props)| match region {
        Some(r) => edit_table_part(raw, r, "tcPr", cx, edit),
        None => root(raw, "tcPr", TCPR, edit),
    };
    let line = |side: &str, b: &Option<Border>| xml_dialog_line(&w, side, b);
    let mut raw = raw.to_string();
    match (start, now) {
        (DialogValues::BordersShading(a), DialogValues::BordersShading(b)) => {
            let edges = |x: &crate::api::services::BordersShadingDialog| {
                (x.top.clone(), x.left.clone(), x.bottom.clone(), x.right.clone(), x.between.clone(), x.inside_vertical.clone())
            };
            let edged = edges(a) != edges(b);
            let cross = |side: &str, was: &Option<Border>, now: &Option<Border>| (was != now).then(|| line(side, now));
            let (down, up) = (cross("tl2br", &a.diagonal_down, &b.diagonal_down), cross("tr2bl", &a.diagonal_up, &b.diagonal_up));
            let sides = [
                ("top", &b.top),
                ("left", &b.left),
                ("bottom", &b.bottom),
                ("right", &b.right),
                ("insideH", &b.between),
                ("insideV", &b.inside_vertical),
            ];
            if edged && region.is_none() {
                let xml = format!("<{w}tblBorders>{}</{w}tblBorders>", sides.iter().map(|(side, x)| line(side, x)).collect::<String>());
                raw = root(&raw, "tblPr", TBLPR, &|p| p.set("tblBorders", xml.clone()));
            }
            if (edged && region.is_some()) || down.is_some() || up.is_some() {
                raw = cell(&raw, &|p| {
                    let mut bd = Props::parse(p.get("tcBorders"), "tcBorders", cx);
                    bd.reorder(TC_BORDERS);
                    if edged && region.is_some() {
                        for side in ["top", "start", "left", "bottom", "end", "right", "insideH", "insideV"] {
                            bd.remove(side);
                        }
                        for (side, x) in &sides {
                            bd.set(side, line(side, x));
                        }
                    }
                    for (side, x) in [("tl2br", &down), ("tr2bl", &up)] {
                        if let Some(x) = x {
                            bd.set(side, x.clone());
                        }
                    }
                    match bd.xml() {
                        Some(x) => p.set("tcBorders", x),
                        None => p.remove("tcBorders"),
                    }
                });
            }
            if (a.fill, a.pattern, a.pattern_color) != (b.fill, b.pattern, b.pattern_color) {
                let fill = match b.fill {
                    crate::api::value::NoneOr::Value(c) => Some(c),
                    crate::api::value::NoneOr::None => None,
                };
                let color = match b.pattern_color {
                    AutoOr::Value(c) => Some(c),
                    AutoOr::Auto => None,
                };
                let clear = fill.is_none() && b.pattern == crate::api::command::ShadingPattern::Clear;
                let sh = super::super::table_format::Shading {
                    pattern: b.pattern.val(),
                    color: color.map(|c| super::super::theme::WordColour::rgb(c.0)),
                    fill: fill.map(|c| super::super::theme::WordColour::rgb(c.0)),
                };
                raw = cell(&raw, &|p| {
                    if clear {
                        p.remove("shd");
                    } else {
                        let x = sh.xml(&w, p.get("shd"), None);
                        p.set("shd", x);
                    }
                });
            }
        }
        (DialogValues::TableProperties(a), DialogValues::TableProperties(b)) => {
            if a.alignment != b.alignment || a.indent != b.indent || a.cell_margins != b.cell_margins {
                let jc = match b.alignment {
                    HorizontalAlignment::Left => "left",
                    HorizontalAlignment::Center => "center",
                    HorizontalAlignment::Right => "right",
                };
                let tw = |p: Points| (p.0 * 20.0).round() as i64;
                let m = &b.cell_margins;
                raw = root(&raw, "tblPr", TBLPR, &|p| {
                    p.set("jc", format!("<{w}jc {w}val=\"{jc}\"/>"));
                    p.set("tblInd", format!("<{w}tblInd {w}w=\"{}\" {w}type=\"dxa\"/>", tw(b.indent)));
                    p.set(
                        "tblCellMar",
                        format!(
                            "<{w}tblCellMar><{w}top {w}w=\"{}\" {w}type=\"dxa\"/><{w}left {w}w=\"{}\" {w}type=\"dxa\"/><{w}bottom {w}w=\"{}\" {w}type=\"dxa\"/><{w}right {w}w=\"{}\" {w}type=\"dxa\"/></{w}tblCellMar>",
                            tw(m.top),
                            tw(m.left),
                            tw(m.bottom),
                            tw(m.right)
                        ),
                    );
                });
            }
            if let (crate::api::state::Tri::Uniform(x), crate::api::state::Tri::Uniform(y)) = (&a.row_can_break, &b.row_can_break)
                && x != y
            {
                let breaks = *y;
                raw = root(&raw, "trPr", TRPR, &|p| if breaks { p.remove("cantSplit") } else { p.set("cantSplit", format!("<{w}cantSplit/>")) });
            }
            if let (crate::api::state::Tri::Uniform(x), crate::api::state::Tri::Uniform(y)) = (&a.cell_alignment, &b.cell_alignment)
                && x != y
            {
                let va = super::super::table_format::v_align_xml(&w, super::super::table_format::cell_alignment(*y));
                raw = cell(&raw, &|p| p.set("vAlign", va.clone()));
            }
        }
        (DialogValues::Banding(a), DialogValues::Banding(b)) if a != b => {
            let (rows, cols) = (b.row_band.max(1), b.column_band.max(1));
            raw = root(&raw, "tblPr", TBLPR, &|p| {
                p.set("tblStyleRowBandSize", format!("<{w}tblStyleRowBandSize {w}val=\"{rows}\"/>"));
                p.set("tblStyleColBandSize", format!("<{w}tblStyleColBandSize {w}val=\"{cols}\"/>"));
            });
        }
        _ => {}
    }
    Ok(raw)
}

// Where the XML route wrote what Word reads otherwise

fn fixture(dir: &str, name: &str) -> Editor {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(dir).join(name);
    Editor::open(&std::fs::read(&path).expect("reads"), fonts()).expect("opens")
}

/// A new style from a paragraph whose formatting is written under the main part's own prefix (or a default
/// namespace) keeps its formatting in the styles part, where the XML route lost it.
#[test]
fn a_style_from_formatting_under_another_prefix_keeps_it() {
    use super::super::position::{Position, StoryId};
    use super::super::selection::{Granularity, Selection};
    use crate::api::styles::{NewStyle, StyleCategory};
    for (k, jc, run) in [(1, "center", "<w:b/>"), (2, "right", "<w:i/>")] {
        let mut e = fixture("model", "namespace-dependencies.docx");
        let at = Position::text(StoryId::Body, super::para_id(&e, k), crate::layout::Unit(2));
        e.selection = Selection::range(at, at, Granularity::Char);
        let new = NewStyle { category: StyleCategory::Paragraph, from_selection: true, ..Default::default() };
        let (out, id) = e.create_style(new, fonts());
        assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
        let id = id.expect("its id").0;
        let typed = e.document().resolver.as_ref().expect("styles").typed();
        let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
        let xml = typed.styles.styles[id.as_str()].xml(&colours, &cx).into_owned();
        assert!(xml.contains(&format!("<w:pPr><w:jc w:val=\"{jc}\"/></w:pPr><w:rPr>{run}</w:rPr>")), "paragraph {k}: {xml}");
        // The XML route's copy: the paragraph's properties as the main part writes them, which the styles part
        // doesn't read
        let d = e.document();
        let res = d.resolver.as_deref().expect("styles");
        let p = d.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None }).nth(k);
        let p = p.expect("the paragraph");
        let (ppr, rpr) = xml_selection(p, p.inlines.iter().find_map(|i| i.run()), res, &Cx::of_xml(&e.docx().source.as_ref().expect("a source").xml));
        assert!(lost_in_styles(&ppr, &rpr, &cx), "paragraph {k}: the XML route kept {ppr:?} {rpr:?}");
    }
}

/// A table style's part emptied and then edited again in one change keeps its type, where the XML route wrote it
/// without one.
#[test]
fn a_table_part_emptied_and_edited_again_keeps_its_type() {
    let mut e = fixture("render", "table-shading.docx");
    let formatting = vec![Command::SetCellShading(None), Command::SetCellAlignment(CellAlignment::Center)];
    let change =
        StyleChange { parts: vec![TablePartFormatting { part: TableStylePart::OddBandedRows, clear: false, formatting }], ..Default::default() };
    let typed = e.document().resolver.as_ref().expect("styles").typed();
    let (cx, colours) = (typed.styles.cx(), typed.cascade.colours);
    let raw = typed.styles.styles["BandedGrid"].xml(&colours, &cx).into_owned();
    let (old, _) = xml_changed_style(&e, raw, StyleKind::Table, &change).expect("the XML route writes it");
    assert!(old.contains("<w:tblStylePr><w:tcPr><w:vAlign w:val=\"center\"/></w:tcPr></w:tblStylePr>"), "the XML route: {old}");
    let out = e.modify_style(&StyleId("BandedGrid".into()), change, fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    let typed = e.document().resolver.as_ref().expect("styles").typed();
    let xml = typed.styles.styles["BandedGrid"].xml(&colours, &cx).into_owned();
    assert_eq!(xml.matches("<w:tblStylePr").count(), xml.matches("<w:tblStylePr w:type=").count(), "{xml}");
    assert!(xml.contains("<w:tblStylePr w:type=\"band1Horz\"><w:tcPr><w:vAlign w:val=\"center\"/></w:tcPr></w:tblStylePr>"), "{xml}");
}
