//! The structured calls: one number per `Session` method (and the engine's font library), each with its JSON
//! request and response, the `api` types' serde form.

use serde::Deserialize;
use serde_json::Value;

use super::{Message, TwipEngine, kind, request, to_value};
use engine::api::command::Command;
use engine::api::engine::Dictionary;
use engine::api::error::Error;
use engine::api::fonts::{Alias, FontAnswer, FontRequest, FontRule, GenericDefaults, SymbolFallback};
use engine::api::input::{ClipboardData, Point};
use engine::api::record::Author;
use engine::api::review::CommentAnchor;
use engine::api::services::{
    DialogKind, DialogModel, DialogPatch, ElementFilter, FindDirection, FindQuery, GestureAction, GestureId, MenuAction, RulerUnit, Shortcut,
    StyleType, TabSelector,
};
use engine::api::session::Session;
use engine::api::state::Mode;
use engine::api::value::{Color, ElementId, HeadingId, MatchId};

macro_rules! methods {
    ($($(#[doc = $doc:literal])* $name:ident = $n:literal,)*) => {
        /// The structured call numbers. The request is the JSON shape named; no request means `null`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u32)]
        pub enum Method {
            $($(#[doc = $doc])* $name = $n,)*
        }

        impl Method {
            /// The method with this number.
            pub fn from_u32(n: u32) -> Option<Method> {
                match n {
                    $($n => Some(Method::$name),)*
                    _ => None,
                }
            }

            /// Every method, by number.
            pub const ALL: &'static [Method] = &[$(Method::$name,)*];
        }
    };
}

methods! {
    /// `Command` → `Commit`.
    Run = 1,
    /// `{ commands: Command[], label? }` → `Commit`.
    Batch = 2,
    /// → `State`.
    State = 3,
    /// → `CommandInfo[]` (any session).
    Catalog = 4,
    /// → `null`: a blank document.
    NewDocument = 5,
    /// `bool` → `null`.
    SetReadOnly = 6,
    /// `Mode` → `null`.
    SetMode = 7,
    /// `Author` → `null`.
    SetAuthor = 8,
    /// `{ synonyms? } | null` → `ContextMenu`: `synonyms` (the host thesaurus's words) adds the Synonyms submenu.
    ContextMenu = 9,
    /// `MenuAction` → `Commit`.
    Invoke = 10,
    /// `DialogKind` → `DialogModel`.
    DialogOpen = 11,
    /// `{ model, patch }` → `DialogModel`.
    DialogUpdate = 12,
    /// `DialogModel` → `Commit`.
    DialogApply = 13,
    /// `{ query, direction }` → `FindResult`.
    Find = 14,
    /// `FindQuery` → `FindMatches`.
    FindAll = 15,
    /// `MatchId` → `null`.
    SelectMatch = 16,
    /// `{ query, with, replaceFormat? }` → `FindResult`.
    Replace = 17,
    /// `{ query, with, replaceFormat? }` → `FindCount`.
    ReplaceAll = 18,
    /// `Dictionary` → `null`.
    SpellingAddDictionary = 19,
    /// `bool` → `null`.
    SpellingSetEnabled = 20,
    /// `string` (BCP 47) → `null`.
    SpellingSetLanguage = 21,
    /// `string` → `null`.
    SpellingIgnore = 22,
    /// `string` → `null`.
    SpellingAdd = 23,
    /// `{ language, words }` → `null`.
    SpellingLoadUserWords = 24,
    /// `{ word, language? }` → `string[]`.
    SpellingSuggestions = 25,
    /// → `Misspelling | null`.
    SpellingNext = 26,
    /// → `Heading[]`.
    Headings = 27,
    /// `HeadingId` → `null`.
    SelectHeading = 28,
    /// `StyleType | null` → `StyleInfo[]`.
    Styles = 29,
    /// → `RevisionInfo[]`.
    Revisions = 30,
    /// → `CommentThread[]`.
    Comments = 31,
    /// → `Person[]`.
    People = 32,
    /// `ElementFilter` → `ElementInfo[]`.
    Elements = 33,
    /// `ElementId` → `ElementDetail`.
    Element = 34,
    /// `{ root?, depth? }` → `Block[]`.
    Structure = 35,
    /// `{ id, reveal }` → `null`.
    SelectElement = 36,
    /// `{ namespace?, itemId? }` → `DataPart[]`.
    DataParts = 37,
    /// → `AccessibleNode`.
    AccessibleTree = 38,
    /// `Shortcut` → `null`.
    ShortcutsRegister = 39,
    /// `string` (the id) → `null`.
    ShortcutsUnregister = 40,
    /// `string` (keys) → `KeyBinding | null`.
    ShortcutsLookup = 41,
    /// → `GestureBinding[]`.
    GesturesBindings = 42,
    /// `{ gesture, action }` → `null`.
    GesturesSet = 43,
    /// `GestureId | null` → `null`.
    GesturesReset = 44,
    /// `WordCountOptions | null` → `WordCount`: the document's counts, or the selection's.
    WordCount = 45,
    /// `{ cut }` → `ClipboardData | null`.
    Copy = 46,
    /// `{ data, asText?, as? }` → `null`: `as` pastes that way (Paste Special), else as the paste options say.
    Paste = 47,
    /// → `string[]`: the families the library draws (any session).
    FontFamilies = 48,
    /// `FontRule[]` → `null` (any session): the draw-time replacements.
    FontSetReplacements = 49,
    /// `string | null` → `null` (any session): the general fallback family.
    FontSetFallback = 50,
    /// `{ request: FontRequest, answer: FontAnswer }` → `null` (any session): a resolver's answer.
    FontAnswer = 51,
    /// `Color` → `null`; `null` puts back the theme's desk.
    HostSetBackground = 52,
    /// `number` (CSS px) → `null`.
    HostSetMarkupPaneWidth = 53,
    /// `{ horizontal, vertical }` → `null`.
    HostSetRulers = 54,
    /// `RulerUnit` → `null`.
    HostSetRulerUnit = 55,
    /// `TabSelector` → `null`.
    HostSetTabSelector = 56,
    /// → `RulerView`.
    HostRulerView = 57,
    /// → `CommentAnchor[]`.
    HostCommentAnchors = 58,
    /// `Point` → `HitResult`.
    HostHitTest = 59,
    /// `Point` → `null`.
    HostDragOver = 60,
    /// → `null`.
    HostDragLeave = 61,
    /// `{ at, command }` → `Commit`.
    HostDrop = 62,
    /// → `null`.
    HostRevealSelection = 63,
    /// → `PageRect[]`.
    HostVisiblePages = 64,
    /// `{ anchors, heights, gap? }` → `number[]`: card tops (any session).
    StackBalloons = 65,
    /// `number` (a `twip_open_prepare` token) → `Warning[]`: the prepared document replaces the open one.
    OpenCommit = 66,
    /// `number` (a `twip_open_prepare` token) → `null`: the open document stays.
    OpenDiscard = 67,
    /// `string` (an accessibility issue's id, from the Accessibility dialog) → `null`.
    DialogSelectIssue = 68,
    /// `bool` → `null` (any session): whether the SDK has a resolver. While it does, a face layout lacks is a
    /// `font_request` callback, answered with `FontAnswer` (after `twip_font_add` for a font); without one it is unresolved
    /// at once.
    FontSetResolver = 69,
    /// `{ intervalMs, stopAfterMs }` (`CaretBlink`) → `null`.
    HostSetCaretBlink = 71,
    /// → `FrameStats`: what the frames have cost so far.
    HostFrameStats = 72,
    /// `OverlayPalette` (colours `#RRGGBB`) → `null`; `null` puts back the theme's.
    HostSetOverlayPalette = 73,
    /// `{ recordings }` (`PaintBudget`, bytes) → `null`.
    HostSetPaintBudget = 74,
    /// `Point[][]` (content CSS px) → `null`: the balloons' leader lines the engine draws.
    HostSetLeaders = 75,
    /// No session → `ShapeGalleryGroup[]`: Word's Insert > Shapes presets in its groups.
    ShapeGallery = 76,
    /// → `SubTemplateInfo[]`: the sub-templates the rules declare.
    SubTemplates = 77,
    /// `AccessibleId` → `null`: the caret to the start of the node's content.
    AccessibleGo = 78,
    /// `{ anchor, focus }` (`WindowPoint`s of the last caret window) → `null`: the screen reader moved the caret.
    HostCaretWindowSelect = 79,
    /// `AnnouncementKind[]` (the kinds the editor makes) → `null`.
    AnnouncementsSet = 80,
    /// `{ name, category? }` → `Bytes`: a standalone DOCX of the sub-template's body.
    SubTemplateExport = 81,
    /// `Alias[]` → `null` (any session): the aliases, for families the library doesn't have.
    FontSetAliases = 82,
    /// `GenericDefaults` → `null` (any session): the family per generic category, for missing families.
    FontSetGenericDefaults = 83,
    /// `string[]` → `null` (any session): the families searched for characters a run's font lacks.
    FontSetCharacterFallback = 84,
    /// `bool` → `null` (any session): whether a document's font table gives missing families their category.
    FontSetClassification = 85,
    /// `"bundled" | "off"` → `null` (any session): the symbol fallback; a font of the application's goes through
    /// `twip_font_symbol_fallback`.
    FontSetSymbolFallback = 86,
    /// `ElementId` → `ClientRect[]`: the element's boxes on screen, one per line it spans.
    ElementBoxes = 87,
    /// `StyleOpen` → `DialogModel`: the style dialog.
    DialogOpenStyle = 88,
    /// `{ kind, style }` (a `DialogKind` and the style dialog's values) → `DialogModel`: a Format menu dialog.
    DialogOpenOnStyle = 89,
    /// `DialogModel` (a Format menu dialog) → `DialogModel`: the style dialog after it.
    DialogApplyToStyle = 90,
    /// `{ id, size }` → `Picture`: a style's gallery tile.
    StyleTile = 91,
    /// `{ model, size }` → `Picture`: an open dialog's preview.
    DialogPreview = 92,
    /// `ListGalleryKind` → `ListGallerySection[]`: a list gallery.
    ListGalleries = 93,
    /// `{ id, size }` → `Picture`: a list gallery entry's preview.
    ListEntryPreview = 94,
    /// `string` (a font family) → `CharRange[]`: the characters the font has.
    FontCharacters = 95,
    /// `{ family, character, size }` → `Picture`: a character in a font.
    CharacterPicture = 96,
    /// → `BookmarkInfo[]`.
    Bookmarks = 97,
    /// → `DocumentProperties`.
    DocumentProperties = 98,
    /// → `WordAt | null`: the word at the selection, for a thesaurus.
    WordAtSelection = 99,
    /// → `SelectionBounds`: the selection on screen.
    SelectionBounds = 100,
    /// → `DragSource | null`: starts a drag of the selection out of the view.
    DragOut = 101,
    /// `DropEffect` → `null`: the drag out ended.
    DragEnd = 102,
    /// `{ at, copy }` → `Commit`: the drag out came back and dropped at `at`.
    DropDragged = 103,
    /// `ObjectWrap` → `null`: the wrap inserted pictures take.
    SetDefaultPictureWrap = 104,
    /// → `ObjectWrap`.
    DefaultPictureWrap = 105,
    /// `StyleId | null` → `null`: the style Click and Type's paragraphs take.
    SetClickAndTypeStyle = 106,
    /// `{ limit? } | null` → `HistoryLabels`: the undo and redo lists.
    HistoryLabels = 107,
    /// `StyleId` → `StyleDefinition | null`.
    StyleDefinition = 108,
    /// → `BuiltInStyle[]`.
    BuiltInStyles = 109,
    /// → `DefaultsDefinition | null`: the document's default formatting.
    DocumentDefaults = 110,
    /// → `ListDefinition | null`: the list at the selection.
    ListDefinition = 111,
    /// → `ThemeColumn[]`: the theme colour grid.
    ThemeColourGrid = 112,
    /// `u32` (a page, from 0) → `ObjectListEntry[]`: the Selection Pane's objects on the page.
    ObjectList = 113,
    /// → `Bytes | null`: the selected picture's image.
    ObjectImage = 114,
    /// → `Bytes | null`: the selected shape's picture fill's image.
    ShapeFillImage = 115,
    /// `u32` (a section, from 0) → `Bytes | null`: its picture watermark's image.
    WatermarkImage = 116,
    /// → `Bytes | null`: the page colour's picture.
    PageFillImage = 117,
    /// `TextToTable` → `TableSize | null`: the table Convert Text to Table would make.
    TextToTableSize = 118,
    /// → `NoteInfo[]`: the footnotes, then the endnotes.
    Notes = 119,
    /// `NoteType` → `NoteSeparators`.
    NoteSeparators = 120,
    /// `ColumnPreset` → `Columns`: the columns a preset gives the selection's section.
    ColumnPreset = 121,
    /// `{ format, chapterSeparator? }` → `string`: Format Page Numbers' example.
    PageNumberSample = 122,
    /// → `StyleId[]`: Formatting Restrictions' Recommended Minimum.
    RecommendedMinimumStyles = 123,
    /// → `EditableRangeInfo[]`.
    EditableRanges = 124,
    /// `FieldCategory | null` → `FieldDescription[]`: Insert Field's fields.
    FieldCatalogue = 125,
    /// → `FieldFormats`.
    FieldFormats = 126,
    /// `FieldChoice` → `ComposedField`.
    ComposeField = 127,
    /// `string` (a field code) → `FieldChoice`.
    ParseField = 128,
    /// → `FieldHere | null`: the field at the selection, with its choices.
    FieldHere = 129,
    /// `string` (a locale) → `string[]`: a date control's display formats.
    DateFormats = 130,
    /// → `u64[]`: each laid-out page's version.
    PageVersions = 131,
    /// → `HeadingId | null`: the heading the selection is under.
    CurrentHeading = 132,
    /// `FindQuery` → `FindCount`: Highlight All.
    FindHighlightAll = 133,
    /// → `null`: Highlight All's highlights go.
    FindClearHighlights = 134,
    /// `ClipboardData` → `PasteChoices`: Paste Special's list.
    PasteChoices = 135,
    /// `string` (a copy's token) → `string`: its HTML, objects drawn.
    ClipboardHtml = 136,
    /// `CrossReferenceKind` → `CrossReferenceTarget[]`.
    CrossReferenceTargets = 137,
    /// `string[]` (the host's labels) → `string[]`: the caption labels.
    CaptionLabels = 138,
    /// → `TablesOfContents`.
    TablesOfContents = 139,
    /// `TableOfContentsOptions` → `TocEntryPreview[]`.
    TocEntries = 140,
    /// `{ data, as? }` → `null`: pastes clipboard data the application read, failing where it can't paste.
    PasteContent = 141,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PageNumberSampleRequest {
    format: engine::api::command::NumberFormat,
    #[serde(default)]
    chapter_separator: Option<engine::api::command::ChapterSeparator>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryLabelsRequest {
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContextMenuRequest {
    #[serde(default)]
    synonyms: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DropDraggedRequest {
    at: Point,
    copy: bool,
}

/// A resolver's answer to a font request.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    request: FontRequest,
    answer: FontAnswer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WindowSelectRequest {
    anchor: engine::api::services::WindowPoint,
    focus: engine::api::services::WindowPoint,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchRequest {
    commands: Vec<Command>,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StyleTileRequest {
    id: engine::api::value::StyleId,
    size: engine::api::services::PreviewSize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListEntryPreviewRequest {
    id: engine::api::services::ListEntryId,
    size: engine::api::services::PreviewSize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CharacterPictureRequest {
    family: String,
    character: char,
    size: engine::api::services::PreviewSize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DialogPreviewRequest {
    model: DialogModel,
    size: engine::api::services::PreviewSize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DialogOnStyleRequest {
    kind: DialogKind,
    style: engine::api::services::StyleDialog,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DialogUpdateRequest {
    model: DialogModel,
    patch: DialogPatch,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FindRequest {
    query: FindQuery,
    direction: FindDirection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplaceRequest {
    query: FindQuery,
    with: String,
    #[serde(default)]
    replace_format: Option<engine::api::services::FindFormat>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UserWordsRequest {
    language: String,
    words: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SuggestionsRequest {
    word: String,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct StructureRequest {
    root: Option<ElementId>,
    depth: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectElementRequest {
    id: ElementId,
    #[serde(default)]
    reveal: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubTemplateExportRequest {
    name: String,
    #[serde(default)]
    category: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct DataPartsRequest {
    namespace: Option<String>,
    item_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GestureSetRequest {
    gesture: GestureId,
    action: GestureAction,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CopyRequest {
    cut: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PasteRequest {
    data: ClipboardData,
    #[serde(default)]
    as_text: bool,
    #[serde(default, rename = "as")]
    as_: Option<engine::api::command::PasteAs>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RulersRequest {
    horizontal: bool,
    vertical: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DropRequest {
    at: Point,
    command: Command,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BalloonsRequest {
    anchors: Vec<CommentAnchor>,
    heights: Vec<f64>,
    #[serde(default)]
    gap: Option<f64>,
}

/// Runs `method` with its JSON request.
pub(super) fn call(e: &mut TwipEngine, session: u32, method: Method, json: &[u8]) -> Result<Value, Error> {
    if let Method::OpenCommit | Method::OpenDiscard = method {
        let token: u32 = request(json)?;
        e.session(session)?;
        let (_, prepared) = match e.prepared.get(&token) {
            Some((owner, _)) if *owner == session => e.prepared.remove(&token).expect("just found"),
            _ => return Err(Error::invalid(format!("no prepared open {token} in this session"))),
        };
        let s = e.session(session)?;
        return match method {
            Method::OpenCommit => to_value(s.open_commit(prepared)?),
            _ => {
                s.open_discard(prepared);
                Ok(Value::Null)
            }
        };
    }
    match engine_call(e, method, json) {
        Some(result) => result,
        None => session_call(e.session(session)?, method, json),
    }
}

/// The methods of the engine's catalog and font library, which take no session; `None` for the rest.
fn engine_call(e: &mut TwipEngine, method: Method, json: &[u8]) -> Option<Result<Value, Error>> {
    let fonts = e.engine.fonts();
    let result = match method {
        Method::Catalog => to_value(Session::catalog()),
        Method::ShapeGallery => to_value(Session::shape_gallery()),
        Method::FontFamilies => to_value(fonts.families()),
        Method::FontSetReplacements => request::<Vec<FontRule>>(json).map(|r| {
            fonts.set_replacements(r);
            Value::Null
        }),
        Method::FontSetFallback => request::<Option<String>>(json).map(|f| {
            fonts.set_fallback(f.as_deref());
            Value::Null
        }),
        Method::FontSetAliases => request::<Vec<Alias>>(json).map(|a| {
            fonts.set_aliases(a);
            Value::Null
        }),
        Method::FontSetGenericDefaults => request::<GenericDefaults>(json).map(|d| {
            fonts.set_generic_defaults(d);
            Value::Null
        }),
        Method::FontSetCharacterFallback => request::<Vec<String>>(json).map(|f| {
            fonts.set_character_fallback(f);
            Value::Null
        }),
        Method::FontSetClassification => request::<bool>(json).map(|on| {
            fonts.set_classification(on);
            Value::Null
        }),
        Method::FontSetSymbolFallback => request::<SymbolFallback>(json).and_then(|f| fonts.set_symbol_fallback(f).map(|()| Value::Null)),
        Method::FontSetResolver => request::<bool>(json).map(|on| {
            if on {
                let queue = e.outbox.clone();
                fonts.on_request(move |r| {
                    if let Ok(payload) = serde_json::to_vec(r) {
                        queue.borrow_mut().push_back(Message { session: 0, kind: kind::FONT_REQUEST, payload });
                    }
                });
            } else {
                fonts.stop_requests();
            }
            Value::Null
        }),
        Method::FontAnswer => request::<Answer>(json).map(|a| {
            fonts.answer(&a.request, a.answer);
            Value::Null
        }),
        Method::StackBalloons => {
            request::<BalloonsRequest>(json).and_then(|r| to_value(engine::api::review::stack_balloons(&r.anchors, &r.heights, r.gap.unwrap_or(8.0))))
        }
        _ => return None,
    };
    if result.is_ok()
        && matches!(
            method,
            Method::FontSetReplacements
                | Method::FontSetFallback
                | Method::FontSetAliases
                | Method::FontSetGenericDefaults
                | Method::FontSetCharacterFallback
                | Method::FontSetClassification
                | Method::FontSetSymbolFallback
                | Method::FontSetResolver
                | Method::FontAnswer
        )
    {
        e.engine.sync_fonts();
    }
    Some(result)
}

/// The session methods.
fn session_call(s: &mut Session, method: Method, json: &[u8]) -> Result<Value, Error> {
    let none = Value::Null;
    match method {
        Method::Run => to_value(s.run(request(json)?)?),
        Method::Batch => {
            let r: BatchRequest = request(json)?;
            to_value(s.batch(r.commands, r.label)?)
        }
        Method::State => to_value(s.state()),
        Method::NewDocument => {
            s.new_document();
            Ok(none)
        }
        Method::SetReadOnly => {
            s.set_read_only(request(json)?);
            Ok(none)
        }
        Method::SetMode => {
            s.set_mode(request::<Mode>(json)?);
            Ok(none)
        }
        Method::SetAuthor => {
            s.set_author(request::<Author>(json)?);
            Ok(none)
        }
        Method::ContextMenu => {
            let r = request::<Option<ContextMenuRequest>>(json)?.unwrap_or_default();
            to_value(s.context_menu_with(r.synonyms))
        }
        Method::WordAtSelection => to_value(s.word_at_selection()),
        Method::SelectionBounds => to_value(s.selection_bounds()),
        Method::DragOut => to_value(s.drag_out()),
        Method::DragEnd => {
            s.drag_end(request(json)?);
            Ok(none)
        }
        Method::DropDragged => {
            let r: DropDraggedRequest = request(json)?;
            to_value(s.drop_dragged(r.at, r.copy)?)
        }
        Method::SetDefaultPictureWrap => {
            s.set_default_picture_wrap(request(json)?);
            Ok(none)
        }
        Method::DefaultPictureWrap => to_value(s.default_picture_wrap()),
        Method::SetClickAndTypeStyle => {
            s.set_click_and_type_style(request(json)?);
            Ok(none)
        }
        Method::HistoryLabels => to_value(s.history_labels(request::<Option<HistoryLabelsRequest>>(json)?.unwrap_or_default().limit)),
        Method::StyleDefinition => to_value(s.style_definition(&request(json)?)),
        Method::BuiltInStyles => to_value(s.built_in_styles()),
        Method::DocumentDefaults => to_value(s.document_defaults()),
        Method::ListDefinition => to_value(s.list_definition()),
        Method::ThemeColourGrid => to_value(s.theme_colour_grid()),
        Method::ObjectList => to_value(s.object_list(request(json)?)),
        Method::ObjectImage => to_value(s.object_image()),
        Method::ShapeFillImage => to_value(s.shape_fill_image()),
        Method::WatermarkImage => to_value(s.watermark_image(request(json)?)),
        Method::PageFillImage => to_value(s.page_fill_image()),
        Method::TextToTableSize => to_value(s.text_to_table_size(&request(json)?)),
        Method::Notes => to_value(s.notes()),
        Method::NoteSeparators => to_value(s.note_separators(request(json)?)),
        Method::ColumnPreset => to_value(s.column_preset(request(json)?)),
        Method::PageNumberSample => {
            let r: PageNumberSampleRequest = request(json)?;
            to_value(s.page_number_sample(r.format, r.chapter_separator))
        }
        Method::RecommendedMinimumStyles => to_value(s.recommended_minimum_styles()),
        Method::EditableRanges => to_value(s.editable_ranges()),
        Method::FieldCatalogue => to_value(s.field_catalogue(request(json)?)),
        Method::FieldFormats => to_value(s.field_formats()),
        Method::ComposeField => to_value(s.compose_field(&request(json)?)),
        Method::ParseField => to_value(s.parse_field(&request::<String>(json)?)),
        Method::FieldHere => to_value(s.field_here()),
        Method::DateFormats => to_value(s.date_formats(&request::<String>(json)?)),
        Method::PageVersions => to_value(s.page_versions()),
        Method::CurrentHeading => to_value(s.current_heading()),
        Method::Invoke => to_value(s.invoke(request::<MenuAction>(json)?)?),
        Method::DialogOpen => to_value(s.dialog_open(request::<DialogKind>(json)?)?),
        Method::DialogUpdate => {
            let r: DialogUpdateRequest = request(json)?;
            to_value(s.dialog_update(r.model, r.patch)?)
        }
        Method::DialogApply => to_value(s.dialog_apply(request(json)?)?),
        Method::DialogOpenStyle => to_value(s.dialog_open_style(request(json)?)?),
        Method::DialogOpenOnStyle => {
            let r: DialogOnStyleRequest = request(json)?;
            to_value(s.dialog_open_on_style(r.kind, r.style)?)
        }
        Method::DialogApplyToStyle => to_value(s.dialog_apply_to_style(request(json)?)?),
        Method::StyleTile => {
            let r: StyleTileRequest = request(json)?;
            to_value(s.style_tile(&r.id, r.size)?)
        }
        Method::DialogPreview => {
            let r: DialogPreviewRequest = request(json)?;
            to_value(s.dialog_preview(&r.model, r.size)?)
        }
        Method::ListGalleries => to_value(s.list_galleries(request(json)?)),
        Method::ListEntryPreview => {
            let r: ListEntryPreviewRequest = request(json)?;
            to_value(s.list_entry_preview(&r.id, r.size)?)
        }
        Method::FontCharacters => to_value(s.font_characters(&request::<String>(json)?)?),
        Method::CharacterPicture => {
            let r: CharacterPictureRequest = request(json)?;
            to_value(s.character_picture(&r.family, r.character, r.size)?)
        }
        Method::DialogSelectIssue => to_value(s.select_accessibility_issue(&request::<String>(json)?)?),
        Method::Find => {
            let r: FindRequest = request(json)?;
            to_value(s.find(&r.query, r.direction)?)
        }
        Method::FindAll => to_value(s.find_all(&request(json)?)?),
        Method::FindHighlightAll => to_value(s.highlight_all(&request(json)?)?),
        Method::FindClearHighlights => {
            s.clear_find_highlights();
            Ok(none)
        }
        Method::SelectMatch => to_value(s.select_match(request::<MatchId>(json)?)?),
        Method::Replace => {
            let r: ReplaceRequest = request(json)?;
            to_value(s.replace(&r.query, &r.with, r.replace_format.as_ref())?)
        }
        Method::ReplaceAll => {
            let r: ReplaceRequest = request(json)?;
            to_value(s.replace_all(&r.query, &r.with, r.replace_format.as_ref())?)
        }
        Method::SpellingAddDictionary => to_value(s.spelling().add_dictionary(request::<Dictionary>(json)?)?),
        Method::SpellingSetEnabled => {
            s.spelling().set_enabled(request(json)?);
            Ok(none)
        }
        Method::SpellingSetLanguage => {
            s.spelling().set_language(&request::<String>(json)?);
            Ok(none)
        }
        Method::SpellingIgnore => {
            s.spelling().ignore(&request::<String>(json)?);
            Ok(none)
        }
        Method::SpellingAdd => {
            s.spelling().add(&request::<String>(json)?);
            Ok(none)
        }
        Method::SpellingLoadUserWords => {
            let r: UserWordsRequest = request(json)?;
            s.spelling().load_user_words(&r.language, &r.words);
            Ok(none)
        }
        Method::SpellingSuggestions => {
            let r: SuggestionsRequest = request(json)?;
            to_value(s.spelling().suggestions(&r.word, r.language.as_deref()))
        }
        Method::SpellingNext => to_value(s.spelling().next_misspelling()),
        Method::Headings => to_value(s.headings()),
        Method::Bookmarks => to_value(s.bookmarks()),
        Method::DocumentProperties => to_value(s.document_properties()),
        Method::SelectHeading => to_value(s.select_heading(request::<HeadingId>(json)?)?),
        Method::Styles => to_value(s.styles(request::<Option<StyleType>>(json)?)),
        Method::Revisions => to_value(s.revisions()),
        Method::Comments => to_value(s.comments()),
        Method::People => to_value(s.people()),
        Method::Elements => to_value(s.elements(&request::<Option<ElementFilter>>(json)?.unwrap_or_default())),
        Method::Element => to_value(s.element(request(json)?)?),
        Method::ElementBoxes => to_value(s.element_boxes(&request::<ElementId>(json)?)?),
        Method::SubTemplates => to_value(s.sub_templates()),
        Method::SubTemplateExport => {
            let r: SubTemplateExportRequest = request(json)?;
            to_value(s.export_sub_template(&r.name, r.category.as_deref())?)
        }
        Method::Structure => {
            let r: StructureRequest = request::<Option<StructureRequest>>(json)?.unwrap_or_default();
            to_value(s.structure(r.root, r.depth)?)
        }
        Method::SelectElement => {
            let r: SelectElementRequest = request(json)?;
            to_value(s.select_element(r.id, r.reveal)?)
        }
        Method::DataParts => {
            let r: DataPartsRequest = request::<Option<DataPartsRequest>>(json)?.unwrap_or_default();
            to_value(s.data_parts(r.namespace.as_deref(), r.item_id.as_deref()))
        }
        Method::AccessibleTree => to_value(s.accessible_tree()),
        Method::AccessibleGo => to_value(s.accessible_go(request(json)?)?),
        Method::AnnouncementsSet => {
            s.set_announcements(&request::<Vec<engine::api::services::AnnouncementKind>>(json)?);
            Ok(none)
        }
        Method::ShortcutsRegister => to_value(s.shortcuts().register(request::<Shortcut>(json)?)?),
        Method::ShortcutsUnregister => {
            s.shortcuts().unregister(&request::<String>(json)?);
            Ok(none)
        }
        Method::ShortcutsLookup => to_value(s.shortcuts().lookup(&request::<String>(json)?)),
        Method::GesturesBindings => to_value(s.gestures().bindings()),
        Method::GesturesSet => {
            let r: GestureSetRequest = request(json)?;
            to_value(s.gestures().set(r.gesture, r.action)?)
        }
        Method::GesturesReset => {
            s.gestures().reset(request(json)?);
            Ok(none)
        }
        Method::WordCount => to_value(s.word_count(request::<Option<_>>(json)?.unwrap_or_default())),
        Method::Copy => to_value(s.copy(request::<CopyRequest>(json)?.cut)),
        Method::Paste => {
            let r: PasteRequest = request(json)?;
            match r.as_ {
                Some(choice) => s.paste_with(&r.data, Some(choice))?,
                None => s.paste(&r.data, r.as_text),
            }
            Ok(none)
        }
        Method::PasteContent => {
            let r: PasteRequest = request(json)?;
            s.paste_with(&r.data, r.as_.or(r.as_text.then_some(engine::api::command::PasteAs::Text)))?;
            Ok(none)
        }
        Method::PasteChoices => to_value(s.paste_choices(&request(json)?)),
        Method::ClipboardHtml => to_value(s.clipboard_html(&request::<String>(json)?)?),
        Method::CrossReferenceTargets => to_value(s.cross_reference_targets(&request(json)?)),
        Method::CaptionLabels => to_value(s.caption_labels(&request::<Option<Vec<String>>>(json)?.unwrap_or_default())),
        Method::TablesOfContents => to_value(s.tables_of_contents()),
        Method::TocEntries => to_value(s.toc_entries(&request(json)?)?),
        Method::HostSetBackground => {
            match request::<Option<Color>>(json)? {
                Some(c) => s.host().set_background(c),
                None => s.host().reset_background(),
            }
            Ok(none)
        }
        Method::HostSetMarkupPaneWidth => {
            s.host().set_markup_pane_width(request(json)?);
            Ok(none)
        }
        Method::HostSetRulers => {
            let r: RulersRequest = request(json)?;
            s.host().set_rulers(r.horizontal, r.vertical);
            Ok(none)
        }
        Method::HostSetRulerUnit => {
            s.host().set_ruler_unit(request::<RulerUnit>(json)?);
            Ok(none)
        }
        Method::HostSetTabSelector => {
            s.host().set_tab_selector(request::<TabSelector>(json)?);
            Ok(none)
        }
        Method::HostRulerView => to_value(s.host().ruler_view()),
        Method::HostCommentAnchors => to_value(s.host().comment_anchors()),
        Method::HostHitTest => to_value(s.host().hit_test(request(json)?)),
        Method::HostDragOver => {
            s.host().drag_over(request(json)?);
            Ok(none)
        }
        Method::HostDragLeave => {
            s.host().drag_leave();
            Ok(none)
        }
        Method::HostDrop => {
            let r: DropRequest = request(json)?;
            to_value(s.host().drop(r.at, r.command)?)
        }
        Method::HostRevealSelection => {
            s.host().reveal_selection();
            Ok(none)
        }
        Method::HostVisiblePages => to_value(s.host().visible_pages()),
        Method::HostFrameStats => to_value(s.host().frame_stats()),
        Method::HostSetLeaders => {
            s.host().set_leaders(request(json)?);
            Ok(none)
        }
        Method::HostSetPaintBudget => {
            s.host().set_paint_budget(request(json)?);
            Ok(none)
        }
        Method::HostCaretWindowSelect => {
            let r: WindowSelectRequest = request(json)?;
            s.host().caret_window_select(r.anchor, r.focus)?;
            Ok(none)
        }
        Method::HostSetOverlayPalette => {
            match request(json)? {
                Some(p) => s.host().set_overlay_palette(p),
                None => s.host().reset_overlay_palette(),
            }
            Ok(none)
        }
        Method::HostSetCaretBlink => {
            s.host().set_caret_blink(request(json)?);
            Ok(none)
        }
        // `call` answers these before looking for a session.
        Method::Catalog
        | Method::ShapeGallery
        | Method::FontFamilies
        | Method::FontSetReplacements
        | Method::FontSetFallback
        | Method::FontSetAliases
        | Method::FontSetGenericDefaults
        | Method::FontSetCharacterFallback
        | Method::FontSetClassification
        | Method::FontSetSymbolFallback
        | Method::FontAnswer
        | Method::FontSetResolver
        | Method::StackBalloons
        | Method::OpenCommit
        | Method::OpenDiscard => Err(Error::invalid("not a session method")),
    }
}
