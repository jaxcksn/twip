import type { EditorError } from './errors.js';
import type { BalloonPlacement, CommentAnchor } from './review.js';
import type { AnnouncementKind } from './services/accessibility.js';
import type { DialogKind } from './services/dialogs.js';
import type { ElementInfo, SubTemplateReference } from './services/elements.js';
import type { ContextMenuEvent } from './services/menus.js';
import type { RulerView } from './services/ruler.js';
import type { GestureEvent, Popover } from './services/view.js';
import type { EditingTarget, SelectionSummary } from './state.js';
import type { Author, ClientPoint, ClientRect, CommentId, ElementId, RevisionId, Warning } from './types.js';

/** Every event of `editor.on(type, listener)`, keyed by type. */
export interface TwipEvents {
  /** A transaction committed: exactly one per commit, in order, never coalesced. */
  documentChanged: TransactionRecord;
  /** The selection changed; coalesced per frame. */
  selectionChanged: SelectionSummary;
  /** The editor switched between the template and a sub-template, or between sub-templates: where it is now. */
  editingTargetChanged: EditingTarget;
  /** A link was activated: a click when read only, Ctrl+click otherwise. */
  linkActivated: { address: string; via: 'ctrlClick' | 'click' };
  /**
   * An unfilled picture placeholder was clicked, or one of its buttons (`action`: the button's id).
   */
  placeholderActivated: { key: string; element: ElementId | null; action?: string };
  /**
   * With `placeholderUi` on, the pointer starts or stops being over a placeholder, or the hovered one's anchor moves;
   * `null` when it leaves.
   */
  placeholderHovered: PlaceholderTarget | null;
  /**
   * With `placeholderUi` on, the selection starts or stops covering exactly one placeholder, or the selected one's
   * anchor moves (scroll, zoom, layout); `null` when it's no longer selected.
   */
  placeholderSelected: PlaceholderTarget | null;
  /** A shown sub-template's newer-version notice (`setSubTemplateVersions`) was pressed: the application updates it. */
  subTemplateUpdateRequested: { name: string; category: string };
  /**
   * Typing, pasting or dropping was refused: into a sub-template's body shown in place of its reference (edit the
   * sub-template instead), or into a locked control.
   */
  editRefused: { reason: { kind: 'subTemplate'; name: string } | { kind: 'locked' }; element?: ElementId };
  /** The user asked for a context menu. */
  contextMenu: ContextMenuEvent;
  /**
   * A click or command asked for a popover by its anchor; opening one closes the one open. A mounted editor shows its
   * own unless the application listens.
   */
  popoverRequested: Popover;
  /** The engine dismissed the open popover: Esc, a press elsewhere, or its target gone. */
  popoverClosed: {};
  /** A key or gesture asked for a dialog. */
  dialogRequested: { kind: DialogKind };
  /** A key the application owns was pressed. */
  hostCommand: { command: HostCommand };
  /** An application shortcut from `shortcuts.register` was pressed. */
  shortcut: { id: string };
  /** Mounted only: a gesture bound to `'emit'`. */
  gesture: GestureEvent;
  /** A word was added to the user's dictionary: persist it. */
  wordAdded: { word: string; language: string };
  /** Fonts started or finished loading, or could not be found. */
  fontsChanged: { pending: string[]; unresolved: string[] };
  /** Design mode: Enter or double-click on an element. */
  elementActivated: { id: ElementId };
  /** Elements were added, removed or changed. */
  elementsChanged: { added: ElementInfo[]; removed: ElementId[]; changed: ElementInfo[] };
  /** Fields the application computes need results: answer with `setFieldResults`. */
  fieldsUpdateRequested: { token: number; fields: { id: ElementId; code: string }[] };
  /** The tracked revisions changed. */
  revisionsChanged: {};
  /** The comments changed. */
  commentsChanged: {};
  /** Mounted only: the visible comments, after layout, zoom or scroll. */
  commentAnchors: CommentAnchor[];
  /** Mounted only: the SDK placed the attached balloons. */
  balloonsLaidOut: BalloonPlacement[];
  /** Mounted only: the rulers changed; coalesced per frame. */
  rulerChanged: RulerView;
  /** Mounted only: hover over a link, element mark, revision or comment, or `null` when it ends. */
  tooltip: Tooltip | null;
  /** What the editor said to screen readers, and what it is about (when one change says several things, the first). */
  announcement: { text: string; kind: AnnouncementKind };
  /** A fidelity warning outside an operation. */
  warning: Warning;
  /** A listener's exception, or a fault (then `fatal` is true). */
  error: EditorError;
}

/** A committed transaction. */
export interface TransactionRecord {
  /** Unique per session. */
  id: number;
  /** The revision it committed. */
  revision: number;
  /** Who started it: the user's input, an API call, undo or redo. */
  origin: 'user' | 'api' | 'undo' | 'redo';
  /** The session's author at the time. */
  author: Author;
  /** When, ISO 8601 UTC. */
  time: string;
  /** The undo label. */
  label: string;
  /** What it touched. */
  summary: { paragraphs: number; tables: number; elements: ElementId[]; revisions: RevisionId[]; comments: CommentId[] };
}

/** A key the application owns. */
export type HostCommand =
  | 'save'
  | 'saveAs'
  | 'print'
  | 'open'
  | 'new'
  | 'find'
  | 'replace'
  | 'goTo'
  | 'insertHyperlink'
  | 'nextMisspelling'
  | 'focusFont'
  | 'focusFontSize'
  | 'help';

/** A hover tooltip to show. */
export interface Tooltip {
  /** What is hovered. */
  kind: 'link' | 'mark' | 'revision' | 'comment' | 'placeholder';
  /** The text to show. */
  text: string;
  /** The pointer. */
  at: ClientPoint;
  /** The hovered thing's box. */
  rect: ClientRect;
}

/** What kind of placeholder a {@link PlaceholderTarget} is. */
export type PlaceholderKind = 'field' | 'control' | 'repeatingSection' | 'subTemplate' | 'inputBlock' | 'ifField';

/** A placeholder the pointer is over or the selection covers, with `placeholderUi` on. */
export interface PlaceholderTarget {
  /** The field or control; absent for an input block outside a content control. */
  element?: ElementId;
  /** What it is. */
  kind: PlaceholderKind;
  /** What it shows, in full: a field's label or result, a control's title. */
  label: string;
  /** A field's name (its first argument), a control's tag, or a picture placeholder's key. */
  key?: string;
  /** A field's code. */
  code?: string;
  /** The caption's type text: `setFieldTypes`' text for the code, else the field type (`MERGEFIELD`), or a control's kind. */
  typeText: string;
  /** The data source lacks the field (`setUnmappedFields`). */
  unmapped: boolean;
  /** For a sub-template reference: what it names and whether its body shows. */
  reference?: SubTemplateReference;
  /** Where it is, in the same space as a popover's anchor: a field's last line, else its box. */
  anchor: ClientRect;
}
