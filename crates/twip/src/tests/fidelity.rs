//! Round-trip gate
//!
//! Each construct sits in a paragraph `Previous` / `Before{construct}After` / `Next`, with a comment on `After`. Every
//! edit opens it fresh, edits
//! through the editor, and saves: the saved package must still hold the construct at least as often as the source did
//! (typing next to formatting copies it), and
//! every part the edit had no reason to touch must be byte-identical. `tests/fidelity/pending.txt` lists the
//! `construct edit` pairs that still fail; one that passes fails the run. `TWIP_FIDELITY_OUT=dir` writes every saved
//! package there, with each construct's source, for the schema check (`tests/fidelity/validate`).

use std::collections::BTreeSet;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use crate::api::command::{DeleteOptions, FillControl, InsertControl, PasteOptions};
use crate::api::review::Body;
use crate::api::value::{ControlKind, Direction, ElementId};
use crate::editing::Editor;
use crate::editing::commands::{Command, Outcome};
use crate::editing::tests::{fonts, para_id, texts};
use crate::layout::{ParaId, Unit};

const NS: &[(&str, &str)] = &[
    ("w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main"),
    ("r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships"),
    ("m", "http://schemas.openxmlformats.org/officeDocument/2006/math"),
    ("mc", "http://schemas.openxmlformats.org/markup-compatibility/2006"),
    ("w14", "http://schemas.microsoft.com/office/word/2010/wordml"),
    ("w15", "http://schemas.microsoft.com/office/word/2012/wordml"),
    ("w16se", "http://schemas.microsoft.com/office/word/2015/wordml/symex"),
];
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A part a construct needs beside the main part: its name, content type, relationship type and XML.
type Part = (&'static str, &'static str, &'static str, &'static str);

struct Construct {
    name: &'static str,
    /// The construct, between `Before` and `After` in one paragraph; a block holding `{PARA}` (a table) takes the three
    /// paragraphs in its place instead.
    xml: &'static str,
    /// What must survive, at least as often as it was read: `prefix:name` counts elements, `prefix:name@prefix:attr`
    /// those with the attribute, `prefix:name@prefix:attr=value` those with it set to the value.
    holds: &'static [&'static str],
    parts: &'static [Part],
}

const FOOTNOTES: Part = (
    "word/footnotes.xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes",
    r#"<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:t xml:space="preserve"> A note.</w:t></w:r></w:p></w:footnote>"#,
);

const COMMENTS: Part = (
    "word/comments.xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
    r#"<w:comment w:id="90" w:author="Reviewer" w:initials="R"><w:p><w:r><w:t>Earlier note</w:t></w:r></w:p></w:comment><w:comment w:id="91" w:author="Reviewer" w:initials="R"><w:p><w:r><w:t>On the construct</w:t></w:r></w:p></w:comment>"#,
);

const CONSTRUCTS: &[Construct] = &[
    Construct {
        name: "bookmark",
        xml: r#"<w:bookmarkStart w:id="7" w:name="Mark"/><w:r><w:t>Mark</w:t></w:r><w:bookmarkEnd w:id="7"/>"#,
        holds: &["w:bookmarkStart", "w:bookmarkEnd"],
        parts: &[],
    },
    Construct {
        name: "comment",
        xml: r#"<w:commentRangeStart w:id="91"/><w:r><w:t>Noted</w:t></w:r><w:commentRangeEnd w:id="91"/><w:r><w:commentReference w:id="91"/></w:r>"#,
        holds: &["w:commentRangeStart@w:id=91", "w:commentRangeEnd@w:id=91", "w:commentReference@w:id=91"],
        parts: &[],
    },
    Construct {
        name: "permission",
        xml: r#"<w:permStart w:id="3" w:edGrp="everyone"/><w:r><w:t>Editable</w:t></w:r><w:permEnd w:id="3"/>"#,
        holds: &["w:permStart", "w:permEnd"],
        parts: &[],
    },
    Construct {
        name: "hyperlink-tooltip",
        xml: r#"<w:hyperlink w:anchor="Top" w:tooltip="Go to the top" w:history="1"><w:r><w:t>Link</w:t></w:r></w:hyperlink>"#,
        holds: &["w:hyperlink@w:tooltip"],
        parts: &[],
    },
    Construct { name: "equation", xml: r#"<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>"#, holds: &["m:oMath"], parts: &[] },
    Construct { name: "symbol-font", xml: r#"<w:r><w:sym w:font="Wingdings" w:char="F04A"/></w:r>"#, holds: &["w:sym@w:font"], parts: &[] },
    Construct { name: "hidden-run", xml: r#"<w:r><w:rPr><w:vanish/></w:rPr><w:t>Hidden</w:t></w:r>"#, holds: &["w:vanish"], parts: &[] },
    Construct {
        name: "form-text",
        xml: r#"<w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Text1"/><w:enabled/><w:calcOnExit w:val="0"/><w:textInput><w:maxLength w:val="20"/></w:textInput></w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Field</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
        holds: &["w:ffData", "w:maxLength"],
        parts: &[],
    },
    Construct {
        name: "custom-mark-note",
        xml: r#"<w:r><w:footnoteReference w:customMarkFollows="1" w:id="1"/><w:t>*</w:t></w:r>"#,
        holds: &["w:footnoteReference@w:customMarkFollows"],
        parts: &[FOOTNOTES],
    },
    Construct {
        name: "custom-xml",
        xml: r#"<w:customXml w:element="client"><w:r><w:t>Custom</w:t></w:r></w:customXml>"#,
        holds: &["w:customXml"],
        parts: &[],
    },
    Construct {
        name: "underline-colour",
        xml: r#"<w:r><w:rPr><w:u w:val="single" w:color="FF0000"/></w:rPr><w:t>Under</w:t></w:r>"#,
        holds: &["w:u@w:color"],
        parts: &[],
    },
    Construct {
        name: "text-effect",
        xml: r#"<w:r><w:rPr><w14:textOutline w14:w="9525" w14:cap="flat" w14:cmpd="sng" w14:algn="ctr"><w14:solidFill><w14:srgbClr w14:val="FF0000"/></w14:solidFill><w14:prstDash w14:val="solid"/><w14:round/></w14:textOutline></w:rPr><w:t>Outlined</w:t></w:r>"#,
        holds: &["w14:textOutline"],
        parts: &[],
    },
    Construct {
        name: "run-kept",
        xml: r#"<w:r><w:rPr><w:webHidden/><w:effect w:val="blinkBackground"/></w:rPr><w:t>Kept</w:t></w:r>"#,
        holds: &["w:effect", "w:webHidden"],
        parts: &[],
    },
    Construct {
        name: "symbol-extension",
        xml: r#"<w:r><mc:AlternateContent><mc:Choice Requires="w16se"><w16se:symEx w16se:font="Segoe UI Emoji" w16se:char="1F600"/></mc:Choice><mc:Fallback><w:t>😀</w:t></mc:Fallback></mc:AlternateContent></w:r>"#,
        holds: &["w16se:symEx"],
        parts: &[],
    },
    Construct {
        name: "alignment-tab",
        xml: r#"<w:r><w:ptab w:relativeTo="margin" w:alignment="center" w:leader="none"/></w:r>"#,
        holds: &["w:ptab"],
        parts: &[],
    },
    Construct {
        name: "cell-cnf-style",
        xml: r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblCaption w:val="Totals"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:cnfStyle w:val="100000000000"/><w:tcW w:w="4000" w:type="dxa"/><w:tcBorders><w:tl2br w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tcBorders></w:tcPr>{PARA}</w:tc></w:tr></w:tbl>"#,
        holds: &["w:cnfStyle", "w:tl2br", "w:tblCaption"],
        parts: &[],
    },
];

/// The edits: name, and what it does to the editor holding the construct's paragraph.
const EDITS: &[(&str, fn(&mut Editor, &Spot) -> Outcome)] = &[
    ("type-inside", |e, s| type_at(e, s.para, s.inside, "x")),
    ("type-before", |e, s| type_at(e, s.para, s.before, "x")),
    ("type-after", |e, s| type_at(e, s.para, s.after, "x")),
    ("format", |e, s| {
        e.select_range(s.para, Unit(0), s.len, fonts());
        e.execute(Command::SetBold(true), fonts())
    }),
    ("wrap-in-control", |e, s| {
        e.select_range(s.para, Unit(0), s.len, fonts());
        e.execute(Command::InsertControl(InsertControl { kind: ControlKind::RichText, wrap_selection: Some(true), ..control() }), fonts())
    }),
    ("fill-control-beside", |e, s| {
        e.select_range(s.para, s.len, s.len, fonts());
        e.execute(Command::InsertControl(InsertControl { kind: ControlKind::PlainText, tag: Some("Fill".into()), ..control() }), fonts());
        let Some((c, _)) = e.document().controls.iter().find(|(_, k)| k.props.tag.as_deref() == Some("Fill")) else {
            return Outcome::Unchanged("no control");
        };
        let id = ElementId(crate::editing::elements::id_str(crate::docx::ElementId::Control(*c)));
        e.execute(Command::FillControl(FillControl { id, text: Some("Filled".into()), image: None, docx: None }), fonts())
    }),
    ("delete-comment", |e, _| e.execute(Command::DeleteComment(crate::api::value::CommentId("c90".into())), fonts())),
    ("comment", |e, s| {
        e.select_range(s.para, Unit(0), s.len, fonts());
        e.execute(Command::AddComment(Body::Text("Note".into())), fonts())
    }),
    ("cut-paste", |e, s| {
        e.select_range(s.para, Unit(0), s.after, fonts());
        e.execute(Command::Cut, fonts());
        e.select_range(s.next, Unit(0), Unit(0), fonts());
        e.execute(Command::Paste(PasteOptions { as_: None }), fonts())
    }),
    ("delete-neighbours", |e, s| {
        e.select_between((s.prev, Unit(0)), (s.para, Unit(0)), fonts());
        e.execute(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None }), fonts());
        let next_len = units(texts(e).iter().find(|t| t.as_str() == "Next").map_or(4, |t| t.chars().count()));
        e.select_between((s.para, s.len), (s.next, next_len), fonts());
        e.execute(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None }), fonts())
    }),
];

fn control() -> InsertControl {
    serde_json::from_value(serde_json::json!({ "kind": "richText" })).expect("an insertControl")
}

fn units(n: usize) -> Unit {
    Unit(u32::try_from(n).expect("a short paragraph"))
}

fn type_at(e: &mut Editor, para: ParaId, at: Unit, text: &str) -> Outcome {
    e.select_range(para, at, at, fonts());
    e.execute(Command::InsertText(text.into()), fonts())
}

/// Where the edits act: the construct's paragraph and its neighbours, and unit offsets in it.
struct Spot {
    prev: ParaId,
    para: ParaId,
    next: ParaId,
    before: Unit,
    inside: Unit,
    after: Unit,
    len: Unit,
}

fn spot(e: &Editor) -> Option<Spot> {
    let all = texts(e);
    let k = all.iter().position(|t| t.starts_with("Before") && t.ends_with("After"))?;
    let (p, n) = (all.iter().position(|t| t == "Previous")?, all.iter().position(|t| t == "Next")?);
    let len = all[k].chars().count();
    let (before, after) = ("Before".len(), len - "After".len());
    let (before, inside, after, len) = (units(before), units((before + after) / 2), units(after), units(len));
    Some(Spot { prev: para_id(e, p), para: para_id(e, k), next: para_id(e, n), before, inside, after, len })
}

/// The construct's document: its main part, with the parts it needs.
fn package(c: &Construct) -> Vec<u8> {
    let decl = NS.iter().map(|(p, u)| format!(r#" xmlns:{p}="{u}""#)).collect::<String>();
    let after =
        r#"<w:commentRangeStart w:id="90"/><w:r><w:t>After</w:t></w:r><w:commentRangeEnd w:id="90"/><w:r><w:commentReference w:id="90"/></w:r>"#;
    let para = format!(r#"<w:p><w:r><w:t>Before</w:t></w:r>{}{after}</w:p>"#, if c.xml.contains("{PARA}") { "" } else { c.xml });
    let paras = format!(r#"<w:p><w:r><w:t>Previous</w:t></w:r></w:p>{para}<w:p><w:r><w:t>Next</w:t></w:r></w:p>"#);
    let target = if c.xml.contains("{PARA}") { c.xml.replace("{PARA}", &paras) + "<w:p/>" } else { paras };
    let body = format!(
        r#"{target}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#
    );
    let main = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document{decl} mc:Ignorable="w14 w15 w16se"><w:body>{body}</w:body></w:document>"#
    );
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>"#,
    );
    let mut rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/styles" Target="styles.xml"/>"#
    );
    let mut files = vec![("word/document.xml".to_string(), main)];
    for (k, (name, ctype, rtype, xml)) in c.parts.iter().chain([&COMMENTS]).enumerate() {
        types += &format!(r#"<Override PartName="/{name}" ContentType="{ctype}"/>"#);
        rels += &format!(r#"<Relationship Id="rIdP{k}" Type="{rtype}" Target="{}"/>"#, name.trim_start_matches("word/"));
        let root = name.trim_start_matches("word/").trim_end_matches(".xml");
        files.push((
            name.to_string(),
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{root}{decl} mc:Ignorable="w14 w15 w16se">{xml}</w:{root}>"#),
        ));
    }
    files.push(("[Content_Types].xml".into(), types + "</Types>"));
    files.push(("word/_rels/document.xml.rels".into(), rels + "</Relationships>"));
    files.push((
        "_rels/.rels".into(),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>"#
        ),
    ));
    files.push((
        "word/styles.xml".into(),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#,
            NS[0].1
        ),
    ));
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, xml) in files {
        z.start_file(name, zip::write::SimpleFileOptions::default()).expect("a zip entry");
        z.write_all(xml.as_bytes()).expect("written");
    }
    z.finish().expect("a package").into_inner()
}

fn entries(package: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(package)).expect("a package");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).expect("an entry");
            let mut b = Vec::new();
            f.read_to_end(&mut b).expect("readable");
            (f.name().expect("a UTF-8 name").to_string(), b)
        })
        .collect()
}

/// How often each of `holds` occurs across the package's XML parts.
// ponytail: counts, not places: a construct a cut leaves behind still counts once. Compare its position against the
// text around it if a move ever lands it in the wrong place.
fn count(package: &[u8], holds: &[&str]) -> Vec<usize> {
    let uri = |p: &str| NS.iter().find(|(q, _)| *q == p).map(|(_, u)| *u).unwrap_or_else(|| panic!("namespace {p}"));
    let qname = |q: &str| q.split_once(':').map(|(p, l)| (uri(p), l.to_string())).expect("prefix:name");
    let mut n = vec![0; holds.len()];
    for (name, bytes) in entries(package) {
        let Ok(xml) = std::str::from_utf8(&bytes) else { continue };
        if !name.ends_with(".xml") {
            continue;
        }
        let Ok(dom) = roxmltree::Document::parse(xml) else { continue };
        for (k, h) in holds.iter().enumerate() {
            let (el, attr) = h.split_once('@').map_or((*h, None), |(e, a)| (e, Some(a)));
            let attr = attr.map(|a| a.split_once('=').map_or((qname(a), None), |(a, v)| (qname(a), Some(v))));
            let (ns, local) = qname(el);
            n[k] += dom
                .descendants()
                .filter(|d| d.tag_name().namespace() == Some(ns) && d.tag_name().name() == local)
                .filter(|d| {
                    attr.as_ref().is_none_or(|((ans, al), v)| {
                        d.attributes().any(|a| a.namespace() == Some(*ans) && a.name() == al && v.is_none_or(|v| a.value() == v))
                    })
                })
                .count();
        }
    }
    n
}

/// Parts an edit may change: the main part, and what a comment or a control adds.
fn may_change(name: &str) -> bool {
    name == "word/document.xml"
        || name == "[Content_Types].xml"
        || name == "word/_rels/document.xml.rels"
        || name.starts_with("word/comments")
        || name == "word/people.xml"
        || name == "word/settings.xml"
}

/// What went wrong with one construct and edit, if anything; a panic too.
fn check_caught(c: &Construct, edit: fn(&mut Editor, &Spot) -> Outcome, out: Option<&Path>, label: &str) -> Option<String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(c, edit, out, label))).unwrap_or_else(|p| {
        let why = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
        Some(format!("panicked: {}", why.lines().next().unwrap_or("")))
    })
}

fn check(c: &Construct, edit: fn(&mut Editor, &Spot) -> Outcome, out: Option<&Path>, label: &str) -> Option<String> {
    let src = package(c);
    if let Some(dir) = out {
        std::fs::write(dir.join(format!("{}-source.docx", c.name)), &src).expect("writes the source package");
    }
    let mut e = Editor::open(&src, fonts()).map_err(|err| format!("opens: {err}")).ok()?;
    let Some(s) = spot(&e) else { return Some("the construct's paragraph isn't read".into()) };
    match edit(&mut e, &s) {
        Outcome::Refused(err) => return Some(format!("refused: {err:?}")),
        Outcome::Unchanged(why) => return Some(format!("the edit did nothing: {why}")),
        _ => {}
    }
    let saved = match crate::docx::write::save(e.document()) {
        Ok(b) => b,
        Err(err) => return Some(format!("save: {err}")),
    };
    if let Some(dir) = out {
        std::fs::write(dir.join(format!("{label}.docx")), &saved).expect("writes the saved package");
    }
    let (was, now) = (count(&src, c.holds), count(&saved, c.holds));
    let lost: Vec<String> =
        c.holds.iter().zip(was.iter().zip(&now)).filter(|(_, (a, b))| b < a).map(|(h, (a, b))| format!("{h} {a} → {b}")).collect();
    let before = entries(&src);
    let touched: Vec<String> = entries(&saved)
        .into_iter()
        .filter(|(n, b)| !may_change(n) && before.iter().find(|(m, _)| m == n).map(|(_, a)| a) != Some(b))
        .map(|(n, _)| n)
        .collect();
    match (lost.is_empty(), touched.is_empty()) {
        (true, true) => None,
        _ => Some([lost.join(", "), touched.iter().map(|t| format!("{t} changed")).collect::<Vec<_>>().join(", ")].join(" ").trim().to_string()),
    }
}

fn pending() -> BTreeSet<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fidelity/pending.txt");
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect()
}

#[test]
fn every_edit_keeps_every_construct() {
    let out = std::env::var_os("TWIP_FIDELITY_OUT").map(std::path::PathBuf::from);
    if let Some(d) = &out {
        let _ = std::fs::remove_dir_all(d);
        std::fs::create_dir_all(d).expect("the output directory");
    }
    let cases: Vec<(&Construct, &str, fn(&mut Editor, &Spot) -> Outcome)> =
        CONSTRUCTS.iter().flat_map(|c| EDITS.iter().map(move |(n, f)| (c, *n, *f))).collect();
    let results = crate::editing::tests::par_map(&cases, |(c, n, f)| {
        let label = format!("{} {n}", c.name);
        let failure = check_caught(c, *f, out.as_deref(), &label.replace(' ', "-"));
        (label, failure)
    });
    let pending = pending();
    let known: BTreeSet<&String> = results.iter().map(|(l, _)| l).collect();
    let mut problems: Vec<String> = pending.iter().filter(|p| !known.contains(p)).map(|p| format!("{p}: in pending.txt but not a case")).collect();
    for (label, failure) in &results {
        match (failure, pending.contains(label)) {
            (Some(why), false) => problems.push(format!("{label}: {why}")),
            (None, true) => problems.push(format!("{label}: passes; delete its line from tests/fidelity/pending.txt")),
            _ => {}
        }
    }
    assert!(problems.is_empty(), "{} round-trip problems:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn content_kept_as_read_is_reported() {
    let c = CONSTRUCTS.iter().find(|c| c.name == "equation").expect("the equation construct");
    let doc = crate::docx::read_editable(&package(c)).expect("reads");
    let w = doc.diagnostics.warnings.iter().find(|w| w.code == crate::docx::diagnostics::WarningCode::UnsupportedElement);
    assert!(w.is_some_and(|w| w.message.starts_with("m:oMath")), "{:?}", doc.diagnostics);
}
