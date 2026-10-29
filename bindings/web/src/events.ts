import type { EditorError } from './errors.js';
import type { BalloonPlacement, CommentAnchor } from './review.js';
import type { DialogKind } from './services/dialogs.js';
import type { ElementInfo } from './services/elements.js';
import type { ContextMenuEvent } from './services/menus.js';
import type { RulerView } from './services/ruler.js';
import type { GestureEvent } from './services/view.js';
import type { SelectionSummary } from './state.js';
import type { Author, ClientPoint, ClientRect, CommentId, ElementId, RevisionId, Warning } from './types.js';

/** Every event of `editor.on(type, listener)`, keyed by type. */
export interface TwipEvents {
  /** A transaction committed: exactly one per commit, in order, never coalesced. */
  documentChanged: TransactionRecord;
  /** The selection changed; coalesced per frame. */
  selectionChanged: SelectionSummary;
  /** A link was activated: a click when read only, Ctrl+click otherwise. */
  linkActivated: { address: string; via: 'ctrlClick' | 'click' };
  /** An unfilled picture placeholder was clicked. */
  placeholderActivated: { key: string; element: ElementId | null };
  /** The user asked for a context menu. */
  contextMenu: ContextMenuEvent;
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
  /** What the editor said to screen readers. */
  announcement: { text: string };
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
