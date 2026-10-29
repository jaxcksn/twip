//! What reading or writing a document couldn't keep or make sense of: warnings with a code, a message and where, one
//! per code and part, with how many times it happened. Recoverable problems never fail a read.

use serde::{Deserialize, Serialize};

/// The kinds of problem a read or a write reports. Each serializes as its stable name (`unsupported-element`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningCode {
    /// An element the reader doesn't keep: it was left out, counted per element name.
    UnsupportedElement,
    /// An image whose data isn't a format the engine decodes: it draws as an empty box.
    ImageUndecodable,
    /// A font family no face in the library has: text falls back to another family.
    FontUnresolved,
    /// A family drawn with a substitute the substitution rules name (informational, off by default).
    FontSubstituted,
    /// Content a write had to rewrite, losing bookmarks, comments or hyperlinks inside it.
    RewrittenContent,
    /// Text a PDF export drew with a fallback font.
    PdfFontFallback,
    /// A revision without a `w:id`: it was given a new one.
    RevisionIdMissing,
    /// A comment or move range end with no start, or a start with no end.
    RangeUnpaired,
    /// A revision or comment with no author: it was attributed to an empty name.
    AuthorMissing,
    /// A comment no range or reference in the document points to.
    CommentUnanchored,
    /// A comment reference or range naming a comment the comments part doesn't have.
    CommentReferenceOrphan,
}

impl WarningCode {
    /// The stable name hosts see.
    pub fn name(self) -> &'static str {
        match self {
            WarningCode::UnsupportedElement => "unsupported-element",
            WarningCode::ImageUndecodable => "image-undecodable",
            WarningCode::FontUnresolved => "font-unresolved",
            WarningCode::FontSubstituted => "font-substituted",
            WarningCode::RewrittenContent => "rewritten-content",
            WarningCode::PdfFontFallback => "pdf-font-fallback",
            WarningCode::RevisionIdMissing => "revision-id-missing",
            WarningCode::RangeUnpaired => "range-unpaired",
            WarningCode::AuthorMissing => "author-missing",
            WarningCode::CommentUnanchored => "comment-unanchored",
            WarningCode::CommentReferenceOrphan => "comment-reference-orphan",
        }
    }
}

/// Where a warning was first found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Location {
    /// The package part (`word/document.xml`, `word/comments.xml`, ...).
    pub part: String,
    /// The paragraph's index in the part's body, when the warning is about one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<u32>,
}

/// One kind of problem in one part, with how many times it happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Warning {
    pub code: WarningCode,
    /// The first occurrence's description.
    pub message: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// The warnings of one read or write, merged by code and part.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diagnostics {
    pub warnings: Vec<Warning>,
}

impl Diagnostics {
    pub fn warn(&mut self, code: WarningCode, part: Option<&str>, message: impl FnOnce() -> String) {
        let same = |w: &&mut Warning| w.code == code && w.location.as_ref().map(|l| l.part.as_str()) == part;
        match self.warnings.iter_mut().find(same) {
            Some(w) => w.count += 1,
            None => self.warnings.push(Warning {
                code,
                message: message(),
                count: 1,
                location: part.map(|p| Location { part: p.to_string(), paragraph: None }),
            }),
        }
    }

    pub fn count(&self, code: WarningCode) -> usize {
        self.warnings.iter().filter(|w| w.code == code).map(|w| w.count as usize).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_merge_by_code_and_part() {
        let mut d = Diagnostics::default();
        d.warn(WarningCode::RangeUnpaired, Some("word/document.xml"), || "comment 3 has no end".into());
        d.warn(WarningCode::RangeUnpaired, Some("word/document.xml"), || unreachable!("merged, not described again"));
        d.warn(WarningCode::RangeUnpaired, Some("word/footnotes.xml"), || "comment 9 has no end".into());
        assert_eq!(d.warnings.len(), 2);
        assert_eq!(d.count(WarningCode::RangeUnpaired), 3);
        assert_eq!(d.warnings[0].message, "comment 3 has no end");
    }

    #[test]
    fn codes_serialize_as_their_names() {
        for code in [WarningCode::UnsupportedElement, WarningCode::PdfFontFallback, WarningCode::CommentReferenceOrphan] {
            assert_eq!(serde_json::to_value(code).ok(), Some(serde_json::Value::from(code.name())));
        }
    }
}
