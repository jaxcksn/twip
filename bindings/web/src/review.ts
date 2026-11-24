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
  /** Sets where and how attached balloons go; what isn't given is kept. */
  setBalloonOptions(options: Partial<BalloonOptions>): void;
  /** Where and how attached balloons go. */
  readonly balloonOptions: BalloonOptions;
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
  /** Show Markup shows it (its kind and its reviewer); one not shown draws as if accepted. */
  shown: boolean;
}

/** A comment and its replies. */
export interface CommentThread {
  /** The first comment's id. */
  id: CommentId;
  /** Resolved. */
  done: boolean;
  /** The commented text, cut to its first 1,000 characters. */
  anchorText: string;
  /** The page it starts on. */
  page: number;
  /** The first comment, then its replies. */
  comments: Comment[];
  /** Show Markup shows the thread. */
  shown: boolean;
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
  /** Show Markup shows it: a reply shows exactly when its thread does. */
  shown: boolean;
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
  /** Show Markup shows their revisions and comments. */
  visible: boolean;
}

/** Where revisions show in All Markup: in balloons beside the page, in place, or only formatting changes in balloons. */
export type BalloonMode = 'balloons' | 'inline' | 'commentsAndFormatting';

/** Which kinds of markup show, and whose. */
export interface ShowMarkup {
  /** Insertions and deletions: of text, moves, rows and cells. */
  insertionsDeletions: boolean;
  /** Formatting changes. */
  formatting: boolean;
  /** Comments. */
  comments: boolean;
  /** The reviewers whose markup doesn't show, by name. */
  hiddenReviewers: string[];
}

/** How a revision marks its text; `hidden`, `caret` and `pound` are for deleted and moved-away text only. */
export type TextMark =
  | 'none'
  | 'colorOnly'
  | 'bold'
  | 'italic'
  | 'underline'
  | 'doubleUnderline'
  | 'strikethrough'
  | 'doubleStrikethrough'
  | 'hidden'
  | 'caret'
  | 'pound';

/** The colour a mark or cell highlight draws in: the author's, the text's own, or a fixed one. */
export type MarkColor = 'byAuthor' | 'auto' | { fixed: Color };

/** How one kind of revision draws. */
export interface MarkLook {
  /** How the text is marked. */
  mark: TextMark;
  /** The mark's colour. */
  color: MarkColor;
}

/** Which side of the text change bars go. */
export type BarSide = 'none' | 'left' | 'right' | 'outside';

/** How revisions draw (Track Changes Options). Not saved. */
export interface MarkupStyle {
  /** Inserted text. */
  insertions: MarkLook;
  /** Deleted text. */
  deletions: MarkLook;
  /** Text with formatting changes. */
  formatting: MarkLook;
  /** Text moved away. */
  movedFrom: MarkLook;
  /** Text moved here. */
  movedTo: MarkLook;
  /** Where change bars go. */
  changeBars: BarSide;
  /** The change bars' colour, or `auto`: red in Simple Markup, grey in All Markup. */
  changeBarColor: Color | 'auto';
  /** Inserted cells' fill. */
  insertedCells: MarkColor;
  /** Deleted cells' fill. */
  deletedCells: MarkColor;
  /** Merged cells' fill. */
  mergedCells: MarkColor;
  /** Split cells' fill. */
  splitCells: MarkColor;
}

/** `setMarkupStyle`'s options: what isn't given is kept. */
export interface MarkupStyleChange {
  /** Inserted text. */
  insertions?: Partial<MarkLook>;
  /** Deleted text (`hidden`, `caret` and `pound` marks too). */
  deletions?: Partial<MarkLook>;
  /** Text with formatting changes. */
  formatting?: Partial<MarkLook>;
  /** Text moved away (`hidden`, `caret` and `pound` marks too). */
  movedFrom?: Partial<MarkLook>;
  /** Text moved here. */
  movedTo?: Partial<MarkLook>;
  /** Where change bars go. */
  changeBars?: BarSide;
  /** The change bars' colour, or `'auto'`. */
  changeBarColor?: Color | 'auto';
  /** Inserted cells' fill. */
  insertedCells?: MarkColor;
  /** Deleted cells' fill. */
  deletedCells?: MarkColor;
  /** Merged cells' fill. */
  mergedCells?: MarkColor;
  /** Split cells' fill. */
  splitCells?: MarkColor;
}

/** How many revisions of each kind, and comment threads. */
export interface MarkupCounts {
  /** Inserted text, rows and cells. */
  insertions: number;
  /** Deleted text, rows and cells. */
  deletions: number;
  /** Both halves of each move. */
  moves: number;
  /** Formatting changes. */
  formatting: number;
  /** Comment threads. */
  comments: number;
}

/** The review counts: of everything, and of what Show Markup shows. */
export interface ReviewCounts {
  /** Every revision and thread. */
  all: MarkupCounts;
  /** Those Show Markup shows. */
  shown: MarkupCounts;
}

/** Where a revision balloon goes, for a host that draws balloons beside the page (`revisionBalloons`). */
export interface RevisionAnchor {
  /** The revision (`review.revisions()` gives its text and description). */
  revision: RevisionId;
  /** What changed. */
  kind: RevisionInfo['kind'];
  /** Who made it. */
  author: string;
  /** The page. */
  page: number;
  /** The revision's first character, at its baseline. */
  start: ContentPoint;
  /** The first line's top. */
  lineTop: number;
  /** The first line's bottom. */
  lineBottom: number;
  /** The x where a leader line leaves the page. */
  pageEdge: number;
  /** The x of the page's left edge, for balloons on the left. */
  pageLeft: number;
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
  /** A link address: a web, mail, FTP or phone one, or a relative one. The run of any other is plain text. */
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
  /** The x of the page's left edge, for balloons on the left. */
  pageLeft: number;
}

/**
 * Where the SDK places attached balloons, and how (Track Changes Options' balloon settings). Not saved: a view
 * setting of the mounted editor.
 */
export interface BalloonOptions {
  /** The margin the balloons go in. Default `'right'`. */
  side: 'left' | 'right';
  /**
   * A balloon's preferred width: in points, or a percent (0 to 100) of the page's width. Default 3.7 in
   * (`{ points: 266.4 }`).
   */
  width: { points: number } | { percent: number };
  /** Leader lines join each balloon to its text. Default true. */
  connectingLines: boolean;
  /** The colour comment cards take: their author's, or one colour. Default `'byAuthor'`. */
  commentColor: 'byAuthor' | { fixed: Color };
}

/** Word's balloon settings. */
export const defaultBalloonOptions: BalloonOptions = { side: 'right', width: { points: 266.4 }, connectingLines: true, commentColor: 'byAuthor' };

/** `options` over `was`, the width's percent kept to 0 to 100 and its points to 0 and up. */
export function balloonOptions(was: BalloonOptions, options: Partial<BalloonOptions>): BalloonOptions {
  const next = { ...was, ...options };
  const width = 'percent' in next.width ? { percent: Math.min(100, Math.max(0, next.width.percent || 0)) } : { points: Math.max(0, next.width.points || 0) };
  return { ...next, width };
}

/**
 * Where a balloon `width` CSS px wide goes beside its anchor: right of the page (or in a markup pane `paneLeft` px
 * from the content's left) or left of it, `gap` px off.
 */
export function balloonLeft(anchor: CommentAnchor, width: number, side: BalloonOptions['side'], gap: number, paneLeft?: number): number {
  if (side === 'left') return anchor.pageLeft - gap - width;
  return paneLeft ?? anchor.pageEdge + gap;
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
