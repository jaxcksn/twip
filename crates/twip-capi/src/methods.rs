//! The structured calls: one number per `Session` method (and the engine's font library), each with its JSON
//! request and response, the `api` types' serde form.

use serde::Deserialize;
use serde_json::Value;

use super::{Message, TwipEngine, kind, request, to_value};
use engine::api::command::Command;
use engine::api::engine::Dictionary;
use engine::api::error::Error;
use engine::api::fonts::{FontRequest, SubstitutionRule};
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
    /// → `ContextMenu`.
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
    /// `FindQuery` → `FindMatch[]`.
    FindAll = 15,
    /// `MatchId` → `null`.
    SelectMatch = 16,
    /// `{ query, with }` → `FindResult`.
    Replace = 17,
    /// `{ query, with }` → `{ count }`.
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
    /// → `WordCount`.
    WordCount = 45,
    /// `{ cut }` → `ClipboardData | null`.
    Copy = 46,
    /// `{ data, asText }` → `null`.
    Paste = 47,
    /// → `string[]`: the families the library draws (any session).
    FontFamilies = 48,
    /// `SubstitutionRule[]` → `null` (any session).
    FontSetSubstitutions = 49,
    /// `string` → `null` (any session).
    FontSetDefault = 50,
    /// `FontRequest` → `null` (any session).
    FontMarkUnresolved = 51,
    /// `Color` → `null`.
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
    /// `font_request` callback, answered with `twip_font_add` or `FontMarkUnresolved`; without one it is unresolved
    /// at once.
    FontSetResolver = 69,
    /// → `PlaceholderInfo[]`: the unfilled picture placeholders.
    Placeholders = 70,
    /// `{ intervalMs, stopAfterMs }` (`CaretBlink`) → `null`.
    HostSetCaretBlink = 71,
    /// → `FrameStats`: what the frames have cost so far.
    HostFrameStats = 72,
    /// `OverlayPalette` (colours `#RRGGBB`) → `null`.
    HostSetOverlayPalette = 73,
    /// `{ recordings }` (`PaintBudget`, bytes) → `null`.
    HostSetPaintBudget = 74,
    /// `Point[][]` (content CSS px) → `null`: the balloons' leader lines the engine draws.
    HostSetLeaders = 75,
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
        Method::FontFamilies => to_value(fonts.families()),
        Method::FontSetSubstitutions => request::<Vec<SubstitutionRule>>(json).map(|r| {
            fonts.set_substitutions(r);
            Value::Null
        }),
        Method::FontSetDefault => request::<String>(json).map(|f| {
            fonts.set_default(&f);
            Value::Null
        }),
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
        Method::FontMarkUnresolved => request::<FontRequest>(json).map(|r| {
            fonts.mark_unresolved(&r);
            Value::Null
        }),
        Method::StackBalloons => {
            request::<BalloonsRequest>(json).and_then(|r| to_value(engine::api::review::stack_balloons(&r.anchors, &r.heights, r.gap.unwrap_or(8.0))))
        }
        _ => return None,
    };
    if result.is_ok()
        && matches!(method, Method::FontSetSubstitutions | Method::FontSetDefault | Method::FontSetResolver | Method::FontMarkUnresolved)
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
        Method::ContextMenu => to_value(s.context_menu()),
        Method::Invoke => to_value(s.invoke(request::<MenuAction>(json)?)?),
        Method::DialogOpen => to_value(s.dialog_open(request::<DialogKind>(json)?)?),
        Method::DialogUpdate => {
            let r: DialogUpdateRequest = request(json)?;
            to_value(s.dialog_update(r.model, r.patch)?)
        }
        Method::DialogApply => to_value(s.dialog_apply(request(json)?)?),
        Method::DialogSelectIssue => to_value(s.select_accessibility_issue(&request::<String>(json)?)?),
        Method::Find => {
            let r: FindRequest = request(json)?;
            to_value(s.find(&r.query, r.direction))
        }
        Method::FindAll => to_value(s.find_all(&request(json)?)),
        Method::SelectMatch => to_value(s.select_match(request::<MatchId>(json)?)?),
        Method::Replace => {
            let r: ReplaceRequest = request(json)?;
            to_value(s.replace(&r.query, &r.with)?)
        }
        Method::ReplaceAll => {
            let r: ReplaceRequest = request(json)?;
            to_value(serde_json::json!({ "count": s.replace_all(&r.query, &r.with)? }))
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
        Method::SelectHeading => to_value(s.select_heading(request::<HeadingId>(json)?)?),
        Method::Styles => to_value(s.styles(request::<Option<StyleType>>(json)?)),
        Method::Revisions => to_value(s.revisions()),
        Method::Comments => to_value(s.comments()),
        Method::People => to_value(s.people()),
        Method::Elements => to_value(s.elements(&request::<Option<ElementFilter>>(json)?.unwrap_or_default())),
        Method::Element => to_value(s.element(request(json)?)?),
        Method::Placeholders => to_value(s.placeholders()),
        Method::Structure => {
            let r: StructureRequest = request::<Option<StructureRequest>>(json)?.unwrap_or_default();
            to_value(s.structure(r.root, r.depth))
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
        Method::WordCount => to_value(s.word_count()),
        Method::Copy => to_value(s.copy(request::<CopyRequest>(json)?.cut)),
        Method::Paste => {
            let r: PasteRequest = request(json)?;
            s.paste(&r.data, r.as_text);
            Ok(none)
        }
        Method::HostSetBackground => {
            s.host().set_background(request::<Color>(json)?);
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
        Method::HostSetOverlayPalette => {
            s.host().set_overlay_palette(request(json)?);
            Ok(none)
        }
        Method::HostSetCaretBlink => {
            s.host().set_caret_blink(request(json)?);
            Ok(none)
        }
        // `call` answers these before looking for a session.
        Method::Catalog
        | Method::FontFamilies
        | Method::FontSetSubstitutions
        | Method::FontSetDefault
        | Method::FontMarkUnresolved
        | Method::FontSetResolver
        | Method::StackBalloons
        | Method::OpenCommit
        | Method::OpenDiscard => Err(Error::invalid("not a session method")),
    }
}
