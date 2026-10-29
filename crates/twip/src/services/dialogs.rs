//! The dialogs as typed models. `open` reads the selection's values from the state (mixed where they differ),
//! `update` recomputes the fields that depend on the ones the user changed, and `commands` turns the values into the
//! commands that set what changed, which the session runs as one transaction.

use std::collections::BTreeMap;

use crate::api::command::{
    AltText, Command, HeaderFooterDistance, Hyperlink, HyperlinkEdit, Indents, Margins, ObjectSize, PaperSize, ParagraphFlow, ParagraphSpacing,
    TableSize,
};
use crate::api::error::Error;
use crate::api::services::{
    AccessibilityDialog, AccessibilityIssue, AltTextDialog, ApplyTo, CellMarginValues, ChoiceOption, DialogKind, DialogModel, DialogValues,
    FieldInfo, FieldKind, FontDialog, FontStyle, HyperlinkDialog, InsertTableDialog, PageSetupDialog, ParagraphDialog, PictureSizeDialog,
    SpecialIndent, TablePropertiesDialog, TabsAction, TabsDialog,
};
use crate::api::state::{State, Tri};
use crate::api::value::{AutoOr, Orientation, Points};

/// What a dialog shows besides the state: the selected text, the document's accessibility issues and the font
/// families.
#[derive(Default)]
pub struct Context {
    pub selected_text: String,
    pub issues: Vec<AccessibilityIssue>,
    pub fonts: Vec<String>,
}

/// The error for a dialog whose object the selection doesn't have.
fn unavailable(what: &str) -> Error {
    Error::unavailable(format!("the selection has no {what}"))
}

/// Every dialog, in the state's order.
const KINDS: [DialogKind; 10] = [
    DialogKind::Font,
    DialogKind::Paragraph,
    DialogKind::PageSetup,
    DialogKind::InsertTable,
    DialogKind::TableProperties,
    DialogKind::PictureSize,
    DialogKind::AltText,
    DialogKind::Hyperlink,
    DialogKind::Tabs,
    DialogKind::Accessibility,
];

/// Whether [`open`] opens `kind` on this state: the table and object dialogs need their table or object.
pub fn available(kind: DialogKind, state: &State) -> bool {
    match kind {
        DialogKind::TableProperties => state.selection.table.is_some(),
        DialogKind::PictureSize | DialogKind::AltText => state.selection.object.is_some(),
        _ => true,
    }
}

/// Each dialog's availability, for the state.
pub fn availability(state: &State) -> BTreeMap<DialogKind, bool> {
    KINDS.iter().map(|&k| (k, available(k, state))).collect()
}

/// The selection's values for `kind`.
pub fn values(kind: DialogKind, state: &State, cx: &Context) -> Result<DialogValues, Error> {
    let f = &state.formatting;
    let p = &state.paragraph;
    Ok(match kind {
        DialogKind::Font => DialogValues::Font(FontDialog {
            font: f.font.clone(),
            style: match (&f.bold, &f.italic) {
                (Tri::Uniform(b), Tri::Uniform(i)) => Tri::Uniform(match (b, i) {
                    (true, true) => FontStyle::BoldItalic,
                    (true, false) => FontStyle::Bold,
                    (false, true) => FontStyle::Italic,
                    (false, false) => FontStyle::Regular,
                }),
                (Tri::Unavailable, _) | (_, Tri::Unavailable) => Tri::Unavailable,
                _ => Tri::Mixed,
            },
            size: f.size,
            color: f.color,
            underline: f.underline,
            // shortcut: the state has no underline colour; it shows automatic and no command sets it.
            underline_color: Tri::Uniform(AutoOr::Auto),
            strikethrough: f.strikethrough,
            double_strikethrough: f.double_strikethrough,
            superscript: f.superscript,
            subscript: f.subscript,
            small_caps: f.small_caps,
            all_caps: f.all_caps,
            hidden: f.hidden,
            character_spacing: f.character_spacing,
            position: f.position,
        }),
        DialogKind::Paragraph => {
            let i = &p.indents;
            let (special, by) = match (&i.first_line, &i.hanging) {
                (Tri::Uniform(Points(h)), _) if *h > 0.0 => (Tri::Uniform(SpecialIndent::FirstLine), i.first_line),
                (_, Tri::Uniform(Points(h))) if *h > 0.0 => (Tri::Uniform(SpecialIndent::Hanging), i.hanging),
                (Tri::Uniform(_), Tri::Uniform(_)) => (Tri::Uniform(SpecialIndent::None), Tri::Uniform(Points(0.0))),
                (Tri::Unavailable, _) => (Tri::Unavailable, Tri::Unavailable),
                _ => (Tri::Mixed, Tri::Mixed),
            };
            DialogValues::Paragraph(ParagraphDialog {
                alignment: p.alignment,
                // shortcut: no command sets a paragraph's own outline level; the field shows body text, disabled.
                outline_level: Tri::Unavailable,
                left: i.left,
                right: i.right,
                special,
                by,
                before: p.space_before,
                after: p.space_after,
                line_spacing: p.line_spacing,
                keep_with_next: p.flow.keep_with_next,
                keep_lines_together: p.flow.keep_lines_together,
                page_break_before: p.flow.page_break_before,
                widow_control: p.flow.widow_control,
            })
        }
        DialogKind::PageSetup => {
            let g = &state.page;
            DialogValues::PageSetup(PageSetupDialog {
                top: g.margins.top,
                bottom: g.margins.bottom,
                left: g.margins.left,
                right: g.margins.right,
                gutter: g.margins.gutter,
                orientation: g.orientation,
                width: g.size.width,
                height: g.size.height,
                header_distance: g.header_distance,
                footer_distance: g.footer_distance,
                different_first_page: g.different_first_page,
                different_odd_even: g.different_odd_even,
                apply_to: ApplyTo::Section,
            })
        }
        DialogKind::InsertTable => DialogValues::InsertTable(InsertTableDialog { rows: 2, columns: 5 }),
        DialogKind::TableProperties => {
            let t = state.selection.table.as_ref().ok_or_else(|| unavailable("table"))?;
            DialogValues::TableProperties(TablePropertiesDialog {
                width: t.width,
                alignment: t.alignment,
                indent: t.indent,
                row_height: t.row_height,
                row_can_break: t.row_can_break,
                header_row: t.header_row,
                column_width: t.column_width,
                cell_alignment: t.cell_alignment,
                // shortcut: the state has no cell margins; the dialog starts from the default (5.4 pt left and right),
                // and sets them only when changed.
                cell_margins: CellMarginValues { left: Points(5.4), right: Points(5.4), ..CellMarginValues::default() },
            })
        }
        DialogKind::PictureSize => {
            let o = state.selection.object.as_ref().ok_or_else(|| unavailable("picture"))?;
            DialogValues::PictureSize(PictureSizeDialog {
                width: o.width,
                height: o.height,
                // shortcut: the original size isn't in the state; scales start at 100% of the size now.
                scale_width: 100.0,
                scale_height: 100.0,
                lock_aspect: true,
                reset: false,
            })
        }
        DialogKind::AltText => {
            let o = state.selection.object.as_ref().ok_or_else(|| unavailable("picture or text box"))?;
            DialogValues::AltText(AltTextDialog { text: o.alt_text.clone(), decorative: o.decorative })
        }
        DialogKind::Hyperlink => DialogValues::Hyperlink(match &state.selection.link {
            Some(l) => HyperlinkDialog { text: l.text.clone(), address: l.address.clone() },
            // The selected text (in one paragraph) is the link's text.
            None => HyperlinkDialog {
                text: if cx.selected_text.contains('\n') { String::new() } else { cx.selected_text.clone() },
                address: String::new(),
            },
        }),
        DialogKind::Tabs => {
            let r = &state.ruler.horizontal;
            DialogValues::Tabs(TabsDialog {
                stops: r.tabs.clone(),
                default_stop: r.default_tab_stop,
                position: None,
                alignment: Default::default(),
                leader: Default::default(),
                cleared: Vec::new(),
                action: TabsAction::None,
            })
        }
        DialogKind::Accessibility => DialogValues::Accessibility(AccessibilityDialog { issues: cx.issues.clone() }),
    })
}

fn kind_of(v: &DialogValues) -> DialogKind {
    match v {
        DialogValues::Font(_) => DialogKind::Font,
        DialogValues::Paragraph(_) => DialogKind::Paragraph,
        DialogValues::PageSetup(_) => DialogKind::PageSetup,
        DialogValues::InsertTable(_) => DialogKind::InsertTable,
        DialogValues::TableProperties(_) => DialogKind::TableProperties,
        DialogValues::PictureSize(_) => DialogKind::PictureSize,
        DialogValues::AltText(_) => DialogKind::AltText,
        DialogValues::Hyperlink(_) => DialogKind::Hyperlink,
        DialogValues::Tabs(_) => DialogKind::Tabs,
        DialogValues::Accessibility(_) => DialogKind::Accessibility,
    }
}

/// A field: its label, kind, and whether it can be edited.
fn field(label: &str, kind: FieldKind) -> FieldInfo {
    FieldInfo { label: label.into(), enabled: true, kind, ..FieldInfo::default() }
}

fn number(label: &str, kind: FieldKind, min: f64, max: f64, step: f64) -> FieldInfo {
    FieldInfo { min: Some(min), max: Some(max), step: Some(step), ..field(label, kind) }
}

fn choice(label: &str, options: &[(&str, &str)]) -> FieldInfo {
    let options = options.iter().map(|(value, label)| ChoiceOption { value: value.to_string(), label: label.to_string() }).collect();
    FieldInfo { options: Some(options), ..field(label, FieldKind::Choice) }
}

fn off(mut f: FieldInfo) -> FieldInfo {
    f.enabled = false;
    f
}

const UNDERLINES: [(&str, &str); 10] = [
    ("none", "(none)"),
    ("single", "Single"),
    ("words", "Words only"),
    ("double", "Double"),
    ("thick", "Thick"),
    ("dotted", "Dotted"),
    ("dash", "Dash"),
    ("dotDash", "Dot dash"),
    ("dotDotDash", "Dot dot dash"),
    ("wave", "Wave"),
];

/// The fields of a dialog with these values: labels, ranges and choices, and which can be edited now.
fn fields(values: &DialogValues, cx: &Context) -> BTreeMap<String, FieldInfo> {
    let list: Vec<(&str, FieldInfo)> = match values {
        DialogValues::Font(_) => vec![
            (
                "font",
                FieldInfo {
                    options: Some(cx.fonts.iter().map(|f| ChoiceOption { value: f.clone(), label: f.clone() }).collect()),
                    ..field("Font", FieldKind::Text)
                },
            ),
            ("style", choice("Font style", &[("regular", "Regular"), ("italic", "Italic"), ("bold", "Bold"), ("boldItalic", "Bold Italic")])),
            ("size", number("Size", FieldKind::Number, 1.0, 1638.0, 0.5)),
            ("color", field("Font color", FieldKind::Color)),
            ("underline", choice("Underline style", &UNDERLINES)),
            ("underlineColor", off(field("Underline color", FieldKind::Color))),
            ("strikethrough", field("Strikethrough", FieldKind::Toggle)),
            ("doubleStrikethrough", field("Double strikethrough", FieldKind::Toggle)),
            ("superscript", field("Superscript", FieldKind::Toggle)),
            ("subscript", field("Subscript", FieldKind::Toggle)),
            ("smallCaps", field("Small caps", FieldKind::Toggle)),
            ("allCaps", field("All caps", FieldKind::Toggle)),
            ("hidden", field("Hidden", FieldKind::Toggle)),
            ("characterSpacing", number("Spacing", FieldKind::Length, -1584.0, 1584.0, 0.1)),
            ("position", number("Position", FieldKind::Length, -1584.0, 1584.0, 0.5)),
        ],
        DialogValues::Paragraph(v) => {
            let special = !matches!(v.special, Tri::Uniform(SpecialIndent::None));
            let multiple = matches!(v.line_spacing, Tri::Uniform(crate::api::value::LineSpacing::Multiple { .. }));
            let mut by = number("By", FieldKind::Length, 0.0, 1584.0, 1.0);
            by.enabled = special;
            vec![
                ("alignment", choice("Alignment", &[("left", "Left"), ("center", "Centered"), ("right", "Right"), ("justify", "Justified")])),
                ("outlineLevel", off(choice("Outline level", &[("body", "Body Text")]))),
                ("left", number("Left", FieldKind::Length, -1584.0, 1584.0, 1.0)),
                ("right", number("Right", FieldKind::Length, -1584.0, 1584.0, 1.0)),
                ("special", choice("Special", &[("none", "(none)"), ("firstLine", "First line"), ("hanging", "Hanging")])),
                ("by", by),
                ("before", number("Before", FieldKind::Length, 0.0, 1584.0, 6.0)),
                ("after", number("After", FieldKind::Length, 0.0, 1584.0, 6.0)),
                ("lineSpacing", FieldInfo { step: Some(if multiple { 0.25 } else { 1.0 }), ..field("Line spacing", FieldKind::Number) }),
                ("keepWithNext", field("Keep with next", FieldKind::Toggle)),
                ("keepLinesTogether", field("Keep lines together", FieldKind::Toggle)),
                ("pageBreakBefore", field("Page break before", FieldKind::Toggle)),
                ("widowControl", field("Widow/Orphan control", FieldKind::Toggle)),
            ]
        }
        DialogValues::PageSetup(_) => vec![
            ("top", number("Top", FieldKind::Length, 0.0, 1584.0, 1.0)),
            ("bottom", number("Bottom", FieldKind::Length, 0.0, 1584.0, 1.0)),
            ("left", number("Left", FieldKind::Length, 0.0, 1584.0, 1.0)),
            ("right", number("Right", FieldKind::Length, 0.0, 1584.0, 1.0)),
            ("gutter", number("Gutter", FieldKind::Length, 0.0, 1584.0, 1.0)),
            ("orientation", choice("Orientation", &[("portrait", "Portrait"), ("landscape", "Landscape")])),
            ("width", number("Width", FieldKind::Length, 72.0, 1584.0, 1.0)),
            ("height", number("Height", FieldKind::Length, 72.0, 1584.0, 1.0)),
            ("headerDistance", number("Header", FieldKind::Length, 0.0, 1584.0, 1.0)),
            ("footerDistance", number("Footer", FieldKind::Length, 0.0, 1584.0, 1.0)),
            ("differentFirstPage", field("Different first page", FieldKind::Toggle)),
            ("differentOddEven", field("Different odd and even", FieldKind::Toggle)),
            // shortcut: page setup applies to the selection's sections; the whole document needs a command for every
            // section.
            ("applyTo", off(choice("Apply to", &[("section", "This section"), ("wholeDocument", "Whole document")]))),
        ],
        DialogValues::InsertTable(_) => vec![
            ("rows", number("Number of rows", FieldKind::Number, 1.0, 32767.0, 1.0)),
            ("columns", number("Number of columns", FieldKind::Number, 1.0, 63.0, 1.0)),
        ],
        DialogValues::TableProperties(v) => {
            let mut indent = number("Indent from left", FieldKind::Length, -1584.0, 1584.0, 1.0);
            indent.enabled = v.alignment == crate::api::value::HorizontalAlignment::Left;
            vec![
                ("width", field("Preferred width", FieldKind::Length)),
                ("alignment", choice("Alignment", &[("left", "Left"), ("center", "Center"), ("right", "Right")])),
                ("indent", indent),
                ("rowHeight", field("Row height", FieldKind::Length)),
                ("rowCanBreak", field("Allow row to break across pages", FieldKind::Toggle)),
                ("headerRow", field("Repeat as header row at the top of each page", FieldKind::Toggle)),
                ("columnWidth", number("Column width", FieldKind::Length, 0.0, 1584.0, 1.0)),
                ("cellAlignment", choice("Vertical alignment", &[("top", "Top"), ("center", "Center"), ("bottom", "Bottom")])),
                ("cellMargins", field("Cell margins", FieldKind::Length)),
            ]
        }
        DialogValues::PictureSize(_) => vec![
            ("width", number("Width", FieldKind::Length, 1.0, 1584.0, 1.0)),
            ("height", number("Height", FieldKind::Length, 1.0, 1584.0, 1.0)),
            ("scaleWidth", number("Scale width (%)", FieldKind::Number, 1.0, 1000.0, 1.0)),
            ("scaleHeight", number("Scale height (%)", FieldKind::Number, 1.0, 1000.0, 1.0)),
            ("lockAspect", field("Lock aspect ratio", FieldKind::Toggle)),
            ("reset", field("Reset", FieldKind::Toggle)),
        ],
        DialogValues::AltText(v) => {
            let mut text = field("Description", FieldKind::Text);
            text.enabled = !v.decorative;
            vec![("text", text), ("decorative", field("Mark as decorative", FieldKind::Toggle))]
        }
        DialogValues::Hyperlink(_) => vec![("text", field("Text to display", FieldKind::Text)), ("address", field("Address", FieldKind::Text))],
        DialogValues::Tabs(_) => vec![
            ("stops", field("Tab stop position", FieldKind::List)),
            ("defaultStop", number("Default tab stops", FieldKind::Length, 0.0, 1584.0, 1.0)),
            ("position", number("Tab stop position", FieldKind::Length, -1584.0, 1584.0, 1.0)),
            ("alignment", choice("Alignment", &[("left", "Left"), ("center", "Center"), ("right", "Right"), ("decimal", "Decimal"), ("bar", "Bar")])),
            (
                "leader",
                choice(
                    "Leader",
                    &[
                        ("none", "None"),
                        ("dots", "Dots"),
                        ("dashes", "Dashes"),
                        ("underline", "Underline"),
                        ("heavy", "Heavy"),
                        ("middleDot", "Middle dot"),
                    ],
                ),
            ),
            ("cleared", field("Tab stops to be cleared", FieldKind::List)),
            ("action", choice("Action", &[("none", ""), ("set", "Set"), ("clear", "Clear"), ("clearAll", "Clear All")])),
        ],
        DialogValues::Accessibility(_) => vec![("issues", off(field("Issues", FieldKind::List)))],
    };
    list.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// A dialog opened on the selection.
pub fn open(kind: DialogKind, state: &State, cx: &Context) -> Result<DialogModel, Error> {
    let values = values(kind, state, cx)?;
    Ok(DialogModel { kind, fields: fields(&values, cx), values })
}

/// Turns one of a pair of exclusive toggles off when the other was just turned on.
fn exclusive(was: (&Tri<bool>, &Tri<bool>), now: (&mut Tri<bool>, &mut Tri<bool>)) {
    if now.0 != was.0 && *now.0 == Tri::Uniform(true) {
        *now.1 = Tri::Uniform(false);
    } else if now.1 != was.1 && *now.1 == Tri::Uniform(true) {
        *now.0 = Tri::Uniform(false);
    }
}

/// The dialog with `values` (the open values with the user's changes): the values that depend on the changed ones
/// recomputed, and the fields' states with them.
pub fn update(model: &DialogModel, values: DialogValues, cx: &Context) -> Result<DialogModel, Error> {
    if kind_of(&values) != model.kind {
        return Err(Error::invalid("the values are for another dialog"));
    }
    let values = match (&model.values, values) {
        (DialogValues::Font(was), DialogValues::Font(mut v)) => {
            exclusive((&was.superscript, &was.subscript), (&mut v.superscript, &mut v.subscript));
            exclusive((&was.strikethrough, &was.double_strikethrough), (&mut v.strikethrough, &mut v.double_strikethrough));
            exclusive((&was.small_caps, &was.all_caps), (&mut v.small_caps, &mut v.all_caps));
            DialogValues::Font(v)
        }
        (DialogValues::Paragraph(was), DialogValues::Paragraph(mut v)) => {
            // A special indent turned on starts at half an inch; turned off, it has none.
            if v.special != was.special {
                v.by = match v.special {
                    Tri::Uniform(SpecialIndent::None) => Tri::Uniform(Points(0.0)),
                    Tri::Uniform(_) if !matches!(v.by, Tri::Uniform(Points(b)) if b > 0.0) => Tri::Uniform(Points(36.0)),
                    _ => v.by,
                };
            }
            DialogValues::Paragraph(v)
        }
        (DialogValues::PageSetup(was), DialogValues::PageSetup(mut v)) => {
            // The orientation swaps the paper's sides; typed sizes decide the orientation.
            let landscape = |w: Points, h: Points| if w.0 > h.0 { Orientation::Landscape } else { Orientation::Portrait };
            if v.orientation != was.orientation && landscape(v.width, v.height) != v.orientation {
                (v.width, v.height) = (v.height, v.width);
            } else if v.width != was.width || v.height != was.height {
                v.orientation = landscape(v.width, v.height);
            }
            DialogValues::PageSetup(v)
        }
        (DialogValues::PictureSize(was), DialogValues::PictureSize(mut v)) => {
            let ratio = |a: f64, b: f64| if b > 0.0 { a / b } else { 1.0 };
            // Scales change the sides, and the sides the scales; locked, one side changes the other.
            if v.scale_width != was.scale_width {
                v.width = Points(was.width.0 * ratio(v.scale_width, was.scale_width));
            }
            if v.scale_height != was.scale_height {
                v.height = Points(was.height.0 * ratio(v.scale_height, was.scale_height));
            }
            if v.lock_aspect && v.width != was.width {
                v.height = Points(was.height.0 * ratio(v.width.0, was.width.0));
            } else if v.lock_aspect && v.height != was.height {
                v.width = Points(was.width.0 * ratio(v.height.0, was.height.0));
            }
            v.scale_width = was.scale_width * ratio(v.width.0, was.width.0);
            v.scale_height = was.scale_height * ratio(v.height.0, was.height.0);
            let r = |x: f64| (x * 10.0).round() / 10.0;
            (v.width, v.height, v.scale_width, v.scale_height) = (Points(r(v.width.0)), Points(r(v.height.0)), r(v.scale_width), r(v.scale_height));
            DialogValues::PictureSize(v)
        }
        (DialogValues::Tabs(_), DialogValues::Tabs(mut v)) => {
            // The Set, Clear and Clear All buttons.
            match (v.action, v.position) {
                (TabsAction::Set, Some(at)) => {
                    v.stops.retain(|s| s.position != at);
                    v.stops.push(crate::api::services::TabStopInfo { position: at, alignment: v.alignment, leader: v.leader, inherited: false });
                    v.stops.sort_by(|a, b| a.position.0.total_cmp(&b.position.0));
                    v.cleared.retain(|c| *c != at);
                }
                (TabsAction::Clear, Some(at)) => {
                    v.stops.retain(|s| s.position != at);
                    v.cleared.push(at);
                }
                (TabsAction::ClearAll, _) => {
                    v.cleared.extend(v.stops.iter().map(|s| s.position));
                    v.stops.clear();
                }
                _ => {}
            }
            v.action = TabsAction::None;
            DialogValues::Tabs(v)
        }
        (_, v) => v,
    };
    Ok(DialogModel { kind: model.kind, fields: fields(&values, cx), values })
}

/// A value the user set: uniform, and different from the selection's now.
fn set<T: Clone + PartialEq>(v: &Tri<T>, now: &Tri<T>) -> Option<T> {
    match v {
        Tri::Uniform(x) if v != now => Some(x.clone()),
        _ => None,
    }
}

fn check(ok: bool, what: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(Error::invalid(what)) }
}

/// The commands that set what the dialog changed from the selection's values now, in order; none when nothing
/// changed.
pub fn commands(values: &DialogValues, state: &State, cx: &Context) -> Result<Vec<Command>, Error> {
    let now = self::values(kind_of(values), state, cx)?;
    let mut out = Vec::new();
    match (values, &now) {
        (DialogValues::Font(v), DialogValues::Font(n)) => {
            if let Some(f) = set(&v.font, &n.font) {
                check(!f.trim().is_empty(), "font needs a name")?;
                out.push(Command::SetFont(f.trim().to_string()));
            }
            if let Some(s) = set(&v.size, &n.size) {
                check((1.0..=1638.0).contains(&s.0), "size is 1 to 1638 points")?;
                out.push(Command::SetFontSize(s));
            }
            if let Some(style) = set(&v.style, &n.style) {
                let (b, i) = (matches!(style, FontStyle::Bold | FontStyle::BoldItalic), matches!(style, FontStyle::Italic | FontStyle::BoldItalic));
                out.extend([Command::SetBold(b), Command::SetItalic(i)]);
            }
            out.extend(set(&v.color, &n.color).map(Command::SetFontColor));
            out.extend(set(&v.underline, &n.underline).map(Command::SetUnderline));
            out.extend(set(&v.strikethrough, &n.strikethrough).map(Command::SetStrikethrough));
            out.extend(set(&v.double_strikethrough, &n.double_strikethrough).map(Command::SetDoubleStrikethrough));
            out.extend(set(&v.superscript, &n.superscript).map(Command::SetSuperscript));
            out.extend(set(&v.subscript, &n.subscript).map(Command::SetSubscript));
            out.extend(set(&v.small_caps, &n.small_caps).map(Command::SetSmallCaps));
            out.extend(set(&v.all_caps, &n.all_caps).map(Command::SetAllCaps));
            out.extend(set(&v.hidden, &n.hidden).map(Command::SetHidden));
            out.extend(set(&v.character_spacing, &n.character_spacing).map(Command::SetCharacterSpacing));
            out.extend(set(&v.position, &n.position).map(Command::SetCharacterPosition));
        }
        (DialogValues::Paragraph(v), DialogValues::Paragraph(n)) => {
            out.extend(set(&v.alignment, &n.alignment).map(Command::SetAlignment));
            let (left, right) = (set(&v.left, &n.left), set(&v.right, &n.right));
            let special = v.special != n.special || v.by != n.by;
            let (first_line, hanging) = match (&v.special, &v.by) {
                (Tri::Uniform(s), Tri::Uniform(by)) if special => match s {
                    SpecialIndent::None => (Some(Points(0.0)), Some(Points(0.0))),
                    SpecialIndent::FirstLine => (Some(*by), Some(Points(0.0))),
                    SpecialIndent::Hanging => (Some(Points(0.0)), Some(*by)),
                },
                _ => (None, None),
            };
            check(!matches!(v.by, Tri::Uniform(Points(b)) if b < 0.0), "by is 0 or more")?;
            if left.is_some() || right.is_some() || first_line.is_some() {
                out.push(Command::SetIndents(Indents { left, right, first_line, hanging }));
            }
            let (before, after) = (set(&v.before, &n.before), set(&v.after, &n.after));
            if before.is_some() || after.is_some() {
                out.push(Command::SetParagraphSpacing(ParagraphSpacing { before, after }));
            }
            out.extend(set(&v.line_spacing, &n.line_spacing).map(Command::SetLineSpacing));
            let flow = ParagraphFlow {
                keep_with_next: set(&v.keep_with_next, &n.keep_with_next),
                keep_lines_together: set(&v.keep_lines_together, &n.keep_lines_together),
                page_break_before: set(&v.page_break_before, &n.page_break_before),
                widow_control: set(&v.widow_control, &n.widow_control),
            };
            if flow != ParagraphFlow::default() {
                out.push(Command::SetParagraphFlow(flow));
            }
        }
        (DialogValues::PageSetup(v), DialogValues::PageSetup(n)) => {
            let changed = |a: Points, b: Points| (a != b).then_some(a);
            for (x, name) in [(v.top, "top"), (v.bottom, "bottom"), (v.left, "left"), (v.right, "right"), (v.gutter, "gutter")] {
                check((0.0..=1584.0).contains(&x.0), &format!("{name} is 0 to 1584 points"))?;
            }
            check(v.left.0 + v.right.0 + v.gutter.0 < v.width.0, "the margins leave no room on the page")?;
            check(v.width.0 >= 72.0 && v.height.0 >= 72.0, "the paper is at least 72 points each way")?;
            if v.orientation != n.orientation {
                out.push(Command::SetOrientation(v.orientation));
            }
            // The orientation swapped the sides already.
            let (w, h) = if v.orientation != n.orientation { (n.height, n.width) } else { (n.width, n.height) };
            if (v.width, v.height) != (w, h) {
                out.push(Command::SetPaperSize(PaperSize { width: v.width, height: v.height }));
            }
            let m = Margins {
                top: changed(v.top, n.top),
                bottom: changed(v.bottom, n.bottom),
                left: changed(v.left, n.left),
                right: changed(v.right, n.right),
                gutter: changed(v.gutter, n.gutter),
            };
            if m != Margins::default() {
                out.push(Command::SetMargins(m));
            }
            let d =
                HeaderFooterDistance { header: changed(v.header_distance, n.header_distance), footer: changed(v.footer_distance, n.footer_distance) };
            if d != HeaderFooterDistance::default() {
                out.push(Command::SetHeaderFooterDistance(d));
            }
            if v.different_first_page != n.different_first_page {
                out.push(Command::SetDifferentFirstPage(v.different_first_page));
            }
            if v.different_odd_even != n.different_odd_even {
                out.push(Command::SetDifferentOddEvenPages(v.different_odd_even));
            }
        }
        (DialogValues::InsertTable(v), _) => {
            check((1..=32767).contains(&v.rows), "rows is 1 to 32767")?;
            check((1..=63).contains(&v.columns), "columns is 1 to 63")?;
            out.push(Command::InsertTable(TableSize { rows: v.rows, columns: v.columns }));
        }
        (DialogValues::TableProperties(v), DialogValues::TableProperties(n)) => {
            if v.width != n.width {
                out.push(Command::SetTableWidth(v.width));
            }
            if v.alignment != n.alignment {
                out.push(Command::SetTableAlignment(v.alignment));
            }
            if v.indent != n.indent && v.alignment == crate::api::value::HorizontalAlignment::Left {
                out.push(Command::SetTableIndent(v.indent));
            }
            out.extend(set(&v.row_height, &n.row_height).map(Command::SetRowHeight));
            out.extend(set(&v.row_can_break, &n.row_can_break).map(Command::SetRowCanBreak));
            out.extend(set(&v.header_row, &n.header_row).map(Command::SetHeaderRow));
            out.extend(set(&v.column_width, &n.column_width).map(Command::SetColumnWidth));
            out.extend(set(&v.cell_alignment, &n.cell_alignment).map(Command::SetCellAlignment));
            let (m, was) = (v.cell_margins, n.cell_margins);
            if m != was {
                let changed = |a: Points, b: Points| (a != b).then_some(a);
                out.push(Command::SetCellMargins(crate::api::command::CellMargins {
                    top: changed(m.top, was.top),
                    bottom: changed(m.bottom, was.bottom),
                    left: changed(m.left, was.left),
                    right: changed(m.right, was.right),
                }));
            }
        }
        (DialogValues::PictureSize(v), DialogValues::PictureSize(n)) => {
            if v.reset {
                out.push(Command::ResetObjectSize);
            } else if (v.width, v.height) != (n.width, n.height) {
                check(v.width.0 >= 1.0 && v.height.0 >= 1.0, "the size is at least 1 point each way")?;
                out.push(Command::SetObjectSize(ObjectSize { width: Some(v.width), height: Some(v.height), lock_aspect: None }));
            }
        }
        (DialogValues::AltText(v), DialogValues::AltText(n)) => {
            if v != n {
                out.push(Command::SetAltText(AltText { text: v.text.trim().to_string(), decorative: Some(v.decorative) }));
            }
        }
        (DialogValues::Hyperlink(v), DialogValues::Hyperlink(n)) => {
            let address = v.address.trim().to_string();
            check(!address.is_empty(), "the link needs an address")?;
            let text = Some(v.text.trim().to_string()).filter(|t| !t.is_empty());
            if state.selection.link.is_some() {
                if v != n {
                    out.push(Command::EditHyperlink(HyperlinkEdit { address: Some(address), text }));
                }
            } else {
                out.push(Command::InsertHyperlink(Hyperlink { address, text }));
            }
        }
        (DialogValues::Tabs(v), DialogValues::Tabs(n)) => {
            out.extend(v.cleared.iter().filter(|c| n.stops.iter().any(|s| s.position == **c)).map(|c| Command::DeleteTabStop(*c)));
            for s in v.stops.iter().filter(|s| !n.stops.contains(s)) {
                out.push(Command::AddTabStop(crate::api::command::AddTabStop {
                    position: s.position,
                    alignment: Some(s.alignment),
                    leader: Some(s.leader),
                }));
            }
            if v.default_stop != n.default_stop {
                check(v.default_stop.0 > 0.0, "the default tab stop is more than 0")?;
                out.push(Command::SetDefaultTabStop(v.default_stop));
            }
        }
        // The accessibility check lists issues and applies nothing.
        _ => {}
    }
    Ok(out)
}

/// An undo label for a dialog's transaction.
pub fn label(kind: DialogKind) -> &'static str {
    match kind {
        DialogKind::Font => "Font",
        DialogKind::Paragraph => "Paragraph",
        DialogKind::PageSetup => "Page Setup",
        DialogKind::InsertTable => "Insert Table",
        DialogKind::TableProperties => "Table Properties",
        DialogKind::PictureSize => "Picture Size",
        DialogKind::AltText => "Alt Text",
        DialogKind::Hyperlink => "Hyperlink",
        DialogKind::Tabs => "Tabs",
        DialogKind::Accessibility => "Check Accessibility",
    }
}

/// Check Accessibility: pictures without alt text (placeholders aside), tables without a header row. Ids name what
/// the issue is on (`picture:<paragraph>:<unit>`, `table:<first paragraph>`).
pub fn issues(doc: &crate::docx::Document) -> Vec<AccessibilityIssue> {
    use crate::docx::{Block, Inline};
    fn walk(doc: &crate::docx::Document, blocks: &[Block], out: &mut Vec<AccessibilityIssue>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    let mut at = 0u32;
                    for i in &p.inlines {
                        if let Inline::Picture(pic, _) = i
                            && pic.alt.is_empty()
                            && !pic.image.is_some_and(|k| doc.placeholders.contains_key(&k) || doc.placeholder_keys.contains_key(&k))
                        {
                            out.push(AccessibilityIssue {
                                id: format!("picture:{}:{at}", p.id),
                                text: "Picture without alt text: add a description.".into(),
                                tip: false,
                            });
                        }
                        at += i.units() as u32;
                    }
                }
                Block::Table(t) => {
                    let first = t
                        .rows
                        .first()
                        .and_then(|r| r.cells.first())
                        .and_then(|c| c.blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(p.id) } else { None }));
                    if let Some(id) = first.filter(|_| !t.rows.first().is_some_and(|r| r.header)) {
                        out.push(AccessibilityIssue {
                            id: format!("table:{id}"),
                            text: "Table without a header row: mark its first row as a header row (Table Layout → Header row).".into(),
                            tip: true,
                        });
                    }
                    for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                        walk(doc, &c.blocks, out);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for s in &doc.sections {
        walk(doc, &s.blocks, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::state::ObjectSelection;

    fn font_state() -> State {
        let mut s = State::default();
        let f = &mut s.formatting;
        (f.bold, f.italic, f.size) = (Tri::Mixed, Tri::Uniform(false), Tri::Uniform(Points(11.0)));
        (f.superscript, f.subscript) = (Tri::Uniform(false), Tri::Uniform(false));
        s
    }

    #[test]
    fn availability_agrees_with_open() {
        let mut table = State::default();
        table.selection.table = Some(crate::api::state::TableSelection::default());
        let mut object = State::default();
        object.selection.object = Some(ObjectSelection::default());
        for state in [State::default(), table, object] {
            for (kind, on) in availability(&state) {
                assert_eq!(on, open(kind, &state, &Context::default()).is_ok(), "{kind:?}");
            }
        }
    }

    fn font(m: &DialogModel) -> FontDialog {
        match &m.values {
            DialogValues::Font(f) => f.clone(),
            v => panic!("{v:?}"),
        }
    }

    #[test]
    fn font_keeps_mixed_values_and_applies_only_changes() {
        let (state, cx) = (font_state(), Context::default());
        let m = open(DialogKind::Font, &state, &cx).expect("opens");
        assert_eq!(font(&m).style, Tri::Mixed);
        assert_eq!(m.fields["style"].kind, FieldKind::Choice);
        let mut v = font(&m);
        v.size = Tri::Uniform(Points(14.0));
        v.subscript = Tri::Uniform(true);
        let m = update(&m, DialogValues::Font(v), &cx).expect("updates");
        let mut v = font(&m);
        v.superscript = Tri::Uniform(true);
        let m = update(&m, DialogValues::Font(v), &cx).expect("updates");
        assert_eq!((font(&m).superscript, font(&m).subscript), (Tri::Uniform(true), Tri::Uniform(false)), "they exclude each other");
        let out = commands(&m.values, &state, &cx).expect("valid");
        assert_eq!(out, [Command::SetFontSize(Points(14.0)), Command::SetSuperscript(true)], "the mixed style isn't set");
        let mut v = font(&m);
        v.size = Tri::Uniform(Points(0.0));
        assert!(commands(&DialogValues::Font(v), &state, &cx).is_err(), "out of range");
    }

    #[test]
    fn page_setup_swaps_sides_with_the_orientation() {
        let mut state = State::default();
        state.page.size = crate::api::state::PageSize { width: Points(612.0), height: Points(792.0) };
        let cx = Context::default();
        let m = open(DialogKind::PageSetup, &state, &cx).expect("opens");
        let DialogValues::PageSetup(mut v) = m.values.clone() else { panic!() };
        v.orientation = Orientation::Landscape;
        let m = update(&m, DialogValues::PageSetup(v), &cx).expect("updates");
        let DialogValues::PageSetup(v) = &m.values else { panic!() };
        assert_eq!((v.width, v.height), (Points(792.0), Points(612.0)));
        assert_eq!(commands(&m.values, &state, &cx).expect("valid"), [Command::SetOrientation(Orientation::Landscape)]);
    }

    #[test]
    fn a_locked_picture_keeps_its_aspect() {
        let mut state = State::default();
        state.selection.object = Some(ObjectSelection { width: Points(72.0), height: Points(54.0), ..ObjectSelection::default() });
        let cx = Context::default();
        let m = open(DialogKind::PictureSize, &state, &cx).expect("opens");
        let DialogValues::PictureSize(mut v) = m.values.clone() else { panic!() };
        v.width = Points(144.0);
        let m = update(&m, DialogValues::PictureSize(v), &cx).expect("updates");
        let DialogValues::PictureSize(v) = &m.values else { panic!() };
        assert_eq!((v.width, v.height, v.scale_width), (Points(144.0), Points(108.0), 200.0));
        assert!(open(DialogKind::TableProperties, &state, &cx).is_err(), "no table");
    }

    #[test]
    fn tabs_buttons_set_and_clear() {
        let (state, cx) = (State::default(), Context::default());
        let m = open(DialogKind::Tabs, &state, &cx).expect("opens");
        let DialogValues::Tabs(mut v) = m.values.clone() else { panic!() };
        (v.position, v.action) = (Some(Points(72.0)), TabsAction::Set);
        let m = update(&m, DialogValues::Tabs(v), &cx).expect("updates");
        let DialogValues::Tabs(v) = &m.values else { panic!() };
        assert_eq!((v.stops.len(), v.action), (1, TabsAction::None));
        assert!(matches!(commands(&m.values, &state, &cx).expect("valid")[..], [Command::AddTabStop(_)]));
    }
}
