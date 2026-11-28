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

impl Paragraph {
    /// Where it was read from.
    pub(in crate::docx) fn src(&self) -> Option<&ParaSrc> {
        self.origin.get()
    }

    /// The paragraphs a vanished mark joined onto its start as read, in order (its own mark among them when that is
    /// vanished too).
    pub fn joined_parts(&self) -> &[JoinedPart] {
        self.src().map_or(&[][..], |s| &s.joined[..])
    }

    /// The paragraphs a vanished mark joined onto its start as read, when its own mark isn't one of them.
    pub fn joined_before(&self) -> &[JoinedPart] {
        self.src().filter(|s| s.joined.len() + 1 == s.ranges.len()).map_or(&[][..], |s| &s.joined[..])
    }

    /// How many paragraphs it was read as: more than one when vanished marks joined them, none for one made since.
    pub fn read_parts(&self) -> usize {
        self.src().map_or(0, |s| s.ranges.len())
    }

    /// The ends of [`Self::joined_parts`] moved by `to` (part, end).
    pub fn move_joined(&mut self, to: impl Fn(usize, u32) -> u32) {
        let Some(s) = self.src() else { return };
        let mut s = s.clone();
        for (k, j) in s.joined.iter_mut().enumerate() {
            j.end = to(k, j.end);
        }
        self.origin = Origin::new(s);
    }
}

impl Table {
    /// Its element's byte range in its part.
    pub(in crate::docx) fn src(&self) -> Option<&Range<usize>> {
        self.origin.get()
    }
}

impl Row {
    /// Its element's byte range in its part.
    pub(in crate::docx) fn src(&self) -> Option<&Range<usize>> {
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

impl Float {
    /// An origin for a float read from its drawing at `drawing`, or from a frame's paragraphs at `frame`.
    pub(in crate::docx) fn origin_at(drawing: Option<Range<usize>>, frame: Option<Range<usize>>) -> Origin {
        if drawing.is_none() && frame.is_none() { Origin::NONE } else { Origin::new(FloatSrc { drawing, frame }) }
    }

    /// Its drawing's byte range in its part.
    pub(in crate::docx) fn src(&self) -> Option<&Range<usize>> {
        self.origin.get::<FloatSrc>()?.drawing.as_ref()
    }

    /// Its frame's paragraphs' byte range in its part.
    pub(in crate::docx) fn frame_src(&self) -> Option<&Range<usize>> {
        self.origin.get::<FloatSrc>()?.frame.as_ref()
    }

    /// Read from a frame's paragraphs (`w:framePr`).
    pub fn read_as_frame(&self) -> bool {
        self.frame_src().is_some()
    }

    /// Whether, as read, the frame came after the start of `p`, its anchor (`None`: either was made since).
    pub fn read_after(&self, p: &Paragraph) -> Option<bool> {
        Some(self.frame_src()?.start > p.src()?.ranges.first()?.start)
    }
}

/// An origin holding the element `xml` as read.
pub(in crate::docx) fn xml_origin(xml: Rc<str>) -> Origin {
    Origin::new(Xml(xml))
}

fn xml_of(o: &Origin) -> Option<&Rc<str>> {
    o.get::<Xml>().map(|x| &x.0)
}

impl Picture {
    /// Its run child as read (`w:drawing`, `w:pict`, `mc:AlternateContent`).
    pub fn origin_xml(&self) -> Option<&Rc<str>> {
        xml_of(&self.origin)
    }

    /// The program id of the embedded object it was read as (`w:object`'s first `ProgID`), when it is one.
    pub fn prog_id(&self) -> Option<&str> {
        prog_id(self.origin_xml()?)
    }
}

impl Ruby {
    /// Its `w:ruby` element as read (or as last written).
    pub fn origin_xml(&self) -> Option<&Rc<str>> {
        xml_of(&self.origin)
    }

    /// It with `xml` in place of its `w:ruby` element: the same element, written otherwise.
    pub fn with_origin_xml(&self, xml: Rc<str>) -> Ruby {
        Ruby { origin: xml_origin(xml), ..self.clone() }
    }
}

impl FormField {
    /// A field read with its `w:ffData` element `xml`.
    pub(in crate::docx) fn read(xml: Rc<str>, data: super::forms::FormData) -> FormField {
        FormField { origin: Origin::new(FormSrc { xml, read: data.clone() }), data }
    }

    pub(in crate::docx) fn src(&self) -> Option<&FormSrc> {
        self.origin.get()
    }

    /// Its data changed since reading.
    pub(in crate::docx) fn changed(&self) -> bool {
        self.src().is_some_and(|s| s.read != self.data)
    }

    /// Its `w:ffData` element as read.
    pub fn origin_xml(&self) -> Option<&Rc<str>> {
        self.src().map(|s| &s.xml)
    }

    /// It with `xml` in place of its `w:ffData` element: the same element, written otherwise.
    pub fn with_origin_xml(&self, xml: Rc<str>) -> FormField {
        match self.src() {
            Some(s) => FormField { origin: Origin::new(FormSrc { xml, read: s.read.clone() }), data: self.data.clone() },
            None => self.clone(),
        }
    }
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
