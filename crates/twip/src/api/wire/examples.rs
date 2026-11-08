//! One example of every variant of every `api` type: the wire fixtures. Unit-variant enums list every variant by
//! their serde names; the rest are written out here, so a changed type fails to compile until its examples follow.
//! Option structs of commands, dialog values and event payloads are covered inside their parents' files.

use std::collections::BTreeMap;

use serde::de::{DeserializeOwned, Error as _, Visitor};
use serde::{Deserializer, Serialize};
use serde_json::Value;

use crate::api::catalog::{CommandGroup, CommandInfo, CommandShortcuts, UndoGrouping};
use crate::api::command::*;
use crate::api::drawing as dr;
use crate::api::engine::{Dictionary, SessionId, SessionOptions};
use crate::api::error::{Error, ErrorCode};
use crate::api::event::{Event, FieldRequest, HostCommand, KeyOutcome, LinkVia, RefusalReason};
use crate::api::fonts::{
    Alias, AliasTarget, FaceInfo, FontAnswer, FontClass, FontDescription, FontPitch, FontRequest, FontRule, GenericDefaults, GenericFamily,
};
use crate::api::input::{ClipboardData, CompositionPhase, Key, Modifiers, Point, PointerButton, PointerKind};
use crate::api::record::{Author, ChangeSummary, Identity, Origin, Timestamp, TransactionRecord};
use crate::api::review::*;
use crate::api::services::*;
use crate::api::session::{
    CaretBlink, Commit, Cursor, Export, ExportFormat, ExportOptions, FrameInfo, FrameStats, HostRequest, OverlayPalette, PaintBudget, PdfOptions,
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
        "announcement-kind": AnnouncementKind,
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
        "delete-unit": DeleteUnit,
        "dialog-kind": DialogKind,
        "direction": Direction,
        "element-class": ElementClass,
        "element-field": ElementField,
        "element-kind": ElementKind,
        "error-code": ErrorCode,
        "export-format": ExportFormat,
        "field-kind": FieldKind,
        "font-class": FontClass,
        "font-style": FontStyle,
        "generic-family": GenericFamily,
        "gesture-id": GestureId,
        "header-footer-kind": HeaderFooterKind,
        "header-footer-type": HeaderFooterType,
        "height-rule": HeightRule,
        "highlight-color": HighlightColor,
        "horizontal-alignment": HorizontalAlignment,
        "horizontal-reference": HorizontalReference,
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
        "object-horizontal-alignment": ObjectHorizontalAlignment,
        "object-vertical-alignment": ObjectVerticalAlignment,
        "object-wrap": ObjectWrap,
        "order": Order,
        "orientation": Orientation,
        "origin": Origin,
        "page-number-format": PageNumberFormat,
        "font-pitch": FontPitch,
        "page-number-place": PageNumberPlace,
        "paste-as": PasteAs,
        "pointer-button": PointerButton,
        "pointer-kind": PointerKind,
        "reference-resolution": ReferenceResolution,
        "revision-kind": RevisionKind,
        "ruler-unit": RulerUnit,
        "section-break-kind": SectionBreakKind,
        "replace-scope": ReplaceScope,
        "select-unit": SelectUnit,
        "selection-kind": SelectionKind,
        "selection-mode": SelectionMode,
        "severity": Severity,
        "special-indent": SpecialIndent,
        "stand-in": StandIn,
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
        "vertical-reference": VerticalReference,
        "vertical-side": VerticalSide,
        "warning-code": WarningCode,
        "wrap-side": WrapSide,
        "zoom-fit": ZoomFit,
        "theme-colour": dr::ThemeColour,
        "transform-kind": dr::TransformKind,
        "gradient-shape": dr::GradientShape,
        "compound": dr::Compound,
        "line-cap": dr::LineCap,
        "line-join": dr::LineJoin,
        "arrow-kind": dr::ArrowKind,
        "arrow-size": dr::ArrowSize,
        "text-anchor": dr::TextAnchor,
        "text-autofit": dr::TextAutofit,
        "text-direction": dr::TextDirection,
        "flip-axis": dr::FlipAxis,
        "arrange": dr::Arrange,
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
        // shapes
        fixture!("shape-preset", dr::ShapePreset, dr::ShapePreset::all().collect()),
        fixture!(
            "colour-value",
            dr::ColourValue,
            vec![
                theme_colour(),
                dr::ColourValue::rgb(Color(0x1F4E79)),
                dr::ColourValue { preset: Some("cornflowerBlue".into()), ..Default::default() }
            ]
        ),
        fixture!("colour-transform", dr::ColourTransform, vec![dr::ColourTransform { kind: dr::TransformKind::Tint, value: 0.5 }]),
        fixture!("gradient-stop", dr::GradientStop, vec![dr::GradientStop { position: 0.5, colour: theme_colour() }]),
        fixture!("fill-value", dr::FillValue, fills()),
        fixture!("line-value", dr::LineValue, vec![line_value()]),
        fixture!("line-change", dr::LineChange, vec![line_change(), dr::LineChange::default()]),
        fixture!("line-ends", dr::LineEnds, vec![line_ends()]),
        fixture!("arrowhead", dr::Arrowhead, vec![arrowhead()]),
        fixture!("shadow", dr::Shadow, vec![shadow()]),
        fixture!("glow", dr::Glow, vec![glow()]),
        fixture!("effects-value", dr::EffectsValue, vec![effects(), dr::EffectsValue::default()]),
        fixture!("effects-change", dr::EffectsChange, vec![effects_change()]),
        fixture!("shape-text-state", dr::ShapeTextState, vec![text_state()]),
        fixture!("shape-text-change", dr::ShapeTextChange, vec![text_change()]),
        fixture!("picture-adjust-change", dr::PictureAdjustChange, vec![picture_adjust()]),
        fixture!("shape-insert", dr::ShapeInsert, vec![dr::ShapeInsert { preset: preset("line"), width: None, height: Some(Points(0.5)) }]),
        fixture!("shape-adjust", dr::ShapeAdjust, vec![dr::ShapeAdjust { index: 1, value: -12500 }]),
        fixture!(
            "fill-state",
            dr::FillState,
            vec![dr::FillState::Explicit(dr::FillValue::None), dr::FillState::FromStyle(dr::FillValue::Solid { colour: theme_colour() })]
        ),
        fixture!("line-state", dr::LineState, vec![dr::LineState::Explicit(line_value()), dr::LineState::FromStyle(line_value())]),
        fixture!("shape-state", dr::ShapeState, vec![shape_state()]),
        fixture!(
            "shape-gallery-group",
            dr::ShapeGalleryGroup,
            vec![dr::ShapeGalleryGroup { label: "Basic Shapes".into(), presets: vec![preset("rect"), preset("ellipse")] }]
        ),
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
        fixture!(
            "chrome-theme",
            ChromeTheme,
            vec![
                ChromeTheme::Light,
                ChromeTheme::Dark,
                ChromeTheme::HighContrast(SystemColors {
                    canvas: Color(0x000000),
                    canvas_text: Color(0xFFFFFF),
                    highlight: Color(0x1AEBFF),
                    highlight_text: Color(0x000000),
                    gray_text: Color(0x3FF23F),
                }),
            ]
        ),
        fixture!("document-appearance", DocumentAppearance, all()),
        fixture!("measure-detail", MeasureDetail, all()),
        fixture!("object-grid", ObjectGrid, all()),
        fixture!("field-shading", FieldShading, all()),
        fixture!("preview-size", PreviewSize, vec![PreviewSize { width: 120.0, height: 60.0, scale: 2.0 }]),
        fixture!("preview-picture", PreviewPicture, vec![PreviewPicture { png: Bytes(vec![0x89, b'P', b'N', b'G']), width: 240, height: 120 }]),
        fixture!("review-state", ReviewState, vec![review_state()]),
        // events
        fixture!("event", Event, events()),
        fixture!("popover-tab-type", Popover, vec![popovers()[0].clone()]),
        fixture!("popover-tab-stop", Popover, vec![popovers()[1].clone()]),
        fixture!("popover-autocorrect", Popover, vec![popovers()[2].clone()]),
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
        fixture!("element-info", ElementInfo, vec![element_info(), reference_info()]),
        fixture!(
            "sub-template-info",
            SubTemplateInfo,
            vec![SubTemplateInfo {
                name: "Notice".into(),
                category: "Sub-templates".into(),
                references: 2,
                nested_references: 1,
                elements: vec![reference_info()]
            }]
        ),
        fixture!("element-detail", ElementDetail, vec![element_detail()]),
        fixture!("block", Block, blocks()),
        fixture!("data-part", DataPart, vec![data_part()]),
        fixture!("data-part-info", DataPartInfo, vec![DataPartInfo { namespace: Some("urn:invoice".into()), item_id: "{6A9C}".into() }]),
        fixture!("picture-control-activation", PictureControlActivation, vec![PictureControlActivation { element: id("control-3") }]),
        fixture!("accessible-node", AccessibleNode, vec![accessible()]),
        fixture!("accessible-id", AccessibleId, vec![AccessibleId(42)]),
        fixture!("cell-place", CellPlace, vec![CellPlace { row: 2, col: 3, row_span: 2, col_span: 1 }]),
        fixture!("list-position", ListPosition, vec![ListPosition { index: 2, count: 5 }]),
        fixture!("a11y-update", A11yUpdate, a11y_updates()),
        fixture!("caret-window", CaretWindow, vec![caret_window()]),
        fixture!("window-point", WindowPoint, vec![WindowPoint { paragraph: 1, offset: 12 }]),
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
        fixture!(
            "font-request",
            FontRequest,
            vec![
                FontRequest { family: "Calibri".into(), bold: false, italic: true, description: None, proposed: None },
                FontRequest {
                    family: "Garamond".into(),
                    bold: true,
                    italic: false,
                    description: Some(font_description()),
                    proposed: Some(GenericFamily::Serif)
                },
            ]
        ),
        fixture!("font-description", FontDescription, vec![font_description(), FontDescription::default()]),
        fixture!(
            "font-answer",
            FontAnswer,
            vec![
                FontAnswer::Font,
                FontAnswer::Family { family: "Arimo".into() },
                FontAnswer::Category { category: GenericFamily::Serif },
                FontAnswer::Missing,
                FontAnswer::Failed { message: "offline".into() },
            ]
        ),
        fixture!("font-rule", FontRule, vec![substitution(), FontRule { from: "Arial".into(), bold: None, italic: None, to: "Arimo".into() }]),
        fixture!(
            "missing-font",
            MissingFont,
            vec![
                missing_font(),
                MissingFont {
                    family: "Brand Sans".into(),
                    bold: false,
                    italic: true,
                    tried: Vec::new(),
                    category: None,
                    drawn_with: None,
                    by: StandIn::LastResort,
                    error: Some("offline".into()),
                },
            ]
        ),
        fixture!("missing-characters", MissingCharacters, vec![missing_characters()]),
        fixture!(
            "alias",
            Alias,
            vec![
                Alias { from: "Arial".into(), bold: None, italic: None, to: AliasTarget::Family("Arimo".into()) },
                Alias {
                    from: "Garamond".into(),
                    bold: Some(true),
                    italic: Some(false),
                    to: AliasTarget::Category { category: GenericFamily::Serif }
                },
            ]
        ),
        fixture!(
            "generic-defaults",
            GenericDefaults,
            vec![
                GenericDefaults::default(),
                GenericDefaults {
                    serif: Some("Tinos".into()),
                    sans_serif: Some("Arimo".into()),
                    monospace: Some("Cousine".into()),
                    script: Some("Dancing Script".into()),
                    decorative: Some("Lobster".into()),
                },
            ]
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
        fixture!(
            "export-options",
            ExportOptions,
            vec![
                ExportOptions::Docx,
                ExportOptions::Pdf(PdfOptions::default()),
                ExportOptions::Pdf(PdfOptions { open_password: Some("secret".into()), keywords: vec!["a".into(), "b".into()] }),
            ]
        ),
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
            ]
            .into_iter()
            .chain(a11y_updates().into_iter().map(|update| HostRequest::A11y { update }))
            .chain([HostRequest::CaretText { window: caret_window() }])
            .collect()
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
        ControlOptions::Picture(PictureOptions { width: Some(Points(216.0)), height: Some(Points(54.0)) }),
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
        Command::UpdateStyleToSelection(style("Heading1")),
        Command::DeleteStyle(style("Callout")),
        Command::SetStyleMeta(StyleMetaChange {
            id: style("Draft"),
            visibility: Some(StyleVisibility::HideUntilUsed),
            priority: Some(50),
            quick: Some(false),
            locked: Some(true),
        }),
        Command::SetDocumentDefaults(DocumentDefaults {
            font: Some("Calibri".into()),
            size: Some(Points(11.0)),
            color: Some(AutoOr::Auto),
            spacing: Some(ParagraphSpacing { before: Some(AutoOr::Value(Points(0.0))), after: Some(AutoOr::Value(Points(8.0))) }),
            line_spacing: Some(LineSpacing::Multiple { lines: 1.08 }),
            indents: None,
        }),
        Command::SetList(Some(ListKind::Numbered)),
        Command::SetListLevel(LevelChange::Increase),
        Command::SetListDefinition(ListDefinitionChange {
            levels: vec![ListLevel {
                format: NumberFormat::UpperRoman,
                text: "Article %1.".into(),
                start: 1,
                alignment: LevelAlignment::Right,
                follow: FollowNumber::Space,
                tab: Some(Points(36.5)),
                number_position: Points(0.5),
                text_indent: Points(18.5),
                restart_after: Some(0),
                legal: true,
                linked_style: Some(style("Heading1")),
                font: Some("Wingdings".into()),
                picture: Some(Bytes(vec![0x89, 0x50, 0x4E, 0x47])),
            }],
            apply_to: ListApplyTo::ThisPointForward,
            name: Some("Legal".into()),
        }),
        Command::SetNumberingValue(NumberingValue { mode: NumberingMode::Continue, value: 7, advance: true }),
        Command::ApplyListEntry(ListEntryId("library:bullet:1".into())),
        Command::RestartNumbering,
        Command::ContinueNumbering,
        Command::AdjustListIndents(ListIndents { number_position: Points(0.5), text_indent: Points(18.5), follow: FollowNumber::Nothing, tab: None }),
        Command::SetParagraphFlow(ParagraphFlow {
            keep_with_next: Some(true),
            keep_lines_together: Some(false),
            page_break_before: Some(true),
            widow_control: Some(true),
        }),
        Command::SetParagraphBorders(BorderPreset::Box),
        Command::SetParagraphShading(Some(Color(0xD9E2F3))),
        Command::SetParagraphBorderLines(ParagraphBorderLines {
            top: Some(Some(Border { style: BorderStyle::ThinThickSmallGap, width: Points(1.5), color: AutoOr::Value(Color(0x1F3864)) })),
            bottom: Some(None),
            left: Some(None),
            right: Some(None),
            between: Some(None),
            shadow: Some(false),
        }),
        Command::SetShading(ShadingChange {
            target: ShadingTarget::Text,
            fill: NoneOr::Value(Color(0xFFF2CC)),
            pattern: ShadingPattern::Pct20,
            color: AutoOr::Auto,
        }),
        Command::SetTextBorder(TextBorderChange {
            border: Some(Border { style: BorderStyle::Double, width: Points(0.75), color: AutoOr::Value(Color(0xC00000)) }),
            shadow: true,
        }),
        Command::SetPageBorders(PageBordersChange {
            top: Some(Some(Border { style: BorderStyle::Wave, width: Points(0.75), color: AutoOr::Value(Color(0x7030A0)) })),
            bottom: Some(None),
            left: Some(None),
            right: Some(None),
            shadow: Some(true),
            offset_from: Some(PageBorderOffset::Text),
            display: Some(PageBorderDisplay::NotFirstPage),
            apply_to: PageBorderScope::WholeDocument,
        }),
        Command::SetFrame(FrameProperties {
            wrap: FrameWrap::None,
            width_rule: FrameWidthRule::Exactly,
            width: Points(144.5),
            height_rule: FrameHeightRule::AtLeast,
            height: Points(36.5),
            horizontal_align: Some(FrameHorizontalAlign::Center),
            horizontal: Points(0.5),
            horizontal_relative: FrameHorizontalRelative::Page,
            horizontal_distance: Points(9.5),
            vertical_align: None,
            vertical: Points(72.5),
            vertical_relative: FrameVerticalRelative::Margin,
            vertical_distance: Points(0.5),
            lock_anchor: true,
        }),
        Command::RemoveFrame,
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
        Command::InsertShape(dr::ShapeInsert { preset: preset("star5"), width: Some(Points(90.5)), height: None }),
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
        Command::SetTablePosition(Some(table_position())),
        Command::SetObjectSize(ObjectSize { width: Some(Points(144.5)), height: Some(Points(72.5)), lock_aspect: Some(true) }),
        Command::ResetObjectSize,
        Command::SetObjectWrap(ObjectWrap::Square),
        Command::SetAnchorLocked(true),
        Command::SetObjectPosition(ObjectPosition {
            horizontal: Some(HorizontalPosition {
                relative_to: HorizontalReference::Margin,
                align: Some(ObjectHorizontalAlignment::Right),
                offset: None,
            }),
            vertical: Some(VerticalPosition { relative_to: VerticalReference::Paragraph, align: None, offset: Some(Points(12.5)) }),
            allow_overlap: Some(false),
            layout_in_cell: Some(true),
        }),
        Command::SetObjectWrapOptions(ObjectWrapOptions {
            side: Some(WrapSide::Largest),
            top: Some(Points(0.5)),
            bottom: Some(Points(0.5)),
            left: Some(Points(9.5)),
            right: Some(Points(9.5)),
        }),
        Command::SetAltText(AltText { text: "A chart".into(), decorative: Some(false) }),
        Command::DeleteObject,
        Command::SetShapePreset(preset("roundRect")),
        Command::SetShapeAdjust(dr::ShapeAdjust { index: 0, value: 25000 }),
        Command::SetShapeFill(dr::FillValue::Solid { colour: theme_colour() }),
        Command::SetShapeLine(line_change()),
        Command::SetShapeEffects(effects_change()),
        Command::SetShapeText(text_change()),
        Command::SetPictureAdjust(picture_adjust()),
        Command::SetObjectRotation(45.5),
        Command::FlipObject(dr::FlipAxis::Horizontal),
        Command::ArrangeObject(dr::Arrange::ToFront),
        Command::GroupObjects,
        Command::UngroupObjects,
        Command::SelectGroupChild(Order::Next),
        Command::SetField(SetField { id: id("field-1"), code: Some("MERGEFIELD Total".into()), display: Some(Body::Text("«Total»".into())) }),
        Command::SetFieldResults(FieldResults { results: vec![FieldResult { id: id("field-1"), display: Body::Rich(rich()) }], token: Some(4) }),
        Command::UpdateFields(ElementIds { ids: ids() }),
        Command::SetFieldsLocked(FieldsLocked { locked: true, ids: ids() }),
        Command::UnlinkFields(ElementIds { ids: None }),
        Command::ShowFieldCodes(ShowFieldCodes { show: true, ids: None }),
        Command::SetFieldLabels(FieldLabels { labels: BTreeMap::from([("MERGEFIELD Name".into(), "Customer name".into())]), merge: Some(true) }),
        Command::SetSubTemplateRules(SubTemplateRules {
            rules: vec![ReferenceRule {
                field_type: "SUBTEMPLATE".into(),
                argument: Some(0),
                category: "Sub-templates".into(),
                gallery: Some("custom1".into()),
            }],
        }),
        Command::SetApplicationFieldTypes(ApplicationFieldTypes { types: vec!["CUSTOMVALUE".into(), "CustomText".into()] }),
        Command::SetSubTemplateView(SubTemplateView { ids: Some(vec![id("field-7")]), expanded: false }),
        Command::EditSubTemplate(SubTemplateName { name: "Violation".into(), category: Some("Sub-templates".into()) }),
        Command::CloseSubTemplate,
        Command::CreateSubTemplate(CreateSubTemplate {
            name: "Payment".into(),
            category: Some("Sub-templates".into()),
            from: None,
            copy_of: Some("Fees".into()),
        }),
        Command::RenameSubTemplate(RenameSubTemplate {
            name: "Violation".into(),
            to: "Citation".into(),
            category: None,
            update_references: Some(true),
        }),
        Command::RemoveSubTemplate(SubTemplateName { name: "Cycle-B".into(), category: None }),
        Command::ReplaceBody(ReplaceBody { from: Bytes(b"PK".to_vec()) }),
        Command::InsertControl(InsertControl {
            kind: ControlKind::DropDown,
            tag: Some("status".into()),
            title: Some("Status".into()),
            lock: Some(ControlLock::Control),
            placeholder: Some("Choose".into()),
            binding: Some(binding()),
            options: Some(ControlOptions::List(ListOptions { items: vec![ListItem { text: "Open".into(), value: "open".into() }] })),
            placeholder_image: None,
            wrap_selection: Some(false),
            float: None,
        }),
        Command::SetControl(SetControl {
            id: id("control-3"),
            tag: Some("status".into()),
            title: None,
            lock: Some(ControlLock::None),
            placeholder: None,
            binding: Some(None),
            options: Some(ControlOptions::Checkbox(CheckboxOptions { checked: Some(false) })),
            placeholder_image: Some(PlaceholderImage { data: picture_bytes(), mime: "image/png".into() }),
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
        Command::ReplaceFont(ReplaceFont {
            from: "Calibri".into(),
            to: "Aptos".into(),
            scope: ReplaceScope::Selection,
            bold: Some(true),
            italic: None,
        }),
        Command::MarkElements(MarkElements {
            marks: vec![ElementMark { id: id("field-1"), severity: Severity::Warning, message: "Unknown field".into() }],
            clear: Some(true),
        }),
        Command::SetProtectedPictureControls(ProtectedPictureControls { tag_prefixes: vec!["Placeholder:".into()] }),
        Command::ActivatePictureControl(ActivatePictureControl { element: Some(id("control-3")) }),
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
        Command::SetViewOptions(ViewOptions {
            formatting_marks: Some(true),
            gridlines: Some(false),
            text_boundaries: Some(true),
            live_layout: Some(false),
            theme: Some(ChromeTheme::Dark),
            theme_colors: Some(ThemeColors {
                light: None,
                dark: Some(ChromeColors {
                    desk: Some(Color(0x1B1F27)),
                    surface: Some(Color(0x252A33)),
                    text: Some(Color(0xE6EDF3)),
                    ..ChromeColors::default()
                }),
            }),
            accent: Some(Color(0x0A74E4)),
            appearance: Some(DocumentAppearance::DarkWithLightPage),
            detail: Some(MeasureDetail::Full),
            object_grid: Some(ObjectGrid::Always),
            field_shading: Some(FieldShading::Always),
            cursor_overrides: Some(vec![
                CursorOverride { on: CursorTarget::Action(PressAction::Toggle), cursor: Cursor::Pointer },
                CursorOverride { on: CursorTarget::Kind(ElementKind::DropDown), cursor: Cursor::Default },
            ]),
        }),
        Command::SetMode(Mode::Design),
        Command::SetRulerTabType(TabSelector::Decimal),
        Command::ShowTabTypes,
        Command::ShowTabStop,
        Command::SetAutoCorrect(AutoCorrectChange {
            smart_quotes: Some(false),
            entries: Some(vec![AutoCorrectEntry { from: "adn".into(), to: "and".into() }]),
            ..AutoCorrectChange::default()
        }),
        Command::RevertAutoCorrect,
        Command::NeverAutoCorrect,
        Command::ShowAutoCorrectOptions,
        Command::ClosePopover,
        Command::DescribeFormatting,
    ]
}

fn popovers() -> [Popover; 3] {
    let anchor = ClientRect { left: 120.5, top: 10.0, width: 14.0, height: 14.0 };
    let choice = |value, name: &str, description: &str| TabTypeChoice { value, name: name.into(), description: description.into() };
    [
        Popover::TabType {
            anchor: ClientRect { left: 0.0, top: 3.0, width: 18.0, height: 18.0 },
            current: TabSelector::Left,
            choices: vec![
                choice(TabSelector::Left, "Left", "Text starts at the stop"),
                choice(TabSelector::Decimal, "Decimal", "Numbers line up on the decimal point"),
            ],
        },
        Popover::TabStop { anchor, position: Points(198.5), alignment: TabAlignment::Right, leader: TabLeader::Dots },
        Popover::AutoCorrect { anchor, from: "(c)".into(), to: "©".into() },
    ]
}

fn command_results() -> Vec<CommandResult> {
    vec![
        CommandResult::Element { id: id("field-1") },
        CommandResult::DataPart { item_id: "{6A9C}".into() },
        CommandResult::Changed { changed: 4 },
        CommandResult::FontReplaced { changed: 12, skipped: 1 },
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
        CommandValue::TabType(TabSelector::Decimal),
        CommandValue::TablePosition(table_position()),
    ]
}

fn table_position() -> TablePosition {
    TablePosition {
        horizontal: Some(HorizontalPosition { relative_to: HorizontalReference::Margin, align: None, offset: Some(Points(72.5)) }),
        vertical: Some(VerticalPosition { relative_to: VerticalReference::Paragraph, align: None, offset: Some(Points(36.5)) }),
        wrap_distance: Some(TextDistance { top: Points(0.0), bottom: Points(0.0), left: Points(9.0), right: Points(9.0) }),
    }
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
            placement: Some(ObjectPlacement {
                horizontal: HorizontalPosition { relative_to: HorizontalReference::Column, align: None, offset: Some(Points(36.5)) },
                vertical: VerticalPosition { relative_to: VerticalReference::Page, align: Some(ObjectVerticalAlignment::Top), offset: None },
                allow_overlap: true,
                layout_in_cell: true,
                wrap_side: WrapSide::BothSides,
                distance: TextDistance { top: Points(3.5), bottom: Points(3.5), left: Points(9.5), right: Points(9.5) },
            }),
            shape: Some(shape_state()),
            rotation: 30.5,
            flip_h: true,
            flip_v: false,
            child_path: vec![1],
            count: 1,
        }),
        context: vec![ElementContext {
            id: id("field-1"),
            kind: ElementKind::Field,
            exact: true,
            code: Some("MERGEFIELD Name".into()),
            tag: Some("name".into()),
            title: Some("Name".into()),
            size: Some(Size { width: Points(216.0), height: Points(54.0) }),
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
        editing: crate::api::state::EditingTarget::SubTemplate { name: "Violation".into(), category: "Sub-templates".into() },
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
        fonts: fonts_state(),
        ruler: ruler(),
        format_painter: false,
        theme_colours: vec![Color(0x000000), Color(0xFFFFFF)],
        auto_correct: AutoCorrectOptions { never: vec!["teh".into()], ..AutoCorrectOptions::default() },
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
        MenuAction::Command { command: Box::new(Command::Cut) },
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
            action: MenuAction::Command { command: Box::new(Command::Cut) },
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
        DialogValues::Style(StyleDialog {
            open: StyleOpen::Modify(style("Heading1")),
            short_form: false,
            name: "Heading 1".into(),
            kind: StyleDialogType::Linked,
            based_on: Some(style("Normal")),
            next: Some(style("Normal")),
            font: "Calibri Light".into(),
            size: Points(16.0),
            bold: true,
            italic: false,
            underline: UnderlineStyle::None,
            color: AutoOr::Value(Color(0x2F5496)),
            alignment: Alignment::Left,
            line_spacing: LineSpacing::Multiple { lines: 1.08 },
            space_before: Points(12.0),
            space_after: Points(0.0),
            left_indent: Points(0.0),
            quick: true,
            auto_update: false,
            table_part: TableStylePart::HeaderRow,
            fill: Some(Color(0xD9E2F3)),
            cell_alignment: CellAlignment::Center,
            border: Some(Border { style: BorderStyle::Single, width: Points(0.5), color: AutoOr::Auto }),
            row_band: 1,
            column_band: 2,
            list_levels: Vec::new(),
            list_level: 0,
            description: "Font: 16 pt, Bold".into(),
            pending: "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/></w:style>".into(),
        }),
        DialogValues::Style(StyleDialog { open: StyleOpen::Create, short_form: true, name: "Style1".into(), ..StyleDialog::default() }),
        DialogValues::BordersShading(BordersShadingDialog {
            tab: BordersTab::Shading,
            setting: BorderSetting::Shadow,
            style: BorderStyle::Double,
            color: AutoOr::Value(Color(0x2F5496)),
            width: Points(0.75),
            top: Some(Border { style: BorderStyle::Double, width: Points(0.75), color: AutoOr::Value(Color(0x2F5496)) }),
            bottom: None,
            left: None,
            right: None,
            between: None,
            inside_vertical: None,
            apply_to: ShadingTarget::Paragraph,
            fill: NoneOr::None,
            pattern: ShadingPattern::Solid,
            pattern_color: AutoOr::Auto,
            page_setting: BorderSetting::Box,
            page_top: Some(Border { style: BorderStyle::Triple, width: Points(0.5), color: AutoOr::Auto }),
            page_bottom: None,
            page_left: None,
            page_right: None,
            page_offset_from: PageBorderOffset::Text,
            page_display: PageBorderDisplay::FirstPage,
            page_apply_to: PageBorderScope::ThisSection,
        }),
        DialogValues::DefineBullet(DefineBulletDialog {
            text: "\u{F0B7}".into(),
            font: Some("Symbol".into()),
            picture: None,
            alignment: LevelAlignment::Center,
        }),
        DialogValues::DefineNumberFormat(DefineNumberFormatDialog {
            format: NumberFormat::LowerLetter,
            text: "%1)".into(),
            font: None,
            alignment: LevelAlignment::Left,
        }),
        DialogValues::DefineMultilevelList(DefineMultilevelDialog {
            levels: vec![ListLevel {
                format: NumberFormat::UpperRoman,
                text: "Article %1.".into(),
                start: 1,
                alignment: LevelAlignment::Right,
                follow: FollowNumber::Space,
                tab: Some(Points(36.5)),
                number_position: Points(0.5),
                text_indent: Points(18.5),
                restart_after: Some(0),
                legal: true,
                linked_style: Some(style("Heading1")),
                font: Some("Wingdings".into()),
                picture: Some(Bytes(vec![0x89, 0x50, 0x4E, 0x47])),
            }],
            level: 0,
            apply_to: ListApplyTo::CurrentParagraph,
            gallery_level: 1,
            list_name: "Legal".into(),
        }),
        DialogValues::SetNumberingValue(SetNumberingValueDialog { mode: NumberingMode::StartNew, value: 3, advance: false }),
        DialogValues::AdjustListIndents(AdjustListIndentsDialog {
            number_position: Points(18.5),
            text_indent: Points(36.5),
            follow: FollowNumber::Tab,
            tab: Some(Points(36.5)),
        }),
        DialogValues::BulletsNumbering(BulletsNumberingDialog {
            gallery: ListGalleryKind::Multilevel,
            entry: Some(ListEntryId("document:4".into())),
        }),
        DialogValues::Banding(BandingDialog { row_band: 2, column_band: 1 }),
        DialogValues::Frame(FrameDialog {
            wrap: FrameWrap::Around,
            width_rule: FrameWidthRule::Auto,
            height_rule: FrameHeightRule::Exactly,
            height: Points(50.5),
            vertical_align: Some(FrameVerticalAlign::Top),
            vertical_relative: FrameVerticalRelative::Paragraph,
            move_with_text: true,
            horizontal_distance: Points(9.5),
            ..FrameDialog::default()
        }),
        DialogValues::ManageStyles(ManageStylesDialog {
            tab: ManageStylesTab::Recommend,
            sort: StyleSort::Alphabetical,
            recommended_only: true,
            styles: vec![ManagedStyle {
                id: style("Draft"),
                name: "Draft".into(),
                kind: StyleType::Paragraph,
                built_in: false,
                visibility: StyleVisibility::HideUntilUsed,
                priority: 50,
                locked: false,
            }],
            selected: Some(style("Draft")),
            description: "Font color: Custom Color(RGB(127,127,127))".into(),
            defaults: DocumentDefaults { font: Some("Calibri".into()), size: Some(Points(11.0)), ..DocumentDefaults::default() },
        }),
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
        on_style: None,
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
        aliases: vec!["H1".into()],
        link: Some(style("Heading1Char")),
        visibility: StyleVisibility::HideUntilUsed,
        locked: false,
    }
}

fn element_filter() -> ElementFilter {
    ElementFilter {
        kind: Some(ElementKind::PlainText),
        matches: Some(ElementMatch { field: ElementField::Tag, text: "name".into() }),
        story: Some(Story::SubTemplate),
        sub_template: Some("Violation".into()),
    }
}

fn reference_info() -> ElementInfo {
    ElementInfo {
        id: id("field-7"),
        kind: ElementKind::Field,
        code: Some("SUBTEMPLATE Violation".into()),
        story: Story::SubTemplate,
        sub_template: Some("Notice".into()),
        reference: Some(SubTemplateReference {
            name: "violation".into(),
            resolution: ReferenceResolution::NearMatch,
            near_match: Some("Violation".into()),
            expanded: true,
        }),
        ..ElementInfo::default()
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
        sub_template: None,
        parent: Some(id("control-1")),
        locked: false,
        showing_placeholder: Some(true),
        reference: None,
        size: Some(Size { width: Points(216.0), height: Points(54.0) }),
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
        sub_template: None,
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
    let node = |id, role| AccessibleNode { id: AccessibleId(id), role, ..AccessibleNode::default() };
    let text = |id, text: &str| AccessibleNode { text: Some(text.into()), ..node(id, AccessibleRole::Text) };
    AccessibleNode {
        name: Some("Contract".into()),
        lang: Some("en-US".into()),
        children: vec![
            AccessibleNode { level: Some(1), children: vec![text(11, "Terms")], ..node(1, AccessibleRole::Heading) },
            AccessibleNode {
                children: vec![AccessibleNode {
                    level: Some(1),
                    label: Some("1.".into()),
                    position: Some(ListPosition { index: 1, count: 2 }),
                    children: vec![
                        AccessibleNode {
                            marks: vec![AccessibleMark::Insertion, AccessibleMark::Misspelling],
                            author: Some("Ada".into()),
                            lang: Some("fr-FR".into()),
                            details: Some(AccessibleId(1 << 52 | 5)),
                            ..text(21, "Payment")
                        },
                        AccessibleNode {
                            name: Some("Footnote 1".into()),
                            target: Some(AccessibleId(1 << 52 | 7)),
                            ..node(22, AccessibleRole::NoteRef)
                        },
                    ],
                    ..node(2, AccessibleRole::ListItem)
                }],
                ..node(3, AccessibleRole::List)
            },
            AccessibleNode {
                name: Some("Table".into()),
                level: Some(1),
                children: vec![AccessibleNode {
                    children: vec![AccessibleNode {
                        cell: Some(CellPlace { row: 1, col: 1, row_span: 1, col_span: 2 }),
                        ..node(6, AccessibleRole::ColumnHeader)
                    }],
                    ..node(5, AccessibleRole::Row)
                }],
                ..node(4, AccessibleRole::Table)
            },
        ],
        ..node(0, AccessibleRole::Document)
    }
}

fn caret_window() -> CaretWindow {
    let segment = |text: &str| WindowSegment { text: text.into(), ..WindowSegment::default() };
    CaretWindow {
        paragraphs: vec![
            WindowParagraph {
                id: AccessibleId(4),
                role: AccessibleRole::Heading,
                level: Some(1),
                label: None,
                lines: vec![WindowLine { segments: vec![segment("Terms")] }],
            },
            WindowParagraph {
                id: AccessibleId(5),
                role: AccessibleRole::ListItem,
                level: None,
                label: Some("1.".into()),
                lines: vec![
                    WindowLine {
                        segments: vec![
                            segment("The tenant pays "),
                            WindowSegment { text: "monthly".into(), marks: vec![AccessibleMark::Insertion], author: Some("Ada".into()), lang: None },
                        ],
                    },
                    WindowLine {
                        segments: vec![WindowSegment { text: "à la date".into(), lang: Some("fr-FR".into()), ..WindowSegment::default() }]
                    },
                ],
            },
        ],
        anchor: WindowPoint { paragraph: 1, offset: 4 },
        focus: WindowPoint { paragraph: 1, offset: 10 },
        all: false,
        lang: "en-US".into(),
        read_only: false,
    }
}

fn a11y_updates() -> Vec<A11yUpdate> {
    let paragraph = AccessibleNode { id: AccessibleId(9), role: AccessibleRole::Paragraph, ..AccessibleNode::default() };
    vec![
        A11yUpdate::Full(Box::new(accessible())),
        A11yUpdate::Patch(vec![
            A11yChange::Replace { node: Box::new(paragraph) },
            A11yChange::Children { parent: AccessibleId(0), ids: vec![AccessibleId(1), AccessibleId(9), AccessibleId(3)] },
        ]),
    ]
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

fn hovered() -> ElementContext {
    ElementContext {
        id: id("control-3"),
        kind: ElementKind::Checkbox,
        exact: false,
        code: None,
        tag: Some("agree".into()),
        title: Some("Agree".into()),
        size: None,
    }
}

fn word_count() -> WordCount {
    WordCount { words: 120, characters: 640, characters_with_spaces: 760, paragraphs: 9, lines: 22, pages: 2 }
}

fn font_description() -> FontDescription {
    FontDescription {
        class: Some(FontClass::Roman),
        pitch: Some(FontPitch::Variable),
        panose: Some([2, 2, 4, 4, 3, 3, 1, 1, 8, 3]),
        charset: Some(0),
    }
}

fn missing_font() -> MissingFont {
    MissingFont {
        family: "Garamond".into(),
        bold: true,
        italic: false,
        tried: vec!["EB Garamond".into()],
        category: Some(GenericFamily::Serif),
        drawn_with: Some("Tinos".into()),
        by: StandIn::Generic,
        error: None,
    }
}

fn missing_characters() -> MissingCharacters {
    MissingCharacters { family: "Calibri".into(), characters: "\u{6f22}\u{2603}".into(), truncated: false }
}

fn fonts_state() -> FontsState {
    let pending = MissingFont {
        family: "Aptos".into(),
        bold: false,
        italic: false,
        tried: Vec::new(),
        category: None,
        drawn_with: Some("Carlito".into()),
        by: StandIn::Fallback,
        error: None,
    };
    FontsState { pending: vec![pending], missing: vec![missing_font()], characters: vec![missing_characters()] }
}

fn substitution() -> FontRule {
    FontRule { from: "Times New Roman".into(), bold: Some(true), italic: Some(false), to: "Tinos".into() }
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
        Event::PictureControlActivated(PictureControlActivation { element: id("control-3") }),
        Event::ContextMenu(ContextMenuEvent { menu: menu(), at: at(), via: MenuVia::Pointer }),
        Event::PopoverRequested(popovers()[1].clone()),
        Event::PopoverClosed,
        Event::DialogRequested { kind: DialogKind::Tabs },
        Event::HostCommand { command: HostCommand::Save },
        Event::Shortcut { id: "insertSignature".into() },
        Event::Gesture(gesture_event()),
        Event::WordAdded { word: "Twip".into(), language: "en-US".into() },
        Event::FontsChanged(fonts_state()),
        Event::ElementActivated { id: id("field-1") },
        Event::ElementsChanged { added: vec![element_info()], removed: vec![id("field-2")], changed: Vec::new() },
        Event::FieldsUpdateRequested { token: 4, fields: vec![FieldRequest { id: id("field-1"), code: "DOCPROPERTY Client".into() }] },
        Event::RevisionsChanged {},
        Event::CommentsChanged {},
        Event::CommentAnchors(vec![anchor()]),
        Event::BalloonsLaidOut(vec![balloon()]),
        Event::RulerChanged(ruler_view()),
        Event::EditingTargetChanged(crate::api::state::EditingTarget::SubTemplate { name: "Violation".into(), category: "Sub-templates".into() }),
        Event::EditingTargetChanged(crate::api::state::EditingTarget::Template),
        Event::EditRefused { reason: RefusalReason::SubTemplate { name: "Violation".into() }, element: Some(id("field-7")) },
        Event::EditRefused { reason: RefusalReason::Locked, element: None },
        Event::Tooltip(Some(tooltip())),
        Event::Tooltip(None),
        Event::ElementHover(Some(ElementHover { element: hovered(), boxes: vec![rect(), rect()], action: PressAction::Toggle })),
        Event::ElementHover(None),
        Event::ElementClick(ElementClick {
            element: hovered(),
            count: 2,
            button: PointerButton::Primary,
            modifiers: Modifiers { ctrl: true, shift: false, alt: false, meta: false },
        }),
        Event::ElementSelect(Some(ElementSelection { element: ElementContext { exact: true, ..hovered() }, boxes: vec![rect()] })),
        Event::ElementSelect(None),
        Event::Announcement { text: "Bold on".into(), kind: AnnouncementKind::Formatting },
        Event::Warning(warning()),
        Event::fault(Error::new(ErrorCode::Internal, "the engine failed")),
    ]
}

fn preset(name: &str) -> dr::ShapePreset {
    dr::ShapePreset::from_name(name).expect("a preset")
}

fn theme_colour() -> dr::ColourValue {
    dr::ColourValue {
        transforms: vec![dr::ColourTransform { kind: dr::TransformKind::LumMod, value: 0.75 }],
        ..dr::ColourValue::theme(dr::ThemeColour::Accent1)
    }
}

fn fills() -> Vec<dr::FillValue> {
    vec![
        dr::FillValue::None,
        dr::FillValue::Solid { colour: theme_colour() },
        dr::FillValue::Gradient {
            kind: dr::GradientShape::Linear,
            angle: Some(90.5),
            stops: vec![
                dr::GradientStop { position: 0.0, colour: theme_colour() },
                dr::GradientStop { position: 1.0, colour: dr::ColourValue::rgb(Color(0xFFFFFF)) },
            ],
        },
        dr::FillValue::Pattern { preset: "dkDnDiag".into(), foreground: theme_colour(), background: dr::ColourValue::rgb(Color(0xFFFFFF)) },
        dr::FillValue::Picture { image: picture_bytes(), tile: Some(true) },
        dr::FillValue::FromStyle,
    ]
}

fn arrowhead() -> dr::Arrowhead {
    dr::Arrowhead { kind: dr::ArrowKind::Stealth, width: dr::ArrowSize::Large, length: dr::ArrowSize::Small }
}

fn line_ends() -> dr::LineEnds {
    dr::LineEnds { head: None, tail: Some(arrowhead()) }
}

fn line_value() -> dr::LineValue {
    dr::LineValue {
        width: Points(1.5),
        fill: dr::FillValue::Solid { colour: theme_colour() },
        dash: "dash".into(),
        compound: dr::Compound::Double,
        cap: dr::LineCap::Round,
        join: dr::LineJoin::Miter,
        ends: line_ends(),
    }
}

fn line_change() -> dr::LineChange {
    dr::LineChange { width: Some(Points(2.5)), dash: Some("sysDot".into()), ends: Some(line_ends()), ..Default::default() }
}

fn shadow() -> dr::Shadow {
    dr::Shadow { colour: dr::ColourValue::rgb(Color(0x000000)), blur: Points(4.5), distance: Points(3.5), direction: 45.5 }
}

fn glow() -> dr::Glow {
    dr::Glow { colour: theme_colour(), radius: Points(5.5) }
}

fn effects() -> dr::EffectsValue {
    dr::EffectsValue { outer_shadow: Some(shadow()), glow: Some(glow()), soft_edge: Some(Points(2.5)), ..Default::default() }
}

fn effects_change() -> dr::EffectsChange {
    dr::EffectsChange { outer_shadow: Some(Some(shadow())), glow: Some(None), ..Default::default() }
}

fn text_state() -> dr::ShapeTextState {
    dr::ShapeTextState {
        insets: [Points(7.2), Points(3.6), Points(7.2), Points(3.6)],
        anchor: dr::TextAnchor::Middle,
        autofit: dr::TextAutofit::Shape,
        wrap_text: true,
        direction: dr::TextDirection::Vertical270,
    }
}

fn text_change() -> dr::ShapeTextChange {
    dr::ShapeTextChange { anchor: Some(dr::TextAnchor::Bottom), wrap_text: Some(false), ..Default::default() }
}

fn picture_adjust() -> dr::PictureAdjustChange {
    dr::PictureAdjustChange { transparency: Some(0.25), grayscale: Some(true), brightness: None, contrast: Some(-0.5) }
}

fn shape_state() -> dr::ShapeState {
    dr::ShapeState {
        preset: Some(preset("wedgeRectCallout")),
        adjust: vec![-20833, 62500],
        fill: dr::FillState::FromStyle(dr::FillValue::Solid { colour: theme_colour() }),
        line: dr::LineState::Explicit(line_value()),
        effects: effects(),
        text: Some(text_state()),
    }
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
