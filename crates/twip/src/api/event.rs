//! Events: what the engine tells its host, in commit order. A session's state is rebuilt before any event of the same
//! change is delivered.

use serde::{Deserialize, Serialize};

use super::error::{Error, ErrorCode};
use super::record::TransactionRecord;
use super::review::{BalloonPlacement, CommentAnchor};
use super::services::{ContextMenuEvent, DialogKind, ElementInfo, GestureEvent, PlaceholderInfo, PlaceholderTarget, Popover, RulerView, Tooltip};
use super::state::{SelectionSummary, State};
use super::value::ElementId;
use super::warning::Warning;
use super::wire::value;

/// An event. On the wire its `type` is the event name (`documentChanged`), and its payload's fields sit beside it;
/// a payload that isn't an object (a list, a value or `null`) travels in `value`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant, reason = "an event is made once per change and lent to the listener; boxing would allocate per change")]
pub enum Event {
    /// The state changed. SDKs receive it as their state snapshot, never as an event.
    StateChanged(State),
    /// Exactly one per commit, in order, never coalesced.
    DocumentChanged(TransactionRecord),
    /// The selection moved (coalesced per frame).
    SelectionChanged(SelectionSummary),
    /// A link was clicked (read only) or Ctrl+clicked.
    LinkActivated {
        /// The link's address.
        address: String,
        /// How it was activated.
        via: LinkVia,
    },
    /// An unfilled picture placeholder was clicked.
    PlaceholderActivated(PlaceholderInfo),
    /// A context menu to open.
    ContextMenu(ContextMenuEvent),
    /// A click or command asked for a popover by its anchor. Opening one closes the one open.
    PopoverRequested(Popover),
    /// The engine dismissed the open popover: Esc, a press elsewhere, or its target gone.
    PopoverClosed,
    /// A key or gesture asked for a dialog.
    DialogRequested {
        /// The dialog to open.
        kind: DialogKind,
    },
    /// A key the application owns was pressed.
    HostCommand {
        /// The key's command.
        command: HostCommand,
    },
    /// An application shortcut was pressed.
    Shortcut {
        /// The id the application registered the shortcut with.
        id: String,
    },
    /// A gesture bound to `emit`.
    Gesture(GestureEvent),
    /// A word was added to the user's dictionary: persist it.
    WordAdded {
        /// The word.
        word: String,
        /// Its language tag (`en-US`).
        language: String,
    },
    /// Fonts being fetched, or with no font found, changed.
    FontsChanged {
        /// The families being fetched.
        pending: Vec<String>,
        /// The families no source had: they draw with a substitute.
        unresolved: Vec<String>,
    },
    /// Design mode: Enter or a double-click on an element.
    ElementActivated {
        /// The element.
        id: ElementId,
    },
    /// Elements came, went or changed.
    ElementsChanged {
        /// The new elements.
        added: Vec<ElementInfo>,
        /// The elements that are gone.
        removed: Vec<ElementId>,
        /// The elements whose properties or content changed.
        changed: Vec<ElementInfo>,
    },
    /// Fields the application computes need results: answer with `setFieldResults` and the token.
    FieldsUpdateRequested {
        /// The request's token, passed back with `setFieldResults`.
        token: u64,
        /// The fields to compute.
        fields: Vec<FieldRequest>,
    },
    /// Revisions came, went or changed.
    RevisionsChanged {},
    /// Comments came, went or changed.
    CommentsChanged {},
    /// The visible comments' anchors, after layout, zoom or scroll.
    CommentAnchors(#[serde(with = "value")] Vec<CommentAnchor>),
    /// Where the SDK placed the attached balloons.
    BalloonsLaidOut(#[serde(with = "value")] Vec<BalloonPlacement>),
    /// The ruler view changed (coalesced per frame).
    RulerChanged(RulerView),
    /// The editor switched between the template and a sub-template, or between sub-templates: where it is now.
    EditingTargetChanged(super::state::EditingTarget),
    /// A shown sub-template's newer-version notice (`setSubTemplateVersions`) was pressed: the application updates it.
    SubTemplateUpdateRequested {
        /// The sub-template's name.
        name: String,
        /// Its category.
        category: String,
    },
    /// Typing, pasting or dropping was refused: why, and the element concerned.
    EditRefused {
        /// Why.
        reason: RefusalReason,
        /// The element concerned: the reference field whose sub-template's body shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        element: Option<ElementId>,
    },
    /// Hover over a link, element mark, revision or comment; `null` hides the tooltip.
    Tooltip(#[serde(with = "value")] Option<Tooltip>),
    /// With the placeholder affordances on, the pointer starts or stops being over a placeholder, or the hovered
    /// one's anchor moves; `null` when it leaves.
    PlaceholderHovered(#[serde(with = "value")] Option<PlaceholderTarget>),
    /// With the placeholder affordances on, the selection starts or stops covering exactly one placeholder, or the
    /// selected one's anchor moves (scroll, zoom, layout); `null` when it's no longer selected.
    PlaceholderSelected(#[serde(with = "value")] Option<PlaceholderTarget>),
    /// What the editor said to screen readers.
    Announcement {
        /// The announced text.
        text: String,
        /// What it is about; when one change says several things, what the first is about.
        kind: crate::api::services::AnnouncementKind,
    },
    /// A fidelity note from reading or writing.
    Warning(Warning),
    /// A fault: the engine failed. With `fatal`, the session rejects later operations with `internal`.
    #[serde(rename = "error")]
    Fault {
        /// What failed.
        code: ErrorCode,
        /// An English description, for logs.
        message: String,
        /// Structured detail, when there is any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        /// Whether the session is unusable: it rejects later operations with `internal`.
        fatal: bool,
    },
}

/// Why typing, pasting or dropping was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RefusalReason {
    /// Into a sub-template's body shown in place of its reference: edit the sub-template instead.
    SubTemplate {
        /// The sub-template's name.
        name: String,
    },
    /// Into a control whose content is locked.
    Locked,
}

/// How a link was activated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkVia {
    /// Ctrl+click (Cmd+click on a Mac).
    CtrlClick,
    /// A plain click, in a read-only document.
    Click,
}

/// A field whose result the application computes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldRequest {
    /// The field.
    pub id: ElementId,
    /// Its field code (`DOCPROPERTY Title`).
    pub code: String,
}

/// Keys whose UI is the application's: pressing one never edits, and emits `hostCommand`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u32)]
pub enum HostCommand {
    /// Ctrl+S.
    Save,
    /// F12.
    SaveAs,
    /// Ctrl+P.
    Print,
    /// Ctrl+O.
    Open,
    /// Ctrl+N.
    New,
    /// Ctrl+F.
    Find,
    /// Ctrl+H.
    Replace,
    /// Ctrl+G.
    GoTo,
    /// Ctrl+K.
    InsertHyperlink,
    /// F7.
    NextMisspelling,
    /// Ctrl+Shift+F.
    FocusFont,
    /// Ctrl+Shift+P.
    FocusFontSize,
    /// F1.
    Help,
}

impl HostCommand {
    /// Every host command, in declaration order (the C ABI numbers them from 0 in this order).
    pub const ALL: [HostCommand; 13] = [
        HostCommand::Save,
        HostCommand::SaveAs,
        HostCommand::Print,
        HostCommand::Open,
        HostCommand::New,
        HostCommand::Find,
        HostCommand::Replace,
        HostCommand::GoTo,
        HostCommand::InsertHyperlink,
        HostCommand::NextMisspelling,
        HostCommand::FocusFont,
        HostCommand::FocusFontSize,
        HostCommand::Help,
    ];
}

/// What a key press did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum KeyOutcome {
    /// The editor used it: the host suppresses the platform's default.
    Handled,
    /// A key the application owns (also emitted as `hostCommand`).
    HostCommand(#[serde(with = "value")] HostCommand),
    /// An application shortcut (also emitted as `shortcut`).
    Shortcut(#[serde(with = "value")] String),
    /// Not the editor's: the platform keeps it.
    Unhandled,
}

impl Event {
    /// The fault event for `error`: fatal, so the session rejects later operations.
    pub fn fault(error: Error) -> Event {
        Event::Fault { code: error.code, message: error.message, details: error.details, fatal: true }
    }
}
