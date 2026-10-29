/** A length in points (1/72 inch). Every length in the API is in points. */
export type Points = number;

/** A colour as `#RRGGBB`. */
export type Color = `#${string}`;

/** Converts inches to points. */
export function inches(n: number): Points {
  return n * 72;
}

/** Converts centimetres to points. */
export function cm(n: number): Points {
  return (n * 72) / 2.54;
}

/** Converts millimetres to points. */
export function mm(n: number): Points {
  return (n * 72) / 25.4;
}

/** Returns `n` points: for call sites that state their unit. */
export function pt(n: number): Points {
  return n;
}

declare const brand: unique symbol;

/** A merge-designer element (a field or a content control). Stable while the document is open. */
export type ElementId = string & { readonly [brand]: 'element' };
/** A comment. Stable while the document is open. */
export type CommentId = string & { readonly [brand]: 'comment' };
/** A tracked revision. Stable while the document is open. */
export type RevisionId = string & { readonly [brand]: 'revision' };
/** A heading in the navigation list. */
export type HeadingId = string & { readonly [brand]: 'heading' };
/** A find match. */
export type MatchId = string & { readonly [brand]: 'match' };
/** A style. */
export type StyleId = string & { readonly [brand]: 'style' };

/** A point relative to the browser viewport, in CSS pixels, like `MouseEvent.clientX`. */
export interface ClientPoint {
  /** CSS pixels from the viewport's left edge. */
  clientX: number;
  /** CSS pixels from the viewport's top edge. */
  clientY: number;
}

/** The colours of what the editor draws over the document (`view.setOverlayColors`). */
export interface OverlayColors {
  /** The caret. Default black. */
  caret: Color;
  /** The selection, blended under the text (multiply), so black text stays black. Default `'#4A90D9'`. */
  selection: Color;
  /** A selected object's outline and handles. Default `'#606060'`. */
  object: Color;
  /** The drop caret of a drag. Default `'#404040'`. */
  drop: Color;
  /** Formatting marks and text boundaries. Default `'#2F6FB5'`. */
  marks: Color;
  /** The markup pane's leader lines. Default `'#767676'`. */
  leaders: Color;
}

/** What a mounted editor's frames have cost so far (`view.frameStats()`): for benchmarks and tests. */
export interface FrameStats {
  /** Frames drawn. */
  frames: number;
  /** Page recordings made: an unchanged page is recorded once. */
  pagesRecorded: number;
  /** Pages drawn again from their recordings. */
  pagesReused: number;
  /** The page recordings kept now, bytes. */
  recordingBytes: number;
  /** The last frame's time, ms. */
  lastFrameMs: number;
  /** The 95th percentile of the last 120 frames' times, ms. */
  p95FrameMs: number;
}

/** A rectangle relative to the browser viewport, in CSS pixels. */
export interface ClientRect {
  /** The left edge. */
  left: number;
  /** The top edge. */
  top: number;
  /** The width. */
  width: number;
  /** The height. */
  height: number;
}

/** A point relative to the editor's balloon layer, in CSS pixels: it scrolls and zooms with the document. */
export interface ContentPoint {
  /** CSS pixels from the layer's left edge. */
  x: number;
  /** CSS pixels from the layer's top edge. */
  y: number;
}

/** Who makes changes: recorded on transactions, revisions and comments. */
export interface Author {
  /** The display name. */
  name: string;
  /** The initials shown on comments. */
  initials?: string;
  /** The identity in a directory, written to the document with the author. */
  identity?: { provider: string; userId: string };
}

/** A recoverable fidelity issue found while reading or writing a document. Aggregated per code and part. */
export interface Warning {
  /** What kind of issue it is. */
  code:
    | 'unsupported-element'
    | 'image-undecodable'
    | 'font-unresolved'
    | 'font-substituted'
    | 'rewritten-content'
    | 'pdf-font-fallback'
    | 'revision-id-missing'
    | 'range-unpaired'
    | 'author-missing'
    | 'comment-unanchored'
    | 'comment-reference-orphan';
  /** A readable description. */
  message: string;
  /** How many times it occurred. */
  count: number;
  /** Where it occurred, when known. */
  location?: { part: string; paragraph?: number };
}

/** Word, character, paragraph, line and page counts, computed on request. */
export interface WordCount {
  /** Words. */
  words: number;
  /** Characters, without spaces. */
  characters: number;
  /** Characters, with spaces. */
  charactersWithSpaces: number;
  /** Paragraphs. */
  paragraphs: number;
  /** Lines. */
  lines: number;
  /** Pages. */
  pages: number;
}
