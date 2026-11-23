//! Field catalogue
//!
//! Word's fields as Insert Field lists them: category, description, syntax, arguments and switches; a field code
//! composed from those choices and read back into them; and the code's problems as the dialog flags them.

use crate::docx::fields::{FieldCode, parse, quote};

/// Insert Field's categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldCategory {
    DateAndTime,
    DocumentAutomation,
    DocumentInformation,
    EquationsAndFormulas,
    IndexAndTables,
    LinksAndReferences,
    MailMerge,
    Numbering,
    UserInformation,
}

impl FieldCategory {
    /// Every category, in Insert Field's order (after All).
    pub const ALL: [FieldCategory; 9] = [
        FieldCategory::DateAndTime,
        FieldCategory::DocumentAutomation,
        FieldCategory::DocumentInformation,
        FieldCategory::EquationsAndFormulas,
        FieldCategory::IndexAndTables,
        FieldCategory::LinksAndReferences,
        FieldCategory::MailMerge,
        FieldCategory::Numbering,
        FieldCategory::UserInformation,
    ];

    /// Its name in Insert Field.
    pub fn label(self) -> &'static str {
        match self {
            FieldCategory::DateAndTime => "Date and Time",
            FieldCategory::DocumentAutomation => "Document Automation",
            FieldCategory::DocumentInformation => "Document Information",
            FieldCategory::EquationsAndFormulas => "Equations and Formulas",
            FieldCategory::IndexAndTables => "Index and Tables",
            FieldCategory::LinksAndReferences => "Links and References",
            FieldCategory::MailMerge => "Mail Merge",
            FieldCategory::Numbering => "Numbering",
            FieldCategory::UserInformation => "User Information",
        }
    }
}

/// What an argument names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    /// Any text.
    Text,
    /// A bookmark of the document.
    Bookmark,
    /// A style's name.
    Style,
    /// A document property's name.
    Property,
    /// A document variable's name.
    Variable,
    /// A file name or address.
    File,
    /// A sequence or list name, an entry or a merge field's name.
    Name,
    /// An expression or a comparison.
    Expression,
    /// A whole number.
    Number,
}

/// An argument a field takes, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgSpec {
    pub name: &'static str,
    pub kind: ArgKind,
    pub required: bool,
}

/// What a switch takes after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchArg {
    /// Nothing: it is on or off.
    None,
    /// Text.
    Text,
    /// A whole number.
    Number,
    /// A bookmark.
    Bookmark,
    /// A style's name.
    Style,
    /// A sequence or caption label's name.
    Name,
    /// Heading or outline levels (`1-3`).
    Levels,
}

/// A field-specific switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwitchSpec {
    /// The letter (or `!`) after the backslash.
    pub flag: &'static str,
    pub arg: SwitchArg,
    pub description: &'static str,
}

/// The general switches a field takes besides its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Formats {
    /// `\*`: text and number formats (Upper, Roman, CardText, ...).
    pub text: bool,
    /// `\#`: a numeric picture.
    pub numeric: bool,
    /// `\@`: a date-time picture.
    pub date: bool,
}

/// A field as Insert Field describes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSpec {
    /// Its name as written in a code (`PAGE`, `=`).
    pub name: &'static str,
    pub categories: &'static [FieldCategory],
    pub description: &'static str,
    /// The syntax Insert Field shows under Field codes.
    pub syntax: &'static str,
    pub args: &'static [ArgSpec],
    pub switches: &'static [SwitchSpec],
    pub formats: Formats,
}

impl FieldSpec {
    /// Whether Twip works out its result; else its result is kept as written.
    pub fn evaluated(&self) -> bool {
        use crate::docx::FieldKind as K;
        if self.name == "HYPERLINK" {
            return true;
        }
        let code = parse(&format!(" {} x ", self.name));
        !matches!(code.kind(), None | Some(K::Bookmark(_) | K::Invalid | K::SubTemplate))
    }

    /// Its switch `flag`.
    pub fn switch(&self, flag: &str) -> Option<&SwitchSpec> {
        self.switches.iter().find(|s| s.flag.eq_ignore_ascii_case(flag))
    }
}

use ArgKind as A;
use FieldCategory as C;
use SwitchArg as S;

const fn arg(name: &'static str, kind: ArgKind, required: bool) -> ArgSpec {
    ArgSpec { name, kind, required }
}

const fn sw(flag: &'static str, arg: SwitchArg, description: &'static str) -> SwitchSpec {
    SwitchSpec { flag, arg, description }
}

const NONE: Formats = Formats { text: false, numeric: false, date: false };
const TEXT: Formats = Formats { text: true, numeric: false, date: false };
const NUM: Formats = Formats { text: true, numeric: true, date: false };
const DATE: Formats = Formats { text: true, numeric: false, date: true };
const ALL: Formats = Formats { text: true, numeric: true, date: true };

const DATE_SW: &[SwitchSpec] = &[
    sw("h", S::None, "Uses the Hijri/Lunar calendar"),
    sw("l", S::None, "Uses the last format chosen with Insert Date and Time"),
    sw("s", S::None, "Uses the Saka Era calendar"),
];
const REF_SW: &[SwitchSpec] = &[
    sw("d", S::Text, "Separates sequence numbers with this text"),
    sw("f", S::None, "Increments footnote, endnote or annotation numbers"),
    sw("h", S::None, "Creates a hyperlink to the bookmarked paragraph"),
    sw("n", S::None, "Inserts the paragraph number without trailing periods"),
    sw("p", S::None, "Inserts above or below relative to the field"),
    sw("r", S::None, "Inserts the paragraph number in relative context"),
    sw("t", S::None, "Suppresses all non-delimiter, non-numerical text"),
    sw("w", S::None, "Inserts the paragraph number in full context"),
];
const PROMPT_SW: &[SwitchSpec] = &[sw("d", S::Text, "The default response"), sw("o", S::None, "Prompts only once in a mail merge")];

/// Every field Insert Field lists, by name.
pub static CATALOGUE: &[FieldSpec] = &[
    FieldSpec {
        name: "=",
        categories: &[C::EquationsAndFormulas],
        description: "Calculates the result of an expression",
        syntax: "= Formula [Bookmark] [\\# Numeric-Picture]",
        args: &[arg("Formula", A::Expression, true)],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "ADDRESSBLOCK",
        categories: &[C::MailMerge],
        description: "Inserts a mail merge address block",
        syntax: "ADDRESSBLOCK [Switches]",
        args: &[],
        switches: &[
            sw("c", S::Number, "Includes the country or region: 0 never, 1 always, 2 when it differs"),
            sw("d", S::None, "Formats the address for the country or region"),
            sw("e", S::Text, "The country or region to exclude"),
            sw("f", S::Text, "The format of the name and address"),
            sw("l", S::Number, "The language of the address"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "ADVANCE",
        categories: &[C::EquationsAndFormulas],
        description: "Offsets subsequent text within a line to the left, right, up, or down",
        syntax: "ADVANCE [Switches]",
        args: &[],
        switches: &[
            sw("d", S::Number, "Moves text down by this many points"),
            sw("l", S::Number, "Moves text left by this many points"),
            sw("r", S::Number, "Moves text right by this many points"),
            sw("u", S::Number, "Moves text up by this many points"),
            sw("x", S::Number, "Moves text this many points from the column's left edge"),
            sw("y", S::Number, "Moves text to this many points from the page's top"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "ASK",
        categories: &[C::MailMerge],
        description: "Prompts the user to assign text to a bookmark",
        syntax: "ASK Bookmark \"Prompt\" [Switches]",
        args: &[arg("Bookmark", A::Bookmark, true), arg("Prompt", A::Text, true)],
        switches: PROMPT_SW,
        formats: NONE,
    },
    FieldSpec {
        name: "AUTHOR",
        categories: &[C::DocumentInformation],
        description: "The name of the document's author from Document Properties",
        syntax: "AUTHOR [\"NewName\"]",
        args: &[arg("NewName", A::Text, false)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "AUTONUM",
        categories: &[C::Numbering],
        description: "Inserts an automatic number",
        syntax: "AUTONUM [Switches]",
        args: &[],
        switches: &[sw("s", S::Text, "The separator character")],
        formats: NONE,
    },
    FieldSpec {
        name: "AUTONUMLGL",
        categories: &[C::Numbering],
        description: "Inserts an automatic number in legal format",
        syntax: "AUTONUMLGL [Switches]",
        args: &[],
        switches: &[sw("e", S::None, "Leaves out the trailing period"), sw("s", S::Text, "The separator character")],
        formats: NONE,
    },
    FieldSpec {
        name: "AUTONUMOUT",
        categories: &[C::Numbering],
        description: "Inserts an automatic number in outline format",
        syntax: "AUTONUMOUT",
        args: &[],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "AUTOTEXT",
        categories: &[C::LinksAndReferences],
        description: "Inserts an AutoText entry",
        syntax: "AUTOTEXT AutoTextEntry",
        args: &[arg("AutoTextEntry", A::Name, true)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "AUTOTEXTLIST",
        categories: &[C::LinksAndReferences],
        description: "Inserts text based on style",
        syntax: "AUTOTEXTLIST \"LiteralText\" [Switches]",
        args: &[arg("LiteralText", A::Text, true)],
        switches: &[sw("s", S::Style, "The style whose AutoText entries it lists"), sw("t", S::Text, "The ScreenTip")],
        formats: TEXT,
    },
    FieldSpec {
        name: "BARCODE",
        categories: &[C::MailMerge],
        description: "Inserts a delivery point barcode (no longer supported)",
        syntax: "BARCODE \"Address\" [Switches]",
        args: &[arg("Address", A::Text, true)],
        switches: &[
            sw("b", S::None, "The address is a bookmark"),
            sw("f", S::Text, "Inserts a Facing Identification Mark"),
            sw("u", S::None, "Identifies the barcode as a US postal address"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "BIBLIOGRAPHY",
        categories: &[C::LinksAndReferences],
        description: "Inserts the bibliography",
        syntax: "BIBLIOGRAPHY [Switches]",
        args: &[],
        switches: &[
            sw("f", S::Number, "Filters the bibliography by this language"),
            sw("l", S::Number, "Shows the sources in this language"),
            sw("m", S::Text, "Includes only the sources cited in this text"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "BIDIOUTLINE",
        categories: &[C::Numbering],
        description: "Inserts an automatic number in bidirectional outline format",
        syntax: "BIDIOUTLINE",
        args: &[],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "CITATION",
        categories: &[C::LinksAndReferences],
        description: "Inserts the contents of a citation",
        syntax: "CITATION Tag [Switches]",
        args: &[arg("Tag", A::Name, true)],
        switches: &[
            sw("f", S::Text, "Adds a prefix"),
            sw("l", S::Number, "Shows the citation in this language"),
            sw("m", S::Text, "Adds another source"),
            sw("n", S::None, "Leaves out the author"),
            sw("p", S::Text, "Adds page numbers"),
            sw("s", S::Text, "Adds a suffix"),
            sw("t", S::None, "Leaves out the title"),
            sw("v", S::Text, "Adds the volume"),
            sw("y", S::None, "Leaves out the year"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "COMMENTS",
        categories: &[C::DocumentInformation],
        description: "The comments from Document Properties",
        syntax: "COMMENTS [\"NewComments\"]",
        args: &[arg("NewComments", A::Text, false)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "COMPARE",
        categories: &[C::DocumentAutomation, C::MailMerge],
        description: "Compares two values and inserts 1 if the comparison is true or 0 if it is false",
        syntax: "COMPARE Expression1 Operator Expression2",
        args: &[arg("Expression1", A::Expression, true), arg("Operator", A::Text, true), arg("Expression2", A::Expression, true)],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "CREATEDATE",
        categories: &[C::DateAndTime],
        description: "The date the document was created",
        syntax: "CREATEDATE [\\@ \"Date-Time Picture\"] [Switches]",
        args: &[],
        switches: DATE_SW,
        formats: DATE,
    },
    FieldSpec {
        name: "DATABASE",
        categories: &[C::MailMerge],
        description: "Inserts the results of a database query",
        syntax: "DATABASE [Switches]",
        args: &[],
        switches: &[
            sw("b", S::Number, "The parts of the table format to apply"),
            sw("c", S::Text, "The connection to the data"),
            sw("d", S::Text, "The path and file name of the database"),
            sw("f", S::Number, "The first record to insert"),
            sw("h", S::None, "Inserts the field names as the table's first row"),
            sw("l", S::Number, "The table format to apply"),
            sw("o", S::None, "Inserts the data at the start of a merge"),
            sw("s", S::Text, "The SQL query"),
            sw("t", S::Number, "The last record to insert"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "DATE",
        categories: &[C::DateAndTime],
        description: "Today's date",
        syntax: "DATE [\\@ \"Date-Time Picture\"] [Switches]",
        args: &[],
        switches: DATE_SW,
        formats: DATE,
    },
    FieldSpec {
        name: "DISPLAYBARCODE",
        categories: &[C::DocumentAutomation],
        description: "Inserts a barcode",
        syntax: "DISPLAYBARCODE FieldArgument BarcodeType [Switches]",
        args: &[arg("FieldArgument", A::Text, true), arg("BarcodeType", A::Name, true)],
        switches: &[
            sw("a", S::None, "Adds a check digit"),
            sw("b", S::Text, "The background colour"),
            sw("c", S::Text, "The case code"),
            sw("d", S::None, "Adds text below the barcode"),
            sw("f", S::Text, "The foreground colour"),
            sw("h", S::Number, "The height"),
            sw("p", S::Text, "The postal barcode style"),
            sw("q", S::Number, "The error correction level"),
            sw("r", S::Number, "The rotation"),
            sw("s", S::Number, "The scale"),
            sw("t", S::None, "Shows the text below the barcode"),
            sw("x", S::None, "Shows the barcode's human-readable text"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "DOCPROPERTY",
        categories: &[C::DocumentInformation],
        description: "Inserts the value of a document property",
        syntax: "DOCPROPERTY \"Name\"",
        args: &[arg("Name", A::Property, true)],
        switches: &[],
        formats: ALL,
    },
    FieldSpec {
        name: "DOCVARIABLE",
        categories: &[C::DocumentAutomation],
        description: "Inserts the value of a document variable",
        syntax: "DOCVARIABLE \"Name\"",
        args: &[arg("Name", A::Variable, true)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "EDITTIME",
        categories: &[C::DateAndTime],
        description: "The total document editing time",
        syntax: "EDITTIME",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "EQ",
        categories: &[C::EquationsAndFormulas],
        description: "Creates an equation",
        syntax: "EQ Instructions",
        args: &[arg("Instructions", A::Expression, true)],
        switches: &[
            sw("a", S::Text, "Array"),
            sw("b", S::Text, "Bracket"),
            sw("d", S::Text, "Displace"),
            sw("f", S::Text, "Fraction"),
            sw("i", S::Text, "Integral"),
            sw("l", S::Text, "List"),
            sw("o", S::Text, "Overstrike"),
            sw("r", S::Text, "Radical"),
            sw("s", S::Text, "Superscript or subscript"),
            sw("x", S::Text, "Box"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "FILENAME",
        categories: &[C::DocumentInformation],
        description: "The document's name and location",
        syntax: "FILENAME [Switches]",
        args: &[],
        switches: &[sw("p", S::None, "Adds the path to the file name")],
        formats: TEXT,
    },
    FieldSpec {
        name: "FILESIZE",
        categories: &[C::DocumentInformation],
        description: "The size of the document on disk",
        syntax: "FILESIZE [Switches]",
        args: &[],
        switches: &[sw("k", S::None, "In kilobytes"), sw("m", S::None, "In megabytes")],
        formats: NUM,
    },
    FieldSpec {
        name: "FILLIN",
        categories: &[C::MailMerge],
        description: "Prompts the user to enter text",
        syntax: "FILLIN [\"Prompt\"] [Switches]",
        args: &[arg("Prompt", A::Text, false)],
        switches: PROMPT_SW,
        formats: TEXT,
    },
    FieldSpec {
        name: "GOTOBUTTON",
        categories: &[C::DocumentAutomation],
        description: "Inserts a jump command",
        syntax: "GOTOBUTTON Destination DisplayText",
        args: &[arg("Destination", A::Bookmark, true), arg("DisplayText", A::Text, true)],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "GREETINGLINE",
        categories: &[C::MailMerge],
        description: "Inserts a mail merge greeting line",
        syntax: "GREETINGLINE [Switches]",
        args: &[],
        switches: &[
            sw("e", S::Text, "The greeting when the name is empty"),
            sw("f", S::Text, "The format of the name"),
            sw("l", S::Number, "The language of the greeting"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "HYPERLINK",
        categories: &[C::LinksAndReferences],
        description: "Opens and jumps to the specified file",
        syntax: "HYPERLINK \"Filename\" [Switches]",
        args: &[arg("Filename", A::File, false)],
        switches: &[
            sw("l", S::Bookmark, "The place in the file to jump to"),
            sw("m", S::None, "Appends coordinates for a server-side image map"),
            sw("n", S::None, "Opens the target in a new window"),
            sw("o", S::Text, "The ScreenTip"),
            sw("t", S::Text, "The frame to open the target in"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "IF",
        categories: &[C::DocumentAutomation, C::MailMerge],
        description: "Evaluates arguments conditionally",
        syntax: "IF Expression1 Operator Expression2 TrueText FalseText",
        args: &[
            arg("Expression1", A::Expression, true),
            arg("Operator", A::Text, true),
            arg("Expression2", A::Expression, true),
            arg("TrueText", A::Text, true),
            arg("FalseText", A::Text, false),
        ],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "INCLUDEPICTURE",
        categories: &[C::LinksAndReferences],
        description: "Inserts a picture from a file",
        syntax: "INCLUDEPICTURE \"FileName\" [Switches]",
        args: &[arg("FileName", A::File, true)],
        switches: &[sw("c", S::Text, "The graphics filter"), sw("d", S::None, "Doesn't store the picture's data with the document")],
        formats: NONE,
    },
    FieldSpec {
        name: "INCLUDETEXT",
        categories: &[C::LinksAndReferences],
        description: "Inserts all or part of the text of another document",
        syntax: "INCLUDETEXT \"FileName\" [Bookmark] [Switches]",
        args: &[arg("FileName", A::File, true), arg("Bookmark", A::Bookmark, false)],
        switches: &[
            sw("!", S::None, "Keeps the fields in the included text from updating"),
            sw("c", S::Text, "The converter"),
            sw("n", S::Text, "The namespace mappings for XPath"),
            sw("t", S::Text, "The XSL transformation"),
            sw("x", S::Text, "The XPath of the part to include"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "INDEX",
        categories: &[C::IndexAndTables],
        description: "Creates an index",
        syntax: "INDEX [Switches]",
        args: &[],
        switches: &[
            sw("b", S::Bookmark, "Indexes only the bookmarked part"),
            sw("c", S::Number, "The number of columns"),
            sw("d", S::Text, "The separator between sequence and page numbers"),
            sw("e", S::Text, "The separator between an entry and its page number"),
            sw("f", S::Text, "Uses only entries of this type"),
            sw("g", S::Text, "The separator in a page range"),
            sw("h", S::Text, "The heading between letter groups"),
            sw("k", S::Text, "The separator for cross-references"),
            sw("l", S::Text, "The separator between page numbers"),
            sw("p", S::Text, "Limits the index to these letters"),
            sw("r", S::None, "Runs subentries into their entry's line"),
            sw("s", S::Name, "Includes the sequence number with the page number"),
            sw("y", S::None, "Uses yomi text for sorting"),
            sw("z", S::Number, "The language for sorting"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "INFO",
        categories: &[C::DocumentInformation],
        description: "Inserts data from Document Properties",
        syntax: "INFO [InfoType] [\"NewValue\"]",
        args: &[arg("InfoType", A::Property, false), arg("NewValue", A::Text, false)],
        switches: &[],
        formats: ALL,
    },
    FieldSpec {
        name: "KEYWORDS",
        categories: &[C::DocumentInformation],
        description: "The keywords from Document Properties",
        syntax: "KEYWORDS [\"NewKeywords\"]",
        args: &[arg("NewKeywords", A::Text, false)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "LASTSAVEDBY",
        categories: &[C::DocumentInformation],
        description: "The name of the last person who saved the document",
        syntax: "LASTSAVEDBY",
        args: &[],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "LINK",
        categories: &[C::LinksAndReferences],
        description: "Links information from another file",
        syntax: "LINK ClassName \"FileName\" [PlaceReference] [Switches]",
        args: &[arg("ClassName", A::Name, true), arg("FileName", A::File, true), arg("PlaceReference", A::Text, false)],
        switches: &[
            sw("a", S::None, "Updates the link automatically"),
            sw("b", S::None, "As a bitmap"),
            sw("d", S::None, "Doesn't store the data with the document"),
            sw("f", S::Number, "How the result is formatted when updated"),
            sw("h", S::None, "As HTML"),
            sw("p", S::None, "As a picture"),
            sw("r", S::None, "As rich text"),
            sw("t", S::None, "As text"),
            sw("u", S::None, "As Unicode text"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "LISTNUM",
        categories: &[C::Numbering],
        description: "Inserts an element in a list",
        syntax: "LISTNUM [\"Name\"] [Switches]",
        args: &[arg("Name", A::Name, false)],
        switches: &[sw("l", S::Number, "The level in the list"), sw("s", S::Number, "The start-at value")],
        formats: NONE,
    },
    FieldSpec {
        name: "MACROBUTTON",
        categories: &[C::DocumentAutomation],
        description: "Runs a macro",
        syntax: "MACROBUTTON MacroName DisplayText",
        args: &[arg("MacroName", A::Name, true), arg("DisplayText", A::Text, true)],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "MERGEBARCODE",
        categories: &[C::MailMerge],
        description: "Inserts a barcode from a mail merge field",
        syntax: "MERGEBARCODE FieldName BarcodeType [Switches]",
        args: &[arg("FieldName", A::Name, true), arg("BarcodeType", A::Name, true)],
        switches: &[
            sw("a", S::None, "Adds a check digit"),
            sw("b", S::Text, "The background colour"),
            sw("c", S::Text, "The case code"),
            sw("d", S::None, "Adds text below the barcode"),
            sw("f", S::Text, "The foreground colour"),
            sw("h", S::Number, "The height"),
            sw("p", S::Text, "The postal barcode style"),
            sw("q", S::Number, "The error correction level"),
            sw("r", S::Number, "The rotation"),
            sw("s", S::Number, "The scale"),
            sw("t", S::None, "Shows the text below the barcode"),
            sw("x", S::None, "Shows the barcode's human-readable text"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "MERGEFIELD",
        categories: &[C::MailMerge],
        description: "Inserts a mail merge field",
        syntax: "MERGEFIELD FieldName [Switches]",
        args: &[arg("FieldName", A::Name, true)],
        switches: &[
            sw("b", S::Text, "Text to insert before the field when it isn't blank"),
            sw("f", S::Text, "Text to insert after the field when it isn't blank"),
            sw("m", S::None, "The field is a mapped field"),
            sw("v", S::None, "Uses vertical formatting"),
        ],
        formats: ALL,
    },
    FieldSpec {
        name: "MERGEREC",
        categories: &[C::MailMerge],
        description: "The number of the current merge record",
        syntax: "MERGEREC",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "MERGESEQ",
        categories: &[C::MailMerge],
        description: "The merge record sequence number",
        syntax: "MERGESEQ",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "NEXT",
        categories: &[C::MailMerge],
        description: "Goes to the next record in a mail merge",
        syntax: "NEXT",
        args: &[],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "NEXTIF",
        categories: &[C::MailMerge],
        description: "Conditionally goes to the next record in a mail merge",
        syntax: "NEXTIF Expression1 Operator Expression2",
        args: &[arg("Expression1", A::Expression, true), arg("Operator", A::Text, true), arg("Expression2", A::Expression, true)],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "NOTEREF",
        categories: &[C::LinksAndReferences],
        description: "Inserts the number of a footnote or endnote",
        syntax: "NOTEREF Bookmark [Switches]",
        args: &[arg("Bookmark", A::Bookmark, true)],
        switches: &[
            sw("f", S::None, "Formats the number as a note reference mark"),
            sw("h", S::None, "Creates a hyperlink to the note"),
            sw("p", S::None, "Inserts above or below relative to the field"),
        ],
        formats: NUM,
    },
    FieldSpec {
        name: "NUMCHARS",
        categories: &[C::DocumentInformation],
        description: "The number of characters in the document",
        syntax: "NUMCHARS",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "NUMPAGES",
        categories: &[C::DocumentInformation],
        description: "The number of pages in the document",
        syntax: "NUMPAGES",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "NUMWORDS",
        categories: &[C::DocumentInformation],
        description: "The number of words in the document",
        syntax: "NUMWORDS",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "PAGE",
        categories: &[C::Numbering],
        description: "Inserts the number of the current page",
        syntax: "PAGE [\\* Format Switch]",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "PAGEREF",
        categories: &[C::LinksAndReferences],
        description: "Inserts the number of the page containing the specified bookmark",
        syntax: "PAGEREF Bookmark [Switches]",
        args: &[arg("Bookmark", A::Bookmark, true)],
        switches: &[sw("h", S::None, "Creates a hyperlink to the bookmark"), sw("p", S::None, "Inserts above or below relative to the field")],
        formats: NUM,
    },
    FieldSpec {
        name: "PRINT",
        categories: &[C::DocumentAutomation],
        description: "Sends a command to the printer",
        syntax: "PRINT \"PrinterInstructions\" [Switches]",
        args: &[arg("PrinterInstructions", A::Text, true)],
        switches: &[sw("p", S::Text, "The PostScript drawing rectangle")],
        formats: NONE,
    },
    FieldSpec {
        name: "PRINTDATE",
        categories: &[C::DateAndTime],
        description: "The date the document was last printed",
        syntax: "PRINTDATE [\\@ \"Date-Time Picture\"] [Switches]",
        args: &[],
        switches: DATE_SW,
        formats: DATE,
    },
    FieldSpec {
        name: "PRIVATE",
        categories: &[C::DocumentAutomation],
        description: "Stores data for documents converted from other file formats",
        syntax: "PRIVATE",
        args: &[],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "QUOTE",
        categories: &[C::LinksAndReferences],
        description: "Inserts literal text",
        syntax: "QUOTE \"Literal Text\"",
        args: &[arg("Literal Text", A::Text, true)],
        switches: &[],
        formats: ALL,
    },
    FieldSpec {
        name: "RD",
        categories: &[C::IndexAndTables],
        description: "Creates an index, table of contents, table of figures, and/or table of authorities by using multiple documents",
        syntax: "RD \"FileName\" [Switches]",
        args: &[arg("FileName", A::File, true)],
        switches: &[sw("f", S::None, "The path is relative to this document")],
        formats: NONE,
    },
    FieldSpec {
        name: "REF",
        categories: &[C::LinksAndReferences],
        description: "Inserts the text of the specified bookmark",
        syntax: "REF Bookmark [Switches]",
        args: &[arg("Bookmark", A::Bookmark, true)],
        switches: REF_SW,
        formats: ALL,
    },
    FieldSpec {
        name: "REVNUM",
        categories: &[C::DocumentInformation],
        description: "The number of times the document has been saved",
        syntax: "REVNUM",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "SAVEDATE",
        categories: &[C::DateAndTime],
        description: "The date the document was last saved",
        syntax: "SAVEDATE [\\@ \"Date-Time Picture\"] [Switches]",
        args: &[],
        switches: DATE_SW,
        formats: DATE,
    },
    FieldSpec {
        name: "SECTION",
        categories: &[C::Numbering],
        description: "Inserts the number of the current section",
        syntax: "SECTION",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "SECTIONPAGES",
        categories: &[C::Numbering],
        description: "Inserts the total number of pages in the section",
        syntax: "SECTIONPAGES",
        args: &[],
        switches: &[],
        formats: NUM,
    },
    FieldSpec {
        name: "SEQ",
        categories: &[C::Numbering],
        description: "Inserts an automatic sequence number",
        syntax: "SEQ Identifier [Bookmark] [Switches]",
        args: &[arg("Identifier", A::Name, true), arg("Bookmark", A::Bookmark, false)],
        switches: &[
            sw("c", S::None, "Repeats the closest preceding sequence number"),
            sw("h", S::None, "Hides the field's result"),
            sw("n", S::None, "Inserts the next sequence number"),
            sw("r", S::Number, "Resets the sequence number to this"),
            sw("s", S::Number, "Resets the number at this heading level"),
        ],
        formats: NUM,
    },
    FieldSpec {
        name: "SET",
        categories: &[C::MailMerge],
        description: "Assigns new text to a bookmark",
        syntax: "SET Bookmark \"Text\"",
        args: &[arg("Bookmark", A::Bookmark, true), arg("Text", A::Text, true)],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "SKIPIF",
        categories: &[C::MailMerge],
        description: "Skips the current record in a mail merge when a condition holds",
        syntax: "SKIPIF Expression1 Operator Expression2",
        args: &[arg("Expression1", A::Expression, true), arg("Operator", A::Text, true), arg("Expression2", A::Expression, true)],
        switches: &[],
        formats: NONE,
    },
    FieldSpec {
        name: "STYLEREF",
        categories: &[C::LinksAndReferences],
        description: "Inserts the text from a like-styled paragraph",
        syntax: "STYLEREF \"StyleIdentifier\" [Switches]",
        args: &[arg("StyleIdentifier", A::Style, true)],
        switches: &[
            sw("l", S::None, "Searches from the bottom of the page up"),
            sw("n", S::None, "Inserts the paragraph number without trailing periods"),
            sw("p", S::None, "Inserts above or below relative to the field"),
            sw("r", S::None, "Inserts the paragraph number in relative context"),
            sw("t", S::None, "Suppresses all non-delimiter, non-numerical text"),
            sw("w", S::None, "Inserts the paragraph number in full context"),
        ],
        formats: TEXT,
    },
    FieldSpec {
        name: "SUBJECT",
        categories: &[C::DocumentInformation],
        description: "The subject from Document Properties",
        syntax: "SUBJECT [\"NewSubject\"]",
        args: &[arg("NewSubject", A::Text, false)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "SYMBOL",
        categories: &[C::EquationsAndFormulas],
        description: "Inserts a special character",
        syntax: "SYMBOL CharNum [Switches]",
        args: &[arg("CharNum", A::Number, true)],
        switches: &[
            sw("a", S::None, "The character is ANSI"),
            sw("f", S::Text, "The font"),
            sw("h", S::None, "Doesn't change the line spacing"),
            sw("j", S::None, "The character is Shift-JIS"),
            sw("s", S::Number, "The font size in points"),
            sw("u", S::None, "The character is Unicode"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "TA",
        categories: &[C::IndexAndTables],
        description: "Marks a table of authorities entry",
        syntax: "TA [Switches]",
        args: &[],
        switches: &[
            sw("b", S::None, "Makes the page number bold"),
            sw("c", S::Number, "The entry's category"),
            sw("i", S::None, "Makes the page number italic"),
            sw("l", S::Text, "The long citation"),
            sw("r", S::Bookmark, "Uses the bookmark's page range"),
            sw("s", S::Text, "The short citation"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "TC",
        categories: &[C::IndexAndTables],
        description: "Marks a table of contents entry",
        syntax: "TC \"Text\" [Switches]",
        args: &[arg("Text", A::Text, true)],
        switches: &[
            sw("f", S::Text, "The list the entry goes in"),
            sw("l", S::Number, "The entry's level"),
            sw("n", S::None, "Leaves out the page number"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "TEMPLATE",
        categories: &[C::DocumentInformation],
        description: "The name of the document's template",
        syntax: "TEMPLATE [Switches]",
        args: &[],
        switches: &[sw("p", S::None, "Adds the path to the file name")],
        formats: TEXT,
    },
    FieldSpec {
        name: "TIME",
        categories: &[C::DateAndTime],
        description: "The current time",
        syntax: "TIME [\\@ \"Date-Time Picture\"]",
        args: &[],
        switches: &[],
        formats: DATE,
    },
    FieldSpec {
        name: "TITLE",
        categories: &[C::DocumentInformation],
        description: "The title from Document Properties",
        syntax: "TITLE [\"NewTitle\"]",
        args: &[arg("NewTitle", A::Text, false)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "TOA",
        categories: &[C::IndexAndTables],
        description: "Builds a table of authorities",
        syntax: "TOA [Switches]",
        args: &[],
        switches: &[
            sw("b", S::Bookmark, "Builds it from the bookmarked part"),
            sw("c", S::Number, "The category of entries"),
            sw("d", S::Text, "The separator between sequence and page numbers"),
            sw("e", S::Text, "The separator between an entry and its page number"),
            sw("f", S::None, "Removes the entries' formatting"),
            sw("g", S::Text, "The separator in a page range"),
            sw("h", S::None, "Includes the category heading"),
            sw("l", S::Text, "The separator between page numbers"),
            sw("p", S::None, "Replaces five or more page references with passim"),
            sw("s", S::Name, "Includes the sequence number with the page number"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "TOC",
        categories: &[C::IndexAndTables],
        description: "Builds a table of contents",
        syntax: "TOC [Switches]",
        args: &[],
        switches: &[
            sw("a", S::Name, "Builds a table of figures of this caption label, without labels and numbers"),
            sw("b", S::Bookmark, "Builds it from the bookmarked part"),
            sw("c", S::Name, "Builds a table of figures of this caption label"),
            sw("d", S::Text, "The separator between sequence and page numbers"),
            sw("f", S::Text, "Builds it from TC entries of this list"),
            sw("h", S::None, "Makes the entries hyperlinks"),
            sw("l", S::Levels, "Builds it from TC entries of these levels"),
            sw("n", S::Levels, "Leaves out page numbers at these levels"),
            sw("o", S::Levels, "Builds it from headings of these outline levels"),
            sw("p", S::Text, "The separator between an entry and its page number"),
            sw("s", S::Name, "Includes the sequence number with the page number"),
            sw("t", S::Text, "Builds it from these styles and their levels"),
            sw("u", S::None, "Builds it from paragraphs' outline levels"),
            sw("w", S::None, "Keeps tabs in entries"),
            sw("x", S::None, "Keeps line breaks in entries"),
            sw("z", S::None, "Hides page numbers in Web Layout view"),
        ],
        formats: NONE,
    },
    FieldSpec {
        name: "USERADDRESS",
        categories: &[C::UserInformation],
        description: "Your postal address from Office Options",
        syntax: "USERADDRESS [\"NewAddress\"]",
        args: &[arg("NewAddress", A::Text, false)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "USERINITIALS",
        categories: &[C::UserInformation],
        description: "Your initials from Office Options",
        syntax: "USERINITIALS [\"NewInitials\"]",
        args: &[arg("NewInitials", A::Text, false)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "USERNAME",
        categories: &[C::UserInformation],
        description: "Your name from Office Options",
        syntax: "USERNAME [\"NewName\"]",
        args: &[arg("NewName", A::Text, false)],
        switches: &[],
        formats: TEXT,
    },
    FieldSpec {
        name: "XE",
        categories: &[C::IndexAndTables],
        description: "Marks an index entry",
        syntax: "XE \"Text\" [Switches]",
        args: &[arg("Text", A::Text, true)],
        switches: &[
            sw("b", S::None, "Makes the page number bold"),
            sw("f", S::Text, "The index the entry goes in"),
            sw("i", S::None, "Makes the page number italic"),
            sw("r", S::Bookmark, "Uses the bookmark's page range"),
            sw("t", S::Text, "Text in place of the page number"),
            sw("y", S::Text, "The entry's yomi"),
        ],
        formats: NONE,
    },
];

/// The field `name` (any case), when Word has one.
pub fn field(name: &str) -> Option<&'static FieldSpec> {
    CATALOGUE.iter().find(|f| f.name.eq_ignore_ascii_case(name))
}

/// The fields of `category` (`None`: All), by name.
pub fn fields_in(category: Option<FieldCategory>) -> impl Iterator<Item = &'static FieldSpec> {
    CATALOGUE.iter().filter(move |f| category.is_none_or(|c| f.categories.contains(&c)))
}

/// The `\*` text formats Field properties offer as Format.
pub const TEXT_FORMATS: &[(&str, &str)] = &[("Upper", "Uppercase"), ("Lower", "Lowercase"), ("FirstCap", "First capital"), ("Caps", "Title case")];

/// The `\*` number formats Field properties offer as Format.
pub const NUMBER_FORMATS: &[(&str, &str)] = &[
    ("Arabic", "1, 2, 3, ..."),
    ("ArabicDash", "- 1 -, - 2 -, - 3 -, ..."),
    ("alphabetic", "a, b, c, ..."),
    ("ALPHABETIC", "A, B, C, ..."),
    ("roman", "i, ii, iii, ..."),
    ("ROMAN", "I, II, III, ..."),
    ("Ordinal", "1st, 2nd, 3rd, ..."),
    ("CardText", "One, Two, Three, ..."),
    ("OrdText", "First, Second, Third, ..."),
    ("Hex", "hex ..."),
    ("DollarText", "Dollar Text"),
];

/// The numeric pictures Field properties lists (`\#`).
pub const NUMERIC_PICTURES: &[&str] = &["0", "0%", "0.00", "0.00%", "#,##0", "#,##0.00", "$#,##0.00;($#,##0.00)", "#,##0.00 €;(#,##0.00 €)"];

/// The date-time pictures Field properties lists (`\@`), as Word lists them for English (United States).
pub const DATE_PICTURES: &[&str] = &[
    "M/d/yyyy",
    "dddd, MMMM d, yyyy",
    "MMMM d, yyyy",
    "M/d/yy",
    "yyyy-MM-dd",
    "d-MMM-yy",
    "M.d.yyyy",
    "MMM. d, yy",
    "d MMMM yyyy",
    "MMMM yy",
    "MMM-yy",
    "M/d/yyyy h:mm am/pm",
    "M/d/yyyy h:mm:ss am/pm",
    "h:mm am/pm",
    "h:mm:ss am/pm",
    "HH:mm",
    "HH:mm:ss",
];

/// What Insert Field's choices make a field code from: the field, its arguments, its own switches, its formats and
/// Preserve formatting during updates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldChoice {
    pub name: String,
    pub args: Vec<String>,
    /// Its own switches (flag, then what follows it).
    pub switches: Vec<(String, Option<String>)>,
    /// `\*` formats other than MERGEFORMAT: a text or number format.
    pub formats: Vec<String>,
    /// `\#`.
    pub numeric: Option<String>,
    /// `\@`.
    pub date: Option<String>,
    /// `\* MERGEFORMAT`.
    pub preserve_formatting: bool,
}

/// A value as a code writes it: bare when it is one word without quotes or backslashes, else quoted.
fn value(v: &str) -> String {
    if !v.is_empty() && !v.contains(|c: char| c.is_whitespace() || c == '"' || c == '\\') { v.to_string() } else { quote(v) }
}

impl FieldChoice {
    /// The field code the choices make, with Word's spaces around it.
    pub fn code(&self) -> String {
        let mut out = format!(" {}", self.name);
        for a in &self.args {
            out.push(' ');
            out.push_str(&value(a));
        }
        for (flag, a) in &self.switches {
            out.push_str(&format!(" \\{flag}"));
            if let Some(a) = a {
                out.push(' ');
                out.push_str(&value(a));
            }
        }
        if let Some(p) = &self.numeric {
            out.push_str(&format!(" \\# {}", value(p)));
        }
        if let Some(p) = &self.date {
            out.push_str(&format!(" \\@ {}", quote(p)));
        }
        for f in &self.formats {
            out.push_str(&format!(" \\* {f}"));
        }
        if self.preserve_formatting {
            out.push_str(" \\* MERGEFORMAT");
        }
        out.push(' ');
        out
    }

    /// The choices a field code makes, as Insert Field opens on an existing field.
    pub fn of_code(code: &str) -> FieldChoice {
        let c: FieldCode = parse(code);
        let mut out = FieldChoice { name: c.name.clone(), args: c.args.clone(), ..FieldChoice::default() };
        for s in &c.switches {
            match s.flag {
                '*' => match s.arg.as_deref() {
                    Some(f) if f.eq_ignore_ascii_case("MERGEFORMAT") => out.preserve_formatting = true,
                    Some(f) => out.formats.push(f.to_string()),
                    None => {}
                },
                '#' => out.numeric = s.arg.clone(),
                '@' => out.date = s.arg.clone(),
                f => out.switches.push((f.to_string(), s.arg.clone())),
            }
        }
        out
    }
}

/// The field the selection is in and its choices, as Insert Field opens on it (its OK then changes the field's code
/// rather than inserting one).
pub fn choice_here(e: &crate::editing::Editor) -> Option<(crate::docx::ElementId, FieldChoice)> {
    let (id, _) = e.selected_element()?;
    let crate::docx::ElementId::Field(_) = id else { return None };
    let code = e.element_context_of(id, false).code?;
    Some((id, FieldChoice::of_code(&code)))
}

/// What is wrong with a field code, as Insert Field flags it before inserting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldProblem {
    /// No field name.
    Empty,
    /// A field Word has no such name for (kept as written, its result too).
    UnknownField(String),
    /// A required argument left out.
    MissingArgument(&'static str),
    /// A switch the field doesn't take.
    UnknownSwitch(String),
    /// A switch that takes a value given none.
    MissingSwitchValue(String),
    /// A switch that takes a whole number given something else.
    NotANumber(String),
    /// A format switch the field doesn't take (`\*`, `\#` or `\@`).
    FormatNotTaken(char),
}

/// The problems of field code `code`; none for a code Insert Field takes as it is.
pub fn check(code: &str) -> Vec<FieldProblem> {
    let c = parse(code);
    if c.name.is_empty() {
        return vec![FieldProblem::Empty];
    }
    let name = if c.name.starts_with('=') { "=" } else { c.name.as_str() };
    let Some(spec) = field(name) else { return vec![FieldProblem::UnknownField(c.name.clone())] };
    let mut out = Vec::new();
    let given = if name == "=" { usize::from(c.name.len() > 1) + c.args.len() } else { c.args.len() };
    if let Some(a) = spec.args.iter().take_while(|a| a.required).nth(given) {
        out.push(FieldProblem::MissingArgument(a.name));
    }
    for s in &c.switches {
        let takes = match s.flag {
            '*' => {
                spec.formats.text || s.arg.as_deref().is_some_and(|a| a.eq_ignore_ascii_case("MERGEFORMAT") || a.eq_ignore_ascii_case("CHARFORMAT"))
            }
            '#' => spec.formats.numeric,
            '@' => spec.formats.date,
            _ => true,
        };
        if !takes {
            out.push(FieldProblem::FormatNotTaken(s.flag));
            continue;
        }
        if matches!(s.flag, '*' | '#' | '@') {
            if s.arg.as_deref().is_none_or(str::is_empty) {
                out.push(FieldProblem::MissingSwitchValue(s.flag.to_string()));
            }
            continue;
        }
        let flag = s.flag.to_string();
        match spec.switch(&flag) {
            None => out.push(FieldProblem::UnknownSwitch(flag)),
            Some(sp) if sp.arg != SwitchArg::None && s.arg.as_deref().is_none_or(str::is_empty) => out.push(FieldProblem::MissingSwitchValue(flag)),
            Some(sp) if sp.arg == SwitchArg::Number && s.arg.as_deref().is_some_and(|a| a.trim().parse::<i64>().is_err()) => {
                out.push(FieldProblem::NotANumber(flag))
            }
            Some(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_word_lists_is_in_a_category_and_sorted() {
        assert!(CATALOGUE.windows(2).all(|w| w[0].name < w[1].name), "by name");
        assert!(CATALOGUE.iter().all(|f| !f.categories.is_empty() && !f.syntax.is_empty()));
        let dates: Vec<&str> = fields_in(Some(FieldCategory::DateAndTime)).map(|f| f.name).collect();
        assert_eq!(dates, ["CREATEDATE", "DATE", "EDITTIME", "PRINTDATE", "SAVEDATE", "TIME"]);
        assert_eq!(fields_in(None).count(), CATALOGUE.len());
        assert!(field("page").is_some_and(FieldSpec::evaluated) && field("=").is_some_and(FieldSpec::evaluated));
        assert!(!field("INCLUDEPICTURE").is_some_and(FieldSpec::evaluated), "kept as written");
    }

    #[test]
    fn choices_make_a_code_and_read_back_from_it() {
        let c = FieldChoice {
            name: "REF".into(),
            args: vec!["Intro".into()],
            switches: vec![("h".into(), None)],
            formats: vec!["Upper".into()],
            preserve_formatting: true,
            ..FieldChoice::default()
        };
        assert_eq!(c.code(), " REF Intro \\h \\* Upper \\* MERGEFORMAT ");
        assert_eq!(FieldChoice::of_code(&c.code()), c);
        let d = FieldChoice { name: "DATE".into(), date: Some("MMMM d, yyyy".into()), ..FieldChoice::default() };
        assert_eq!(d.code(), " DATE \\@ \"MMMM d, yyyy\" ");
        assert_eq!(FieldChoice::of_code(&d.code()), d);
    }

    #[test]
    fn a_code_s_problems_are_found() {
        assert!(check(" PAGE \\* roman ").is_empty());
        assert!(check(" REF Intro \\h ").is_empty());
        assert_eq!(check(""), [FieldProblem::Empty]);
        assert_eq!(check(" REF "), [FieldProblem::MissingArgument("Bookmark")]);
        assert_eq!(check(" PAGEREF Intro \\q "), [FieldProblem::UnknownSwitch("q".into())]);
        assert_eq!(check(" SEQ Figure \\r x "), [FieldProblem::NotANumber("r".into())]);
        assert_eq!(check(" TOC \\o "), [FieldProblem::MissingSwitchValue("o".into())]);
        assert_eq!(check(" NUMPAGES \\@ \"d\" "), [FieldProblem::FormatNotTaken('@')]);
        assert_eq!(check(" NOSUCH "), [FieldProblem::UnknownField("NOSUCH".into())]);
        assert!(check(" =2+2 ").is_empty() && check(" = ").contains(&FieldProblem::MissingArgument("Formula")));
    }
}
