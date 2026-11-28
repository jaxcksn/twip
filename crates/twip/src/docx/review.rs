//! Tracked revisions, comments and people in the document model. Revisions sit on what they change: a run's insert,
//! delete and format-change slots (the three a run can carry), a paragraph mark's run, and the property changes of
//! paragraphs, sections, tables, rows and cells, each with the properties before the change. Comment and move ranges
//! are zero-unit inline markers; comments and their threads live in [`Comments`].

use std::collections::HashMap;
use std::rc::Rc;

pub use crate::document::review::*;

/// A protection password's hashing, as Word hashes it.
pub trait PasswordHash: Sized {
    /// The stored form of `password`, as Word stores it: Word's legacy key for it, salted, hashed with SHA-512 and spun
    /// 100 000 times.
    fn new(password: &str) -> Self;

    /// `password` is this one: hashed from Word's legacy key for it (its hex digits padded or not) or, as ECMA-376
    /// describes, from the password itself. An algorithm this doesn't compute never matches.
    fn matches(&self, password: &str) -> bool;
}

impl PasswordHash for Password {
    fn new(password: &str) -> Password {
        let mut salt = [0u8; 16];
        getrandom::fill(&mut salt).expect("the operating system provides random bytes");
        let spin = 100_000;
        let key = legacy_key(password, true);
        let hash = hash("SHA-512", &salt, &key, spin).unwrap_or_default();
        Password { algorithm: "SHA-512".into(), hash: crate::base64::base64(&hash), salt: crate::base64::base64(&salt), spin }
    }

    fn matches(&self, password: &str) -> bool {
        let Some(salt) = crate::base64::from_base64(&self.salt) else { return false };
        // shortcut: the spin count is capped, so a document can't make a check take minutes; Word writes 100 000.
        let spin = self.spin.min(10_000_000);
        let tries = [legacy_key(password, true), legacy_key(password, false), password.to_string()];
        let mut seen: Vec<&str> = Vec::with_capacity(3);
        tries.iter().any(|t| {
            if seen.contains(&t.as_str()) {
                return false;
            }
            seen.push(t);
            hash(&self.algorithm, &salt, t, spin).is_some_and(|h| crate::base64::base64(&h) == self.hash)
        })
    }
}

/// The algorithm name for `w:cryptAlgorithmSid`'s number.
pub fn algorithm_name(sid: &str) -> &str {
    match sid {
        "4" => "SHA-1",
        "12" => "SHA-256",
        "13" => "SHA-384",
        "14" => "SHA-512",
        s => s,
    }
}

/// Word's legacy 32-bit password key (ECMA-376 1st edition, 17.15.1.29), its bytes reversed, as upper-case hex
/// (`pad`: two digits a byte). Empty for an empty password.
fn legacy_key(password: &str, pad: bool) -> String {
    const INITIAL: [u16; 15] =
        [0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9, 0x280C, 0xA96A, 0x4EC3];
    const MATRIX: [[u16; 7]; 15] = [
        [0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09],
        [0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7, 0x9DCF, 0x2BBF],
        [0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0],
        [0x0375, 0x06EA, 0x0DD4, 0x1BA8, 0x3750, 0x6EA0, 0xDD40],
        [0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5],
        [0x6F45, 0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A],
        [0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC, 0x5FD9],
        [0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0],
        [0xB861, 0x60E3, 0xC1C6, 0x93AD, 0x377B, 0x6EF6, 0xDDEC],
        [0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10],
        [0xAA51, 0x4483, 0x8906, 0x022D, 0x045A, 0x08B4, 0x1168],
        [0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C],
        [0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD],
        [0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373, 0x06E6, 0x0DCC],
        [0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4],
    ];
    // Each character's low byte, or its high byte when that is zero; at most 15.
    let bytes: Vec<u8> = password
        .encode_utf16()
        .take(15)
        .map(|c| {
            let [lo, hi] = c.to_le_bytes();
            if lo == 0 { hi } else { lo }
        })
        .collect();
    if bytes.is_empty() {
        return String::new();
    }
    let n = bytes.len();
    let mut high = INITIAL[n - 1];
    for (i, b) in bytes.iter().enumerate() {
        for (bit, m) in MATRIX[15 - n + i].iter().enumerate() {
            if b & (1 << bit) != 0 {
                high ^= m;
            }
        }
    }
    let rotate = |v: u16| ((v >> 14) & 1) | ((v << 1) & 0x7FFF);
    let mut low = 0u16;
    for b in bytes.iter().rev() {
        low = rotate(low) ^ u16::from(*b);
    }
    low = rotate(low) ^ n as u16 ^ 0xCE4B;
    let key = (u32::from(high) << 16) | u32::from(low);
    key.to_le_bytes().iter().map(|b| if pad { format!("{b:02X}") } else { format!("{b:X}") }).collect()
}

/// H(salt + password as UTF-16LE), then `spin` rounds of H(previous + round number as 32-bit little endian).
fn hash(algorithm: &str, salt: &[u8], password: &str, spin: u32) -> Option<Vec<u8>> {
    use sha2::Digest;
    // One digest buffer for every round.
    fn run<D: Digest + sha2::digest::FixedOutputReset>(salt: &[u8], pw: &[u8], spin: u32) -> Vec<u8> {
        let mut d = D::new();
        Digest::update(&mut d, salt);
        Digest::update(&mut d, pw);
        let mut h = d.finalize_reset();
        for i in 0..spin {
            Digest::update(&mut d, &h);
            Digest::update(&mut d, i.to_le_bytes());
            Digest::finalize_into_reset(&mut d, &mut h);
        }
        h.to_vec()
    }
    let pw: Vec<u8> = password.encode_utf16().flat_map(u16::to_le_bytes).collect();
    Some(match algorithm_name(&algorithm.to_ascii_uppercase()) {
        "SHA-512" => run::<sha2::Sha512>(salt, &pw, spin),
        "SHA-384" => run::<sha2::Sha384>(salt, &pw, spin),
        "SHA-256" => run::<sha2::Sha256>(salt, &pw, spin),
        "SHA-1" => run::<sha1::Sha1>(salt, &pw, spin),
        _ => return None,
    })
}

/// `p`'s `w:documentProtection` element (`w`: the prefix with its colon).
pub fn protection_xml(p: &Protection, w: &str) -> String {
    let edit = match p.edit {
        ProtectionKind::None => "none",
        ProtectionKind::ReadOnly => "readOnly",
        ProtectionKind::Comments => "comments",
        ProtectionKind::TrackedChanges => "trackedChanges",
        ProtectionKind::Forms => "forms",
    };
    let flag = |b: bool| if b { "1" } else { "0" };
    let pw = p.password.as_ref().map_or(String::new(), |p| {
        let esc = crate::docx::write::props::esc;
        format!(
            " {w}algorithmName=\"{}\" {w}hashValue=\"{}\" {w}saltValue=\"{}\" {w}spinCount=\"{}\"",
            esc(&p.algorithm),
            esc(&p.hash),
            esc(&p.salt),
            p.spin
        )
    });
    let formatting = if p.formatting { format!(" {w}formatting=\"1\"") } else { String::new() };
    format!("<{w}documentProtection {w}edit=\"{edit}\"{formatting} {w}enforcement=\"{}\"{pw}/>", flag(p.enforced))
}

/// Where revisions show in All Markup: Word's Balloons setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BalloonMode {
    /// Deletions and formatting changes in balloons beside the page, insertions in place.
    Balloons,
    /// Every revision in place. Comments stay in their margin cards.
    Inline,
    /// Formatting changes in balloons, insertions and deletions in place.
    #[default]
    CommentsAndFormatting,
}

/// How a revision marks its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextMark {
    /// Not marked: neither style nor colour.
    None,
    /// In the mark's colour only.
    ColorOnly,
    /// Bold.
    Bold,
    /// Italic.
    Italic,
    /// Underlined.
    Underline,
    /// Double underlined.
    DoubleUnderline,
    /// Struck through.
    Strikethrough,
    /// Double struck through.
    DoubleStrikethrough,
    /// A deletion's text does not show.
    Hidden,
    /// A deletion's text shows as one `^`.
    Caret,
    /// A deletion's text shows as one `#`.
    Pound,
}

impl TextMark {
    /// Whether a deletion's own text does not lay out.
    pub fn hides_text(self) -> bool {
        matches!(self, TextMark::Hidden | TextMark::Caret | TextMark::Pound)
    }

    /// The character a deletion's text shows as.
    pub fn stand_in(self) -> Option<char> {
        match self {
            TextMark::Caret => Some('^'),
            TextMark::Pound => Some('#'),
            _ => None,
        }
    }
}

/// The colour a mark draws in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkColor {
    /// The revision's author's.
    ByAuthor,
    /// The text's own colour.
    Auto,
    /// This colour (0xRRGGBB).
    Fixed(u32),
}

/// How one kind of revision draws: its mark and the mark's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarkLook {
    /// How the text is marked.
    pub mark: TextMark,
    /// The mark's colour.
    pub color: MarkColor,
}

/// Which side of the text change bars go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BarSide {
    /// No change bars.
    None,
    /// Left of the text.
    Left,
    /// Right of the text.
    Right,
    /// The page's outside edge: the right on odd pages with mirrored margins, else the left.
    #[default]
    Outside,
}

/// How revisions draw: Word's Track Changes Options. View state: never saved.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkupStyle {
    /// Inserted text.
    pub insertions: MarkLook,
    /// Deleted text.
    pub deletions: MarkLook,
    /// Text with formatting changes.
    pub formatting: MarkLook,
    /// Text moved away.
    pub moved_from: MarkLook,
    /// Text moved here.
    pub moved_to: MarkLook,
    /// Where change bars go.
    pub change_bars: BarSide,
    /// `None`: automatic ([`ReviewView::change_bar_color`]).
    pub change_bar_color: Option<u32>,
    /// Inserted, deleted, merged and split cells' fill: `ByAuthor` tints the cell with the author's colour, `Auto`
    /// leaves its own fill.
    pub inserted_cells: MarkColor,
    /// Deleted cells' fill.
    pub deleted_cells: MarkColor,
    /// Merged cells' fill.
    pub merged_cells: MarkColor,
    /// Split cells' fill: a cell whose vertical merge was undone.
    pub split_cells: MarkColor,
}

impl Default for MarkupStyle {
    fn default() -> MarkupStyle {
        let look = |mark, color| MarkLook { mark, color };
        MarkupStyle {
            insertions: look(TextMark::Underline, MarkColor::ByAuthor),
            deletions: look(TextMark::Strikethrough, MarkColor::ByAuthor),
            formatting: look(TextMark::None, MarkColor::ByAuthor),
            moved_from: look(TextMark::DoubleStrikethrough, MarkColor::Fixed(AuthorPalette::MOVE)),
            moved_to: look(TextMark::DoubleUnderline, MarkColor::Fixed(AuthorPalette::MOVE)),
            change_bars: BarSide::Outside,
            change_bar_color: None,
            inserted_cells: MarkColor::Fixed(MarkupStyle::LIGHT_BLUE),
            deleted_cells: MarkColor::Fixed(MarkupStyle::PINK),
            merged_cells: MarkColor::Fixed(MarkupStyle::LIGHT_YELLOW),
            split_cells: MarkColor::Fixed(MarkupStyle::LIGHT_ORANGE),
        }
    }
}

impl MarkupStyle {
    /// Word's Light Blue cell highlight, inserted cells' by default.
    pub const LIGHT_BLUE: u32 = 0x99CCFF;
    /// Word's Pink cell highlight, deleted cells' by default.
    pub const PINK: u32 = 0xFF99CC;
    /// Word's Light Yellow cell highlight, merged cells' by default.
    pub const LIGHT_YELLOW: u32 = 0xFFFF99;
    /// Word's Light Orange cell highlight, split cells' by default.
    pub const LIGHT_ORANGE: u32 = 0xFFCC99;

    /// How a revision mark of `kind` draws.
    pub fn look(&self, kind: MarkKind) -> MarkLook {
        match kind {
            MarkKind::Insert => self.insertions,
            MarkKind::Delete => self.deletions,
            MarkKind::MoveFrom => self.moved_from,
            MarkKind::MoveTo => self.moved_to,
            MarkKind::Format => self.formatting,
        }
    }
}

/// The parts of the review view that change how revisions lay out and draw beyond the markup view.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Markup {
    /// Which markup shows.
    pub show: ShowMarkup,
    /// Where revisions show.
    pub balloons: BalloonMode,
    /// How revisions draw.
    pub style: MarkupStyle,
}

impl Markup {
    /// Whether a deletion's text leaves the line in `view`: hidden by the view, not shown (shows as accepted), in a
    /// balloon, or drawn hidden or as a stand-in.
    fn hides_deletion(&self, view: MarkupView, m: &RevisionMark) -> bool {
        view == MarkupView::All && (!self.show.shows(m) || self.balloons == BalloonMode::Balloons || self.style.look(m.kind).mark.hides_text())
    }

    /// Whether a run with `rev` leaves the line in `view`.
    pub fn hides(&self, view: MarkupView, rev: &RunRevisions) -> bool {
        view.hides_change(rev.ins.is_some(), rev.del.is_some()) || rev.del.as_ref().is_some_and(|d| self.hides_deletion(view, d))
    }

    /// Whether a deletion's text shows as a stand-in character (`^`, `#`) in `view`.
    pub fn stand_in(&self, view: MarkupView, rev: &RunRevisions) -> Option<char> {
        let d = rev.del.as_ref().filter(|_| view == MarkupView::All && self.balloons != BalloonMode::Balloons)?;
        self.style.look(d.kind).mark.stand_in().filter(|_| self.show.shows(d))
    }

    /// Rows and cells stay in place in balloon mode and with any deletion mark: only filtered out, they go.
    pub fn hides_row(&self, view: MarkupView, rev: &RowRevisions) -> bool {
        view.hides_change(rev.ins.is_some(), rev.del.is_some()) || rev.del.as_ref().is_some_and(|d| view == MarkupView::All && !self.show.shows(d))
    }

    /// Whether a cell with `rev` leaves the table in `view`.
    pub fn hides_cell(&self, view: MarkupView, rev: &CellRevisions) -> bool {
        match &rev.structural {
            Some((_, CellMarkKind::Insert)) => view.hides_change(true, false),
            Some((m, CellMarkKind::Delete)) => view.hides_change(false, true) || view == MarkupView::All && !self.show.shows(m),
            _ => false,
        }
    }
}

/// Author colours: the default palette by author, unless the host chose one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AuthorPalette {
    pub chosen: HashMap<AuthorId, u32>,
}

impl AuthorPalette {
    /// Colours for authors in order of first appearance (0xRRGGBB).
    pub const DEFAULT: [u32; 12] =
        [0xD13438, 0x0078D4, 0x5C2E91, 0x498205, 0xCC3595, 0x7160E8, 0x038387, 0x6D5700, 0xCF0F1F, 0x4E6AED, 0xB146C2, 0x394146];
    /// Moves, whoever made them.
    pub const MOVE: u32 = 0x00A050;

    pub fn color(&self, author: AuthorId) -> u32 {
        self.chosen.get(&author).copied().unwrap_or(Self::DEFAULT[author.0 as usize % Self::DEFAULT.len()])
    }
}

/// How the document shows its review: the markup view, the author colours, which markup shows and how it draws; and
/// whether hidden text and optional hyphens show (laid out, as Word's display options show them). View state: never
/// saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewView {
    pub view: MarkupView,
    pub palette: Rc<AuthorPalette>,
    /// Which markup shows, where revisions go and how they draw.
    pub markup: Rc<Markup>,
    pub show_hidden: bool,
    /// Optional hyphens show as `¬` (laid out with their width).
    pub show_optional_hyphens: bool,
}

impl ReviewView {
    /// The view a document with `settings` opens in: its Show Markup, the rest as by default.
    pub fn opening(settings: &super::model::Settings) -> ReviewView {
        ReviewView { markup: Rc::new(Markup { show: settings.revision_view.clone(), ..Default::default() }), ..Default::default() }
    }

    /// Whether a run with `rev` leaves the line.
    pub fn hides(&self, rev: &RunRevisions) -> bool {
        self.markup.hides(self.view, rev)
    }

    /// Whether a row with `rev` leaves the table.
    pub fn hides_row(&self, rev: &RowRevisions) -> bool {
        self.markup.hides_row(self.view, rev)
    }

    /// Whether a cell with `rev` leaves the table.
    pub fn hides_cell(&self, rev: &CellRevisions) -> bool {
        self.markup.hides_cell(self.view, rev)
    }

    /// Whether a revision draws its marks: All Markup, its kind and reviewer shown.
    pub fn marks(&self, m: &RevisionMark) -> bool {
        self.view == MarkupView::All && self.markup.show.shows(m)
    }

    /// Whether the view draws change bars at all.
    pub fn change_bars(&self) -> bool {
        self.view.has_change_bars() && self.markup.style.change_bars != BarSide::None
    }

    /// The change bars' colour (0xRRGGBB): the style's, or automatic: red in Simple Markup, grey in All Markup (as
    /// Word draws them).
    pub fn change_bar_color(&self) -> u32 {
        let auto = if self.view == MarkupView::Simple { Self::SIMPLE_BAR } else { Self::MARKUP_BAR };
        self.markup.style.change_bar_color.unwrap_or(auto)
    }

    /// Simple Markup's automatic change bar colour.
    pub const SIMPLE_BAR: u32 = 0xFF0000;
    /// All Markup's automatic change bar colour.
    pub const MARKUP_BAR: u32 = 0x808080;

    /// Whether a revision puts a change bar beside its lines.
    pub fn bars(&self, m: &RevisionMark) -> bool {
        self.change_bars() && self.markup.show.shows(m)
    }

    /// The colour a revision's mark draws in, `None` for the text's own: its look's, an author's from the palette.
    pub fn mark_color(&self, m: &RevisionMark) -> Option<u32> {
        self.color_of(self.markup.style.look(m.kind).color, m.author)
    }

    /// The colour `c` names for `author`'s revision, `None` for the text's own.
    pub fn color_of(&self, c: MarkColor, author: AuthorId) -> Option<u32> {
        match c {
            MarkColor::ByAuthor => Some(self.palette.color(author)),
            MarkColor::Auto => None,
            MarkColor::Fixed(c) => Some(c),
        }
    }

    /// Whether a comment by `author` shows: comments shown, and its reviewer.
    pub fn shows_comment(&self, author: AuthorId) -> bool {
        self.markup.show.shows_comment(author)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_hash_from_words_legacy_key() {
        // ECMA-376 1st edition's example: "Example" keys to 0x64CEED7E, written reversed.
        assert_eq!(legacy_key("Example", true), "7EEDCE64");
        assert_eq!(legacy_key("", true), "");
        let p = Password::new("Example");
        assert!(p.matches("Example") && !p.matches("example") && !p.matches(""));
        // A hash of the password itself, and Word 2007's SHA-1 (`w:cryptAlgorithmSid="4"`).
        for algorithm in ["SHA-256", "SHA-1"] {
            let h = hash(algorithm, b"0123456789abcdef", "Example", 10).expect("computed");
            let p =
                Password { algorithm: algorithm.into(), hash: crate::base64::base64(&h), salt: crate::base64::base64(b"0123456789abcdef"), spin: 10 };
            assert!(p.matches("Example") && !p.matches("Other"), "{algorithm}");
        }
        // Word 16.112 protecting a document with "Example" (`w:cryptAlgorithmSid="14"`).
        let word = Password {
            algorithm: "14".into(),
            hash: "aqKd5zF+ZLDhXamFt+PhIOVMMDHd6mvzt30mAzZQNGV9pwjlCwKexsb0sjaSOJD/nw05xlAb+FGU3TNUgKq0oA==".into(),
            salt: "mRL8HCYKqg4tYiEVGBiVZA==".into(),
            spin: 100_000,
        };
        assert!(word.matches("Example") && !word.matches("example"));
        let salt = crate::base64::from_base64(&word.salt).expect("base64");
        let h = hash("SHA-512", &salt, &legacy_key("Example", true), word.spin).expect("computed");
        assert_eq!(crate::base64::base64(&h), word.hash, "Password::new's form");
    }
}
