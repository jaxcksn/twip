//! What a node read from DOCX keeps of its source ([`Origin`]), and the model's views of it

use std::ops::Range;
use std::rc::Rc;

use super::model::*;

/// A paragraph's place in its part's XML (byte offsets), for lossless saving.
#[derive(Clone, Debug)]
pub struct ParaSrc {
    pub ranges: Vec<Range<usize>>,
    /// The elements that produced its units, in order, with how many units each did: `w:t`, `w:tab`, a drawing, a
    /// whole complex field (from its `begin` run to its `end` run), ...
    pub segs: Vec<(Range<usize>, usize)>,
    /// The elements inside each simple field and hyperlink seg (by its start) that produced its result's units.
    pub simple: Vec<(usize, Vec<(Range<usize>, usize)>)>,
    /// Content that can't be mapped back (a field whose runs don't share a parent): edits rewrite it from the model.
    pub lossy: bool,
    /// Made by the reader to close a story that ends without a paragraph: no element, `ranges[0]` is where its XML
    /// goes, an empty range before the story's end tag or the story's whole element when it is written empty.
    pub closing: bool,
    /// The paragraphs a vanished mark joined onto this one's start, in order (the last of [`Self::ranges`] is its own).
    pub joined: Vec<JoinedPart>,
}

/// A paragraph read from DOCX: its place in its part.
pub(crate) trait ParagraphOrigin {
    /// Where it was read from.
    fn src(&self) -> Option<&ParaSrc>;

    /// The paragraphs a vanished mark joined onto its start as read, in order (its own mark among them when that is
    /// vanished too).
    fn joined_parts(&self) -> &[JoinedPart];

    /// The paragraphs a vanished mark joined onto its start as read, when its own mark isn't one of them.
    fn joined_before(&self) -> &[JoinedPart];

    /// How many paragraphs it was read as: more than one when vanished marks joined them, none for one made since.
    #[cfg(test)]
    fn read_parts(&self) -> usize;

    /// The ends of [`Self::joined_parts`] moved by `to` (part, end).
    fn move_joined(&mut self, to: impl Fn(usize, u32) -> u32);
}

impl ParagraphOrigin for Paragraph {
    fn src(&self) -> Option<&ParaSrc> {
        self.origin.get()
    }

    fn joined_parts(&self) -> &[JoinedPart] {
        self.src().map_or(&[][..], |s| &s.joined[..])
    }

    fn joined_before(&self) -> &[JoinedPart] {
        self.src().filter(|s| s.joined.len() + 1 == s.ranges.len()).map_or(&[][..], |s| &s.joined[..])
    }

    #[cfg(test)]
    fn read_parts(&self) -> usize {
        self.src().map_or(0, |s| s.ranges.len())
    }

    fn move_joined(&mut self, to: impl Fn(usize, u32) -> u32) {
        let Some(s) = self.src() else { return };
        let mut s = s.clone();
        for (k, j) in s.joined.iter_mut().enumerate() {
            j.end = to(k, j.end);
        }
        self.origin = Origin::new(s);
    }
}

/// A table read from DOCX: its place in its part.
pub(crate) trait TableOrigin {
    /// Its element's byte range in its part.
    fn src(&self) -> Option<&Range<usize>>;
}

impl TableOrigin for Table {
    fn src(&self) -> Option<&Range<usize>> {
        self.origin.get()
    }
}

/// A row read from DOCX: its place in its part.
pub(crate) trait RowOrigin {
    /// Its element's byte range in its part.
    fn src(&self) -> Option<&Range<usize>>;
}

impl RowOrigin for Row {
    fn src(&self) -> Option<&Range<usize>> {
        self.origin.get()
    }
}

/// An element as read.
#[derive(Debug)]
pub struct Xml(pub Rc<str>);

/// Where a float was read from: its drawing's byte range, or its frame's paragraphs'.
#[derive(Debug)]
pub struct FloatSrc {
    pub drawing: Option<Range<usize>>,
    pub frame: Option<Range<usize>>,
}

/// A legacy form field as read: its `w:ffData`, and its data as it holds them.
#[derive(Debug)]
pub struct FormSrc {
    pub xml: Rc<str>,
    pub read: super::forms::FormData,
}

/// A float read from DOCX: its drawing or frame as read.
pub(crate) trait FloatOrigin {
    /// An origin for a float read from its drawing at `drawing`, or from a frame's paragraphs at `frame`.
    fn origin_at(drawing: Option<Range<usize>>, frame: Option<Range<usize>>) -> Origin;

    /// Its drawing's byte range in its part.
    fn src(&self) -> Option<&Range<usize>>;

    /// Its frame's paragraphs' byte range in its part.
    fn frame_src(&self) -> Option<&Range<usize>>;

    /// Read from a frame's paragraphs (`w:framePr`).
    fn read_as_frame(&self) -> bool;

    /// Whether, as read, the frame came after the start of `p`, its anchor (`None`: either was made since).
    fn read_after(&self, p: &Paragraph) -> Option<bool>;

    /// Equal but for the text box story's blocks.
    fn same_but_story(&self, o: &Float) -> bool;

    /// A frame (`w:framePr` paragraphs made a float): read from one, or made since.
    fn is_frame(&self) -> bool;
}

impl FloatOrigin for Float {
    fn origin_at(drawing: Option<Range<usize>>, frame: Option<Range<usize>>) -> Origin {
        if drawing.is_none() && frame.is_none() { Origin::NONE } else { Origin::new(FloatSrc { drawing, frame }) }
    }

    fn src(&self) -> Option<&Range<usize>> {
        self.origin.get::<FloatSrc>()?.drawing.as_ref()
    }

    fn frame_src(&self) -> Option<&Range<usize>> {
        self.origin.get::<FloatSrc>()?.frame.as_ref()
    }

    fn read_as_frame(&self) -> bool {
        self.frame_src().is_some()
    }

    fn read_after(&self, p: &Paragraph) -> Option<bool> {
        Some(self.frame_src()?.start > p.src()?.ranges.first()?.start)
    }

    fn same_but_story(&self, o: &Float) -> bool {
        let Float {
            id,
            w,
            auto_w,
            h,
            image,
            alt,
            lock_aspect,
            text_box,
            h_relative,
            h_align,
            h_offset,
            v_relative,
            v_align,
            v_offset,
            wrap,
            wrap_text,
            dist,
            effect,
            poly,
            poly_edited,
            behind,
            allow_overlap,
            layout_in_cell,
            order,
            z,
            rot_swap,
            vml,
            relative,
            origin: _,
            locked,
            in_note,
            parts,
            shape,
        } = self;
        let story = match (text_box, &o.text_box) {
            (Some(a), Some(b)) => a.same_box(b),
            (a, b) => a.is_none() && b.is_none(),
        };
        story
            && *id == o.id
            && *w == o.w
            && *auto_w == o.auto_w
            && *h == o.h
            && *image == o.image
            && *alt == o.alt
            && *lock_aspect == o.lock_aspect
            && *h_relative == o.h_relative
            && *h_align == o.h_align
            && *h_offset == o.h_offset
            && *v_relative == o.v_relative
            && *v_align == o.v_align
            && *v_offset == o.v_offset
            && *wrap == o.wrap
            && *wrap_text == o.wrap_text
            && *dist == o.dist
            && *effect == o.effect
            && *poly == o.poly
            && *poly_edited == o.poly_edited
            && *behind == o.behind
            && *allow_overlap == o.allow_overlap
            && *layout_in_cell == o.layout_in_cell
            && *order == o.order
            && *z == o.z
            && *rot_swap == o.rot_swap
            && *vml == o.vml
            && *relative == o.relative
            && self.src() == o.src()
            && *locked == o.locked
            && self.frame_src() == o.frame_src()
            && *in_note == o.in_note
            && *parts == o.parts
            && crate::docx::drawing::same_shape(shape.as_deref(), o.shape.as_deref())
    }

    fn is_frame(&self) -> bool {
        self.read_as_frame() || self.text_box.as_ref().is_some_and(|tb| tb.blocks.iter().any(|b| matches!(b, Block::Para(p) if p.style.in_frame)))
    }
}

/// An origin holding the element `xml` as read.
pub(in crate::docx) fn xml_origin(xml: Rc<str>) -> Origin {
    Origin::new(Xml(xml))
}

fn xml_of(o: &Origin) -> Option<&Rc<str>> {
    o.get::<Xml>().map(|x| &x.0)
}

/// A picture read from DOCX: its run child as read.
pub(crate) trait PictureOrigin {
    /// Its run child as read (`w:drawing`, `w:pict`, `mc:AlternateContent`).
    fn origin_xml(&self) -> Option<&Rc<str>>;

    /// The program id of the embedded object it was read as (`w:object`'s first `ProgID`), when it is one.
    fn prog_id(&self) -> Option<&str>;

    /// Equal but for the text story's blocks.
    fn same_but_story(&self, o: &Picture) -> bool;
}

impl PictureOrigin for Picture {
    fn origin_xml(&self) -> Option<&Rc<str>> {
        xml_of(&self.origin)
    }

    fn prog_id(&self) -> Option<&str> {
        prog_id(self.origin_xml()?)
    }

    fn same_but_story(&self, o: &Picture) -> bool {
        let Picture { w, h, image, alt, lock_aspect, effect, shape, text, origin: _ } = self;
        let text = match (text, &o.text) {
            (Some(a), Some(b)) => a.id == b.id && a.text_box.same_box(&b.text_box),
            (a, b) => a.is_none() && b.is_none(),
        };
        text && *w == o.w
            && *h == o.h
            && *image == o.image
            && *alt == o.alt
            && *lock_aspect == o.lock_aspect
            && *effect == o.effect
            && crate::docx::drawing::same_shape(shape.as_deref(), o.shape.as_deref())
    }
}

/// A ruby read from DOCX: its element as read.
pub(crate) trait RubyOrigin {
    /// Its `w:ruby` element as read (or as last written).
    fn origin_xml(&self) -> Option<&Rc<str>>;

    /// It with `xml` in place of its `w:ruby` element: the same element, written otherwise.
    fn with_origin_xml(&self, xml: Rc<str>) -> Ruby;
}

impl RubyOrigin for Ruby {
    fn origin_xml(&self) -> Option<&Rc<str>> {
        xml_of(&self.origin)
    }

    fn with_origin_xml(&self, xml: Rc<str>) -> Ruby {
        Ruby { origin: xml_origin(xml), ..self.clone() }
    }
}

/// A legacy form field read from DOCX: its `w:ffData` as read.
pub(crate) trait FormFieldOrigin {
    /// A field read with its `w:ffData` element `xml`.
    fn read(xml: Rc<str>, data: super::forms::FormData) -> FormField;

    /// Where it was read from.
    fn src(&self) -> Option<&FormSrc>;

    /// Its data changed since reading.
    fn changed(&self) -> bool;

    /// Its `w:ffData` element as read.
    fn origin_xml(&self) -> Option<&Rc<str>>;

    /// It with `xml` in place of its `w:ffData` element: the same element, written otherwise.
    fn with_origin_xml(&self, xml: Rc<str>) -> FormField;
}

impl FormFieldOrigin for FormField {
    fn read(xml: Rc<str>, data: super::forms::FormData) -> FormField {
        FormField { origin: Origin::new(FormSrc { xml, read: data.clone() }), data }
    }

    fn src(&self) -> Option<&FormSrc> {
        self.origin.get()
    }

    fn changed(&self) -> bool {
        self.src().is_some_and(|s| s.read != self.data)
    }

    fn origin_xml(&self) -> Option<&Rc<str>> {
        self.src().map(|s| &s.xml)
    }

    fn with_origin_xml(&self, xml: Rc<str>) -> FormField {
        match self.src() {
            Some(s) => FormField { origin: Origin::new(FormSrc { xml, read: s.read.clone() }), data: self.data.clone() },
            None => self.clone(),
        }
    }
}

/// A field edge's markers' runs as read: their XML, and where they are in their part.
#[derive(Debug)]
pub struct EdgeSource {
    pub xml: Rc<str>,
    pub at: Range<usize>,
}

/// A field edge read from DOCX: its markers' runs as read.
pub(crate) trait EdgeOrigin {
    /// Where it was read from.
    fn src(&self) -> Option<&EdgeSource>;

    /// Its markers' runs as read.
    fn origin_xml(&self) -> Option<&Rc<str>>;

    /// It with `xml` in place of its markers' runs: the same runs, written otherwise.
    fn with_origin_xml(&self, xml: Rc<str>) -> Edge;
}

impl EdgeOrigin for Edge {
    fn src(&self) -> Option<&EdgeSource> {
        self.origin.get()
    }

    fn origin_xml(&self) -> Option<&Rc<str>> {
        self.src().map(|s| &s.xml)
    }

    fn with_origin_xml(&self, xml: Rc<str>) -> Edge {
        match self.src() {
            Some(s) => Edge { kind: self.kind.clone(), origin: Origin::new(EdgeSource { xml, at: s.at.clone() }) },
            None => self.clone(),
        }
    }
}

/// An editing permission marker read from DOCX: its element as read.
pub(crate) trait PermissionOrigin {
    /// Its element as read.
    fn origin_xml(&self) -> Option<&Rc<str>>;

    /// It with `xml` in place of its element: the same element, written otherwise.
    fn with_origin_xml(&self, xml: Rc<str>) -> super::review::Permission;
}

impl PermissionOrigin for super::review::Permission {
    fn origin_xml(&self) -> Option<&Rc<str>> {
        xml_of(&self.origin)
    }

    fn with_origin_xml(&self, xml: Rc<str>) -> super::review::Permission {
        super::review::Permission { origin: xml_origin(xml), ..self.clone() }
    }
}

/// A section read from DOCX: its page as read.
pub(crate) trait SectionOrigin {
    /// Its page as read.
    fn read_page(&self) -> Option<&SectionPage>;
}

impl SectionOrigin for Section {
    fn read_page(&self) -> Option<&SectionPage> {
        self.origin.get()
    }
}

/// A content control as read: its start tag with its properties (`w:sdtPr`, `w:sdtEndPr`) and the content's start
/// tag, the content's and the control's end tags, its properties, and where the tags start in its part (the writer
/// checks the bytes there before using them).
#[derive(Clone, Debug)]
pub struct ControlSrc {
    pub open: String,
    pub close: String,
    pub read: ControlProps,
    pub at: (usize, usize),
}

/// A content control read from DOCX: its markup as read.
pub(crate) trait ControlOrigin {
    /// Where it was read from.
    fn src(&self) -> Option<&ControlSrc>;

    /// Its start markup as read (empty for a new control, or one written from its properties).
    fn open(&self) -> &str;

    /// Its end markup as read.
    fn close(&self) -> &str;

    /// Its properties as read.
    fn read(&self) -> Option<&ControlProps>;

    /// Where its start and end markup start in its part.
    fn at(&self) -> Option<(usize, usize)>;

    /// Its origin rebuilt by `f` (none: nothing to change).
    fn edit_src(&mut self, f: impl FnOnce(&mut ControlSrc));

    /// The bytes its markup as read takes.
    fn origin_bytes(&self) -> usize;

    /// The `w:id` its markup as read gives it.
    fn origin_id(&self) -> Option<i64>;

    /// Its markup as read with the `w:id` it gives `id()` in place of its own, when it gives one.
    fn renumber_origin(&mut self, id: impl FnOnce() -> i64);

    /// Whether it is written with the markup it was read with.
    fn as_read(&self) -> bool;
}

impl ControlOrigin for Control {
    fn src(&self) -> Option<&ControlSrc> {
        self.origin.get()
    }

    fn open(&self) -> &str {
        self.src().map_or("", |s| &s.open)
    }

    fn close(&self) -> &str {
        self.src().map_or("", |s| &s.close)
    }

    fn read(&self) -> Option<&ControlProps> {
        self.src().map(|s| &s.read)
    }

    fn at(&self) -> Option<(usize, usize)> {
        self.src().map(|s| s.at)
    }

    fn edit_src(&mut self, f: impl FnOnce(&mut ControlSrc)) {
        if let Some(s) = self.src() {
            let mut s = s.clone();
            f(&mut s);
            self.origin = Origin::new(s);
        }
    }

    fn origin_bytes(&self) -> usize {
        self.open().len() + self.close().len()
    }

    fn origin_id(&self) -> Option<i64> {
        sdt_id_at(self.open()).and_then(|r| self.open()[r].parse().ok())
    }

    fn renumber_origin(&mut self, id: impl FnOnce() -> i64) {
        if let Some(r) = sdt_id_at(self.open()) {
            let n = id();
            self.edit_src(|s| s.open.replace_range(r, &n.to_string()));
        }
    }

    fn as_read(&self) -> bool {
        !self.open().is_empty() && self.read() == Some(&self.props)
    }
}

/// Where a control's start markup holds its `w:id` value.
fn sdt_id_at(open: &str) -> Option<Range<usize>> {
    let k = open.find(":id ").or_else(|| open.find("<w:id "))?;
    let v = k + open[k..].find("val=\"")? + 5;
    Some(v..v + open[v..].find('"')?)
}

/// The local name of the element `xml` starts with, whatever its prefix.
fn local_name(xml: &str) -> Option<&str> {
    let tag = xml.trim_start().strip_prefix('<')?;
    let name = &tag[..tag.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(tag.len())];
    name.rsplit(':').next()
}

/// An embedded object's program id, from its `w:object` as read (in whatever prefix: a picture's source is a run
/// child of the main namespace or `mc:AlternateContent`): the first `ProgID` attribute, either quote, any spacing round
/// its `=`.
fn prog_id(src: &str) -> Option<&str> {
    if local_name(src) != Some("object") {
        return None;
    }
    let mut rest = src;
    while let Some(i) = rest.find("ProgID") {
        let before = rest[..i].chars().next_back();
        rest = &rest[i + "ProgID".len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = rest.trim_start().strip_prefix('=').map(str::trim_start) else { continue };
        let Some(quote) = value.chars().next().filter(|&q| q == '"' || q == '\'') else { continue };
        return value[1..].split(quote).next();
    }
    None
}

/// The byte range a read block spans in its part.
#[cfg(test)]
pub(crate) fn read_range(b: &Block) -> Option<Range<usize>> {
    match b {
        Block::Para(p) => p.src().map(|s| s.ranges[0].start..s.ranges[s.ranges.len() - 1].end),
        Block::Table(t) => t.src().cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::prog_id;

    #[test]
    fn program_ids() {
        assert_eq!(prog_id(r#"<w:object><o:OLEObject Type="Embed" ProgID="Excel.Sheet.12" r:id="x"/></w:object>"#), Some("Excel.Sheet.12"));
        assert_eq!(prog_id("<w:pict/>"), None);
        assert_eq!(prog_id(r#"<wpml:object><o:OLEObject ProgID="Excel.Sheet.12"/></wpml:object>"#), Some("Excel.Sheet.12"), "any prefix");
        assert_eq!(prog_id(r#"<w:objects ProgID="x"/>"#), None);
        assert_eq!(prog_id("<w:object><o:OLEObject ProgID = 'Word.Document.12'/></w:object>"), Some("Word.Document.12"));
        assert_eq!(prog_id(r#"<w:object><o:OLEObject XProgID="a" ProgID="b"/></w:object>"#), Some("b"));
    }
}
