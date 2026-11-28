//! Authored paragraph formatting, independent of any file format: what one level (a paragraph, a style, a list level,
//! a table style region, the document defaults) sets, with unset and explicitly off kept apart.

use std::rc::Rc;

use super::{ForeignProps, Shading, TextBorder};
use crate::document::authored::Empty;
use crate::document::format::{Jc, Leader, LineRule, Tab, TabKind};
use crate::document::paragraph::{ListId, TextAlignment};
use crate::docx::props::FramePr;

/// Authored paragraph properties at one level. `None` means "not set here". Its `==` is authored equality.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParaProps {
    /// The paragraph style.
    pub style: Option<Rc<str>>,
    /// The alignment.
    pub align: Option<Jc>,
    /// The start indent, points.
    pub indent_left: Option<f64>,
    /// The end indent, points.
    pub indent_right: Option<f64>,
    /// The first line's offset from the start indent, points: + further in, − hanging.
    pub indent_first: Option<f64>,
    /// The space before, points.
    pub space_before: Option<f64>,
    /// The space before is chosen automatically.
    pub space_before_auto: Option<bool>,
    /// The space after, points.
    pub space_after: Option<f64>,
    /// The space after is chosen automatically.
    pub space_after_auto: Option<bool>,
    /// The line height's value, which its rule (set here or at another level) measures: 240ths of a line under
    /// [`LineRule::Auto`], else twentieths of a point.
    pub line: Option<f64>,
    /// How the line height is measured.
    pub line_rule: Option<LineRule>,
    /// The on/off properties this level sets, and their values.
    pub flags: ParaFlags,
    /// Which of a text box's lines wrap tightly around its text.
    pub tight_wrap: Option<TightWrap>,
    /// The HTML division the paragraph is in.
    pub division: Option<i64>,
    /// The tab stops this level sets and clears, in order.
    pub tabs: Vec<TabStop>,
    /// The list: `Some(ListChoice::None)` takes the paragraph out of a list its style puts it in.
    pub list: Option<ListChoice>,
    /// The list level, which goes with the list set here or at another level.
    pub list_level: Option<i64>,
    /// The borders.
    pub borders: ParaBorders,
    /// The shading.
    pub shading: Option<Shading>,
    /// The frame the paragraph is placed in.
    pub frame: Option<FramePr>,
    /// The frame's anchor is locked to the paragraph.
    pub frame_locked: Option<bool>,
    /// The text flow: 0 left to right and top to bottom, else the flow's code (`docx::model::text_flow`).
    pub text_flow: Option<u8>,
    /// How a line's runs sit on it vertically.
    pub text_alignment: Option<TextAlignment>,
    /// The outline level, 0 to 8; 9 is body text.
    pub outline_level: Option<i64>,
    /// The heading opens collapsed.
    pub collapsed: Option<bool>,
    /// What the source format wrote at this level that this struct does not model. Owned by the adapter that read
    /// it; never read by formatting or layout.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A paragraph's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListChoice {
    /// No list, over a style's.
    None,
    /// This list.
    In(ListId),
}

/// A paragraph's borders, each set or not on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParaBorders {
    /// Above.
    pub top: Option<TextBorder>,
    /// Below.
    pub bottom: Option<TextBorder>,
    /// At the start.
    pub left: Option<TextBorder>,
    /// At the end.
    pub right: Option<TextBorder>,
    /// Between paragraphs with the same borders.
    pub between: Option<TextBorder>,
}

impl ParaBorders {
    /// Whether no border is set.
    pub fn is_empty(&self) -> bool {
        *self == ParaBorders::default()
    }
}

/// A paragraph border's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderSide {
    /// Above.
    Top,
    /// At the start.
    Left,
    /// Below.
    Bottom,
    /// At the end.
    Right,
    /// Between paragraphs.
    Between,
}

impl ParaBorders {
    /// The border on `side`.
    pub fn side_mut(&mut self, side: BorderSide) -> &mut Option<TextBorder> {
        match side {
            BorderSide::Top => &mut self.top,
            BorderSide::Left => &mut self.left,
            BorderSide::Bottom => &mut self.bottom,
            BorderSide::Right => &mut self.right,
            BorderSide::Between => &mut self.between,
        }
    }
}

/// A tab stop a level sets, or clears ([`TabKind::Clear`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabStop {
    /// Its position from the start indent's origin, points.
    pub pos: f64,
    /// Its kind.
    pub kind: TabKind,
    /// What fills the space before it.
    pub leader: Leader,
}

/// Which of a text box's lines wrap tightly around its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TightWrap {
    /// None.
    None,
    /// Every line.
    AllLines,
    /// The first and the last.
    FirstAndLastLine,
    /// The first.
    FirstLineOnly,
    /// The last.
    LastLineOnly,
}

impl TightWrap {
    /// Its code in the effective format: 0 none, 1 all lines, 2 first and last, 3 first only, 4 last only.
    pub fn code(self) -> u8 {
        match self {
            TightWrap::None => 0,
            TightWrap::AllLines => 1,
            TightWrap::FirstAndLastLine => 2,
            TightWrap::FirstLineOnly => 3,
            TightWrap::LastLineOnly => 4,
        }
    }
}

/// The on/off properties a level sets (`set`) and their values (`on`), as bits of [`ParaFlag`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParaFlags {
    /// The flags this level sets.
    pub set: u32,
    /// Their values; a bit outside `set` is off.
    pub on: u32,
}

/// One on/off paragraph property's bit in [`ParaFlags`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParaFlag(pub u32);

impl ParaFlag {
    /// Kept on the page with the next paragraph.
    pub const KEEP_NEXT: ParaFlag = ParaFlag(1);
    /// Its lines kept on one page.
    pub const KEEP_LINES: ParaFlag = ParaFlag(1 << 1);
    /// Starts a page.
    pub const PAGE_BREAK_BEFORE: ParaFlag = ParaFlag(1 << 2);
    /// Widow and orphan lines are avoided.
    pub const WIDOW_CONTROL: ParaFlag = ParaFlag(1 << 3);
    /// Its lines are not numbered.
    pub const SUPPRESS_LINE_NUMBERS: ParaFlag = ParaFlag(1 << 4);
    /// Not hyphenated automatically.
    pub const SUPPRESS_AUTO_HYPHENS: ParaFlag = ParaFlag(1 << 5);
    /// East Asian line-breaking rules apply.
    pub const KINSOKU: ParaFlag = ParaFlag(1 << 6);
    /// Latin words are kept whole in East Asian text.
    pub const WORD_WRAP: ParaFlag = ParaFlag(1 << 7);
    /// Punctuation may hang past the end of a line.
    pub const OVERFLOW_PUNCT: ParaFlag = ParaFlag(1 << 8);
    /// An opening bracket starting a line is compressed.
    pub const TOP_LINE_PUNCT: ParaFlag = ParaFlag(1 << 9);
    /// Space is added between East Asian and Latin text.
    pub const AUTO_SPACE_DE: ParaFlag = ParaFlag(1 << 10);
    /// Space is added between East Asian text and digits.
    pub const AUTO_SPACE_DN: ParaFlag = ParaFlag(1 << 11);
    /// Right to left.
    pub const BIDI: ParaFlag = ParaFlag(1 << 12);
    /// Its lines snap to the document grid.
    pub const SNAP_TO_GRID: ParaFlag = ParaFlag(1 << 13);
    /// No space between it and paragraphs of its style.
    pub const CONTEXTUAL: ParaFlag = ParaFlag(1 << 14);
    /// Its indents swap sides on mirrored pages.
    pub const MIRROR_INDENTS: ParaFlag = ParaFlag(1 << 15);
    /// Its frame may not overlap other frames.
    pub const SUPPRESS_OVERLAP: ParaFlag = ParaFlag(1 << 16);
}

impl ParaFlags {
    /// What this level sets `f` to.
    pub fn get(&self, f: ParaFlag) -> Option<bool> {
        (self.set & f.0 != 0).then_some(self.on & f.0 != 0)
    }

    /// Sets `f` to `on` at this level.
    pub fn put(&mut self, f: ParaFlag, on: bool) {
        self.set |= f.0;
        if on {
            self.on |= f.0;
        } else {
            self.on &= !f.0;
        }
    }

    /// Unsets `f` at this level.
    pub fn clear(&mut self, f: ParaFlag) {
        self.set &= !f.0;
        self.on &= !f.0;
    }
}

/// A paragraph property an edit sets, as a source format writes it: one written element or attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParaProp {
    /// The style.
    Style,
    /// The alignment.
    Align,
    /// The start indent.
    IndentLeft,
    /// The end indent.
    IndentRight,
    /// The first line's offset.
    IndentFirst,
    /// The space before.
    SpaceBefore,
    /// Automatic space before.
    SpaceBeforeAuto,
    /// The space after.
    SpaceAfter,
    /// Automatic space after.
    SpaceAfterAuto,
    /// The line height's value.
    Line,
    /// The line height's rule.
    LineRule,
    /// An on/off property.
    Flag(ParaFlag),
    /// Tight wrap.
    TightWrap,
    /// The HTML division.
    Division,
    /// The tab stops.
    Tabs,
    /// The list and its level.
    List,
    /// The borders.
    Borders,
    /// One border, among the others as written.
    Border(BorderSide),
    /// The shading.
    Shading,
    /// The frame.
    Frame,
    /// The text flow.
    TextFlow,
    /// The vertical alignment of runs.
    TextAlignment,
    /// The outline level.
    OutlineLevel,
    /// Collapsed.
    Collapsed,
}

/// A paragraph property an edit set: whole (written anew, whatever it was), or in part (among what is written with
/// it, which stays as written).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParaSet {
    /// The property.
    pub prop: ParaProp,
    /// Set whole.
    pub whole: bool,
}

/// Authored paragraph properties being edited, with what the edit set, in the order it set it: a source format
/// writes what was set again, the rest as it was written.
#[derive(Clone, Debug, Default)]
pub struct ParaEdit {
    /// The properties.
    pub props: ParaProps,
    /// What was set, in order.
    pub set: Vec<ParaSet>,
}

impl ParaEdit {
    /// An edit of `props`.
    pub fn new(props: ParaProps) -> ParaEdit {
        ParaEdit { props, set: Vec::new() }
    }

    /// Notes `prop` set whole.
    pub fn whole(&mut self, prop: ParaProp) {
        self.set.push(ParaSet { prop, whole: true });
    }

    /// Notes `prop` set in part.
    pub fn part(&mut self, prop: ParaProp) {
        self.set.push(ParaSet { prop, whole: false });
    }

    /// Sets on/off property `f` whole.
    pub fn flag(&mut self, f: ParaFlag, on: bool) {
        self.props.flags.put(f, on);
        self.whole(ParaProp::Flag(f));
    }
}

impl std::ops::Deref for ParaEdit {
    type Target = ParaProps;
    fn deref(&self) -> &ParaProps {
        &self.props
    }
}

impl std::ops::DerefMut for ParaEdit {
    fn deref_mut(&mut self) -> &mut ParaProps {
        &mut self.props
    }
}

thread_local! {
    /// No properties, shared.
    static EMPTY: Rc<ParaProps> = Rc::new(ParaProps::default());
}

impl Empty for ParaProps {
    fn empty() -> Rc<ParaProps> {
        EMPTY.with(Rc::clone)
    }
}

impl ParaProps {
    /// Overrides everything `o` sets; tab stops accumulate (a cleared one removes the stop inherited there).
    /// `foreign` stays this level's.
    pub fn merge(&mut self, o: &ParaProps) {
        macro_rules! take {
            ($($f:ident).+) => { if o.$($f).+.is_some() { self.$($f).+ = o.$($f).+.clone(); } };
        }
        take!(style);
        take!(align);
        take!(indent_left);
        take!(indent_right);
        take!(indent_first);
        take!(space_before);
        take!(space_after);
        take!(space_before_auto);
        take!(space_after_auto);
        take!(line);
        take!(line_rule);
        take!(tight_wrap);
        take!(division);
        take!(list);
        take!(list_level);
        take!(borders.top);
        take!(borders.bottom);
        take!(borders.left);
        take!(borders.right);
        take!(borders.between);
        take!(shading);
        take!(frame);
        take!(frame_locked);
        take!(text_flow);
        take!(text_alignment);
        take!(outline_level);
        take!(collapsed);
        let f = &mut self.flags;
        f.on = (f.on & !o.flags.set) | (o.flags.on & o.flags.set);
        f.set |= o.flags.set;
        self.tabs.extend(o.tabs.iter().copied());
    }

    /// The effective tab stops: later levels replace stops at the same position; a cleared one removes them.
    pub fn resolved_tabs(&self) -> Vec<Tab> {
        let mut out: Vec<Tab> = Vec::new();
        for t in &self.tabs {
            out.retain(|x| (x.pos - t.pos).abs() >= 0.025);
            if t.kind != TabKind::Clear {
                out.push(Tab { pos: t.pos, kind: t.kind, leader: t.leader });
            }
        }
        out.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        out
    }
}
