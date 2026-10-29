//! One example of every variant of every `api` type: the wire fixtures. Unit-variant enums list every variant by
//! their serde names; the rest are written out here, so a changed type fails to compile until its examples follow.
//! Option structs of commands, dialog values and event payloads are covered inside their parents' files.

use std::collections::BTreeMap;

use serde::de::{DeserializeOwned, Error as _, Visitor};
use serde::{Deserializer, Serialize};
use serde_json::Value;

use crate::api::catalog::{CommandGroup, CommandInfo, CommandShortcuts, UndoGrouping};
use crate::api::command::*;
use crate::api::engine::{Dictionary, SessionId, SessionOptions};
use crate::api::error::{Error, ErrorCode};
use crate::api::event::{Event, FieldRequest, HostCommand, KeyOutcome, LinkVia};
use crate::api::fonts::{FaceInfo, FontRequest, SubstitutionRule};
use crate::api::input::{ClipboardData, CompositionPhase, Key, Modifiers, Point, PointerButton, PointerKind};
use crate::api::record::{Author, ChangeSummary, Identity, Origin, Timestamp, TransactionRecord};
use crate::api::review::*;
use crate::api::services::*;
use crate::api::session::{
    CaretBlink, Commit, Cursor, Export, ExportFormat, ExportOptions, FrameInfo, FrameStats, HostRequest, OverlayPalette, PaintBudget,
};
use crate::api::state::*;
use crate::api::value::*;
use crate::api::warning::{Location, Warning, WarningCode};

/// One fixture file: a type's examples, and a round trip through the type.
pub struct Fixture {
    /// The file name without `.json`: the type's name in kebab case.
    pub name: &'static str,
    /// The examples, as the type serializes them.
    pub examples: fn() -> Vec<Value>,
    /// Decodes a value as the type and encodes it again.
    pub round_trip: fn(&Value) -> Result<Value, String>,
}

fn json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("api values serialize to JSON")
}

fn round_trip<T: Serialize + DeserializeOwned>(v: &Value) -> Result<Value, String> {
    let t: T = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    serde_json::to_value(&t).map_err(|e| e.to_string())
}

/// Every variant of a unit-variant enum, read from its serde names.
fn all<T: DeserializeOwned>() -> Vec<T> {
    /// Records the variant names a derived `Deserialize` asks an enum for, and reads nothing.
    struct Probe(Option<&'static [&'static str]>);

    impl<'de> Deserializer<'de> for &mut Probe {
        type Error = serde::de::value::Error;

        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
            Err(Self::Error::custom("not a unit-variant enum"))
        }

        fn deserialize_enum<V: Visitor<'de>>(self, _: &'static str, variants: &'static [&'static str], _: V) -> Result<V::Value, Self::Error> {
            self.0 = Some(variants);
            Err(Self::Error::custom("probed"))
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit unit_struct
            newtype_struct seq tuple tuple_struct map struct identifier ignored_any
        }
    }

    let mut probe = Probe(None);
    let _ = T::deserialize(&mut probe);
    let names = probe.0.expect("units! lists only unit-variant enums");
    names.iter().map(|n| serde_json::from_value(Value::String((*n).to_string())).expect("a unit variant reads from its name")).collect()
}

macro_rules! fixture {
    ($name:literal, $ty:ty, $examples:expr) => {
        Fixture {
            name: $name,
            examples: || {
                let v: Vec<$ty> = $examples;
                v.iter().map(json).collect()
            },
            round_trip: round_trip::<$ty>,
        }
    };
}

macro_rules! units {
    ($($name:literal: $ty:ty),* $(,)?) => {
        [$(fixture!($name, $ty, all::<$ty>()),)*]
    };
}

/// Every fixture, by file name.
pub fn fixtures() -> Vec<Fixture> {
    let mut out = Vec::from(units! {
        "alignment": Alignment,
        "apply-to": ApplyTo,
        "accessible-mark": AccessibleMark,
        "accessible-role": AccessibleRole,
        "area-kind": AreaKind,
        "auto-fit": AutoFit,
        "border-edges": BorderEdges,
        "border-preset": BorderPreset,
        "border-state": BorderState,
        "border-style": BorderStyle,
        "case-change": CaseChange,
        "cell-alignment": CellAlignment,
        "cell-shift": CellShift,
        "command-group": CommandGroup,
        "command-id": CommandId,
        "composition-phase": CompositionPhase,
        "control-kind": ControlKind,
        "control-lock": ControlLock,
        "cursor": Cursor,
        "default-keyword": DefaultKeyword,
        "delete-unit": DeleteUnit,
        "dialog-kind": DialogKind,
        "direction": Direction,
        "element-class": ElementClass,
        "element-field": ElementField,
        "element-kind": ElementKind,
        "error-code": ErrorCode,
        "export-format": ExportFormat,
        "field-kind": FieldKind,
        "font-style": FontStyle,
        "gesture-id": GestureId,
        "header-footer-kind": HeaderFooterKind,
        "header-footer-type": HeaderFooterType,
        "height-rule": HeightRule,
        "highlight-color": HighlightColor,
        "horizontal-alignment": HorizontalAlignment,
        "horizontal-side": HorizontalSide,
        "host-command": HostCommand,
        "key": Key,
        "level-change": LevelChange,
        "link-via": LinkVia,
        "list-kind": ListKind,
        "list-state": ListState,
        "load-state": LoadState,
        "markup-view": MarkupView,
        "menu-via": MenuVia,
        "mode": Mode,
        "move-unit": MoveUnit,
        "object-kind": ObjectKind,
        "object-wrap": ObjectWrap,
        "order": Order,
        "orientation": Orientation,
        "origin": Origin,
        "page-number-format": PageNumberFormat,
        "page-number-place": PageNumberPlace,
        "paste-as": PasteAs,
        "pointer-button": PointerButton,
        "pointer-kind": PointerKind,
        "revision-kind": RevisionKind,
        "ruler-unit": RulerUnit,
        "section-break-kind": SectionBreakKind,
        "select-unit": SelectUnit,
        "selection-kind": SelectionKind,
        "selection-mode": SelectionMode,
        "severity": Severity,
        "special-indent": SpecialIndent,
        "story": Story,
        "style-type": StyleType,
        "tab-alignment": TabAlignment,
        "tab-leader": TabLeader,
        "tab-selector": TabSelector,
        "tabs-action": TabsAction,
        "tooltip-kind": TooltipKind,
        "undo-grouping": UndoGrouping,
        "underline-style": UnderlineStyle,
        "vertical-direction": VerticalDirection,
        "vertical-merge": VerticalMerge,
        "vertical-side": VerticalSide,
        "warning-code": WarningCode,
        "zoom-fit": ZoomFit,
    });
    out.extend([
        // values
        fixture!("points", Points, vec![Points(12.5)]),
        fixture!("color", Color, vec![Color(0x1F4E79)]),
        fixture!("auto-or", AutoOr<Color>, vec![AutoOr::Auto, AutoOr::Value(Color(0xC00000))]),
        fixture!("none-or", NoneOr<Color>, vec![NoneOr::None, NoneOr::Value(Color(0xFFFF00))]),
        fixture!("tri", Tri<bool>, vec![Tri::Uniform(true), Tri::Mixed, Tri::Unavailable]),
        fixture!("outline-level", OutlineLevel, vec![OutlineLevel::Body, OutlineLevel::Level(2)]),
        fixture!("bytes", Bytes, vec![Bytes(vec![0, 1, 2, 250])]),
        fixture!("element-id", ElementId, vec![id("field-1")]),
        fixture!("comment-id", CommentId, vec![CommentId("c4".into())]),
        fixture!("revision-id", RevisionId, vec![RevisionId("r7".into())]),
        fixture!("heading-id", HeadingId, vec![HeadingId("h2".into())]),
        fixture!("match-id", MatchId, vec![MatchId("m1".into())]),
        fixture!("style-id", StyleId, vec![style("Heading1")]),
        fixture!("line-spacing", LineSpacing, line_spacings()),
        fixture!("table-width", TableWidth, vec![TableWidth::Auto, TableWidth::Points { value: Points(432.5) }, TableWidth::Percent { value: 80.5 }]),
        fixture!("row-height", RowHeight, vec![row_height()]),
        fixture!("data-binding", DataBinding, vec![binding()]),
        fixture!("control-options", ControlOptions, control_options()),
        // errors, warnings, records
        fixture!(
            "error",
            Error,
            vec![Error::unsupported("PDF export"), Error { details: Some(serde_json::json!({ "index": 2 })), ..Error::invalid("rows") }]
        ),
        fixture!("warning", Warning, vec![warning(), Warning { location: None, ..warning() }]),
        fixture!("location", Location, vec![Location { part: "word/document.xml".into(), paragraph: Some(3) }]),
        fixture!("transaction-record", TransactionRecord, vec![record()]),
        fixture!("author", Author, vec![author(), Author { name: "Sam".into(), initials: None, identity: None }]),
        fixture!("identity", Identity, vec![identity()]),
        fixture!("timestamp", Timestamp, vec![time()]),
        fixture!("change-summary", ChangeSummary, vec![summary()]),
        // commands and the catalog
        fixture!("command", Command, commands()),
        fixture!("command-result", CommandResult, command_results()),
        fixture!("zoom", Zoom, vec![Zoom::Percent(125.5), Zoom::Fit(ZoomFit::PageWidth)]),
        fixture!("placeholder-match", PlaceholderMatch, placeholder_matches()),
        fixture!("placeholder-look", PlaceholderLook, placeholder_looks()),
        fixture!("body", Body, vec![Body::Text("Check this".into()), Body::Rich(rich())]),
        fixture!("command-info", CommandInfo, vec![command_info()]),
        fixture!(
            "commit",
            Commit,
            vec![Commit { record: Some(record()), value: Some(CommandResult::Element { id: id("field-1") }) }, Commit { record: None, value: None }]
        ),
        // state
        fixture!("state", State, vec![state()]),
        fixture!(
            "command-state",
            CommandState,
            vec![CommandState { enabled: true, checked: Some(true), value: Some(CommandValue::Toggle(Tri::Mixed)) }, CommandState::default()]
        ),
        fixture!("command-value", CommandValue, command_values()),
        fixture!("selection-summary", SelectionSummary, vec![selection()]),
        fixture!("character-formatting", CharacterFormatting, vec![character()]),
        fixture!("paragraph-formatting", ParagraphFormatting, vec![paragraph()]),
        fixture!("page-formatting", PageFormatting, vec![page()]),
        fixture!("view-state", ViewState, vec![ViewState::default()]),
        fixture!("review-state", ReviewState, vec![review_state()]),
        // events
        fixture!("event", Event, events()),
        fixture!(
            "key-outcome",
            KeyOutcome,
            vec![
                KeyOutcome::Handled,
                KeyOutcome::HostCommand(HostCommand::Save),
                KeyOutcome::Shortcut("insertSignature".into()),
                KeyOutcome::Unhandled
            ]
        ),
        // review
        fixture!("revision-info", RevisionInfo, vec![revision()]),
        fixture!("comment-thread", CommentThread, vec![thread()]),
        fixture!("person", Person, vec![person()]),
        fixture!("rich-text", RichText, vec![rich()]),
        fixture!("comment-anchor", CommentAnchor, vec![anchor()]),
        fixture!("balloon-placement", BalloonPlacement, vec![balloon()]),
        // services
        fixture!("client-point", ClientPoint, vec![at()]),
        fixture!("client-rect", ClientRect, vec![rect()]),
        fixture!("content-point", ContentPoint, vec![ContentPoint { x: 640.5, y: 98.5 }]),
        fixture!("context-menu", ContextMenu, vec![menu()]),
        fixture!("menu-target", MenuTarget, menu_targets()),
        fixture!("menu-item", MenuItem, menu_items()),
        fixture!("menu-action", MenuAction, menu_actions()),
        fixture!("context-menu-event", ContextMenuEvent, vec![ContextMenuEvent { menu: menu(), at: at(), via: MenuVia::Keyboard }]),
        fixture!("dialog-model", DialogModel, vec![dialog_model()]),
        fixture!("dialog-values", DialogValues, dialog_values()),
        fixture!("field-info", FieldInfo, vec![field_info()]),
        fixture!("find-query", FindQuery, vec![FindQuery { text: "total".into(), match_case: Some(true), whole_words: Some(false) }]),
        fixture!("find-result", FindResult, vec![FindResult { found: true, wrapped: true }]),
        fixture!("find-match", FindMatch, vec![find_match()]),
        fixture!("misspelling", Misspelling, vec![Misspelling { word: "teh".into(), suggestions: vec!["the".into(), "ten".into()] }]),
        fixture!("heading", Heading, vec![Heading { id: HeadingId("h2".into()), level: 2, text: "Terms".into(), page: 3 }]),
        fixture!("style-info", StyleInfo, vec![style_info()]),
        fixture!("element-filter", ElementFilter, vec![element_filter(), ElementFilter::default()]),
        fixture!("element-info", ElementInfo, vec![element_info()]),
        fixture!("element-detail", ElementDetail, vec![element_detail()]),
        fixture!("block", Block, blocks()),
        fixture!("data-part", DataPart, vec![data_part()]),
        fixture!("data-part-info", DataPartInfo, vec![DataPartInfo { namespace: Some("urn:invoice".into()), item_id: "{6A9C}".into() }]),
        fixture!("placeholder-info", PlaceholderInfo, vec![PlaceholderInfo { key: "logo".into(), element: Some(id("control-3")) }]),
        fixture!("accessible-node", AccessibleNode, vec![accessible()]),
        fixture!("shortcut", Shortcut, vec![shortcut()]),
        fixture!(
            "shortcut-keys",
            ShortcutKeys,
            vec![ShortcutKeys::Same("Ctrl+Alt+S".into()), ShortcutKeys::PerPlatform { windows: "Ctrl+Alt+S".into(), mac: "⌥⌘S".into() }]
        ),
        fixture!("key-binding", KeyBinding, key_bindings()),
        fixture!("gesture-action", GestureAction, gesture_actions()),
        fixture!("gesture-binding", GestureBinding, vec![gesture_binding()]),
        fixture!(
            "gesture-target",
            GestureTarget,
            vec![GestureTarget::Menu(MenuTarget::Link { address: "https://example.com".into() }), GestureTarget::Area { kind: AreaKind::HeaderArea }]
        ),
        fixture!("gesture-event", GestureEvent, vec![gesture_event()]),
        fixture!("ruler-model", RulerModel, vec![ruler()]),
        fixture!("ruler-view", RulerView, vec![ruler_view()]),
        fixture!("tab-stop-info", TabStopInfo, vec![tab_stop()]),
        fixture!("page-rect", PageRect, vec![PageRect { page: 2, rect: rect() }]),
        fixture!("hit-result", HitResult, vec![HitResult { element: Some(id("field-1")), editable: true }]),
        fixture!("tooltip", Tooltip, vec![tooltip()]),
        fixture!("word-count", WordCount, vec![word_count()]),
        // fonts, engine, input, documents
        fixture!("face-info", FaceInfo, vec![FaceInfo { family: "Carlito".into(), bold: true, italic: false }]),
        fixture!("font-request", FontRequest, vec![FontRequest { family: "Calibri".into(), bold: false, italic: true }]),
        fixture!(
            "substitution-rule",
            SubstitutionRule,
            vec![substitution(), SubstitutionRule { from: "Arial".into(), bold: None, italic: None, to: "Arimo".into() }]
        ),
        fixture!("session-id", SessionId, vec![SessionId(3)]),
        fixture!(
            "dictionary",
            Dictionary,
            vec![Dictionary { language: "en-US".into(), aff: Bytes(b"SET UTF-8".to_vec()), dic: Bytes(b"1\nhello".to_vec()) }]
        ),
        fixture!("session-options", SessionOptions, vec![session_options(), SessionOptions::default()]),
        fixture!("modifiers", Modifiers, vec![Modifiers { ctrl: true, shift: true, alt: false, meta: false }]),
        fixture!("point", Point, vec![Point { x: 120.5, y: 48.5 }]),
        fixture!("clipboard-data", ClipboardData, vec![clipboard(), ClipboardData { text: "plain".into(), html: None, internal: None }]),
        fixture!("export-options", ExportOptions, vec![ExportOptions { format: ExportFormat::Pdf }]),
        fixture!("export", Export, vec![Export { bytes: Bytes(b"PK".to_vec()), revision: 7, warnings: vec![warning()] }]),
        fixture!("frame-info", FrameInfo, vec![frame_info()]),
        fixture!("frame-stats", FrameStats, vec![frame_stats()]),
        fixture!("overlay-palette", OverlayPalette, vec![OverlayPalette::default()]),
        fixture!("paint-budget", PaintBudget, vec![PaintBudget::default()]),
        fixture!(
            "caret-blink",
            CaretBlink,
            vec![
                CaretBlink::default(),
                CaretBlink { interval_ms: None, stop_after_ms: None },
                CaretBlink { interval_ms: Some(600), stop_after_ms: Some(5000) }
            ]
        ),
        fixture!(
            "host-request",
            HostRequest,
            vec![
                HostRequest::Cursor { cursor: Cursor::ResizeHorizontal },
                HostRequest::Capture,
                HostRequest::Release,
                HostRequest::Clipboard { data: clipboard() },
                HostRequest::Caret { x: 96.5, y: 120.0, height: 18.5, visible: true },
                HostRequest::Scroll { x: -12.0, y: 1180.5 },
                HostRequest::ScrollTo { x: 96.5, y: 1240.0, width: 1.5, height: 18.5 },
                HostRequest::A11y { tree: accessible() },
            ]
        ),
    ]);
    out.sort_by_key(|f| f.name);
    out
}

fn id(s: &str) -> ElementId {
    ElementId(s.into())
}

fn style(s: &str) -> StyleId {
    StyleId(s.into())
}

fn time() -> Timestamp {
    Timestamp(1_792_000_000)
}

fn identity() -> Identity {
    Identity { provider: "AD".into(), user_id: "S-1-5-21".into() }
}

fn author() -> Author {
    Author { name: "Ada Lovelace".into(), initials: Some("AL".into()), identity: Some(identity()) }
}

fn summary() -> ChangeSummary {
    ChangeSummary {
        paragraphs: 2,
        tables: 1,
        elements: vec![id("field-1")],
        revisions: vec![RevisionId("r7".into())],
        comments: vec![CommentId("c4".into())],
    }
}

fn record() -> TransactionRecord {
    TransactionRecord { id: 12, revision: 7, origin: Origin::Api, author: author(), time: time(), label: "Bold".into(), summary: summary() }
}

fn warning() -> Warning {
    Warning {
        code: WarningCode::UnsupportedElement,
        message: "w:customXml is not kept".into(),
        count: 3,
        location: Some(Location { part: "word/document.xml".into(), paragraph: None }),
    }
}

fn line_spacings() -> Vec<LineSpacing> {
    vec![LineSpacing::Multiple { lines: 1.5 }, LineSpacing::Exact { size: Points(14.5) }, LineSpacing::AtLeast { size: Points(12.5) }]
}

fn row_height() -> RowHeight {
    RowHeight { height: Points(20.5), rule: HeightRule::Exact }
}

fn binding() -> DataBinding {
    DataBinding { xpath: "/invoice/total".into(), store_item_id: Some("{6A9C}".into()), prefix_mappings: Some("xmlns:i='urn:invoice'".into()) }
}

fn control_options() -> Vec<ControlOptions> {
    vec![
        ControlOptions::Checkbox(CheckboxOptions { checked: Some(true) }),
        ControlOptions::Date(DateOptions { format: Some("yyyy-MM-dd".into()), locale: Some("en-GB".into()) }),
        ControlOptions::List(ListOptions { items: vec![ListItem { text: "Yes".into(), value: "y".into() }] }),
        ControlOptions::PlainText(PlainTextOptions { multiline: Some(true) }),
    ]
}

fn rich() -> RichText {
    RichText {
        paragraphs: vec![RichParagraph {
            runs: vec![
                RichRun { text: "See ".into(), ..RichRun::default() },
                RichRun {
                    text: "clause 4".into(),
                    bold: Some(true),
                    italic: Some(false),
                    underline: Some(true),
                    strikethrough: Some(false),
                    link: Some("https://example.com/terms".into()),
                },
            ],
        }],
    }
}

fn picture_bytes() -> Bytes {
    Bytes(vec![0x89, b'P', b'N', b'G'])
}

/// Every command, in command set order.
fn commands() -> Vec<Command> {
    let ids = || Some(vec![id("field-1"), id("field-2")]);
    vec![
        Command::InsertText("Hello".into()),
        Command::InsertParagraph,
        Command::InsertLineBreak,
        Command::InsertPageBreak,
        Command::InsertColumnBreak,
        Command::InsertSectionBreak(SectionBreakKind::NextPage),
        Command::InsertTab,
        Command::InsertSymbol(InsertSymbol { char: '§', font: Some("Symbol".into()) }),
        Command::Delete(DeleteOptions { direction: Direction::Backward, unit: Some(DeleteUnit::Word) }),
        Command::Cut,
        Command::Copy,
        Command::Paste(PasteOptions { as_: Some(PasteAs::Text) }),
        Command::PasteText("pasted".into()),
        Command::Undo,
        Command::Redo,
        Command::Repeat,
        Command::SelectAll,
        Command::Select(SelectUnit::Sentence),
        Command::MoveSelection(MoveSelection { by: MoveUnit::Word, direction: Direction::Forward, extend: Some(true) }),
        Command::MoveToCell(Order::Next),
        Command::SetSelectionMode(SelectionMode::Extend),
        Command::GoBack,
        Command::Escape,
        Command::SelectNextObject(Order::Previous),
        Command::GoToPage(3),
        Command::SetBold(true),
        Command::SetItalic(false),
        Command::SetUnderline(UnderlineStyle::Double),
        Command::SetStrikethrough(true),
        Command::SetDoubleStrikethrough(false),
        Command::SetSubscript(true),
        Command::SetSuperscript(false),
        Command::SetSmallCaps(true),
        Command::SetAllCaps(false),
        Command::SetHidden(true),
        Command::SetFont("Carlito".into()),
        Command::SetFontSize(Points(10.5)),
        Command::GrowFont,
        Command::ShrinkFont,
        Command::SetFontColor(AutoOr::Value(Color(0xC00000))),
        Command::SetHighlight(Some(HighlightColor::BrightGreen)),
        Command::SetCharacterSpacing(Points(0.5)),
        Command::SetCharacterPosition(Points(-2.5)),
        Command::ClearFormatting,
        Command::ChangeCase(CaseChange::Title),
        Command::CopyFormatting,
        Command::PasteFormatting,
        Command::SetAlignment(Alignment::Justify),
        Command::SetLineSpacing(LineSpacing::Multiple { lines: 1.5 }),
        Command::SetParagraphSpacing(ParagraphSpacing { before: Some(AutoOr::Auto), after: Some(AutoOr::Value(Points(8.5))) }),
        Command::SetIndents(Indents { left: Some(Points(36.5)), right: Some(Points(0.5)), first_line: Some(Points(18.5)), hanging: None }),
        Command::IncreaseIndent,
        Command::DecreaseIndent,
        Command::SetParagraphStyle(style("Heading1")),
        Command::SetList(Some(ListKind::Numbered)),
        Command::SetListLevel(LevelChange::Increase),
        Command::SetParagraphFlow(ParagraphFlow {
            keep_with_next: Some(true),
            keep_lines_together: Some(false),
            page_break_before: Some(true),
            widow_control: Some(true),
        }),
        Command::SetParagraphBorders(BorderPreset::Box),
        Command::SetParagraphShading(Some(Color(0xD9E2F3))),
        Command::AddTabStop(AddTabStop { position: Points(72.5), alignment: Some(TabAlignment::Decimal), leader: Some(TabLeader::Dots) }),
        Command::MoveTabStop(MoveTabStop { from: Points(72.5), to: Points(90.5) }),
        Command::DeleteTabStop(Points(90.5)),
        Command::ClearTabStops,
        Command::SetDefaultTabStop(Points(36.5)),
        Command::SetMargins(Margins {
            top: Some(Points(72.5)),
            bottom: Some(Points(72.5)),
            left: Some(Points(90.5)),
            right: Some(Points(90.5)),
            gutter: Some(Points(0.5)),
        }),
        Command::SetOrientation(Orientation::Landscape),
        Command::SetPaperSize(PaperSize { width: Points(595.5), height: Points(841.5) }),
        Command::SetColumns(Columns { count: 2, spacing: Some(Points(36.5)), separator: Some(true) }),
        Command::SetHeaderFooterDistance(HeaderFooterDistance { header: Some(Points(36.5)), footer: Some(Points(30.5)) }),
        Command::SetDifferentFirstPage(true),
        Command::SetDifferentOddEvenPages(false),
        Command::MoveColumnBoundary(ColumnBoundaryMove { column: 0, to: Points(250.5) }),
        Command::EditHeader,
        Command::EditFooter,
        Command::CloseHeaderFooter,
        Command::InsertPageNumber(PageNumber {
            format: PageNumberFormat::PageOfTotal,
            place: Some(PageNumberPlace::Footer),
            alignment: Some(HorizontalAlignment::Center),
        }),
        Command::InsertTable(TableSize { rows: 3, columns: 2 }),
        Command::InsertPicture(Picture { data: picture_bytes(), alt_text: Some("Company logo".into()) }),
        Command::InsertTextBox,
        Command::InsertFootnote,
        Command::InsertEndnote,
        Command::InsertHyperlink(Hyperlink { address: "https://example.com".into(), text: Some("Example".into()) }),
        Command::EditHyperlink(HyperlinkEdit { address: Some("https://example.org".into()), text: None }),
        Command::RemoveHyperlink,
        Command::InsertField(InsertField { code: "MERGEFIELD Name".into(), display: Some("«Name»".into()), show_code: Some(false) }),
        Command::InsertRows(InsertRows { side: VerticalSide::Below, count: Some(2) }),
        Command::InsertColumns(InsertColumns { side: HorizontalSide::Left, count: Some(1) }),
        Command::DeleteRows,
        Command::DeleteColumns,
        Command::DeleteTable,
        Command::DeleteCells(CellShift::Up),
        Command::MergeCells,
        Command::SplitCells(SplitCells { columns: 2, rows: Some(1) }),
        Command::SplitTable,
        Command::DistributeRows,
        Command::DistributeColumns,
        Command::MoveRows(VerticalDirection::Down),
        Command::AutoFitColumns(AutoFit::Contents),
        Command::SetCellAlignment(CellAlignment::Center),
        Command::SetCellShading(None),
        Command::SetCellBorders(CellBorders {
            edges: BorderEdges::Outside,
            border: Some(Border { style: BorderStyle::Double, width: Points(1.5), color: AutoOr::Auto }),
        }),
        Command::SetCellMargins(CellMargins { top: Some(Points(2.5)), bottom: Some(Points(2.5)), left: Some(Points(5.5)), right: Some(Points(5.5)) }),
        Command::SetHeaderRow(true),
        Command::SetRowHeight(AutoOr::Value(row_height())),
        Command::SetRowCanBreak(false),
        Command::SetColumnWidth(Points(120.5)),
        Command::MoveTableColumnBoundary(BoundaryMove { index: 1, to: Points(200.5) }),
        Command::MoveTableRowBoundary(BoundaryMove { index: 0, to: Points(30.5) }),
        Command::SetTableWidth(TableWidth::Percent { value: 100.5 }),
        Command::SetTableAlignment(HorizontalAlignment::Right),
        Command::SetTableIndent(Points(5.5)),
        Command::SetTableStyle(style("TableGrid")),
        Command::SetTableStyleOptions(TableStyleChange {
            header_row: Some(true),
            total_row: Some(false),
            first_column: Some(true),
            last_column: Some(false),
            banded_rows: Some(true),
            banded_columns: Some(false),
        }),
        Command::SetObjectSize(ObjectSize { width: Some(Points(144.5)), height: Some(Points(72.5)), lock_aspect: Some(true) }),
        Command::ResetObjectSize,
        Command::SetObjectWrap(ObjectWrap::Square),
        Command::SetAnchorLocked(true),
        Command::SetAltText(AltText { text: "A chart".into(), decorative: Some(false) }),
        Command::DeleteObject,
        Command::SetField(SetField { id: id("field-1"), code: Some("MERGEFIELD Total".into()), display: Some(Body::Text("«Total»".into())) }),
        Command::SetFieldResults(FieldResults { results: vec![FieldResult { id: id("field-1"), display: Body::Rich(rich()) }], token: Some(4) }),
        Command::UpdateFields(ElementIds { ids: ids() }),
        Command::SetFieldsLocked(FieldsLocked { locked: true, ids: ids() }),
        Command::UnlinkFields(ElementIds { ids: None }),
        Command::ShowFieldCodes(ShowFieldCodes { show: true, ids: None }),
        Command::SetFieldLabels(FieldLabels { labels: BTreeMap::from([("MERGEFIELD Name".into(), "Customer name".into())]), merge: Some(true) }),
        Command::InsertControl(InsertControl {
            kind: ControlKind::DropDown,
            tag: Some("status".into()),
            title: Some("Status".into()),
            lock: Some(ControlLock::Control),
            placeholder: Some("Choose".into()),
            binding: Some(binding()),
            options: Some(ControlOptions::List(ListOptions { items: vec![ListItem { text: "Open".into(), value: "open".into() }] })),
            wrap_selection: Some(false),
        }),
        Command::SetControl(SetControl {
            id: id("control-3"),
            tag: Some("status".into()),
            title: None,
            lock: Some(ControlLock::None),
            placeholder: None,
            binding: Some(None),
            options: Some(ControlOptions::Checkbox(CheckboxOptions { checked: Some(false) })),
        }),
        Command::RemoveControl(RemoveControl { id: id("control-3"), keep_content: Some(true) }),
        Command::FillControl(FillControl { id: id("control-4"), text: None, image: Some(picture_bytes()), docx: None }),
        Command::SetCheckbox(SetCheckbox { id: id("control-5"), checked: true }),
        Command::WrapInRepeatingSection(RepeatingSection { tag: Some("lines".into()), title: Some("Invoice lines".into()) }),
        Command::RemoveElement(id("field-2")),
        Command::ReplaceInElements(ReplaceInElements {
            find: "Name".into(),
            replace: "FullName".into(),
            field: ElementField::Code,
            kind: Some(ElementClass::Field),
            ids: ids(),
        }),
        Command::MarkElements(MarkElements {
            marks: vec![ElementMark { id: id("field-1"), severity: Severity::Warning, message: "Unknown field".into() }],
            clear: Some(true),
        }),
        Command::SetPlaceholderLooks(PlaceholderLooks {
            looks: vec![PlaceholderLookRule {
                matches: PlaceholderMatch::Tag { tag: "logo".into() },
                look: PlaceholderLook::Drawn { fill: Some(Color(0xEEEEEE)), label: Some("Logo".into()), icon_svg: None },
            }],
            merge: Some(false),
        }),
        Command::FillPlaceholder(FillPlaceholder { key: Some("logo".into()), image: picture_bytes() }),
        Command::WriteDataPart(WriteDataPart { namespace: "urn:invoice".into(), item_id: Some("{6A9C}".into()), xml: "<invoice/>".into() }),
        Command::RemoveDataPart(DataPartRef { namespace: "urn:invoice".into(), item_id: None }),
        Command::SetTrackChanges(true),
        Command::SetMarkupView(MarkupView::Simple),
        Command::AcceptRevision(RevisionId("r7".into())),
        Command::RejectRevision(RevisionId("r8".into())),
        Command::AcceptRevisionsInSelection,
        Command::RejectRevisionsInSelection,
        Command::AcceptAllRevisions(AuthorFilter { author: Some("Ada Lovelace".into()) }),
        Command::RejectAllRevisions(AuthorFilter { author: None }),
        Command::SelectRevision(Order::Next),
        Command::AddComment(Body::Text("Check this".into())),
        Command::ReplyToComment(CommentReply { to: CommentId("c4".into()), body: Body::Rich(rich()) }),
        Command::EditComment(CommentEdit { id: CommentId("c5".into()), body: Body::Text("Fixed".into()) }),
        Command::DeleteComment(CommentId("c5".into())),
        Command::ResolveComment(CommentResolve { id: CommentId("c4".into()), done: true }),
        Command::SetActiveComment(Some(CommentId("c4".into()))),
        Command::SelectComment(CommentId("c4".into())),
        Command::SetAuthorColor(AuthorColor { author: "Ada Lovelace".into(), color: Color(0x7030A0) }),
        Command::SetZoom(Zoom::Fit(ZoomFit::WholePage)),
        Command::SetViewOptions(ViewOptions { formatting_marks: Some(true), gridlines: Some(false), text_boundaries: Some(true) }),
        Command::SetMode(Mode::Design),
    ]
}

fn command_results() -> Vec<CommandResult> {
    vec![CommandResult::Element { id: id("field-1") }, CommandResult::DataPart { item_id: "{6A9C}".into() }, CommandResult::Changed { changed: 4 }]
}

fn placeholder_matches() -> Vec<PlaceholderMatch> {
    vec![
        PlaceholderMatch::Default(DefaultKeyword::Default),
        PlaceholderMatch::Id { id: id("control-3") },
        PlaceholderMatch::Tag { tag: "logo".into() },
        PlaceholderMatch::Prefix { prefix: "img-".into() },
    ]
}

fn placeholder_looks() -> Vec<PlaceholderLook> {
    vec![
        PlaceholderLook::Image { image: picture_bytes(), mime: "image/png".into() },
        PlaceholderLook::Drawn { fill: Some(Color(0xEEEEEE)), label: Some("Logo".into()), icon_svg: Some("<svg/>".into()) },
    ]
}

fn command_info() -> CommandInfo {
    CommandInfo {
        id: CommandId::SetBold,
        label: "Bold".into(),
        group: CommandGroup::Character,
        shortcuts: CommandShortcuts { windows: vec!["Ctrl+B".into()], mac: vec!["⌘B".into()] },
        takes_arguments: true,
        mutates: true,
        undo: UndoGrouping::Step,
    }
}

fn command_values() -> Vec<CommandValue> {
    vec![
        CommandValue::Toggle(Tri::Mixed),
        CommandValue::Text(Tri::Uniform("Typing".into())),
        CommandValue::Length(Tri::Uniform(Points(11.5))),
        CommandValue::Underline(Tri::Uniform(UnderlineStyle::Single)),
        CommandValue::Color(Tri::Uniform(AutoOr::Auto)),
        CommandValue::Alignment(Tri::Uniform(Alignment::Center)),
        CommandValue::LineSpacing(Tri::Uniform(LineSpacing::AtLeast { size: Points(12.5) })),
        CommandValue::Toggle(Tri::Unavailable),
    ]
}

fn selection() -> SelectionSummary {
    SelectionSummary {
        kind: SelectionKind::Cells,
        story: Story::Header,
        page: 2,
        mode: SelectionMode::Block,
        empty: false,
        text: Some("Total due".into()),
        table: Some(TableSelection {
            rows: 4,
            columns: 3,
            row: 1,
            column: 2,
            can_merge: true,
            header_row: Tri::Uniform(true),
            row_can_break: Tri::Mixed,
            cell_alignment: Tri::Uniform(CellAlignment::Bottom),
            cell_shading: Tri::Uniform(NoneOr::None),
            row_height: Tri::Uniform(AutoOr::Value(row_height())),
            column_width: Tri::Mixed,
            style: Some(style("TableGrid")),
            style_options: TableStyleOptions {
                header_row: true,
                total_row: false,
                first_column: true,
                last_column: false,
                banded_rows: true,
                banded_columns: false,
            },
            width: TableWidth::Points { value: Points(432.5) },
            alignment: HorizontalAlignment::Center,
            indent: Points(0.5),
        }),
        object: Some(ObjectSelection {
            kind: ObjectKind::TextBox,
            wrap: ObjectWrap::InFrontOfText,
            width: Points(144.5),
            height: Points(72.5),
            alt_text: "A note".into(),
            decorative: false,
            anchor_locked: Some(true),
        }),
        context: vec![ElementContext {
            id: id("field-1"),
            kind: ElementKind::Field,
            exact: true,
            code: Some("MERGEFIELD Name".into()),
            tag: Some("name".into()),
            title: Some("Name".into()),
        }],
        link: Some(LinkInfo { address: "https://example.com".into(), text: "Example".into() }),
        comment: Some(CommentId("c4".into())),
        revision: Some(RevisionId("r7".into())),
        header_footer: Some(HeaderFooterInfo {
            kind: HeaderFooterKind::Header,
            pages: HeaderFooterType::First,
            section: 2,
            page: 2,
            label: "First Page Header -Section 2-".into(),
        }),
    }
}

fn character() -> CharacterFormatting {
    CharacterFormatting {
        font: Tri::Uniform("Calibri".into()),
        size: Tri::Uniform(Points(11.5)),
        bold: Tri::Mixed,
        italic: Tri::Uniform(false),
        underline: Tri::Uniform(UnderlineStyle::None),
        strikethrough: Tri::Uniform(false),
        double_strikethrough: Tri::Uniform(false),
        subscript: Tri::Uniform(false),
        superscript: Tri::Uniform(false),
        small_caps: Tri::Uniform(false),
        all_caps: Tri::Uniform(false),
        hidden: Tri::Uniform(false),
        color: Tri::Uniform(AutoOr::Auto),
        highlight: Tri::Uniform(NoneOr::Value(HighlightColor::Yellow)),
        character_spacing: Tri::Uniform(Points(0.5)),
        position: Tri::Unavailable,
    }
}

fn paragraph() -> ParagraphFormatting {
    ParagraphFormatting {
        style: Tri::Uniform(style("Normal")),
        alignment: Tri::Uniform(Alignment::Left),
        line_spacing: Tri::Uniform(LineSpacing::Multiple { lines: 1.5 }),
        space_before: Tri::Uniform(AutoOr::Auto),
        space_after: Tri::Uniform(AutoOr::Value(Points(8.5))),
        indents: IndentState { left: Tri::Uniform(Points(0.5)), right: Tri::Uniform(Points(0.5)), first_line: Tri::Mixed, hanging: Tri::Unavailable },
        list: Tri::Uniform(ListState::Bullet),
        list_level: Tri::Uniform(1),
        flow: FlowState {
            keep_with_next: Tri::Uniform(true),
            keep_lines_together: Tri::Uniform(false),
            page_break_before: Tri::Mixed,
            widow_control: Tri::Uniform(true),
        },
        borders: Tri::Uniform(BorderState::Other),
        shading: Tri::Uniform(NoneOr::Value(Color(0xD9E2F3))),
    }
}

fn page() -> PageFormatting {
    PageFormatting {
        size: PageSize { width: Points(612.5), height: Points(792.5) },
        orientation: Orientation::Portrait,
        margins: PageMargins { top: Points(72.5), bottom: Points(72.5), left: Points(72.5), right: Points(72.5), gutter: Points(0.5) },
        columns: ColumnLayout { count: 1, spacing: Points(36.5), separator: false },
        header_distance: Points(36.5),
        footer_distance: Points(36.5),
        different_first_page: true,
        different_odd_even: false,
    }
}

fn review_state() -> ReviewState {
    ReviewState {
        tracking: true,
        tracking_locked: false,
        markup_view: MarkupView::All,
        revisions: 3,
        comments: 2,
        active_comment: Some(CommentId("c4".into())),
    }
}

fn state() -> State {
    State {
        revision: 7,
        sequence: 19,
        load: LoadState::Ready,
        mode: Mode::Editing,
        read_only: false,
        modified: true,
        selection: selection(),
        formatting: character(),
        paragraph: paragraph(),
        page: page(),
        commands: BTreeMap::from([
            (CommandId::SetBold, CommandState { enabled: true, checked: None, value: Some(CommandValue::Toggle(Tri::Mixed)) }),
            (CommandId::Undo, CommandState { enabled: true, checked: None, value: Some(CommandValue::Text(Tri::Uniform("Bold".into()))) }),
        ]),
        dialogs: crate::services::dialogs::availability(&State::default()),
        history: HistoryState { can_undo: true, can_redo: false, undo_label: Some("Bold".into()), redo_label: None },
        pages: PagesState { count: 4, current: 2, complete: false },
        view: ViewState::default(),
        review: review_state(),
        spelling: SpellingState { enabled: true, language: "en-US".into(), misspellings: 2 },
        fonts: FontsState { pending: vec!["Aptos".into()], unresolved: vec!["Brand Sans".into()] },
        ruler: ruler(),
        format_painter: false,
    }
}

fn at() -> ClientPoint {
    ClientPoint { client_x: 310.5, client_y: 220.5 }
}

fn rect() -> ClientRect {
    ClientRect { left: 40.5, top: 80.5, width: 816.5, height: 1056.5 }
}

fn person() -> Person {
    Person { name: "Ada Lovelace".into(), initials: Some("AL".into()), identity: Some(identity()), color: Color(0x7030A0) }
}

fn revision() -> RevisionInfo {
    RevisionInfo {
        id: RevisionId("r7".into()),
        kind: RevisionKind::MoveFrom,
        author: person(),
        time: Some(time()),
        text: "moved text".into(),
        description: String::new(),
        page: 2,
        story: Story::Body,
        pair: Some(RevisionId("r8".into())),
    }
}

fn thread() -> CommentThread {
    CommentThread {
        id: CommentId("c4".into()),
        done: false,
        anchor_text: "clause 4".into(),
        page: 3,
        comments: vec![Comment { id: CommentId("c4".into()), author: person(), time: None, body: rich(), plain_text: "See clause 4".into() }],
    }
}

fn anchor() -> CommentAnchor {
    CommentAnchor {
        thread: CommentId("c4".into()),
        page: 3,
        active: true,
        done: false,
        start: ContentPoint { x: 120.5, y: 300.5 },
        end: ContentPoint { x: 240.5, y: 300.5 },
        line_top: 288.5,
        line_bottom: 304.5,
        page_edge: 856.5,
    }
}

fn balloon() -> BalloonPlacement {
    BalloonPlacement {
        thread: CommentId("c4".into()),
        top: 288.5,
        left: 870.5,
        leader: vec![ContentPoint { x: 240.5, y: 300.5 }, ContentPoint { x: 856.5, y: 300.5 }, ContentPoint { x: 870.5, y: 296.5 }],
    }
}

fn menu_targets() -> Vec<MenuTarget> {
    vec![
        MenuTarget::Text,
        MenuTarget::Misspelling { word: "teh".into(), language: "en-US".into() },
        MenuTarget::Link { address: "https://example.com".into() },
        MenuTarget::Table,
        MenuTarget::Picture,
        MenuTarget::TextBox,
        MenuTarget::Element { id: id("field-1") },
        MenuTarget::Revision { id: RevisionId("r7".into()) },
        MenuTarget::Comment { id: CommentId("c4".into()) },
        MenuTarget::HeaderFooter,
    ]
}

fn menu_actions() -> Vec<MenuAction> {
    vec![
        MenuAction::Command { command: Command::Cut },
        MenuAction::ReplaceWord { with: "the".into() },
        MenuAction::IgnoreWord { word: "teh".into() },
        MenuAction::AddWord { word: "Twip".into(), language: "en-US".into() },
    ]
}

fn menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::Command {
            command: CommandId::Cut,
            label: "Cut".into(),
            enabled: true,
            checked: Some(false),
            shortcut: Some("Ctrl+X".into()),
            action: MenuAction::Command { command: Command::Cut },
        },
        MenuItem::Suggestion { label: "the".into(), action: MenuAction::ReplaceWord { with: "the".into() } },
        MenuItem::Submenu {
            label: "Paste Options".into(),
            items: vec![MenuItem::Suggestion { label: "Ignore".into(), action: MenuAction::IgnoreWord { word: "teh".into() } }],
        },
        MenuItem::Separator,
    ]
}

fn menu() -> ContextMenu {
    ContextMenu { target: MenuTarget::Misspelling { word: "teh".into(), language: "en-US".into() }, items: menu_items() }
}

fn field_info() -> FieldInfo {
    FieldInfo {
        label: "Size".into(),
        enabled: true,
        kind: FieldKind::Choice,
        min: Some(1.5),
        max: Some(1638.5),
        step: Some(0.5),
        options: Some(vec![ChoiceOption { value: "12".into(), label: "12".into() }]),
    }
}

fn tab_stop() -> TabStopInfo {
    TabStopInfo { position: Points(72.5), alignment: TabAlignment::Right, leader: TabLeader::Dots, inherited: true }
}

fn dialog_values() -> Vec<DialogValues> {
    vec![
        DialogValues::Font(FontDialog {
            font: Tri::Uniform("Calibri".into()),
            style: Tri::Uniform(FontStyle::BoldItalic),
            size: Tri::Mixed,
            color: Tri::Uniform(AutoOr::Auto),
            underline: Tri::Uniform(UnderlineStyle::Wave),
            underline_color: Tri::Uniform(AutoOr::Value(Color(0xFF0000))),
            strikethrough: Tri::Uniform(false),
            double_strikethrough: Tri::Uniform(false),
            superscript: Tri::Uniform(false),
            subscript: Tri::Uniform(false),
            small_caps: Tri::Uniform(true),
            all_caps: Tri::Uniform(false),
            hidden: Tri::Uniform(false),
            character_spacing: Tri::Uniform(Points(0.5)),
            position: Tri::Unavailable,
        }),
        DialogValues::Paragraph(ParagraphDialog {
            alignment: Tri::Uniform(Alignment::Right),
            outline_level: Tri::Uniform(OutlineLevel::Level(1)),
            left: Tri::Uniform(Points(36.5)),
            right: Tri::Uniform(Points(0.5)),
            special: Tri::Uniform(SpecialIndent::Hanging),
            by: Tri::Uniform(Points(18.5)),
            before: Tri::Uniform(AutoOr::Auto),
            after: Tri::Uniform(AutoOr::Value(Points(8.5))),
            line_spacing: Tri::Uniform(LineSpacing::Exact { size: Points(14.5) }),
            keep_with_next: Tri::Uniform(true),
            keep_lines_together: Tri::Mixed,
            page_break_before: Tri::Uniform(false),
            widow_control: Tri::Uniform(true),
        }),
        DialogValues::PageSetup(PageSetupDialog {
            top: Points(72.5),
            bottom: Points(72.5),
            left: Points(90.5),
            right: Points(90.5),
            gutter: Points(0.5),
            orientation: Orientation::Landscape,
            width: Points(792.5),
            height: Points(612.5),
            header_distance: Points(36.5),
            footer_distance: Points(36.5),
            different_first_page: false,
            different_odd_even: true,
            apply_to: ApplyTo::WholeDocument,
        }),
        DialogValues::InsertTable(InsertTableDialog { rows: 3, columns: 4 }),
        DialogValues::TableProperties(TablePropertiesDialog {
            width: TableWidth::Auto,
            alignment: HorizontalAlignment::Left,
            indent: Points(5.5),
            row_height: Tri::Uniform(AutoOr::Auto),
            row_can_break: Tri::Uniform(true),
            header_row: Tri::Uniform(false),
            column_width: Tri::Uniform(Points(120.5)),
            cell_alignment: Tri::Mixed,
            cell_margins: CellMarginValues { top: Points(0.5), bottom: Points(0.5), left: Points(5.5), right: Points(5.5) },
        }),
        DialogValues::PictureSize(PictureSizeDialog {
            width: Points(144.5),
            height: Points(72.5),
            scale_width: 50.5,
            scale_height: 50.5,
            lock_aspect: true,
            reset: false,
        }),
        DialogValues::AltText(AltTextDialog { text: "A chart".into(), decorative: false }),
        DialogValues::Hyperlink(HyperlinkDialog { text: "Example".into(), address: "https://example.com".into() }),
        DialogValues::Tabs(TabsDialog {
            stops: vec![tab_stop()],
            default_stop: Points(36.5),
            position: Some(Points(72.5)),
            alignment: TabAlignment::Right,
            leader: TabLeader::Dots,
            cleared: vec![Points(144.5)],
            action: TabsAction::Set,
        }),
        DialogValues::Accessibility(AccessibilityDialog {
            issues: vec![AccessibilityIssue { id: "alt-1".into(), text: "Picture 1 has no alt text".into(), tip: false }],
        }),
    ]
}

fn dialog_model() -> DialogModel {
    DialogModel {
        kind: DialogKind::InsertTable,
        values: DialogValues::InsertTable(InsertTableDialog { rows: 2, columns: 5 }),
        fields: BTreeMap::from([(
            "rows".into(),
            FieldInfo { label: "Rows".into(), enabled: true, kind: FieldKind::Number, min: Some(1.5), max: None, step: None, options: None },
        )]),
    }
}

fn find_match() -> FindMatch {
    FindMatch { id: MatchId("m1".into()), before: "the ".into(), text: "total".into(), after: " due".into(), page: 2 }
}

fn style_info() -> StyleInfo {
    StyleInfo {
        id: style("Heading1"),
        name: "heading 1".into(),
        kind: StyleType::Paragraph,
        built_in: true,
        quick: true,
        priority: 9,
        hidden: false,
        based_on: Some(style("Normal")),
        preview: StylePreview { font: "Calibri Light".into(), size: Points(16.5), bold: false, italic: false, color: AutoOr::Value(Color(0x2F5496)) },
    }
}

fn element_filter() -> ElementFilter {
    ElementFilter {
        kind: Some(ElementKind::PlainText),
        matches: Some(ElementMatch { field: ElementField::Tag, text: "name".into() }),
        story: Some(Story::Body),
    }
}

fn element_info() -> ElementInfo {
    ElementInfo {
        id: id("control-3"),
        kind: ElementKind::PlainText,
        code: None,
        tag: Some("name".into()),
        title: Some("Name".into()),
        display: "Ada".into(),
        story: Story::Body,
        parent: Some(id("control-1")),
        locked: false,
    }
}

fn element_detail() -> ElementDetail {
    ElementDetail {
        id: id("control-3"),
        kind: ElementKind::Date,
        code: Some("DATE".into()),
        tag: Some("due".into()),
        title: Some("Due".into()),
        display: "2026-10-27".into(),
        story: Story::Footer,
        parent: None,
        locked: true,
        lock: ControlLock::Both,
        placeholder: Some("Pick a date".into()),
        binding: Some(binding()),
        options: Some(ControlOptions::Date(DateOptions { format: Some("yyyy-MM-dd".into()), locale: None })),
        children: vec![id("field-9")],
    }
}

fn blocks() -> Vec<Block> {
    vec![
        Block::Paragraph { text_length: 42, elements: vec![id("field-1")] },
        Block::Table {
            rows: vec![BlockRow {
                cells: vec![BlockCell {
                    span: 2,
                    vertical_merge: VerticalMerge::Start,
                    empty: false,
                    blocks: vec![Block::Paragraph { text_length: 5, elements: Vec::new() }],
                }],
            }],
        },
    ]
}

fn data_part() -> DataPart {
    DataPart { namespace: None, item_id: "{6A9C}".into(), xml: "<invoice/>".into() }
}

fn accessible() -> AccessibleNode {
    AccessibleNode {
        role: AccessibleRole::Document,
        name: Some("Contract".into()),
        text: None,
        level: None,
        marks: None,
        children: vec![AccessibleNode {
            role: AccessibleRole::Heading,
            name: None,
            text: Some("Terms".into()),
            level: Some(1),
            marks: Some(vec![AccessibleMark::Insertion, AccessibleMark::Misspelling]),
            children: Vec::new(),
        }],
    }
}

fn shortcut() -> Shortcut {
    Shortcut { id: "insertSignature".into(), keys: ShortcutKeys::PerPlatform { windows: "Ctrl+Alt+S".into(), mac: "⌥⌘S".into() } }
}

fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::Command { id: CommandId::SetBold },
        KeyBinding::Shortcut { id: "insertSignature".into() },
        KeyBinding::Host { command: HostCommand::Print },
    ]
}

fn gesture_actions() -> Vec<GestureAction> {
    vec![GestureAction::Default, GestureAction::Emit, GestureAction::None, GestureAction::Op(Command::Select(SelectUnit::Paragraph))]
}

fn gesture_binding() -> GestureBinding {
    GestureBinding { gesture: GestureId::HeaderAreaDoubleClick, action: GestureAction::Emit, label: "Double-click the header area".into() }
}

fn gesture_event() -> GestureEvent {
    GestureEvent { gesture: GestureId::RulerDoubleClick, at: at(), page: 1, target: GestureTarget::Area { kind: AreaKind::Ruler } }
}

fn ruler() -> RulerModel {
    RulerModel {
        mixed: true,
        horizontal: HorizontalRuler {
            page_width: Points(612.5),
            text: Span { start: Points(72.5), end: Points(540.5) },
            indents: RulerIndents { left: Points(36.5), right: Points(0.5), first_line: Points(-18.5) },
            tabs: vec![tab_stop()],
            default_tab_stop: Points(36.5),
            columns: vec![Span { start: Points(72.5), end: Points(288.5) }, Span { start: Points(324.5), end: Points(540.5) }],
            table_columns: vec![Points(72.5), Points(200.5)],
        },
        vertical: VerticalRuler {
            page_height: Points(792.5),
            text: Span { start: Points(72.5), end: Points(720.5) },
            table_rows: vec![Points(100.5)],
        },
    }
}

fn ruler_view() -> RulerView {
    RulerView {
        model: ruler(),
        page: 1,
        horizontal: RulerAxis { origin: 40.5, scale: 1.5 },
        vertical: RulerAxis { origin: 80.5, scale: 1.5 },
        unit: RulerUnit::Centimeters,
    }
}

fn tooltip() -> Tooltip {
    Tooltip { kind: TooltipKind::Revision, text: "Ada Lovelace inserted".into(), at: at(), rect: rect() }
}

fn word_count() -> WordCount {
    WordCount { words: 120, characters: 640, characters_with_spaces: 760, paragraphs: 9, lines: 22, pages: 2 }
}

fn substitution() -> SubstitutionRule {
    SubstitutionRule { from: "Times New Roman".into(), bold: Some(true), italic: Some(false), to: "Tinos".into() }
}

fn session_options() -> SessionOptions {
    SessionOptions {
        author: author(),
        read_only: true,
        mode: Mode::Design,
        language: "en-GB".into(),
        dictionaries: vec![Dictionary { language: "en-GB".into(), aff: Bytes(b"SET UTF-8".to_vec()), dic: Bytes(b"1\ncolour".to_vec()) }],
        track_changes: true,
    }
}

fn clipboard() -> ClipboardData {
    ClipboardData { text: "copied".into(), html: Some("<b>copied</b>".into()), internal: Some("3.17".into()) }
}

fn frame_stats() -> FrameStats {
    FrameStats { frames: 120, pages_recorded: 4, pages_reused: 1062, recording_bytes: 524_288, last_frame_ms: 1.5, p95_frame_ms: 2.5 }
}

fn frame_info() -> FrameInfo {
    FrameInfo { content_width: 816.5, content_height: 2112.5, caret_x: 96.5, caret_y: 120.5, caret_height: 14.5, caret_visible: true, pages: 2 }
}

/// Every event.
fn events() -> Vec<Event> {
    vec![
        Event::StateChanged(state()),
        Event::DocumentChanged(record()),
        Event::SelectionChanged(selection()),
        Event::LinkActivated { address: "https://example.com".into(), via: LinkVia::CtrlClick },
        Event::PlaceholderActivated(PlaceholderInfo { key: "logo".into(), element: None }),
        Event::ContextMenu(ContextMenuEvent { menu: menu(), at: at(), via: MenuVia::Pointer }),
        Event::DialogRequested { kind: DialogKind::Tabs },
        Event::HostCommand { command: HostCommand::Save },
        Event::Shortcut { id: "insertSignature".into() },
        Event::Gesture(gesture_event()),
        Event::WordAdded { word: "Twip".into(), language: "en-US".into() },
        Event::FontsChanged { pending: vec!["Aptos".into()], unresolved: vec!["Brand Sans".into()] },
        Event::ElementActivated { id: id("field-1") },
        Event::ElementsChanged { added: vec![element_info()], removed: vec![id("field-2")], changed: Vec::new() },
        Event::FieldsUpdateRequested { token: 4, fields: vec![FieldRequest { id: id("field-1"), code: "DOCPROPERTY Client".into() }] },
        Event::RevisionsChanged {},
        Event::CommentsChanged {},
        Event::CommentAnchors(vec![anchor()]),
        Event::BalloonsLaidOut(vec![balloon()]),
        Event::RulerChanged(ruler_view()),
        Event::Tooltip(Some(tooltip())),
        Event::Tooltip(None),
        Event::Announcement { text: "Bold on".into() },
        Event::Warning(warning()),
        Event::fault(Error::new(ErrorCode::Internal, "the engine failed")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_an_example() {
        let ids: Vec<CommandId> = commands().iter().map(Command::id).collect();
        assert_eq!(ids, CommandId::ALL, "the command examples follow the command set, one each");
    }

    #[test]
    fn fixture_names_are_unique() {
        let names: Vec<&str> = fixtures().iter().map(|f| f.name).collect();
        assert!(names.windows(2).all(|w| w[0] < w[1]), "{names:?}");
    }
}
