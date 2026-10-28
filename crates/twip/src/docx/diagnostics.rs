//! What reading a document couldn't keep or make sense of: warnings with a code, a message and where, one per code
//! and place kind, with how many times it happened. Recoverable problems never fail a read.

/// The kinds of problem a read reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WarningCode {
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
            WarningCode::RevisionIdMissing => "revision-id-missing",
            WarningCode::RangeUnpaired => "range-unpaired",
            WarningCode::AuthorMissing => "author-missing",
            WarningCode::CommentUnanchored => "comment-unanchored",
            WarningCode::CommentReferenceOrphan => "comment-reference-orphan",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub code: WarningCode,
    /// The first occurrence's description.
    pub message: String,
    /// The part it was first found in (`word/document.xml`, `word/comments.xml`, ...).
    pub part: Option<String>,
    pub count: usize,
}

/// The warnings of one read, merged by code and part.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diagnostics {
    pub warnings: Vec<Warning>,
}

impl Diagnostics {
    pub fn warn(&mut self, code: WarningCode, part: Option<&str>, message: impl FnOnce() -> String) {
        match self.warnings.iter_mut().find(|w| w.code == code && w.part.as_deref() == part) {
            Some(w) => w.count += 1,
            None => self.warnings.push(Warning { code, message: message(), part: part.map(str::to_string), count: 1 }),
        }
    }

    pub fn count(&self, code: WarningCode) -> usize {
        self.warnings.iter().filter(|w| w.code == code).map(|w| w.count).sum()
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
}
