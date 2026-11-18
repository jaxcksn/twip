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

use crate::api::command::{
    DeleteOptions, FillControl, HorizontalSide, InsertColumns, InsertControl, InsertRows, PasteOptions, SplitCells, VerticalSide,
};
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
    ("v", "urn:schemas-microsoft-com:vml"),
    ("o", "urn:schemas-microsoft-com:office:office"),
    ("wp", "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"),
    ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    ("pic", "http://schemas.openxmlformats.org/drawingml/2006/picture"),
    ("wps", "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"),
];

/// A 1×1 PNG, `rIdImg` in every fixture.
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65,
    84, 120, 156, 99, 248, 207, 192, 0, 0, 3, 1, 1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
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
    /// those with the attribute, `prefix:name@prefix:attr=value` those with it set to the value, `~text` the text's
    /// occurrences in the XML.
    holds: &'static [&'static str],
    parts: &'static [Part],
    /// Edits that don't apply, each with why.
    exempt: &'static [(&'static str, &'static str)],
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
        exempt: &[],
    },
    Construct {
        name: "bookmark-blocks",
        xml: r#"<w:bookmarkStart w:id="8" w:name="Around"/>{PARA}<w:bookmarkEnd w:id="8"/>"#,
        holds: &["w:bookmarkStart@w:name=Around", "w:bookmarkEnd@w:id"],
        parts: &[],
        exempt: &[
            ("table-insert-row", "not in a table"),
            ("table-insert-column", "not in a table"),
            ("table-split", "not in a table"),
            ("table-merge", "not in a table"),
        ],
    },
    Construct {
        name: "comment",
        xml: r#"<w:commentRangeStart w:id="91"/><w:r><w:t>Noted</w:t></w:r><w:commentRangeEnd w:id="91"/><w:r><w:commentReference w:id="91"/></w:r>"#,
        holds: &["w:commentRangeStart@w:id=91", "w:commentRangeEnd@w:id=91", "w:commentReference@w:id=91"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "vml-picture",
        xml: r##"<w:r><w:pict><v:shape id="p1" style="width:36pt;height:36pt" type="#_x0000_t75"><v:imagedata r:id="rIdImg" o:title=""/></v:shape></w:pict></w:r>"##,
        holds: &["v:imagedata"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "floating-picture",
        xml: r#"<w:r><w:t>Mi</w:t></w:r><w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="2" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="457200" cy="457200"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="5" name="Float"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="5" name="dot.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImg"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="457200" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>d</w:t></w:r>"#,
        holds: &["wp:anchor"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "shape-fallback",
        xml: r##"<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="457200" cy="228600"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="6" name="Shape"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="457200" cy="228600"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="4472C4"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:rect id="r6" style="width:36pt;height:18pt" fillcolor="#4472c4"/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"##,
        holds: &["wps:wsp", "v:rect"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "prefix-below-root",
        xml: r#"<w:sdt><w:sdtPr/><w:sdtContent xmlns:x14="http://schemas.microsoft.com/office/word/2010/wordml"><w:r><w:rPr><x14:textOutline x14:w="9525"><x14:solidFill><x14:srgbClr x14:val="FF0000"/></x14:solidFill></x14:textOutline></w:rPr><w:t>Prefixed</w:t></w:r></w:sdtContent></w:sdt>"#,
        holds: &["w14:textOutline"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "style-separator",
        xml: r#"<w:r><w:rPr><w:vanish/><w:specVanish/></w:rPr><w:t>.</w:t></w:r>"#,
        holds: &["w:specVanish"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "permission",
        xml: r#"<w:permStart w:id="3" w:edGrp="everyone"/><w:r><w:t>Editable</w:t></w:r><w:permEnd w:id="3"/>"#,
        holds: &["w:permStart", "w:permEnd"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "hyperlink-tooltip",
        xml: r#"<w:hyperlink w:anchor="Top" w:tooltip="Go to the top" w:history="1"><w:r><w:t>Link</w:t></w:r></w:hyperlink>"#,
        holds: &["~Go to the top"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "equation",
        xml: r#"<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>"#,
        holds: &["m:oMath"],
        parts: &[],
        // An equation takes no room until equations are drawn: selecting the text before it doesn't reach it.
        exempt: &[("cut-paste", "an equation has no units")],
    },
    Construct {
        name: "symbol-font",
        xml: r#"<w:r><w:sym w:font="Wingdings" w:char="F04A"/></w:r>"#,
        holds: &["w:sym@w:font"],
        parts: &[],
        exempt: &[],
    },
    Construct { name: "hidden-run", xml: r#"<w:r><w:rPr><w:vanish/></w:rPr><w:t>Hidden</w:t></w:r>"#, holds: &["w:vanish"], parts: &[], exempt: &[] },
    Construct {
        name: "form-text",
        xml: r#"<w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Text1"/><w:enabled/><w:calcOnExit w:val="0"/><w:textInput><w:maxLength w:val="20"/></w:textInput></w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Field</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
        holds: &["w:ffData", "w:maxLength"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "custom-mark-note",
        xml: r#"<w:r><w:footnoteReference w:customMarkFollows="1" w:id="1"/><w:t>*</w:t></w:r>"#,
        holds: &["w:footnoteReference@w:customMarkFollows"],
        parts: &[FOOTNOTES],
        exempt: &[],
    },
    Construct {
        name: "custom-xml",
        xml: r#"<w:customXml w:element="client"><w:r><w:t>Custom</w:t></w:r></w:customXml>"#,
        holds: &["w:customXml"],
        parts: &[],
        // Word removes custom XML markup when it opens a document: text moved out leaves it empty, and it goes.
        exempt: &[("cut-paste", "custom XML markup isn't modeled")],
    },
    Construct {
        name: "underline-colour",
        xml: r#"<w:r><w:rPr><w:u w:val="single" w:color="FF0000"/></w:rPr><w:t>Under</w:t></w:r>"#,
        holds: &["w:u@w:color"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "text-effect",
        xml: r#"<w:r><w:rPr><w14:textOutline w14:w="9525" w14:cap="flat" w14:cmpd="sng" w14:algn="ctr"><w14:solidFill><w14:srgbClr w14:val="FF0000"/></w14:solidFill><w14:prstDash w14:val="solid"/><w14:round/></w14:textOutline></w:rPr><w:t>Outlined</w:t></w:r>"#,
        holds: &["w14:textOutline"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "run-kept",
        xml: r#"<w:r><w:rPr><w:webHidden/><w:effect w:val="blinkBackground"/></w:rPr><w:t>Kept</w:t></w:r>"#,
        holds: &["w:effect", "w:webHidden"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "linked-picture",
        xml: r#"<w:r><w:t>Li</w:t></w:r><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="457200" cy="457200"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="6" name="Linked"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="6" name="linked.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:link="rIdLink"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="457200" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r><w:r><w:t>nk</w:t></w:r>"#,
        holds: &["a:blip@r:link=rIdLink", "~TargetMode=\"External\""],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "ole-object",
        xml: r##"<w:r><w:t>Ole</w:t></w:r><w:r><w:object w:dxaOrig="720" w:dyaOrig="720"><v:shape id="ole1" type="#_x0000_t75" style="width:36pt;height:36pt" o:ole=""><v:imagedata r:id="rIdImg" o:title=""/></v:shape><o:OLEObject Type="Embed" ProgID="Package" ShapeID="ole1" DrawAspect="Icon" ObjectID="_1"/></w:object></w:r>"##,
        holds: &["w:object", "o:OLEObject", "~ProgID=\"Package\"", "v:imagedata"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "run-unknown",
        xml: r#"<w:r xmlns:x1="urn:example:future" mc:Ignorable="x1"><w:rPr><w:i/></w:rPr><w:t>Un</w:t><x1:future x1:val="1"/><w:t>known</w:t></w:r>"#,
        holds: &["~mc:Ignorable=\"x1\"", "~<x1:future"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "symbol-extension",
        xml: r#"<w:r><mc:AlternateContent><mc:Choice Requires="w16se"><w16se:symEx w16se:font="Segoe UI Emoji" w16se:char="1F600"/></mc:Choice><mc:Fallback><w:t>😀</w:t></mc:Fallback></mc:AlternateContent></w:r>"#,
        holds: &["w16se:symEx"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "alignment-tab",
        xml: r#"<w:r><w:ptab w:relativeTo="margin" w:alignment="center" w:leader="none"/></w:r>"#,
        holds: &["w:ptab"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "cell-properties",
        xml: r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblCaption w:val="Totals"/><w:tblDescription w:val="Yearly totals"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:cnfStyle w:val="100000000000"/><w:tcW w:w="4000" w:type="dxa"/><w:tcBorders><w:tl2br w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tcBorders><w:noWrap/><w:hideMark/></w:tcPr>{PARA}</w:tc><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        holds: &["w:cnfStyle@w:val=100000000000", "w:tl2br", "w:noWrap", "w:hideMark", "w:tblCaption", "w:tblDescription"],
        parts: &[],
        exempt: &[],
    },
    Construct {
        name: "row-exception",
        xml: r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tblPrEx><w:tblBorders><w:top w:val="double" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders></w:tblPrEx><w:trPr><w:cnfStyle w:val="010000000000"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr>{PARA}</w:tc><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        holds: &["w:tblPrEx", "w:cnfStyle@w:val=010000000000"],
        parts: &[],
        exempt: &[],
    },
];

/// A Word 97 movie: kept as a run of its own like any element a run holds that the engine doesn't model. Not in the
/// schemas the saved packages are checked against, so outside [`CONSTRUCTS`].
const MOVIE: Construct = Construct {
    name: "movie",
    xml: r#"<w:r><w:t>Mo</w:t><w:movie r:id="rIdImg"/><w:t>vie</w:t></w:r>"#,
    holds: &["w:movie@r:id=rIdImg"],
    parts: &[],
    exempt: &[],
};

#[test]
fn every_edit_keeps_a_movie() {
    let lost: Vec<String> = EDITS
        .iter()
        .filter(|(name, _)| !name.starts_with("table-"))
        .filter_map(|(name, edit)| Some(format!("{name}: {}", check_caught(&MOVIE, *edit, None, name)?)))
        .collect();
    assert!(lost.is_empty(), "{lost:?}");
}

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
        e.execute(Command::FillControl(FillControl { id, text: Some("Filled".into()), image: None, docx: None, building_block: None }), fonts())
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
    ("table-insert-row", |e, s| {
        e.select_range(s.para, s.inside, s.inside, fonts());
        e.execute(Command::InsertRows(InsertRows { side: VerticalSide::Below, count: None }), fonts())
    }),
    ("table-insert-column", |e, s| {
        e.select_range(s.para, s.inside, s.inside, fonts());
        e.execute(Command::InsertColumns(InsertColumns { side: HorizontalSide::Right, count: None }), fonts())
    }),
    ("table-split", |e, s| {
        e.select_range(s.para, s.inside, s.inside, fonts());
        e.execute(Command::SplitCells(SplitCells { columns: 2, rows: None }), fonts())
    }),
    ("table-merge", |e, s| {
        let second = texts(e).iter().position(|t| t == "Second").map(|k| para_id(e, k));
        let Some(second) = second else { return Outcome::Unchanged("no second cell") };
        e.select_between((s.para, s.inside), (second, Unit(3)), fonts());
        e.execute(Command::MergeCells, fonts())
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
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document{decl} mc:Ignorable="w14 w15 w16se wps"><w:body>{body}</w:body></w:document>"#
    );
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>"#,
    );
    let mut rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/styles" Target="styles.xml"/><Relationship Id="rIdImg" Type="{REL}/image" Target="media/dot.png"/><Relationship Id="rIdLink" Type="{REL}/image" Target="file:///C:/linked.png" TargetMode="External"/>"#
    );
    let mut files = vec![("word/document.xml".to_string(), main.into_bytes()), ("word/media/dot.png".to_string(), PNG.to_vec())];
    for (k, (name, ctype, rtype, xml)) in c.parts.iter().chain([&COMMENTS]).enumerate() {
        types += &format!(r#"<Override PartName="/{name}" ContentType="{ctype}"/>"#);
        rels += &format!(r#"<Relationship Id="rIdP{k}" Type="{rtype}" Target="{}"/>"#, name.trim_start_matches("word/"));
        let root = name.trim_start_matches("word/").trim_end_matches(".xml");
        files.push((
            name.to_string(),
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{root}{decl} mc:Ignorable="w14 w15 w16se wps">{xml}</w:{root}>"#)
                .into_bytes(),
        ));
    }
    files.push(("[Content_Types].xml".into(), (types + "</Types>").into_bytes()));
    files.push(("word/_rels/document.xml.rels".into(), (rels + "</Relationships>").into_bytes()));
    files.push((
        "_rels/.rels".into(),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>"#
        )
        .into_bytes(),
    ));
    files.push((
        "word/styles.xml".into(),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#,
            NS[0].1
        )
        .into_bytes(),
    ));
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        z.start_file(name, zip::write::SimpleFileOptions::default()).expect("a zip entry");
        z.write_all(&bytes).expect("written");
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

/// How often each of `holds` occurs across the package's XML parts, or only in the paragraphs whose text has `within`.
fn count(package: &[u8], holds: &[&str], within: Option<&str>) -> Vec<usize> {
    let uri = |p: &str| NS.iter().find(|(q, _)| *q == p).map(|(_, u)| *u).unwrap_or_else(|| panic!("namespace {p}"));
    let qname = |q: &str| q.split_once(':').map(|(p, l)| (uri(p), l.to_string())).expect("prefix:name");
    let mut n = vec![0; holds.len()];
    for (name, bytes) in entries(package) {
        let Ok(xml) = std::str::from_utf8(&bytes) else { continue };
        if !name.ends_with(".xml") {
            continue;
        }
        let Ok(dom) = roxmltree::Document::parse(xml) else { continue };
        let w = NS[0].1;
        let text_of = |p: &roxmltree::Node| {
            p.descendants().filter(|d| d.tag_name().namespace() == Some(w) && d.tag_name().name() == "t").filter_map(|t| t.text()).collect::<String>()
        };
        let scope: Vec<roxmltree::Node> = match within {
            Some(t) => {
                dom.descendants().filter(|d| d.tag_name().namespace() == Some(w) && d.tag_name().name() == "p" && text_of(d).contains(t)).collect()
            }
            None => vec![dom.root_element()],
        };
        for (k, h) in holds.iter().enumerate() {
            if let Some(text) = h.strip_prefix('~') {
                n[k] += scope.iter().map(|p| xml[p.range()].matches(text).count()).sum::<usize>();
                continue;
            }
            let (el, attr) = h.split_once('@').map_or((*h, None), |(e, a)| (e, Some(a)));
            let attr = attr.map(|a| a.split_once('=').map_or((qname(a), None), |(a, v)| (qname(a), Some(v))));
            let (ns, local) = qname(el);
            n[k] += scope
                .iter()
                .flat_map(|p| p.descendants())
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
    // A cut and paste moves the construct (one of its paragraph's) into `Next`'s paragraph.
    let moved = (label.ends_with("cut-paste") && !c.xml.contains("{PARA}")).then_some("Next");
    let (was, now) = (count(&src, c.holds, None), count(&saved, c.holds, moved));
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
        // Section and page settings, for the schema check too.
        std::fs::write(d.join("section-settings.docx"), section_settings_package()).expect("writes");
        std::fs::write(d.join("converted-notes.docx"), converted_notes_package()).expect("writes");
        std::fs::write(d.join("table-properties.docx"), table_properties_package()).expect("writes");
        std::fs::write(d.join("document-properties.docx"), document_properties_package()).expect("writes");
        std::fs::write(d.join("form-fields.docx"), form_fields_package()).expect("writes");
        std::fs::write(d.join("control-options.docx"), control_options_package()).expect("writes");
        std::fs::write(d.join("protection.docx"), protection_package()).expect("writes");
        let (pictures, boxes) = picture_tools_packages();
        std::fs::write(d.join("picture-tools.docx"), pictures).expect("writes");
        std::fs::write(d.join("text-box-columns.docx"), boxes).expect("writes");
    }
    let cases: Vec<(&Construct, &str, fn(&mut Editor, &Spot) -> Outcome)> = CONSTRUCTS
        .iter()
        .flat_map(|c| {
            EDITS
                .iter()
                .filter(|(n, _)| !c.exempt.iter().any(|(e, _)| e == n) && (!n.starts_with("table-") || c.xml.contains("{PARA}")))
                .map(move |(n, f)| (c, *n, *f))
        })
        .collect();
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

#[test]
fn a_resized_vml_picture_is_saved_at_its_new_size() {
    let c = CONSTRUCTS.iter().find(|c| c.name == "vml-picture").expect("the VML picture");
    let mut e = Editor::open(&package(c), fonts()).expect("opens");
    let s = spot(&e).expect("its paragraph");
    use crate::editing::selection::{ObjectRef, Selection};
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Inline { para: s.para, offset: s.before }));
    let size = crate::api::command::ObjectSize {
        width: Some(crate::api::value::Points(72.0)),
        height: Some(crate::api::value::Points(54.0)),
        ..Default::default()
    };
    let out = e.execute(Command::SetObjectSize(size), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = String::from_utf8(entries(&saved).into_iter().find(|(n, _)| n == "word/document.xml").expect("the main part").1).expect("UTF-8");
    assert!(xml.contains("width:72pt") && xml.contains("height:54pt") && xml.contains("v:imagedata"), "{xml}");
}

#[test]
fn page_setup_writes_only_what_changed() {
    use crate::api::value::Points;
    let cols = r#"<w:cols w:num="2" w:space="720" w:equalWidth="0"><w:col w:w="3000" w:space="720"/><w:col w:w="5640"/></w:cols>"#;
    let src = String::from_utf8(entries(&package(&CONSTRUCTS[0])).into_iter().find(|(n, _)| n == "word/document.xml").unwrap().1).unwrap();
    let bytes = crate::editing::tests::blank_body(&src[src.find("<w:body>").unwrap() + 8..src.find("<w:sectPr>").unwrap()]);
    let doc = String::from_utf8(entries(&bytes).into_iter().find(|(n, _)| n == "word/document.xml").unwrap().1).unwrap();
    let bytes = with_main(&bytes, &doc.replacen(r#"<w:cols w:space="720"/>"#, cols, 1));
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    let margins = crate::api::command::Margins { top: Some(Points(36.0)), bottom: None, left: None, right: None, gutter: None };
    let out = e.execute(Command::SetMargins(margins), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = String::from_utf8(entries(&saved).into_iter().find(|(n, _)| n == "word/document.xml").unwrap().1).unwrap();
    assert!(xml.contains(r#"w:top="720""#) && xml.contains(cols) && xml.contains(r#"<w:pgSz w:w="12240" w:h="15840"/>"#), "{xml}");
}

/// `package` with its main part replaced.
fn with_main(package: &[u8], xml: &str) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries(package) {
        z.start_file(&*name, zip::write::SimpleFileOptions::default()).expect("a zip entry");
        z.write_all(if name == "word/document.xml" { xml.as_bytes() } else { &bytes }).expect("written");
    }
    z.finish().expect("a package").into_inner()
}

/// A document with every section and page setting set, saved.
fn section_settings_package() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SECTIONS.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let n = |v: serde_json::Value| serde_json::from_value::<Command>(v).expect("a command");
    for c in [
        serde_json::json!({ "type": "setPageNumbering", "format": "lowerRoman", "start": 3, "chapter": { "level": 1, "separator": "enDash" } }),
        serde_json::json!({ "type": "setPageVerticalAlignment", "value": "center" }),
        serde_json::json!({ "type": "setLineNumbering", "value": { "countBy": 5, "distance": 18 } }),
        serde_json::json!({ "type": "setSectionBreakType", "value": "oddPage" }),
        serde_json::json!({ "type": "setSectionTextDirection", "value": "tbRl" }),
        serde_json::json!({ "type": "setColumns", "count": 2, "widths": [{ "width": 144, "spacing": 36 }, { "width": 288, "spacing": 0 }] }),
        serde_json::json!({ "type": "setMultiplePages", "mode": "bookFold", "gutterAtTop": true }),
        serde_json::json!({ "type": "setDifferentOddEvenPages", "value": true }),
        serde_json::json!({ "type": "setPageBorders", "top": { "style": "single", "width": 0.5, "color": "auto" }, "surroundHeader": false }),
    ] {
        let out = e.execute(n(c.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{c}: {out:?}");
    }
    // The second section's own header, an empty first-page footer, and an alignment tab.
    let k = texts(&e).iter().position(|t| t.starts_with("S2 P1")).expect("the second section");
    e.select_range(para_id(&e, k), Unit(0), Unit(0), fonts());
    for c in [
        serde_json::json!({ "type": "setHeaderFooterLinked", "kind": "header", "pages": "default", "linked": false }),
        serde_json::json!({ "type": "removeHeaderFooter", "kind": "footer", "pages": "first" }),
        serde_json::json!({ "type": "insertAlignmentTab", "alignment": "center", "relativeTo": "margin" }),
    ] {
        let out = e.execute(n(c.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{c}: {out:?}");
    }
    crate::docx::write::save(e.document()).expect("saves")
}

#[test]
fn section_settings_are_saved_where_the_schema_puts_them() {
    let saved = section_settings_package();
    let settings = String::from_utf8(entries(&saved).into_iter().find(|(n, _)| n == "word/settings.xml").expect("settings").1).expect("UTF-8");
    for el in ["evenAndOddHeaders", "gutterAtTop", "bookFoldPrinting", "bordersDoNotSurroundHeader"] {
        assert!(settings.contains(&format!("w:{el}")), "{el}: {settings}");
    }
}

/// A document whose footnotes and endnotes were swapped, with a new footnote of its own mark, saved.
fn converted_notes_package() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STORIES.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let n = |v: serde_json::Value| serde_json::from_value::<Command>(v).expect("a command");
    for c in [
        serde_json::json!({ "type": "insertNote", "kind": "footnote", "customMark": "*" }),
        serde_json::json!({ "type": "convertNotes", "value": "swap" }),
        serde_json::json!({ "type": "setNoteProperties", "kind": "endnote", "position": "sectionEnd", "format": "lowerRoman", "restart": "eachSection" }),
        serde_json::json!({ "type": "setNoteProperties", "kind": "footnote", "columns": 2 }),
    ] {
        let out = e.execute(n(c.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{c}: {out:?}");
    }
    crate::docx::write::save(e.document()).expect("saves")
}

#[test]
fn converted_notes_keep_their_references_and_bodies() {
    let refs = |pkg: &[u8]| {
        let xml = String::from_utf8(entries(pkg).into_iter().find(|(n, _)| n == "word/document.xml").expect("the main part").1).expect("UTF-8");
        (xml.matches("<w:footnoteReference").count(), xml.matches("<w:endnoteReference").count(), xml.matches("customMarkFollows").count())
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STORIES.docx");
    let (f, e, _) = refs(&std::fs::read(path).expect("the fixture"));
    let saved = converted_notes_package();
    // The new footnote (its own mark) became an endnote with the others.
    assert_eq!(refs(&saved), (e, f + 1, 1), "references swapped");
    let main = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    assert!(main.contains("footnoteColumns") && main.contains("w:val=\"2\"/>"), "footnote columns");
    let doc = crate::docx::read_editable(&saved).expect("reads back");
    assert_eq!((doc.footnotes.keys().filter(|k| **k > 0).count(), doc.endnotes.keys().filter(|k| **k > 0).count()), (e, f + 1), "bodies swapped");
}

/// A table with every cell, row and table property set, and a new table in the default table style, saved.
fn table_properties_package() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TBL-BASIC.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let n = |v: serde_json::Value| serde_json::from_value::<Command>(v).expect("a command");
    let first = crate::editing::tests::texts_ids(&e)
        .into_iter()
        .find(|p| e.index().para_slot(*p).is_some_and(|s| matches!(s.container, crate::editing::index::Container::Cell { .. })));
    e.select_range(first.expect("a cell"), Unit(0), Unit(0), fonts());
    for c in [
        serde_json::json!({ "type": "setCellProperties", "width": { "kind": "percent", "value": 30 }, "textDirection": "btLr", "wrapText": false, "fitText": true, "hideMark": true }),
        serde_json::json!({ "type": "setCellBorders", "edges": "diagonalDown" }),
        serde_json::json!({ "type": "setRowProperties", "hidden": true, "exceptions": { "alignment": "center", "cellSpacing": 1.5, "shading": "#D9E2F3", "borders": { "edges": "outside" } } }),
        serde_json::json!({ "type": "setTableCellSpacing", "value": 2 }),
        serde_json::json!({ "type": "setTableAltText", "title": "Totals", "description": "Yearly totals" }),
        serde_json::json!({ "type": "setDefaultTableStyle", "value": "TableNormal" }),
    ] {
        let out = e.execute(n(c.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{c}: {out:?}");
    }
    let last = *crate::editing::tests::texts_ids(&e).last().expect("a paragraph");
    e.select_range(last, Unit(0), Unit(0), fonts());
    for c in [
        serde_json::json!({ "type": "insertTable", "rows": 2, "columns": 2 }),
        serde_json::json!({ "type": "setCellBorders", "edges": "diagonalUp" }),
        serde_json::json!({ "type": "setCellProperties", "hideMark": true }),
        serde_json::json!({ "type": "setRowProperties", "hidden": true }),
        serde_json::json!({ "type": "setTableAltText", "title": "New" }),
    ] {
        let out = e.execute(n(c.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{c}: {out:?}");
    }
    crate::docx::write::save(e.document()).expect("saves")
}

#[test]
fn table_properties_are_saved_and_read_back() {
    let saved = table_properties_package();
    let xml = String::from_utf8(entries(&saved).into_iter().find(|(n, _)| n == "word/document.xml").expect("the main part").1).expect("UTF-8");
    for el in [
        "<w:tcW w:w=\"1500\" w:type=\"pct\"/>",
        "<w:textDirection w:val=\"btLr\"/>",
        "<w:noWrap/>",
        "<w:tcFitText/>",
        "<w:hideMark/>",
        "<w:tl2br ",
        "<w:tr2bl ",
        "<w:hidden/>",
        "<w:tblPrEx>",
        "<w:tblCellSpacing w:w=\"40\" w:type=\"dxa\"/>",
        "<w:tblCaption w:val=\"Totals\"/>",
        "<w:tblDescription w:val=\"Yearly totals\"/>",
        "<w:tblCaption w:val=\"New\"/>",
    ] {
        assert!(xml.contains(el), "{el} in {xml}");
    }
    let settings = String::from_utf8(entries(&saved).into_iter().find(|(n, _)| n == "word/settings.xml").expect("settings").1).expect("UTF-8");
    assert!(settings.contains("<w:defaultTableStyle w:val=\"TableNormal\"/>"), "{settings}");
    let doc = crate::docx::read_editable(&saved).expect("reads back");
    let tables: Vec<&crate::docx::Table> =
        doc.sections.iter().flat_map(|s| s.blocks.iter()).filter_map(|b| if let crate::docx::Block::Table(t) = b { Some(t) } else { None }).collect();
    let (t, new) = (tables[0], tables[tables.len() - 1]);
    let c = &t.rows[0].cells[0];
    assert_eq!((c.text_direction.as_str(), c.no_wrap, c.fit_text, c.hide_mark), ("btLr", true, true, true));
    assert!(c.diagonals.as_ref().is_some_and(|d| d[0].is_some()));
    assert!(t.rows[0].hidden && t.rows[0].shading == Some(0xD9E2F3) && t.rows[0].borders.is_some());
    assert_eq!((t.cell_spacing, t.alt.as_ref().map(|a| a.title.as_str())), (2.0, Some("Totals")));
    assert!(xml.contains("<w:tblStyle w:val=\"TableNormal\"/>"), "the new table takes the default style");
    // The new table follows the last one with nothing between: they read as one.
    let row = &new.rows[new.rows.len() - 2];
    assert!(row.hidden && row.cells[0].hide_mark && row.cells[0].diagonals.as_ref().is_some_and(|d| d[1].is_some()));
}

/// A floating picture cropped, recoloured, sized and placed relative to the page, and a text box in two columns, saved.
fn picture_tools_packages() -> (Vec<u8>, Vec<u8>) {
    use crate::editing::selection::{ObjectRef, Selection};
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-OBJ.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let n = |v: serde_json::Value| serde_json::from_value::<Command>(v).expect("a command");
    let floats: Vec<(u64, bool)> = crate::editing::tests::texts_ids(&e)
        .into_iter()
        .filter_map(|p| e.index().para(e.document(), p))
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            crate::docx::Inline::Float(f, _) => Some((f.id, f.text_box.is_some())),
            _ => None,
        })
        .collect();
    let pic = floats.iter().find(|f| !f.1).expect("a floating picture").0;
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(crate::layout::FloatId(pic))));
    for c in [
        serde_json::json!({ "type": "setPictureCrop", "left": 0.1, "bottom": 0.2 }),
        serde_json::json!({ "type": "setPictureAdjust", "transparentColor": "#FFFFFF", "grayscale": true }),
        serde_json::json!({ "type": "setObjectSize", "relativeWidth": { "percent": 40, "relativeTo": "margin" }, "relativeHeight": { "percent": 20, "relativeTo": "page" } }),
        serde_json::json!({ "type": "setObjectPosition", "horizontal": { "relativeTo": "page", "percent": 10 }, "vertical": { "relativeTo": "page", "percent": 15 } }),
    ] {
        let out = e.execute(n(c.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{c}: {out:?}");
    }
    let saved = crate::docx::write::save(e.document()).expect("saves");
    // A text box in two columns, in a document of its own.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-NEST-2-BOX.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let tb = crate::editing::tests::texts_ids(&e)
        .into_iter()
        .filter_map(|p| e.index().para(e.document(), p))
        .flat_map(|p| p.inlines.iter())
        .find_map(|i| match i {
            crate::docx::Inline::Float(f, _) if f.text_box.is_some() && f.shape.is_some() => Some(f.id),
            _ => None,
        })
        .expect("a text box");
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(crate::layout::FloatId(tb))));
    let out = e.execute(n(serde_json::json!({ "type": "setShapeText", "columns": { "count": 2, "spacing": 18 }, "autofit": "shrinkText" })), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "text box columns: {out:?}");
    let boxed = crate::docx::write::save(e.document()).expect("saves");
    let xml = crate::docx::package::read(&boxed, &[], "word/document.xml").expect("reads").expect("the main part");
    assert!(xml.contains("numCol=\"2\"") && xml.contains("<a:normAutofit"), "the columns are saved");
    (saved, boxed)
}

#[test]
fn picture_tools_and_text_box_columns_are_saved() {
    picture_tools_packages();
}

/// A document with its properties and a variable set, and fields that show them, saved.
fn document_properties_package() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STORIES.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let n = |v: serde_json::Value| serde_json::from_value::<Command>(v).expect("a command");
    e.select_range(para_id(&e, 0), Unit(0), Unit(0), fonts());
    for c in [
        serde_json::json!({ "type": "setDocumentProperties", "title": "Annual report", "author": "Ada", "company": "Engines & Co",
            "custom": [{ "name": "Client", "kind": "text", "value": "Acme" }, { "name": "Year", "kind": "number", "value": "2026" }] }),
        serde_json::json!({ "type": "setDocumentVariable", "name": "CaseNumber", "value": "42" }),
        serde_json::json!({ "type": "insertField", "code": "DOCPROPERTY Client" }),
        serde_json::json!({ "type": "insertField", "code": "DOCVARIABLE CaseNumber" }),
        serde_json::json!({ "type": "insertField", "code": "TITLE" }),
        serde_json::json!({ "type": "selectAll" }),
        serde_json::json!({ "type": "updateFields" }),
    ] {
        let out = e.execute(n(c.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_) | Outcome::Moved), "{c}: {out:?}");
    }
    let shown = texts(&e)[0].clone();
    assert!(shown.contains("Acme") && shown.contains("42") && shown.contains("Annual report"), "{shown}");
    crate::docx::write::save(e.document()).expect("saves")
}

#[test]
fn document_properties_and_variables_are_saved() {
    let saved = document_properties_package();
    let part = |n: &str| crate::docx::package::read(&saved, &[], n).expect("reads").unwrap_or_default();
    assert!(part("docProps/core.xml").contains("<dc:title>Annual report</dc:title>"));
    assert!(part("docProps/app.xml").contains("<Company>Engines &amp; Co</Company>"));
    assert!(part("docProps/custom.xml").contains("name=\"Client\"><vt:lpwstr>Acme</vt:lpwstr>"));
    assert!(part("word/settings.xml").contains("w:name=\"CaseNumber\" w:val=\"42\""), "{}", part("word/settings.xml"));
    assert!(part("_rels/.rels").contains("custom-properties"));
    let doc = crate::docx::read_editable(&saved).expect("reads back");
    assert_eq!(doc.properties.lookup("client").as_deref(), Some("Acme"));
    assert_eq!(doc.settings.variables, vec![("CaseNumber".to_string(), "42".to_string())]);
}

/// Legacy form fields inserted, set and filled, the source's own text field filled too, saved.
fn form_fields_package() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STORIES.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let n = |v: serde_json::Value| serde_json::from_value::<Command>(v).expect("a command");
    e.select_range(para_id(&e, 0), Unit(0), Unit(0), fonts());
    let run = |e: &mut Editor, v: serde_json::Value| {
        let out = e.execute(n(v.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{v}: {out:?}");
    };
    run(&mut e, serde_json::json!({ "type": "insertFormField", "value": "dropDown" }));
    let dd = e.document().sections[0].blocks.iter().find_map(|b| match b {
        crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
            crate::docx::Inline::FieldBegin { id, form: Some(_), .. } => Some(*id),
            _ => None,
        }),
        _ => None,
    });
    let id = crate::editing::elements::id_str(crate::docx::ElementId::Field(dd.expect("the drop-down")));
    run(
        &mut e,
        serde_json::json!({ "type": "setFormField", "id": id, "name": "Choice", "statusText": "Pick one", "dropDown": { "entries": ["Red", "Green"], "default": 1 } }),
    );
    run(&mut e, serde_json::json!({ "type": "fillFormField", "id": id, "entry": 0 }));
    run(&mut e, serde_json::json!({ "type": "insertFormField", "value": "checkBox" }));
    run(&mut e, serde_json::json!({ "type": "insertFormField", "value": "text" }));
    run(&mut e, serde_json::json!({ "type": "setFormFieldShading", "value": false }));
    crate::docx::write::save(e.document()).expect("saves")
}

#[test]
fn form_fields_are_saved_and_read_back() {
    let saved = form_fields_package();
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    for el in [
        "<w:name w:val=\"Choice\"/>",
        "<w:listEntry w:val=\"Green\"/>",
        "<w:result w:val=\"0\"/>",
        "<w:name w:val=\"Check1\"/>",
        "<w:textInput>",
        "FORMDROPDOWN",
    ] {
        assert!(xml.contains(el), "{el}");
    }
    let settings = crate::docx::package::read(&saved, &[], "word/settings.xml").expect("reads").unwrap_or_default();
    assert!(settings.contains("doNotShadeFormData"));
}

/// Content controls with their look, contents' style, check box symbols and repeating section options set, saved.
fn control_options_package() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STORIES.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let n = |v: serde_json::Value| serde_json::from_value::<Command>(v).expect("a command");
    let run = |e: &mut Editor, v: serde_json::Value| {
        let out = e.execute(n(v.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{v}: {out:?}");
    };
    let id_of = |e: &Editor, tag: &str| {
        let (c, _) = e.document().controls.iter().find(|(_, k)| k.props.tag.as_deref() == Some(tag)).expect("the control");
        crate::editing::elements::id_str(crate::docx::ElementId::Control(*c))
    };
    e.select_range(para_id(&e, 0), Unit(0), Unit(0), fonts());
    run(&mut e, serde_json::json!({ "type": "insertControl", "kind": "richText", "tag": "Look" }));
    let look = id_of(&e, "Look");
    run(
        &mut e,
        serde_json::json!({ "type": "setControl", "id": look, "appearance": "tags", "color": "#2E74B5", "temporary": true, "style": "Strong" }),
    );
    e.select_range(para_id(&e, 1), Unit(0), Unit(0), fonts());
    run(&mut e, serde_json::json!({ "type": "insertControl", "kind": "checkbox", "tag": "Tick" }));
    let tick = id_of(&e, "Tick");
    run(
        &mut e,
        serde_json::json!({ "type": "setControl", "id": tick, "options": { "checkedSymbol": { "font": "Wingdings", "character": "\u{FC}" } } }),
    );
    crate::docx::write::save(e.document()).expect("saves")
}

#[test]
fn control_options_are_saved_and_read_back() {
    let saved = control_options_package();
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    for el in ["w15:val=\"tags\"", "<w15:color", "<w:temporary/>", "<w:rStyle w:val=\"Strong\"/>", "w14:font=\"Wingdings\"", "w14:val=\"00FC\""] {
        assert!(xml.contains(el), "{el}");
    }
    let doc = crate::docx::read_editable(&saved).expect("reads back");
    assert!(doc.controls.values().any(|c| matches!(c.props.kind, crate::docx::ControlKind::Checkbox { checked: '\u{FC}', .. })));
}

/// A read-only document with a password and a range everyone may edit, saved.
fn protection_package() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STORIES.docx");
    let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), fonts()).expect("opens");
    let n = |v: serde_json::Value| serde_json::from_value::<Command>(v).expect("a command");
    e.select_range(para_id(&e, 0), Unit(0), Unit(4), fonts());
    for c in [
        serde_json::json!({ "type": "addEditableRange" }),
        serde_json::json!({ "type": "setDocumentProtection", "kind": "readOnly", "enforce": true, "password": "secret" }),
    ] {
        let out = e.execute(n(c.clone()), fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{c}: {out:?}");
    }
    crate::docx::write::save(e.document()).expect("saves")
}

#[test]
fn protection_and_editable_ranges_are_saved() {
    let saved = protection_package();
    let xml = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    assert!(xml.contains("w:edGrp=\"everyone\"") && xml.contains("<w:permEnd"), "the editable range");
    let doc = crate::docx::read_editable(&saved).expect("reads back");
    let p = doc.settings.protection.expect("protection");
    assert!(p.enforced && p.password.as_ref().is_some_and(|pw| pw.matches("secret") && !pw.matches("Secret")));
}
