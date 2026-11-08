//! Revision markup as written from the model: a mark's attributes, the wrappers around tracked runs, paragraph-mark
//! and row marks, and the property-change elements (`w:rPrChange`, `w:pPrChange`, ...), each holding the properties
//! before the change as they were read.

use super::props::{Cx, Props, esc};
use crate::docx::model::{Raw, RunStyle, VMerge};
use crate::docx::review::{CellMarkKind, CellRevisions, GridChange, MarkKind, People, RevisionMark, RowRevisions, TableChange};

/// ` w:id=".." w:author=".." w:date=".."`: a mark's attributes, its date as it was read.
pub(super) fn attrs(w: &str, m: &RevisionMark, people: &People) -> String {
    let author = people.get(m.author).map_or("", |p| &p.name);
    let mut s = format!(" {w}id=\"{}\" {w}author=\"{}\"", m.id.0, esc(author));
    if let Some(d) = &m.date {
        s.push_str(&format!(" {w}date=\"{}\"", esc(&d.raw)));
    }
    s
}

/// The element a mark writes as.
pub(super) fn tag(m: &RevisionMark) -> &'static str {
    match m.kind {
        MarkKind::Insert => "ins",
        MarkKind::Delete => "del",
        MarkKind::MoveFrom => "moveFrom",
        MarkKind::MoveTo => "moveTo",
        // Property changes are written by their own elements (`w:rPrChange`, ...); no run, mark or row slot holds one
        // (the integrity checks enforce it).
        MarkKind::Format => "ins",
    }
}

/// A mark as an empty element (a paragraph mark's or a row's insertion or deletion).
pub(super) fn mark_element(w: &str, m: &RevisionMark, people: &People) -> String {
    format!("<{w}{}{}/>", tag(m), attrs(w, m, people))
}

/// The start and end tags around runs carrying `ins` and `del`: the insertion outside the deletion.
pub(super) fn wrappers(w: &str, ins: Option<&RevisionMark>, del: Option<&RevisionMark>, people: &People) -> (String, String) {
    let (mut open, mut close) = (String::new(), String::new());
    for m in [ins, del].into_iter().flatten() {
        open.push_str(&format!("<{w}{}{}>", tag(m), attrs(w, m, people)));
        close.insert_str(0, &format!("</{w}{}>", tag(m)));
    }
    (open, close)
}

/// A property-change element: `name` with its mark's attributes around the earlier `inner` properties as read (an
/// empty `inner` element when there were none).
pub(super) fn change(w: &str, name: &str, inner: &str, m: &RevisionMark, previous: &Raw, people: &People) -> String {
    let prev = match previous.0.as_deref().map(str::trim) {
        Some(p) if is_element(p, inner) => p.to_string(),
        Some(p) if !p.is_empty() => format!("<{w}{inner}>{p}</{w}{inner}>"),
        _ => format!("<{w}{inner}/>"),
    };
    format!("<{w}{name}{}>{prev}</{w}{name}>", attrs(w, m, people))
}

/// Whether `xml` is one `local` element (any prefix), not its children.
fn is_element(xml: &str, local: &str) -> bool {
    let name = xml.strip_prefix('<').and_then(|s| s.split(|c: char| c.is_whitespace() || c == '>' || c == '/').next()).unwrap_or("");
    name.rsplit(':').next() == Some(local) && xml.ends_with('>')
}

/// A run's `w:rPr` as written for it: its direct properties, with its formatting change last (CT_RPr order).
pub(super) fn run_rpr(run: &RunStyle, cx: &Cx, people: &People) -> String {
    let direct = run.direct.0.as_deref();
    let Some(f) = &run.rev.fmt else { return direct.unwrap_or("").to_string() };
    let mut p = Props::parse(direct, "rPr", cx);
    p.set("rPrChange", change(&cx.w, "rPrChange", "rPr", &f.mark, &f.previous_raw, people));
    p.xml().unwrap_or_default()
}

/// A paragraph mark's `w:rPr`: its direct properties, its insertion or deletion first and its formatting change last
/// (CT_ParaRPr order); `None` when it has nothing.
pub(super) fn mark_rpr(mark: &RunStyle, cx: &Cx, people: &People) -> Option<String> {
    let mut p = Props::parse(mark.direct.0.as_deref(), "rPr", cx);
    for name in ["ins", "del", "moveFrom", "moveTo", "rPrChange"] {
        p.remove(name);
    }
    for m in [&mark.rev.ins, &mark.rev.del].into_iter().flatten() {
        p.set(tag(m), mark_element(&cx.w, m, people));
    }
    if let Some(f) = &mark.rev.fmt {
        p.set("rPrChange", change(&cx.w, "rPrChange", "rPr", &f.mark, &f.previous_raw, people));
    }
    p.xml()
}

/// `w:tblGridChange` with the column widths before the change.
pub(super) fn grid_change(w: &str, g: &GridChange, people: &People) -> String {
    let cols: String = g.previous.iter().map(|c| format!("<{w}gridCol {w}w=\"{}\"/>", super::twips(*c))).collect();
    format!("<{w}tblGridChange{}><{w}tblGrid>{cols}</{w}tblGrid></{w}tblGridChange>", attrs(w, &g.mark, people))
}

/// A table-level change element (`w:tblPrChange`, `w:trPrChange`, `w:tcPrChange`, `w:tblPrExChange`).
pub(super) fn table_change(w: &str, name: &str, inner: &str, c: &TableChange, people: &People) -> String {
    change(w, name, inner, &c.mark, &c.previous_raw, people)
}

/// CT_TrPr's children in schema order (the base properties are a choice group: any order before the marks).
pub(super) const TRPR: &[&str] = &[
    "cnfStyle",
    "divId",
    "gridBefore",
    "gridAfter",
    "wBefore",
    "wAfter",
    "cantSplit",
    "trHeight",
    "tblHeader",
    "tblCellSpacing",
    "jc",
    "hidden",
    "ins",
    "del",
    "trPrChange",
];

/// CT_TblPrEx's children in schema order.
pub(super) const TBLPREX: &[&str] =
    &["tblW", "jc", "tblCellSpacing", "tblInd", "tblBorders", "shd", "tblLayout", "tblCellMar", "tblLook", "tblPrExChange"];

/// CT_TcPr's children in schema order.
pub(super) const TCPR: &[&str] = &[
    "cnfStyle",
    "tcW",
    "gridSpan",
    "hMerge",
    "vMerge",
    "tcBorders",
    "shd",
    "noWrap",
    "tcMar",
    "textDirection",
    "tcFitText",
    "vAlign",
    "hideMark",
    "headers",
    "cellIns",
    "cellDel",
    "cellMerge",
    "tcPrChange",
];

/// The schema names a child must follow: those before `name` in `order`.
pub(super) fn before<'o>(order: &'o [&'o str], name: &str) -> &'o [&'o str] {
    &order[..order.iter().position(|n| *n == name).unwrap_or(order.len())]
}

/// A row's revision children (`w:ins`, `w:del`, `w:trPrChange`) by name, as written from the model (empty: none).
pub(super) fn row_marks(w: &str, r: &RowRevisions, people: &People) -> [(&'static str, String); 3] {
    let el = |m: &Option<std::rc::Rc<RevisionMark>>, name: &str| m.as_ref().map_or(String::new(), |m| format!("<{w}{name}{}/>", attrs(w, m, people)));
    [
        ("ins", el(&r.ins, "ins")),
        ("del", el(&r.del, "del")),
        ("trPrChange", r.change.as_ref().map_or(String::new(), |c| table_change(w, "trPrChange", "trPr", c, people))),
    ]
}

/// A row's `w:tblPrExChange`, as written from the model (empty: none).
pub(super) fn ex_change(w: &str, r: &RowRevisions, people: &People) -> String {
    r.ex_change.as_ref().map_or(String::new(), |c| table_change(w, "tblPrExChange", "tblPrEx", c, people))
}

/// A cell's revision children (`w:cellIns`, `w:cellDel`, `w:cellMerge`, `w:tcPrChange`) by name, as written from the
/// model (empty: none).
pub(super) fn cell_marks(w: &str, c: &CellRevisions, people: &People) -> [(&'static str, String); 4] {
    let mut out = [("cellIns", String::new()), ("cellDel", String::new()), ("cellMerge", String::new()), ("tcPrChange", String::new())];
    if let Some((m, kind)) = &c.structural {
        let a = attrs(w, m, people);
        match kind {
            CellMarkKind::Insert => out[0].1 = format!("<{w}cellIns{a}/>"),
            CellMarkKind::Delete => out[1].1 = format!("<{w}cellDel{a}/>"),
            CellMarkKind::Merge { v_merge, v_merge_orig } => {
                let v = |name: &str, m: &Option<VMerge>| match m {
                    Some(VMerge::Restart) => format!(" {w}{name}=\"rest\""),
                    Some(VMerge::Continue) => format!(" {w}{name}=\"cont\""),
                    Some(VMerge::None) | None => String::new(),
                };
                out[2].1 = format!("<{w}cellMerge{a}{}{}/>", v("vMerge", v_merge), v("vMergeOrig", v_merge_orig));
            }
        }
    }
    if let Some(ch) = &c.change {
        out[3].1 = table_change(w, "tcPrChange", "tcPr", ch, people);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::docx::review::{AuthorId, FormatChange, RevisionId, Timestamp};

    fn people() -> People {
        let mut p = People::default();
        p.author("Author A", None);
        p.author("B & C", None);
        p
    }

    fn mark(id: i64, author: u32, kind: MarkKind) -> Rc<RevisionMark> {
        Rc::new(RevisionMark {
            id: RevisionId(id),
            author: AuthorId(author),
            date: Some(Timestamp::parse("2026-01-02T10:00:00Z")),
            kind,
            move_name: None,
        })
    }

    #[test]
    fn a_deleted_insertion_nests_the_deletion_inside() {
        let (ins, del) = (mark(3, 0, MarkKind::Insert), mark(4, 1, MarkKind::Delete));
        let (open, close) = wrappers("w:", Some(&ins), Some(&del), &people());
        assert_eq!(
            open,
            r#"<w:ins w:id="3" w:author="Author A" w:date="2026-01-02T10:00:00Z"><w:del w:id="4" w:author="B &amp; C" w:date="2026-01-02T10:00:00Z">"#
        );
        assert_eq!(close, "</w:del></w:ins>");
        let (open, close) = wrappers("w:", None, Some(&mark(5, 0, MarkKind::MoveFrom)), &people());
        assert!(open.starts_with("<w:moveFrom w:id=\"5\""), "{open}");
        assert_eq!(close, "</w:moveFrom>");
    }

    #[test]
    fn format_changes_go_last_in_the_run_properties() {
        let cx = Cx::of_xml(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#);
        let doc = crate::docx::read_editable(&crate::docx::blank::blank_docx()).expect("a blank document");
        let mut run = match doc.sections.first().and_then(|s| s.blocks.first()) {
            Some(crate::docx::model::Block::Para(p)) => (*p.mark).clone(),
            _ => panic!("a blank document starts with a paragraph"),
        };
        run.direct = Raw(Some("<w:rPr><w:b/><w:lang w:val=\"en-US\"/></w:rPr>".into()));
        assert_eq!(run_rpr(&run, &cx, &people()), "<w:rPr><w:b/><w:lang w:val=\"en-US\"/></w:rPr>");
        run.rev.fmt = Some(Rc::new(FormatChange {
            mark: mark(7, 0, MarkKind::Insert),
            previous: Rc::new(run.clone()),
            previous_raw: Raw(Some("<w:rPr><w:i/></w:rPr>".into())),
        }));
        assert_eq!(
            run_rpr(&run, &cx, &people()),
            r#"<w:rPr><w:b/><w:lang w:val="en-US"/><w:rPrChange w:id="7" w:author="Author A" w:date="2026-01-02T10:00:00Z"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr>"#
        );
        // A mark's insertion comes first, its formatting change last.
        run.rev.ins = Some(mark(8, 1, MarkKind::Insert));
        let m = mark_rpr(&run, &cx, &people()).expect("properties");
        assert!(m.starts_with(r#"<w:rPr><w:ins w:id="8" w:author="B &amp; C""#), "{m}");
        assert!(m.ends_with("</w:rPrChange></w:rPr>"), "{m}");
        // Children of the earlier properties as read are wrapped in their element.
        assert_eq!(
            change("w:", "rPrChange", "rPr", &mark(1, 0, MarkKind::Insert), &Raw(Some("<w:i/>".into())), &People::default())
                .matches("<w:rPr>")
                .count(),
            1
        );
        assert!(change("w:", "pPrChange", "pPr", &mark(1, 0, MarkKind::Insert), &Raw(None), &People::default()).ends_with("<w:pPr/></w:pPrChange>"));
    }

    #[test]
    fn cell_merges_name_their_merge_states() {
        let c = CellRevisions {
            structural: Some((
                mark(4, 0, MarkKind::Insert),
                CellMarkKind::Merge { v_merge: Some(VMerge::Continue), v_merge_orig: Some(VMerge::Restart) },
            )),
            change: None,
        };
        let m = cell_marks("w:", &c, &people());
        assert_eq!(m[2].1, r#"<w:cellMerge w:id="4" w:author="Author A" w:date="2026-01-02T10:00:00Z" w:vMerge="cont" w:vMergeOrig="rest"/>"#);
        assert!(m[0].1.is_empty() && m[1].1.is_empty() && m[3].1.is_empty());
        let g = grid_change("w:", &GridChange { mark: mark(9, 0, MarkKind::Insert), previous: vec![72.0, 144.0] }, &people());
        assert!(g.ends_with(r#"><w:tblGrid><w:gridCol w:w="1440"/><w:gridCol w:w="2880"/></w:tblGrid></w:tblGridChange>"#), "{g}");
        assert_eq!(before(TCPR, "tcPrChange").last(), Some(&"cellMerge"));
    }
}
