import type { Session } from './editor.js';
import type { SelectionSummary } from './state.js';
import { Method } from './transport/protocol.js';
import type { Color, CommentId, ContentPoint, RevisionId } from './types.js';

/** Reading tracked changes and comments. Changing them is done with the review commands. */
export interface Review {
  /** Every tracked revision, in document order. */
  revisions(): Promise<RevisionInfo[]>;
  /** Every comment thread, in document order. */
  comments(): Promise<CommentThread[]>;
  /** Everyone who made a revision or a comment. */
  people(): Promise<Person[]>;
  /** Card tops that don't overlap: `heights[i]` is the card height for `anchors[i]`. Pure and synchronous. */
  stackBalloons(anchors: readonly CommentAnchor[], heights: readonly number[], options?: { gap?: number }): number[];
}

/** {@link Review} on a mounted editor: adds the markup pane and balloons. */
export interface MountedReview extends Review {
  /** Room beside the pages for balloons, CSS px. 0 (the default) removes it. */
  setMarkupPaneWidth(width: number): Promise<void>;
  /**
   * An SDK-owned element inside the scrolling content, in ContentPoint coordinates. Put cards and leader lines here.
   * It has `pointer-events: none`: a card placed here needs `pointer-events: auto` to be clickable.
   */
  readonly balloonLayer: HTMLElement;
  /** Places `card` beside its thread, stacked with the others, and keeps it there through scroll, zoom and edits. */
  attachBalloon(thread: CommentId, card: HTMLElement): () => void;
  /** The current anchors, also delivered by the `commentAnchors` event. */
  anchors(): CommentAnchor[];
}

/** A tracked revision. */
export interface RevisionInfo {
  /** The revision. */
  id: RevisionId;
  /** What changed. */
  kind:
    | 'insertion'
    | 'deletion'
    | 'moveFrom'
    | 'moveTo'
    | 'formatting'
    | 'paragraphProperties'
    | 'sectionProperties'
    | 'tableProperties'
    | 'rowInsertion'
    | 'rowDeletion'
    | 'cellInsertion'
    | 'cellDeletion'
    | 'cellMerge';
  /** Who made it. */
  author: Person;
  /** When, ISO 8601 UTC, if the document says. */
  time: string | null;
  /** The inserted, deleted or moved text, when there is some. */
  text: string;
  /** For example `'Formatted: Bold, Italic'` for formatting changes. */
  description: string;
  /** The page it starts on. */
  page: number;
  /** The story it is in. */
  story: SelectionSummary['story'];
  /** A move's other half. */
  pair?: RevisionId;
}

/** A comment and its replies. */
export interface CommentThread {
  /** The first comment's id. */
  id: CommentId;
  /** Resolved. */
  done: boolean;
  /** The commented text. */
  anchorText: string;
  /** The page it starts on. */
  page: number;
  /** The first comment, then its replies. */
  comments: Comment[];
}

/** One comment. */
export interface Comment {
  /** The comment. */
  id: CommentId;
  /** Who wrote it. */
  author: Person;
  /** When, ISO 8601 UTC, if the document says. */
  time: string | null;
  /** The body. */
  body: RichText;
  /** The body as plain text. */
  plainText: string;
}

/** Someone who made a revision or a comment. */
export interface Person {
  /** The display name. */
  name: string;
  /** The initials. */
  initials?: string;
  /** The identity in a directory. */
  identity?: { provider: string; userId: string };
  /** The colour the editor marks their changes with. */
  color: Color;
}

/** Formatted text: paragraphs of runs. */
export interface RichText {
  /** The paragraphs. */
  paragraphs: { runs: RichRun[] }[];
}

/** A run of {@link RichText}. */
export interface RichRun {
  /** The text. */
  text: string;
  /** Bold. */
  bold?: boolean;
  /** Italic. */
  italic?: boolean;
  /** Underlined. */
  underline?: boolean;
  /** Struck through. */
  strikethrough?: boolean;
  /** A link address. */
  link?: string;
}

/** Where a visible comment's range is, for placing its card. */
export interface CommentAnchor {
  /** The thread. */
  thread: CommentId;
  /** The page. */
  page: number;
  /** The active thread. */
  active: boolean;
  /** Resolved. */
  done: boolean;
  /** The range's first character, at its baseline. */
  start: ContentPoint;
  /** The range's last character, at its baseline. */
  end: ContentPoint;
  /** The first line's top, for a card level with it. */
  lineTop: number;
  /** The first line's bottom. */
  lineBottom: number;
  /** The x where a leader line leaves the page. */
  pageEdge: number;
}

/** Where the SDK placed an attached balloon. */
export interface BalloonPlacement {
  /** The thread. */
  thread: CommentId;
  /** The card's top. */
  top: number;
  /** The card's left. */
  left: number;
  /** The leader line, a polyline. */
  leader: ContentPoint[];
}

/**
 * Card tops that don't overlap, `gap` apart: card `i` (`heights[i]` tall) is for `anchors[i]`. The first active card
 * sits level with its line; the cards before it rise above it only as far as they must, and the cards after it fall
 * below it. No card goes above 0. With no active card, each card is level with its line or just below the card before
 * it. The engine's `stack_balloons`, the same algorithm.
 */
export function stackBalloons(anchors: readonly CommentAnchor[], heights: readonly number[], gap = 8): number[] {
  const n = Math.min(anchors.length, heights.length);
  const tops = anchors.slice(0, n).map((a) => a.lineTop);
  const active = Math.max(0, anchors.slice(0, n).findIndex((a) => a.active));
  for (let i = active - 1; i >= 0; i--) tops[i] = Math.min(tops[i]!, tops[i + 1]! - gap - heights[i]!);
  let bottom = -Infinity;
  for (let i = 0; i < n; i++) {
    tops[i] = Math.max(tops[i]!, 0, bottom + gap);
    bottom = tops[i]! + heights[i]!;
  }
  return tops;
}

/** Review over a session. */
export function createReview(s: Session): Review {
  return {
    revisions: () => s.call(Method.Revisions),
    comments: () => s.call(Method.Comments),
    people: () => s.call(Method.People),
    stackBalloons: (anchors, heights, options) => stackBalloons(anchors, heights, options?.gap),
  };
}

/** The mounted editor's review members this layer provides: the markup pane, and the anchors last sent. */
export function createMountedReview(s: Session): Pick<MountedReview, 'setMarkupPaneWidth' | 'anchors'> {
  let anchors: CommentAnchor[] = [];
  s.on('commentAnchors', (a) => (anchors = a));
  return {
    setMarkupPaneWidth: (width) => s.call(Method.HostSetMarkupPaneWidth, width),
    anchors: () => anchors,
  };
}
