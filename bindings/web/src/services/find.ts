import type { FrameProperties, Ligatures, NumberForm, NumberSpacing } from '../commands.js';
import type { Session } from '../editor.js';
import type { HighlightColor, LineSpacing, UnderlineStyle } from '../state.js';
import { Method } from '../transport/protocol.js';
import type { Color, FillValue, Glow, HeadingId, MatchId, Points, StyleId, TextOutline, TextReflection, TextShadow } from '../types.js';
import type { TabAlignment, TabLeader } from './ruler.js';

/**
 * Find and replace, over the text of the current markup view. A query whose text doesn't parse (an unknown `^` code,
 * a regular expression that doesn't compile, more than 255 characters) fails `invalid`.
 */
export interface Find {
  /** Selects and reveals the next match. */
  next(query: FindQuery): Promise<FindResult>;
  /** Selects and reveals the previous match. */
  previous(query: FindQuery): Promise<FindResult>;
  /** Every match, for a navigation pane's results list. */
  all(query: FindQuery): Promise<FindMatches>;
  /** Selects and reveals a match from {@link Find.all}. */
  select(match: MatchId): Promise<void>;
  /**
   * Replaces the current match, then finds the next. `replacement` takes Word's `^` codes (`^&` the text found, `^c`
   * the editor's last copy), or a regular expression's `$1` and `${name}` groups; `replaceFormat` is the formatting
   * the replacement gets (with an empty `replacement`, the match keeps its text and only takes the formatting).
   */
  replace(query: FindQuery, replacement: string, replaceFormat?: FindFormat): Promise<FindResult>;
  /** Replaces every match, as {@link Find.replace} does: one transaction. Within the selection, it stays round them. */
  replaceAll(query: FindQuery, replacement: string, replaceFormat?: FindFormat): Promise<FindCount>;
  /**
   * Highlight All: every match highlighted on the pages until the document changes or {@link Find.clearHighlights}
   * (`state.view.findHighlights`); how many. None clears them.
   */
  highlightAll(query: FindQuery): Promise<FindCount>;
  /** Takes Highlight All's highlights away. */
  clearHighlights(): Promise<void>;
}

/** What to find. */
export interface FindQuery {
  /**
   * The text, with Word's `^` codes (`^p` a paragraph mark, `^t` a tab, `^?` any character, `^#` any digit, `^^` a
   * caret), or a regular expression with `regex`. At most 255 characters; empty with a `format` finds the formatting.
   */
  text: string;
  /** Case-sensitive. Default false. */
  matchCase?: boolean;
  /** Whole words only. Default false. */
  wholeWords?: boolean;
  /**
   * `text` is a regular expression, searched paragraph by paragraph. Whole words, prefix, suffix and the ignore
   * options don't apply to it; match case does. Default false.
   */
  regex?: boolean;
  /** A match starts at a word's start. Default false. */
  matchPrefix?: boolean;
  /** A match ends at a word's end. Default false. */
  matchSuffix?: boolean;
  /** Punctuation is passed over. Default false. */
  ignorePunctuation?: boolean;
  /** White space is passed over. Default false. */
  ignoreWhiteSpace?: boolean;
  /** `next` and `previous` wrap round the document's end (default true); off, they stop there with `found` false. */
  wrap?: boolean;
  /** Only within the selection, with no wrapping. Default false. */
  inSelection?: boolean;
  /** The formatting a match must have. */
  format?: FindFormat;
}

/** Character formatting a search looks for, or a replacement applies; each one left out is any. */
export interface FindFont {
  /** The font name, matched ignoring case. */
  font?: string;
  /** Exactly this combination of bold and italic. */
  style?: 'regular' | 'italic' | 'bold' | 'boldItalic';
  /** The size. */
  size?: Points;
  /** The text colour. */
  color?: Color | 'auto';
  /** The underline. */
  underline?: UnderlineStyle;
  /** The underline's colour. */
  underlineColor?: Color | 'auto';
  /** Strikethrough. */
  strikethrough?: boolean;
  /** Double strikethrough. */
  doubleStrikethrough?: boolean;
  /** Superscript. */
  superscript?: boolean;
  /** Subscript. */
  subscript?: boolean;
  /** Small capitals. */
  smallCaps?: boolean;
  /** All capitals. */
  allCaps?: boolean;
  /** Hidden text. */
  hidden?: boolean;
  /** Extra space between characters; negative condenses. */
  characterSpacing?: Points;
  /** The baseline offset; positive raises. */
  position?: Points;
  /** Emboss. */
  emboss?: boolean;
  /** Engrave. */
  engrave?: boolean;
  /** Outline. */
  outline?: boolean;
  /** Shadow. */
  shadow?: boolean;
  /** The character width scale, percent. */
  scale?: number;
  /** Kerning from this size up; 0 for none. */
  kerning?: Points;
  /** The OpenType ligatures. */
  ligatures?: Ligatures;
  /** The number spacing. */
  numberSpacing?: NumberSpacing;
  /** The number forms. */
  numberForm?: NumberForm;
  /** The stylistic set, 1 to 20; 0 for the default. */
  stylisticSet?: number;
  /** Contextual alternates. */
  contextualAlternates?: boolean;
  /** The text fill, or `'none'` for none of the text's own. */
  textFill?: FillValue | 'none';
  /** The text outline, or `'none'`. */
  textOutline?: TextOutline | 'none';
  /** The shadow, or `'none'`. */
  textShadow?: TextShadow | 'none';
  /** The reflection, or `'none'`. */
  textReflection?: TextReflection | 'none';
  /** The glow, or `'none'`. */
  textGlow?: Glow | 'none';
}

/** Paragraph formatting a match's paragraphs must have, or a replacement applies; each one left out is any. */
export interface FindParagraph {
  /** The alignment. */
  alignment?: 'left' | 'center' | 'right' | 'justify';
  /** The outline level, 1 to 9, or body text. */
  outlineLevel?: number | 'body';
  /** The left indent. */
  left?: Points;
  /** The right indent. */
  right?: Points;
  /** The special indent. */
  special?: 'none' | 'firstLine' | 'hanging';
  /** The special indent's size. */
  by?: Points;
  /** The space before, or automatic. */
  before?: Points | 'auto';
  /** The space after, or automatic. */
  after?: Points | 'auto';
  /** The line spacing. */
  lineSpacing?: LineSpacing;
  /** Keep with next. */
  keepWithNext?: boolean;
  /** Keep lines together. */
  keepLinesTogether?: boolean;
  /** Page break before. */
  pageBreakBefore?: boolean;
  /** Widow and orphan control. */
  widowControl?: boolean;
  /** Collapsed by default. */
  collapsed?: boolean;
  /** Mirror indents. */
  mirrorIndents?: boolean;
  /** Don't add space between paragraphs of the same style. */
  contextualSpacing?: boolean;
  /** Suppress line numbers. */
  suppressLineNumbers?: boolean;
  /** Don't hyphenate. */
  dontHyphenate?: boolean;
}

/** Formatting a search looks for (each value given must be the text's), or a replacement applies. */
export interface FindFormat {
  /** Character formatting. */
  font?: FindFont;
  /** Paragraph formatting. */
  paragraph?: FindParagraph;
  /** A paragraph style the paragraphs have, or a character style the text has. */
  style?: StyleId;
  /** Highlighted in any colour, or not. */
  highlight?: boolean;
  /** The colour a replacement highlights in (yellow by default). */
  highlightColor?: HighlightColor;
  /** The language, a BCP 47 tag, matched ignoring case. */
  language?: string;
  /** Spelling and grammar not checked. */
  noProof?: boolean;
  /** Tab stops the paragraphs have (each of these, among others); a replacement adds them. */
  tabs?: { position: Points; alignment?: TabAlignment; leader?: TabLeader }[];
  /** The frame the paragraphs are in; a replacement frames them so. */
  frame?: FrameProperties;
}

/** The result of a find step. */
export interface FindResult {
  /** A match was found and selected. */
  found: boolean;
  /** The search wrapped around the document's end. */
  wrapped: boolean;
  /** The search stopped early: matches past its budget of work were given up. */
  truncated?: boolean;
}

/** Every match, for a results list. */
export interface FindMatches {
  /** The matches, in document order. */
  matches: FindMatch[];
  /** The search stopped early: matches past its budget of work were given up. */
  truncated?: boolean;
}

/** How many matches Replace All replaced or Highlight All highlighted. */
export interface FindCount {
  /** How many. */
  count: number;
  /** The search stopped early: matches past its budget of work were given up. */
  truncated?: boolean;
}

/** A match in a results list. */
export interface FindMatch {
  /** The match, for {@link Find.select}. */
  id: MatchId;
  /** Some text before it. */
  before: string;
  /** The matched text. */
  text: string;
  /** Some text after it. */
  after: string;
  /** Its page. */
  page: number;
  /**
   * The heading it is under (the nearest one `navigation.headings` lists at or before it), for grouping results by
   * heading; absent before the first heading or outside the body.
   */
  heading?: HeadingId;
}

/** Find and replace over a session. */
export function createFind(s: Session): Find {
  const format = (replaceFormat?: FindFormat) => (replaceFormat ? { replaceFormat } : {});
  return {
    next: (query) => s.call(Method.Find, { query, direction: 'next' }),
    previous: (query) => s.call(Method.Find, { query, direction: 'previous' }),
    all: (query) => s.call(Method.FindAll, query),
    select: (match) => s.call(Method.SelectMatch, match),
    replace: (query, replacement, replaceFormat) => s.call(Method.Replace, { query, with: replacement, ...format(replaceFormat) }),
    replaceAll: (query, replacement, replaceFormat) => s.call(Method.ReplaceAll, { query, with: replacement, ...format(replaceFormat) }),
    highlightAll: (query) => s.call(Method.FindHighlightAll, query),
    clearHighlights: () => s.call(Method.FindClearHighlights),
  };
}
