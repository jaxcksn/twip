import type { EditorError } from './errors.js';
import type { FontsState } from './fonts/library.js';
import type { BalloonPlacement, CommentAnchor } from './review.js';
import type { AnnouncementKind } from './services/accessibility.js';
import type { DialogKind } from './services/dialogs.js';
import type { ElementInfo } from './services/elements.js';
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
   * A picture control was activated: clicked while unfilled, double-clicked or given Enter in editing and read-only
   * modes, or named by `activatePictureControl`.
   */
  pictureControlActivated: { element: ElementId };
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
  /**
   * The document's fonts that draw with a stand-in (being fetched, or nothing provided them), or its characters no
   * font has, changed. Carries the whole report, as `state.fonts` has it.
   */
  fontsChanged: FontsState;
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
  kind: 'link' | 'mark' | 'revision' | 'comment';
  /** The text to show. */
  text: string;
  /** The pointer. */
  at: ClientPoint;
  /** The hovered thing's box. */
  rect: ClientRect;
}
