import type { BalloonMode, MarkupStyleChange, RichText } from './review.js';
import type { ControlKind, ControlOptions, DataBinding, PlaceholderImage, ReferenceRule } from './services/elements.js';
import type { MarginSide, TabAlignment, TabLeader, TabSelector } from './services/ruler.js';
import type { NewStyle, StyleChange } from './services/navigation.js';
import type { ViewOptions } from './services/view.js';
import type {
  AutoCorrectChange,
  HighlightColor,
  HorizontalPosition,
  LineSpacing,
  MarkupView,
  ObjectWrap,
  RelativeSize,
  RowHeight,
  Shading,
  TableFit,
  TablePosition,
  TightWrap,
  TableWidth,
  UnderlineStyle,
  VerticalPosition,
  WrapSide,
} from './state.js';
import type {
  Arrange,
  BoxPoint,
  Color,
  ColourValue,
  CommentId,
  CustomProperty,
  Degrees,
  EffectsChange,
  ElementId,
  FillValue,
  FlipAxis,
  HeadingId,
  LineChange,
  ObjectId,
  PictureAdjustChange,
  Recolor,
  Points,
  RevisionId,
  TextEffectsChange,
  Watermark,
  ShapeAdjust,
  ShapeInsert,
  ShapePreset,
  ShapeTextChange,
  StyleId,
  CellBlock,
  TextRange,
} from './types.js';

/** One of the theme's two fonts: its heading font (`+Headings`) or its body font (`+Body`). */
export type ThemeFont = 'headings' | 'body';

/** A font: a family by name, or the theme's heading or body font, which follows the theme. */
export type FontChoice = string | { theme: ThemeFont };

/**
 * A text or table colour as commands take it: an RGB colour (`'#RRGGBB'` or `{ rgb }`), or a theme colour with at
 * most one tint or shade (`{ theme: 'accent1', transforms: [{ kind: 'tint', value: 0.4 }] }`).
 */
export type TextColour = Color | ColourValue;

/** The sections a page setup command applies to: the selection's, every section, or from the selection on. */
export type ApplyTo = 'section' | 'wholeDocument' | 'thisPointForward';

/** Where `setColumns` applies: the Columns dialog's choices, and the selected text alone. */
export type ColumnsApplyTo = ApplyTo | 'selectedText';

/** The outline commands' options. */
export interface OutlineOptions {
  /** The headings under each heading changed move by as many levels (each kept to 1 to 9). Default: false. */
  subtopics?: boolean;
}

/** One of the separators between the text and the notes. */
export type NoteSeparatorPart = 'separator' | 'continuationSeparator' | 'continuationNotice';

/** A group (`everyone`, `administrators`, `contributors`, `editors`, `owners`, `current`) or a person (a name or email address). */
export type RangeEditorRef = { group: string } | { user: string };

/** The document's default formatting: what text has before any style's. */
export interface DocumentDefaults {
  /** The font: a family, or the theme's heading or body font. */
  font?: FontChoice;
  /** The size. */
  size?: Points;
  /** The font colour. */
  color?: TextColour | 'auto';
  /** The space before and after paragraphs. */
  spacing?: { before?: Points | 'auto'; after?: Points | 'auto' };
  /** The line spacing. */
  lineSpacing?: LineSpacing;
  /** The left and right indents. */
  indents?: { left?: Points; right?: Points; firstLine?: Points; hanging?: Points };
}

/** A list level's number style. */
export type NumberFormat =
  | 'decimal'
  | 'upperRoman'
  | 'lowerRoman'
  | 'upperLetter'
  | 'lowerLetter'
  | 'ordinal'
  | 'cardinalText'
  | 'ordinalText'
  | 'decimalZero'
  | 'bullet'
  | 'none'
  | 'numberInDash'
  | 'hex'
  | 'chicago'
  | 'russianLower'
  | 'russianUpper'
  | 'hindiVowels'
  | 'hindiConsonants'
  | 'hindiNumbers'
  | 'thaiLetters'
  | 'thaiNumbers'
  /** Another style by its WordprocessingML name (1 to 64 ASCII letters and digits), numbered in decimal. */
  | { other: string };

/** What follows a list level's number. */
export type FollowNumber = 'tab' | 'space' | 'nothing';

/** A list gallery entry, by its id. */
export type ListEntryId = string;

/** A list level: its number or bullet, and where it and the text start. */
export type ListLevel = {
  /** The number style. */
  format: NumberFormat;
  /** The number format, with `%1` to `%9` for the numbers of this level and those above it; a bullet's character. */
  text: string;
  /** The number it starts at. */
  start: number;
  /** How the number aligns at `numberPosition`. */
  alignment: 'left' | 'center' | 'right';
  /** What follows the number. */
  follow: FollowNumber;
  /** The tab stop after the number; absent uses the text indent. */
  tab?: Points;
  /** Where the number is, from the margin. */
  numberPosition: Points;
  /** Where the text is, from the margin. */
  textIndent: Points;
  /** The level after which it restarts (1 to 9), 0 never; absent after the level above. */
  restartAfter?: number;
  /** Legal style numbering. */
  legal: boolean;
  /** The paragraph style linked to the level. */
  linkedStyle?: StyleId;
  /** The number's or bullet's font. */
  font?: string;
  /** A picture bullet's image (PNG, JPEG or GIF): bytes, or base64 as the editor gives it. */
  picture?: Uint8Array | string;
  /** The number's own bold (absent: the paragraph's). Setting a level, an absent one is cleared, as the others below. */
  bold?: boolean;
  /** The number's own italic. */
  italic?: boolean;
  /** The number's own underline. */
  underline?: UnderlineStyle;
  /** The number's own size, 1 to 1638. */
  size?: Points;
  /** The number's own colour, or automatic. */
  color?: Color | 'auto';
  /** The number's own highlight, or none. */
  highlight?: HighlightColor | 'none';
};

/** A border line's style. */
export type BorderLineStyle =
  | 'single'
  | 'double'
  | 'dotted'
  | 'dashed'
  | 'thick'
  | 'dotDash'
  | 'dotDotDash'
  | 'triple'
  | 'thinThickSmallGap'
  | 'thickThinSmallGap'
  | 'thinThickThinSmallGap'
  | 'dashSmallGap'
  | 'dashDotStroked'
  | 'thinThickMediumGap'
  | 'thickThinMediumGap'
  | 'thinThickThinMediumGap'
  | 'thinThickLargeGap'
  | 'thickThinLargeGap'
  | 'thinThickThinLargeGap'
  | 'wave'
  | 'doubleWave'
  | 'threeDEmboss'
  | 'threeDEngrave'
  | 'outset'
  | 'inset';

/** The pointer's table tool: none, the Draw Table pen, or the eraser. */
export type TableTool = 'none' | 'pen' | 'eraser';

/** A border line. */
export interface BorderLine {
  /** The line style. */
  style: BorderLineStyle;
  /** The width, 0.25 to 6 pt. */
  width: Points;
  /** The colour, or automatic. The editor reads it back as a {@link ColourValue}. */
  color: TextColour | 'auto';
  /**
   * The distance from what it borders, 0 to 31 (a page border measured from the page, from the page's edge). Default:
   * 0 for text and cells, 1 for paragraphs, and for a page border 24 from the page's edge, 1 (top and bottom) or 4
   * (left and right) from the text; a kept page border edge keeps its own.
   */
  space?: Points;
  /** A shadow. Default: none. */
  shadow?: boolean;
}

/** OpenType ligatures, each kind on or off. */
export interface LigatureFlags {
  /** Standard ligatures (`liga`). */
  standard?: boolean;
  /** Contextual ligatures (`clig`). */
  contextual?: boolean;
  /** Historical ligatures (`hlig`). */
  historical?: boolean;
  /** Discretionary ligatures (`dlig`). */
  discretionary?: boolean;
}

/**
 * A shading pattern (`ST_Shd`): none, clear (the fill only), solid (the pattern color), a percentage of the pattern
 * color over the fill, or stripes and crosses of it over the fill. `other` is a pattern the document names that isn't
 * listed: reading gives it, and setting it keeps each target's own pattern.
 */
export type ShadingPattern =
  | 'nil'
  | 'clear'
  | 'solid'
  | 'pct5'
  | 'pct10'
  | 'pct12'
  | 'pct15'
  | 'pct20'
  | 'pct25'
  | 'pct30'
  | 'pct35'
  | 'pct37'
  | 'pct40'
  | 'pct45'
  | 'pct50'
  | 'pct55'
  | 'pct60'
  | 'pct62'
  | 'pct65'
  | 'pct70'
  | 'pct75'
  | 'pct80'
  | 'pct85'
  | 'pct87'
  | 'pct90'
  | 'pct95'
  | 'horzStripe'
  | 'vertStripe'
  | 'reverseDiagStripe'
  | 'diagStripe'
  | 'horzCross'
  | 'diagCross'
  | 'thinHorzStripe'
  | 'thinVertStripe'
  | 'thinReverseDiagStripe'
  | 'thinDiagStripe'
  | 'thinHorzCross'
  | 'thinDiagCross'
  | 'other';

/** Where a section starts. */
export type SectionBreakKind = 'nextPage' | 'continuous' | 'evenPage' | 'oddPage' | 'nextColumn';

/** A column's width and the space after it. */
export interface ColumnWidth {
  /** The width. */
  width: Points;
  /** The space after it (none after the last). */
  spacing: Points;
}

/** Chapter numbers in page numbers: the number of the last heading of a level, then a separator. */
export interface ChapterNumbers {
  /** The heading level whose number leads (1 to 9). */
  level: number;
  /** Between the chapter and page numbers. */
  separator: 'hyphen' | 'period' | 'colon' | 'emDash' | 'enDash';
}

/** A section's line numbering. */
export interface LineNumbers {
  /** The first number (1 to 32767). Default: 1. */
  start?: number;
  /** Shows every nth number (1 to 100). Default: 1. */
  countBy?: number;
  /** From the text (0 to 1584 pt); automatic when not given. */
  distance?: Points;
  /** When numbering starts again. Default: each page. */
  restart?: 'eachPage' | 'eachSection' | 'continuous';
}

/** Where notes go: footnotes at the page's bottom or beneath the text, endnotes at a section's or the document's end. */
export type NotePosition = 'pageBottom' | 'beneathText' | 'sectionEnd' | 'documentEnd';

/** A section's footnote or endnote settings. */
export interface NoteSettings {
  /** Where the notes go. */
  position: NotePosition;
  /** The number format. */
  format: NumberFormat;
  /** The first number. */
  start: number;
  /** When numbering starts again. */
  restart: 'continuous' | 'eachSection' | 'eachPage';
}

/** How the document's pages pair. */
export type MultiplePagesMode = 'normal' | 'mirrorMargins' | 'bookFold' | 'reverseBookFold';

/** What a text form field takes. */
export type TextInputType = 'regular' | 'number' | 'date' | 'currentDate' | 'currentTime' | 'calculation';

/** The edges `setCellBorders` sets: sides, the inside lines, or each cell's diagonals. */
export type CellBorderEdges =
  | 'all'
  | 'outside'
  | 'inside'
  | 'insideHorizontal'
  | 'insideVertical'
  | 'top'
  | 'bottom'
  | 'left'
  | 'right'
  | 'diagonalDown'
  | 'diagonalUp'
  | 'none';

/**
 * Which way text flows: lines left to right, top to bottom (`lrTb`); vertical lines right to left (`tbRl`) or bottom to
 * top (`btLr`); and their East Asian forms with characters turned (`lrTbV`, `tbRlV`, `tbLrV`).
 */
export type TextFlow = 'lrTb' | 'tbRl' | 'btLr' | 'lrTbV' | 'tbRlV' | 'tbLrV';

/** The ligatures text may use (Word's choices). */
export type Ligatures = 'none' | 'standard' | 'standardContextual' | 'historicalDiscretional' | 'all';

/** The digits' widths: the font's, proportional or tabular. */
export type NumberSpacing = 'default' | 'proportional' | 'tabular';

/** The digits' forms: the font's, lining or old style. */
export type NumberForm = 'default' | 'lining' | 'oldStyle';

/** The scale, kerning and OpenType features to set (the Font dialog's Advanced tab); what isn't given is kept. */
export interface Typography {
  /** The horizontal scale, percent (1…600). */
  scale?: number;
  /** Kerning for text at or above this size (0…1638 pt); 0 turns it off. */
  kerning?: Points;
  /** Which ligatures the font may use. */
  ligatures?: Ligatures;
  /** The digits' widths. */
  numberSpacing?: NumberSpacing;
  /** The digits' forms. */
  numberForm?: NumberForm;
  /** The font's stylistic set (1…20), or its default (0). */
  stylisticSet?: number;
  /** The font's contextual alternates. */
  contextualAlternates?: boolean;
  /** Each kind of ligature on or off, over `ligatures`. */
  ligatureFlags?: LigatureFlags;
  /** The stylistic sets on (each 1 to 20; empty: none), over `stylisticSet`. */
  stylisticSets?: number[];
}

/** The text's language and proofing to set; what isn't given is kept. */
export interface Language {
  /** The language, a BCP 47 tag (`en-GB`). */
  language?: string;
  /** Do not check spelling or grammar. */
  noProof?: boolean;
}

/** Character effects to set; what isn't given is kept. Emboss and engrave exclude each other. */
export interface CharacterEffects {
  /** Raised relief. */
  emboss?: boolean;
  /** Sunken relief. */
  engrave?: boolean;
  /** The letters' outlines only. */
  outline?: boolean;
  /** A shadow behind the letters. */
  shadow?: boolean;
}

/** A paragraph's drop cap: where its first letter goes, its font, how many lines it drops and its distance from text. */
export type DropCap = {
  position: 'none' | 'dropped' | 'inMargin';
  /** The letter's font; default: the text's. */
  font?: string;
  /** Lines it drops, 1 to 10; default 3. */
  lines?: number;
  /** How far the text keeps from it. */
  distance?: Points;
};

/** A frame's wrapping, size, position and distance from text. */
export type FrameProperties = {
  /** Text wrapping: around the frame, or above and below it (`'none'`). */
  wrap: 'none' | 'around';
  /** How the width is set. */
  widthRule: 'auto' | 'exactly';
  /** The width, when exact. */
  width: Points;
  /** How the height is set. */
  heightRule: 'auto' | 'atLeast' | 'exactly';
  /** The height, unless automatic. */
  height: Points;
  /** The horizontal alignment, or `null` for `horizontal`. */
  horizontalAlign: 'left' | 'center' | 'right' | 'inside' | 'outside' | null;
  /** The horizontal position, from `horizontalRelative`. */
  horizontal: Points;
  /** What the horizontal position is relative to. */
  horizontalRelative: 'margin' | 'page' | 'column';
  /** The distance from text, left and right. */
  horizontalDistance: Points;
  /** The vertical alignment, or `null` for `vertical`. */
  verticalAlign: 'top' | 'center' | 'bottom' | 'inside' | 'outside' | null;
  /** The vertical position, from `verticalRelative`. */
  vertical: Points;
  /** What the vertical position is relative to: the paragraph moves it with the text. */
  verticalRelative: 'margin' | 'page' | 'paragraph';
  /** The distance from text, above and below. */
  verticalDistance: Points;
  /** The anchor stays with its paragraph. */
  lockAnchor: boolean;
  /** Other frames and floating objects may overlap it (`false`: kept apart); absent keeps it. `setFrame` only. */
  allowOverlap?: boolean;
};

/** Which elements a field command acts on: the given ids, or the selection's by default. */

export interface FieldTargets {
  /** The fields; default: the selection's. */
  ids?: ElementId[];
}

/**
 * Every edit, one method per command. Each resolves after its change commits, with `getState()` already at that
 * revision, and rejects with `EditorError`. Commands act on the selection at the time they run. Each is one undo
 * step unless its documentation says otherwise: *typing* joins consecutive `insertText` and `delete` calls in one
 * paragraph into one step; *view* changes no document and adds no undo step.
 */
export interface Commands {
  // Text
  /** Inserts text at the selection, replacing it. Typing. */
  insertText(text: string): Promise<void>;
  /** Starts a new paragraph (Enter). */
  insertParagraph(): Promise<void>;
  /**
   * Inserts a line break (Shift+Enter), or with `clear` a text wrapping break: the text after it goes below the
   * floating objects on that side.
   */
  insertLineBreak(options?: { clear?: 'left' | 'right' | 'both' }): Promise<void>;
  /** Inserts a page break (Ctrl+Enter); in the body, not in a table. */
  insertPageBreak(): Promise<void>;
  /** Inserts a column break (Ctrl+Shift+Enter); in the body, not in a table. */
  insertColumnBreak(): Promise<void>;
  /** Inserts a section break; in the body, not in a table. */
  insertSectionBreak(kind: SectionBreakKind): Promise<void>;
  /**
   * Inserts a style separator (Ctrl+Alt+Enter): a hidden paragraph mark that runs the paragraph before it into the one
   * after, each keeping its own style. At a paragraph's end that paragraph's mark becomes one; elsewhere the paragraph
   * splits at the caret first. One undo step.
   */
  insertStyleSeparator(): Promise<void>;
  /** Inserts a tab character. Typing. */
  insertTab(): Promise<void>;
  /** Inserts one character, optionally in a symbol font. */
  insertSymbol(options: { char: string; font?: string }): Promise<void>;
  /**
   * Alt+X: the hexadecimal code before the caret (up to six digits, `U+` taken too) becomes its character, or the
   * character before the caret (or the selected one) becomes its code. One undo step.
   */
  toggleCharacterCode(): Promise<void>;
  /** Deletes backward or forward, by character (the default) or word. Typing. */
  delete(options: { direction: 'backward' | 'forward'; unit?: 'character' | 'word' }): Promise<void>;

  // Clipboard and history
  /** Cuts the selection to the clipboard. */
  cut(): Promise<void>;
  /** Copies the selection to the clipboard; also when read only. */
  copy(): Promise<void>;
  /**
   * Pastes from the system clipboard, as the paste options say for where it came from (`formatted`, the default), or
   * as `as` takes it (Paste Special's formats and Paste Options' choices: refused when the clipboard holds nothing to
   * paste that way). Fails `not-allowed` where the browser denies reading it. Headless, it pastes the editor's own last
   * copy.
   */
  paste(options?: { as?: PasteAs }): Promise<void>;
  /** Pastes the given text. */
  pasteText(text: string): Promise<void>;
  /**
   * Pastes the paste just made again as `as` takes it, in place of it (the Paste Options button's choices, popover
   * `pasteOptions`): refused when there is no paste to change or it can't be pasted that way. One undo step takes the
   * document back to before the first paste.
   */
  repaste(as: PasteAs): Promise<void>;
  /** Hides the Paste Options button of the paste just made. Not an edit. */
  dismissPasteOptions(): Promise<void>;
  /**
   * What a paste does when it isn't told, and what goes with it (Word's cut, copy and paste options; the state's
   * `pasteOptions`): the options given change. A session setting, never saved; also when read only.
   */
  setPasteOptions(change: PasteOptionsChange): Promise<void>;
  /** Inserts a DOCX's content in place of the selection (Insert Text from File): its paragraphs, tables, styles and sections. */
  insertFile(options: { from: Uint8Array }): Promise<void>;
  /**
   * F2: the selected text moves to the place the next click or arrow keys choose, when Enter is pressed
   * (`selection.pendingMove` while it waits); Esc or any other command gives up.
   */
  moveText(): Promise<void>;
  /** Shift+F2: as {@link Commands.moveText}, copying the text. */
  copyText(): Promise<void>;
  /** Undoes the last `count` steps (default 1; more than there are undoes them all), as one change. */
  undo(options?: { count?: number }): Promise<void>;
  /** Redoes the last `count` undone steps (default 1; more than there are redoes them all), as one change. */
  redo(options?: { count?: number }): Promise<void>;
  /** Repeats the last edit (F4). */
  repeat(): Promise<void>;

  // Selection and navigation: never change the document
  /** Selects the whole story. */
  selectAll(): Promise<void>;
  /** Selects the unit around the selection; the table units in a table. */
  select(unit: 'word' | 'sentence' | 'paragraph' | 'cell' | 'row' | 'column' | 'table'): Promise<void>;
  /** Moves or extends the selection. */
  moveSelection(options: {
    by: 'character' | 'word' | 'line' | 'paragraph' | 'lineEdge' | 'document' | 'screen' | 'page' | 'rowEdge' | 'columnEdge' | 'windowEdge';
    direction: 'backward' | 'forward';
    extend?: boolean;
  }): Promise<void>;
  /** Moves to the next or previous cell; `next` from the last cell adds a row (one step). */
  moveToCell(direction: 'next' | 'previous'): Promise<void>;
  /** Sets the selection mode (F8 extends, Ctrl+Shift+F8 selects a block). */
  setSelectionMode(mode: 'normal' | 'extend' | 'block'): Promise<void>;
  /** Returns to an earlier edit location (Shift+F5). */
  goBack(): Promise<void>;
  /** Leaves a header or footer, a text box, or object selection (Esc). */
  escape(): Promise<void>;
  /** Selects the next or previous floating object. */
  selectNextObject(direction: 'next' | 'previous'): Promise<void>;
  /** Selects the whole next or previous field in the story, outermost fields only (F11, Shift+F11). */
  selectField(direction: 'next' | 'previous'): Promise<void>;
  /**
   * Selects ranges of text and blocks of cells of one story, a discontiguous selection when there are several
   * (`selection.ranges` and `selection.cellBlocks` read it back): overlapping and touching ranges join, empty ones go.
   * The last range, or the `activeCells` block, is the selection the next command or gesture goes on from; when every
   * range is empty, the caret goes to the last one's end. At most 100,000 ranges and blocks. Not an edit: no undo step.
   */
  setSelection(options: { ranges: TextRange[]; cells?: CellBlock[]; activeCells?: number }): Promise<void>;
  /**
   * Shrink Selection (Shift+F8), in extend mode: the selection one level smaller around where extending started (the
   * story to the paragraph, the sentence, the word, then the caret), still extending.
   */
  shrinkSelection(): Promise<void>;
  /**
   * Select Text with Similar Formatting: every stretch of text in the story with the same character formatting as the
   * selection's first character (or the caret's), as a discontiguous selection.
   */
  selectSimilarFormatting(): Promise<void>;
  /**
   * Selects every use of a paragraph or character style in the main document: its paragraphs (marks included,
   * consecutive ones as one range) or its stretches of runs (`StyleInfo.uses` counts them).
   */
  selectStyleInstances(style: StyleId): Promise<void>;
  /** Moves the caret to a page, from 1. */
  goToPage(page: number): Promise<void>;
  /** Selects a bookmark's text, or puts the caret at it when it holds none (Go To); `'_top'` is the document's start. */
  goToBookmark(name: string): Promise<void>;
  /**
   * Go To: the n-th page, section, line, bookmark, comment, note, field, table, graphic, equation, embedded object or
   * heading (`{ absolute: n }`, from 1; numbers past the last go to the last), n on or back (`{ relative: n }`), or the
   * next or previous. Pages and sections count from the selection's page and section, the others by position from
   * the selection's start. `name`: a bookmark's (with `absolute`, that bookmark), a comment's reviewer, a field's kind
   * (its code's first word), an embedded object's program id. Fails `unavailable` when there is none to go to.
   */
  goTo(options: { what: GoToWhat; target: GoToTarget; name?: string }): Promise<void>;

  // Character formatting
  /** Sets bold (Ctrl+B). */
  setBold(on: boolean): Promise<void>;
  /** Sets italic (Ctrl+I). */
  setItalic(on: boolean): Promise<void>;
  /** Sets the underline style (Ctrl+U: single). */
  setUnderline(style: UnderlineStyle): Promise<void>;
  /** Sets strikethrough. */
  setStrikethrough(on: boolean): Promise<void>;
  /** Sets double strikethrough. */
  setDoubleStrikethrough(on: boolean): Promise<void>;
  /** Sets subscript (Ctrl+=). */
  setSubscript(on: boolean): Promise<void>;
  /** Sets superscript (Ctrl+Shift+=). */
  setSuperscript(on: boolean): Promise<void>;
  /** Sets small caps (Ctrl+Shift+K). */
  setSmallCaps(on: boolean): Promise<void>;
  /** Sets all caps (Ctrl+Shift+A). */
  setAllCaps(on: boolean): Promise<void>;
  /** Sets hidden text (Ctrl+Shift+H). */
  setHidden(on: boolean): Promise<void>;
  /** Sets the font family (non-empty), or the theme's heading or body font (`{ theme }`), which follows the theme. */
  setFont(font: FontChoice): Promise<void>;
  /** Sets the font size, 1 to 1638 in 0.5 steps. */
  setFontSize(size: Points): Promise<void>;
  /** Grows the font to the next standard size (Ctrl+Shift+&gt;). */
  growFont(): Promise<void>;
  /** Shrinks the font to the previous standard size (Ctrl+Shift+&lt;). */
  shrinkFont(): Promise<void>;
  /** Each selected run's size a point larger, up to 1638 (Ctrl+]). */
  growFontOnePoint(): Promise<void>;
  /** Each selected run's size a point smaller, down to 1 (Ctrl+[). */
  shrinkFontOnePoint(): Promise<void>;
  /** Sets the font colour: RGB, a theme colour with at most one tint or shade, or automatic. */
  setFontColor(color: TextColour | 'auto'): Promise<void>;
  /** Sets or removes (`null`) the highlight. */
  setHighlight(color: HighlightColor | null): Promise<void>;
  /** Sets the character spacing, -1584 to 1584. */
  setCharacterSpacing(spacing: Points): Promise<void>;
  /** Raises (positive) or lowers the text, -1584 to 1584. */
  setCharacterPosition(position: Points): Promise<void>;
  /** Applies a character style to the selected text (a linked paragraph style's character half), or removes it (`null`). */
  applyCharacterStyle(styleId: StyleId | null): Promise<void>;
  /** Sets the scale, kerning and OpenType features; what isn't given is kept. */
  setTypography(typography: Typography): Promise<void>;
  /**
   * Fits the selected text to `width` (more than 0, at most 1584; Fit Text), or lets it take its own width (`null`). A
   * caret in fitted text acts on its whole region; elsewhere a caret is `unavailable`.
   */
  setFitText(width: Points | null): Promise<void>;
  /** Sets the text's language and whether spelling is checked; what isn't given is kept. */
  setLanguage(language: Language): Promise<void>;
  /** Sets emboss, engrave, outline and shadow; what isn't given is kept. */
  setCharacterEffects(effects: CharacterEffects): Promise<void>;
  /**
   * Sets the text's fill, outline, shadow, glow and reflection: each one given replaces the text's own, `null` turns it
   * off (over a style's too; a fill is only removed), and what isn't given is kept.
   */
  setTextEffects(change: TextEffectsChange): Promise<void>;
  /** Sets the underline's colour, or the text's (`'auto'`); underlined text only. */
  setUnderlineColor(color: TextColour | 'auto'): Promise<void>;
  /** Clears character formatting (Ctrl+Space). */
  clearFormatting(): Promise<void>;
  /**
   * Clear All Formatting: the selected paragraphs take the default paragraph style and lose their direct paragraph and
   * character formatting; highlight and revision marks stay.
   */
  clearAllFormatting(): Promise<void>;
  /**
   * Changes the case of the selection, or of the word at a collapsed caret. Shift+F3 cycles lower, UPPER, Capitalize Each
   * Word (Sentence case for a whole sentence).
   */
  changeCase(to: 'lower' | 'upper' | 'sentence' | 'title' | 'toggle'): Promise<void>;
  /** Holds the selection's formatting for {@link Commands.pasteFormatting} (Ctrl+Shift+C). */
  copyFormatting(): Promise<void>;
  /**
   * Applies the held formatting to the selection (Ctrl+Shift+V). A host forwards Ctrl+Shift+C (some browsers take it for
   * their inspector); the first Ctrl+Shift+V paste after Copy Formatting pastes formatting, whatever was copied in
   * another application in between.
   */
  pasteFormatting(): Promise<void>;

  // Paragraph formatting
  /** Sets the alignment (Ctrl+L, E, R, J). */
  setAlignment(alignment: 'left' | 'center' | 'right' | 'justify'): Promise<void>;
  /** Sets the line spacing: 0.06 to 132 lines, or 0.7 to 1584 points. */
  setLineSpacing(spacing: LineSpacing): Promise<void>;
  /** Sets the space before and after, 0 to 1584; unset fields keep their value. */
  setParagraphSpacing(options: {
    before?: Points | 'auto';
    after?: Points | 'auto';
    /** No space between paragraphs of the same style. */
    contextual?: boolean;
  }): Promise<void>;
  /** Sets the indents, -1584 to 1584; `firstLine` and `hanging` are exclusive. Unset fields keep their value. */
  setIndents(options: {
    left?: Points;
    right?: Points;
    firstLine?: Points;
    hanging?: Points;
    /** Mirror indents: on facing pages `left` is the inside indent and `right` the outside. */
    mirror?: boolean;
  }): Promise<void>;
  /**
   * Increases the indent (Ctrl+M): list items go a level down, or the whole list moves from its first item; other
   * paragraphs go to the next default tab stop.
   */
  increaseIndent(): Promise<void>;
  /**
   * Decreases the indent (Ctrl+Shift+M): list items go a level up, or the whole list moves back from its first item; other
   * paragraphs go to the previous default tab stop.
   */
  decreaseIndent(): Promise<void>;
  /** A hanging indent to the next default tab stop: the left indent moves, the first line stays (Ctrl+T). */
  increaseHangingIndent(): Promise<void>;
  /** The hanging indent back to the previous default tab stop (Ctrl+Shift+T). */
  decreaseHangingIndent(): Promise<void>;
  /** Reset Paragraph: removes direct paragraph formatting, numbering included; the style stays (Ctrl+Q). */
  clearParagraphFormatting(): Promise<void>;
  /** Moves the selected paragraphs above the one before or below the one after (Alt+Shift+Up, Down); tracked, a move. */
  moveParagraphs(direction: 'up' | 'down'): Promise<void>;
  /** Applies an existing paragraph style. */
  setParagraphStyle(styleId: StyleId): Promise<void>;
  /** Changes a style to the selection's formatting (its paragraph's and its first run's direct formatting). */
  updateStyleToSelection(styleId: StyleId): Promise<void>;
  /**
   * Deletes a style the document made: its paragraphs take the default paragraph style, its runs lose it, and the
   * styles based on it take its base. Built-in styles can't be deleted (`not-allowed`).
   */
  deleteStyle(styleId: StyleId): Promise<void>;
  /** Sets a style's list flags, a built-in style the document doesn't define too; what isn't given is kept. */
  setStyleMeta(options: { id: StyleId; visibility?: 'show' | 'hideUntilUsed' | 'hide'; priority?: number; quick?: boolean; locked?: boolean }): Promise<void>;
  /** Sets the document's default formatting (Manage Styles' Set Defaults); what isn't given is kept. */
  setDocumentDefaults(defaults: DocumentDefaults): Promise<void>;
  /**
   * Sets the document's default formatting with any character and paragraph formatting ops but `setFrame` and
   * `removeFrame`, run on the defaults as on a style; every style's text lays out again. One undo step.
   */
  setDefaultFormatting(commands: NestedOp[]): Promise<void>;
  /**
   * Makes a style (New Style): its type, name, base, following style, link, list and formatting (character and
   * paragraph ops, and for a table style its table ops and parts'), from the selection's formatting or not, applied to
   * the selection or not. Resolves with its id. `invalid-argument` for a name that's empty, taken or a built-in
   * style's, a base of another type or that loops, and formatting the type doesn't take. One undo step.
   */
  createStyle(options: NewStyle): Promise<{ styleId: StyleId }>;
  /**
   * Changes a style (Modify Style); a built-in style the document lacks joins it. Renaming a built-in style adds an
   * alias; linking sets both sides and unlinks their old partners; a linked style's character formatting goes to its
   * other half too. `unavailable` for no such style.
   */
  modifyStyle(options: { id: StyleId; change: StyleChange }): Promise<void>;
  /** Makes the paragraphs a bullet or numbered list, or removes the list (`null`). */
  setList(kind: 'bullet' | 'numbered' | null): Promise<void>;
  /** Changes the list level (Tab, Shift+Tab at an item's start); in a list. */
  setListLevel(change: 'increase' | 'decrease'): Promise<void>;
  /** Sets the pagination flags; unset fields keep their value. */
  setParagraphFlow(options: {
    keepWithNext?: boolean;
    keepLinesTogether?: boolean;
    pageBreakBefore?: boolean;
    widowControl?: boolean;
    /** Don't hyphenate the paragraph. */
    dontHyphenate?: boolean;
    /** Leaves the paragraph's lines out of line numbering. */
    suppressLineNumbers?: boolean;
    /** In a text box, which lines text around the box wraps tightly to. */
    textboxTightWrap?: TightWrap;
  }): Promise<void>;
  /** Sets the outline level (1 to 9, or 0 for body text) and whether the paragraphs open collapsed; unset fields keep their value. */
  setOutline(options: { level?: number; collapsed?: boolean }): Promise<void>;
  /**
   * Promote (Alt+Shift+Left): each selected paragraph a level up; body text takes the level of the heading before it
   * (Heading 1 when none). A direct outline level changes in place, else the paragraph takes built-in Heading N. With
   * `subtopics` the headings under each heading changed move by as many levels.
   */
  promoteOutline(options?: OutlineOptions): Promise<void>;
  /** Demote (Alt+Shift+Right): each selected heading a level down; Heading 9 and body text stay. */
  demoteOutline(options?: OutlineOptions): Promise<void>;
  /** Promote to Heading 1: the selected paragraphs take Heading 1, their direct outline level removed. */
  promoteToHeading1(options?: OutlineOptions): Promise<void>;
  /** Demote to Body Text: the selected paragraphs take the default paragraph style, their direct outline level removed. */
  demoteToBodyText(): Promise<void>;
  /** A navigation pane's Promote on one heading; its subheadings move with it unless `subtopics` is false. */
  promoteHeading(options: { heading: HeadingId; subtopics?: boolean }): Promise<void>;
  /** A navigation pane's Demote on one heading; its subheadings move with it unless `subtopics` is false. */
  demoteHeading(options: { heading: HeadingId; subtopics?: boolean }): Promise<void>;
  /**
   * Moves a heading with its content before another heading, or after that heading's content, optionally at a new
   * level (1 to 9; its subheadings follow by as many levels). Tracked as a move while tracking; the caret goes to the
   * moved heading's start.
   */
  moveHeading(options: { heading: HeadingId; target: HeadingId; side: 'before' | 'after'; level?: number }): Promise<void>;
  /** Deletes a heading and its content. */
  deleteHeading(heading: HeadingId): Promise<void>;
  /** An empty heading at a heading's level just before it, the caret in it. */
  insertHeadingBefore(heading: HeadingId): Promise<void>;
  /** An empty heading at a heading's level after its content, the caret in it. */
  insertHeadingAfter(heading: HeadingId): Promise<void>;
  /** An empty heading a level below a heading, after its content, the caret in it. */
  insertSubheading(heading: HeadingId): Promise<void>;
  /** Selects a heading and its content (for Print Heading and Content, print the selection). */
  selectHeadingContent(heading: HeadingId): Promise<void>;
  /** Sets the selected paragraphs' text direction. */
  setParagraphTextDirection(direction: TextFlow): Promise<void>;
  /** Sets the paragraph borders (a half-point single line). */
  setParagraphBorders(preset: 'box' | 'bottom' | 'none'): Promise<void>;
  /** Sets (a clear fill) or removes (`null`) the paragraph shading. */
  setParagraphShading(color: TextColour | null): Promise<void>;
  /** Sets the selected paragraphs' border lines: each edge given is set, or removed (`null`); the rest are kept. */
  setParagraphBorderLines(options: {
    top?: BorderLine | null;
    bottom?: BorderLine | null;
    left?: BorderLine | null;
    right?: BorderLine | null;
    /** The line between paragraphs with the same borders. */
    between?: BorderLine | null;
    /** A shadow on the bottom and right edges set here whose line gives none of its own. */
    shadow?: boolean;
  }): Promise<void>;
  /** Shades the selected paragraphs, text, cells or tables (a table's shows around cells with spacing): the fill, and a pattern of `color` over it. `fill: 'none'` with a clear pattern removes the shading. */
  setShading(options: { target: 'paragraph' | 'text' | 'cell' | 'table'; fill: TextColour | 'none'; pattern?: ShadingPattern; color?: TextColour | 'auto' }): Promise<void>;
  /** Sets or removes (`border: null`) the selected text's border: a box of one line, with a shadow or not (`shadow`, when the line gives none). */
  setTextBorder(options: { border: BorderLine | null; shadow?: boolean }): Promise<void>;
  /**
   * Sets the page borders of the selection's section, or of every section: each edge given is set or removed (`null`),
   * the rest kept; what they're measured from, and the pages they show on.
   */
  setPageBorders(options: {
    top?: BorderLine | null;
    bottom?: BorderLine | null;
    left?: BorderLine | null;
    right?: BorderLine | null;
    /** A shadow on the bottom and right edges. */
    shadow?: boolean;
    offsetFrom?: 'page' | 'text';
    display?: 'allPages' | 'firstPage' | 'notFirstPage';
    applyTo?: 'wholeDocument' | 'thisSection';
    /** The borders go around the header (every section's). */
    surroundHeader?: boolean;
    /** The borders go around the footer (every section's). */
    surroundFooter?: boolean;
    /** The borders are behind the text and objects they meet (Word's Always display in front, off). */
    behindText?: boolean;
  }): Promise<void>;
  /**
   * Sets the page colour of the document: no fill, a solid colour (a theme colour takes one tint or shade), a gradient,
   * a pattern, or a picture (`tile` makes it a texture). A picture fill with an empty image keeps the page's own.
   */
  setPageColor(fill: FillValue): Promise<void>;
  /** Sets the document's watermark, in every header: the text or the picture replaces the one there. */
  setWatermark(watermark: Watermark): Promise<void>;
  /** Removes the document's watermark. */
  removeWatermark(): Promise<void>;
  /** Sets the properties of the frame the selection is in (`unavailable` outside a frame). */
  setFrame(properties: FrameProperties): Promise<void>;
  /** Removes the frame the selection is in: its paragraphs go back in the text, before the paragraph it was anchored to. */
  removeFrame(): Promise<void>;
  /** The selection's paragraph's drop cap: its first letter dropped into the text or in the margin, or put back (`'none'`). */
  setDropCap(dropCap: DropCap): Promise<void>;
  /**
   * Defines the selection's list levels (level `k` of `levels` is level `k`; the levels after them are kept) for the
   * whole list, from this paragraph on, or for the selected paragraphs. Outside a list, makes a new one.
   */
  setListDefinition(options: { levels: ListLevel[]; applyTo?: 'wholeList' | 'thisPointForward' | 'currentParagraph'; name?: string }): Promise<void>;
  /** Set Numbering Value: starts a new list at `value`, or continues the previous list (skipping to `value` with `advance`). */
  setNumberingValue(options: { mode: 'startNew' | 'continue'; value: number; advance?: boolean }): Promise<void>;
  /** Restarts the list at this paragraph, at its level's start. */
  restartNumbering(): Promise<void>;
  /** Continues the previous list from this paragraph. */
  continueNumbering(): Promise<void>;
  /** Adjust List Indents: the selection's list level's indents and what follows its number, for its whole list. */
  adjustListIndents(options: { numberPosition: Points; textIndent: Points; follow: FollowNumber; tab?: Points }): Promise<void>;
  /**
   * Applies a list gallery entry (`lists.galleries`): a library or recent entry defines the selection's list, or makes
   * a new one; a document list or list style continues that list.
   */
  applyListEntry(id: ListEntryId): Promise<void>;

  // Tab stops: positions from the paragraph's left text edge
  /** Adds a tab stop at 0 to 1584, replacing one at the same position. Left aligned with no leader by default. */
  addTabStop(options: { position: Points; alignment?: TabAlignment; leader?: TabLeader }): Promise<void>;
  /** Moves the tab stop at `from` (within 0.5 pt). */
  moveTabStop(options: { from: Points; to: Points }): Promise<void>;
  /** Deletes the tab stop at a position; an inherited one is cleared in the paragraph. */
  deleteTabStop(position: Points): Promise<void>;
  /** Clears the paragraphs' own and inherited tab stops. */
  clearTabStops(): Promise<void>;
  /** Sets the document's default tab spacing, 1 to 1584. */
  setDefaultTabStop(spacing: Points): Promise<void>;

  // Hyphenation: the document's settings
  /** Turns automatic hyphenation on or off, with the hyphenation patterns of the text's language. */
  setAutoHyphenation(on: boolean): Promise<void>;
  /**
   * Sets the hyphenation zone, 0 to 1584: in documents in compatibility mode 14 and earlier, a word is hyphenated
   * only when moving it whole would leave this gap.
   */
  setHyphenationZone(zone: Points): Promise<void>;
  /** Sets the most consecutive lines that may end in a hyphen, 0 to 32767; 0 for no limit. */
  setConsecutiveHyphenLimit(limit: number): Promise<void>;
  /** Hyphenates words in capitals, or not. */
  setHyphenateCaps(on: boolean): Promise<void>;

  // Page layout: the selection's sections, or with `applyTo` every section or the sections from the selection on (a
  // new-page section break goes at the selection's start; `unavailable` for a cell, block or object selection)
  /** Sets the margins, 0 to 1584; unset fields keep their value. */
  setMargins(options: { top?: Points; bottom?: Points; left?: Points; right?: Points; gutter?: Points; applyTo?: ApplyTo }): Promise<void>;
  /** Sets the orientation. */
  setOrientation(orientation: 'portrait' | 'landscape', options?: { applyTo?: ApplyTo }): Promise<void>;
  /** Sets the paper size, in portrait, 36 to 1584. */
  setPaperSize(options: { width: Points; height: Points; applyTo?: ApplyTo }): Promise<void>;
  /** Sets the text columns: 1 to 45, with spacing and a separator line. */
  setColumns(options: {
    count: number;
    spacing?: Points;
    separator?: boolean;
    /** Columns of their own widths (`count` of them), each with the space after it; equal columns when not given. */
    widths?: ColumnWidth[];
    /**
     * Where they apply: the selection's sections (the default), every section, from the selection on (a continuous
     * section break there), or the selected text alone (continuous breaks around it).
     */
    applyTo?: ColumnsApplyTo;
    /** With `thisPointForward`: the text from the selection on starts a new column. */
    startNewColumn?: boolean;
  }): Promise<void>;
  /** Sets the header and footer distances; unset fields keep their value. */
  setHeaderFooterDistance(options: { header?: Points; footer?: Points; applyTo?: ApplyTo }): Promise<void>;
  /** Sets a different first page header and footer. */
  setDifferentFirstPage(on: boolean): Promise<void>;
  /** Sets different odd and even page headers and footers. */
  setDifferentOddEvenPages(on: boolean): Promise<void>;
  /** Sets the section's page number format, first number (`null` continues from the section before) and chapter numbers (`null` for none); unset fields keep their value. */
  setPageNumbering(options: { format?: NumberFormat; start?: number | null; chapter?: ChapterNumbers | null; applyTo?: ApplyTo }): Promise<void>;
  /** Sets where text sits between the section's top and bottom margins. */
  setPageVerticalAlignment(alignment: 'top' | 'center' | 'justified' | 'bottom', options?: { applyTo?: ApplyTo }): Promise<void>;
  /** Numbers the section's lines, or stops (`null`). */
  setLineNumbering(numbers: LineNumbers | null, options?: { applyTo?: ApplyTo }): Promise<void>;
  /** Sets where the section starts: the break before it. */
  setSectionBreakType(kind: SectionBreakKind, options?: { applyTo?: ApplyTo }): Promise<void>;
  /** Sets the section's text direction. */
  setSectionTextDirection(direction: TextFlow, options?: { applyTo?: ApplyTo }): Promise<void>;
  /** Sets how the document's pages pair and where the gutter is; unset fields keep their value. */
  setMultiplePages(options: { mode?: MultiplePagesMode; gutterAtTop?: boolean }): Promise<void>;
  /** Moves a section text column's boundary. */
  moveColumnBoundary(options: { column: number; to: Points }): Promise<void>;

  // Headers and footers
  /** Edits the caret page's header, creating it when the page has none. */
  editHeader(): Promise<void>;
  /** Edits the caret page's footer, creating it when the page has none. */
  editFooter(): Promise<void>;
  /** Returns to the body (Esc). */
  closeHeaderFooter(): Promise<void>;
  /**
   * Inserts a page number at the caret, in the header or footer, or in a page margin (in a frame anchored in the
   * header). In the header or footer `alignment` aligns its paragraph, or (`inside`, `outside`) frames the number on
   * the facing page's inside or outside edge.
   */
  insertPageNumber(options: {
    format: 'page' | 'pageOfTotal' | 'total';
    where?: 'caret' | 'header' | 'footer' | { margin: { side: MarginSide } };
    alignment?: 'left' | 'center' | 'right' | 'inside' | 'outside';
    /** The number shows on the section's first page (default true); false gives the section a different first page. */
    showOnFirstPage?: boolean;
  }): Promise<void>;
  /** The caret section's header or footer takes the section before's (Link to Previous), or gets its own copy of it. */
  setHeaderFooterLinked(options: { kind: 'header' | 'footer'; pages: 'default' | 'first' | 'even'; linked: boolean }): Promise<void>;
  /** Empties the caret section's header or footer (its own, so a linked one stops linking). */
  removeHeaderFooter(options: { kind: 'header' | 'footer'; pages: 'default' | 'first' | 'even' }): Promise<void>;
  /** Inserts a tab to a place set from the margin or the indent, not from the tab stops (Insert Alignment Tab). */
  insertAlignmentTab(options: { alignment: 'left' | 'center' | 'right'; relativeTo: 'margin' | 'indent'; leader?: TabLeader }): Promise<void>;

  // Insert
  /** Inserts a table of 1 to 32767 rows and 1 to 63 columns. */
  insertTable(options: { rows: number; columns: number }): Promise<void>;
  /** Inserts a picture (PNG, JPEG, GIF, BMP, TIFF, JPEG XR, SVG, EMF or WMF). */
  insertPicture(options: { data: Uint8Array; altText?: string }): Promise<void>;
  /** Inserts a text box. */
  insertTextBox(): Promise<void>;
  /**
   * Inserts a floating shape at the caret's paragraph, in front of the text, with the theme's default style, and
   * selects it.
   */
  insertShape(options: ShapeInsert): Promise<void>;
  /** Inserts a footnote. */
  insertFootnote(): Promise<void>;
  /** Inserts an endnote. */
  insertEndnote(): Promise<void>;
  /** Inserts a footnote or an endnote at the caret, numbered or with its own mark (1 to 10 characters). */
  insertNote(options: { kind: 'footnote' | 'endnote'; customMark?: string }): Promise<void>;
  /**
   * Sets footnotes' or endnotes' position, number format, first number and restarting, for the caret section or the
   * whole document (the default); unset fields keep their value. Footnotes go at the `pageBottom` or `beneathText`;
   * endnotes at the `sectionEnd` or `documentEnd` (the whole document's).
   */
  setNoteProperties(options: {
    kind: 'footnote' | 'endnote';
    position?: NotePosition;
    format?: NumberFormat;
    start?: number;
    restart?: 'continuous' | 'eachSection' | 'eachPage';
    applyTo?: 'wholeDocument' | 'thisSection';
    /** Footnotes only: the footnote area's columns, 1 to 4, or the section's own (0). */
    columns?: number;
  }): Promise<void>;
  /** Turns every footnote into an endnote, every endnote into a footnote, or both (`'swap'`). */
  convertNotes(conversion: 'footnotesToEndnotes' | 'endnotesToFootnotes' | 'swap'): Promise<void>;
  /**
   * Sets the separator, continuation separator or continuation notice of footnotes or endnotes to `text` (paragraphs
   * `\n` apart; `''` an empty line), or back to the built-in one (`text` absent: a line; no notice).
   */
  setNoteSeparator(options: { kind: 'footnote' | 'endnote'; part: NoteSeparatorPart; text?: string }): Promise<void>;
  /**
   * Inserts a link; `text` replaces the selection (default: the selection, or the address). `bookmark` is the place
   * in the target (it wins over a `#bookmark` in `address`), `screenTip` the text shown on hover, `frame` the window
   * or frame it opens in (`_blank`).
   */
  insertHyperlink(options: { address: string; text?: string; bookmark?: string; screenTip?: string; frame?: string }): Promise<void>;
  /** Changes the link at the selection; what isn't given is kept, and `''` removes a ScreenTip or frame. */
  editHyperlink(options: { address?: string; text?: string; screenTip?: string; frame?: string }): Promise<void>;
  /** Removes the link at the selection, keeping its text. */
  removeHyperlink(): Promise<void>;
  /**
   * Bookmarks the selection: the name starts with a letter (or `_`, for a hidden bookmark) and holds letters, digits
   * and `_`, 40 at most. A bookmark of the same name (compared without case) moves here.
   */
  insertBookmark(name: string): Promise<void>;
  /** Deletes a bookmark (compared without case), keeping its text. */
  deleteBookmark(name: string): Promise<void>;
  /**
   * Restrict Editing: what the document allows, and starting or stopping its enforcement. Starting takes the password
   * stopping will need (none when absent); stopping a password-protected document needs it. With filling in forms,
   * `unprotectedSections` are the sections (by index) anyone may edit; it can't change while enforced.
   */
  setDocumentProtection(options: {
    kind?: 'none' | 'readOnly' | 'comments' | 'trackedChanges' | 'forms';
    formatting?: boolean;
    enforce?: boolean;
    password?: string;
    /** Starting with a password: the password again; it must match. */
    confirmPassword?: string;
    unprotectedSections?: number[];
    /** AutoFormat as you type may override the formatting restriction. Not while enforced. */
    autoFormatOverride?: boolean;
    /** The formatting restriction keeps the theme from changing. Not while enforced. */
    blockThemeSwitching?: boolean;
    /** The formatting restriction keeps the style set from changing. Not while enforced. */
    blockStyleSetSwitching?: boolean;
  }): Promise<void>;
  /**
   * Makes the selection a range everyone (or a person, `editor`, or a group, `group`: `editors`, `owners`, ...) may
   * edit while the document is protected.
   */
  addEditableRange(options?: { editor?: string; group?: string }): Promise<void>;
  /** Removes the editable ranges at the selection. */
  removeEditableRanges(): Promise<void>;
  /**
   * Lets an editor edit the selection (`allowed`), or stops them editing the editable ranges reaching it, as ticking
   * or clearing them in Restrict Editing's exceptions does.
   */
  setEditableRangeEditor(options: { editor: RangeEditorRef; allowed: boolean }): Promise<void>;
  /** Selects the next or previous editable range the author may edit (Find Next Region I Can Edit). */
  selectNextEditableRange(direction: 'next' | 'previous'): Promise<void>;
  /** Selects every editable range the author may edit (Show All Regions I Can Edit). */
  selectEditableRanges(): Promise<void>;
  /** The groups the author is in; editable ranges for them let the author edit. Kept for the documents opened after. */
  setEditingGroups(groups: string[]): Promise<void>;
  /** Inserts a legacy form field (FORMTEXT, FORMCHECKBOX, FORMDROPDOWN) at the selection. */
  insertFormField(kind: 'text' | 'checkBox' | 'dropDown'): Promise<{ id: ElementId }>;
  /** Sets a legacy form field's options (default: the one at the selection); only its own kind's options apply. */
  setFormField(options: {
    id?: ElementId;
    name?: string;
    enabled?: boolean;
    calculateOnExit?: boolean;
    entryMacro?: string;
    exitMacro?: string;
    helpText?: string;
    statusText?: string;
    /** `helpText` names an AutoText entry whose text is the help. */
    helpIsAutoText?: boolean;
    /** `statusText` names an AutoText entry whose text is the help. */
    statusIsAutoText?: boolean;
    text?: { inputType?: TextInputType; default?: string; maxLength?: number; format?: string };
    checkBox?: { size?: Points | 'auto'; checkedByDefault?: boolean };
    dropDown?: { entries?: string[]; default?: number };
  }): Promise<void>;
  /** Fills in a legacy form field (default: the one at the selection): a text field's text, a check box's state, a drop-down's entry. */
  fillFormField(options: { id?: ElementId; text?: string; checked?: boolean; entry?: number }): Promise<void>;
  /** Selects the next or previous legacy form field (Tab and Shift+Tab where forms are filled in). */
  selectNextFormField(direction: 'next' | 'previous'): Promise<void>;
  /** Shades legacy form fields, or not. */
  setFormFieldShading(on: boolean): Promise<void>;
  /**
   * Sets the document's properties; unset fields keep their value, an empty text removes one, and `custom` replaces
   * the custom properties.
   */
  setDocumentProperties(options: {
    title?: string;
    subject?: string;
    author?: string;
    keywords?: string;
    comments?: string;
    category?: string;
    status?: string;
    lastModifiedBy?: string;
    company?: string;
    manager?: string;
    hyperlinkBase?: string;
    custom?: CustomProperty[];
  }): Promise<void>;
  /** Sets a document variable (read by DOCVARIABLE), or removes it when `value` is absent. */
  setDocumentVariable(options: { name: string; value?: string }): Promise<void>;
  /**
   * The document's file name, with its folder when FILENAME `\p` should show one: what FILENAME shows. Not saved, and
   * not an edit.
   */
  setFileName(name: string): Promise<void>;
  /** Inserts a field. With a selection and an empty code, wraps it (Ctrl+F9). */
  insertField(options: { code: string; display?: string; showCode?: boolean }): Promise<{ id: ElementId }>;

  // Tables: the selection's cells
  /** Inserts rows above or below, 1 by default. */
  insertRows(options: { where: 'above' | 'below'; count?: number }): Promise<void>;
  /** Inserts columns left or right, 1 by default. */
  insertColumns(options: { where: 'left' | 'right'; count?: number }): Promise<void>;
  /** Deletes the selected rows. */
  deleteRows(): Promise<void>;
  /** Deletes the selected columns. */
  deleteColumns(): Promise<void>;
  /** Deletes the table. */
  deleteTable(): Promise<void>;
  /** Deletes the selected cells, shifting the rest up or left. */
  deleteCells(shift: 'up' | 'left'): Promise<void>;
  /** Merges a rectangular block of cells. */
  mergeCells(): Promise<void>;
  /** Splits the selected cells. */
  splitCells(options: { columns: number; rows?: number }): Promise<void>;
  /** Splits the table above the current row. */
  splitTable(): Promise<void>;
  /** Converts the selected paragraphs to a table: a row for each paragraph, split into cells at the separator (`character` with `'other'`); `columns` (1-63) by default the most pieces a paragraph has. */
  convertTextToTable(options: {
    separator: 'paragraphs' | 'tabs' | 'commas' | 'other';
    character?: string;
    columns?: number;
    /** How the new table's columns are sized; default `'fixed'`. */
    autofit?: TableFit;
    /** With fixed columns, every column this wide (1 to 1584); default the text's width shared equally. */
    columnWidth?: Points;
  }): Promise<void>;
  /**
   * Converts the selected rows, or the whole table, to text: a paragraph for each row, its cells joined by the separator
   * (`character` with `'other'`); `convertNested` converts the tables in its cells too (default: they stay tables).
   */
  convertTableToText(options: { separator: 'paragraphs' | 'tabs' | 'commas' | 'other'; character?: string; convertNested?: boolean }): Promise<void>;
  /** Gives the selected rows equal heights. */
  distributeRows(): Promise<void>;
  /** Gives the selected columns equal widths. */
  distributeColumns(): Promise<void>;
  /** Moves the selected rows up or down (Alt+Shift+Up, Down). */
  moveRows(direction: 'up' | 'down'): Promise<void>;
  /** Fits the columns to their contents or the window, or fixes their widths. */
  autoFitColumns(to: 'contents' | 'window' | 'fixed'): Promise<void>;
  /** Sets the cells' vertical alignment. */
  setCellAlignment(vertical: 'top' | 'center' | 'bottom'): Promise<void>;
  /** Sets (a clear fill) or removes (`null`) the cell shading. */
  setCellShading(color: TextColour | null): Promise<void>;
  /** Sets the cell borders on some edges, or the cells' diagonals; `'none'` removes them all, diagonals too. */
  setCellBorders(options: { edges: CellBorderEdges; border?: BorderLine }): Promise<void>;
  /** Sets the cell margins, 0 to 1584; unset fields keep their value. */
  setCellMargins(options: { top?: Points; bottom?: Points; left?: Points; right?: Points }): Promise<void>;
  /**
   * Sets the cells' preferred width (points 0 to 1584, or percent of the table 0 to 100), text direction, wrapping,
   * fitting and end mark height (`hideMark`); unset fields keep their value.
   */
  setCellProperties(options: {
    width?: TableWidth;
    textDirection?: TextFlow;
    wrapText?: boolean;
    fitText?: boolean;
    hideMark?: boolean;
  }): Promise<void>;
  /**
   * Hides the rows' end marks (a row hides when its text is hidden too), and sets the table properties the rows
   * override: `reset` drops their own first.
   */
  setRowProperties(options: {
    hidden?: boolean;
    exceptions?: {
      reset?: boolean;
      alignment?: 'left' | 'center' | 'right';
      cellSpacing?: Points;
      /** The rows' indent from the table's left edge, -1584 to 1584; not in a fixed-width table. */
      indent?: Points;
      /** A fill and a pattern over it, or a colour alone as a clear fill. */
      shading?: Shading | TextColour;
      borders?: { edges: CellBorderEdges; border?: BorderLine };
    };
  }): Promise<void>;
  /**
   * Sets the rows and columns in each of the table's bands (1 to 32767): each given is set, or left to the table's
   * style (`null`); the rest are kept. `unavailable` outside a table.
   */
  setTableBandSizes(options: { rows?: number | null; columns?: number | null }): Promise<void>;
  /** Sets the space between the table's cells, 0 to 1584, or none (`null`). */
  setTableCellSpacing(spacing: Points | null): Promise<void>;
  /** Sets the table's alternative text title and description; unset fields keep their value. */
  setTableAltText(options: { title?: string; description?: string }): Promise<void>;
  /** Repeats the rows as a header row. */
  setHeaderRow(on: boolean): Promise<void>;
  /** Sets the row height. */
  setRowHeight(height: RowHeight): Promise<void>;
  /** Lets rows break across pages. */
  setRowCanBreak(on: boolean): Promise<void>;
  /** Sets the column width, 1 to 1584. */
  setColumnWidth(width: Points): Promise<void>;
  /** Moves a column boundary of the caret's row. */
  moveTableColumnBoundary(options: { index: number; to: Points }): Promise<void>;
  /** Moves a row boundary of the caret's table. */
  moveTableRowBoundary(options: { index: number; to: Points }): Promise<void>;
  /** Sets the table width. */
  setTableWidth(width: TableWidth): Promise<void>;
  /** Sets the table alignment. */
  setTableAlignment(alignment: 'left' | 'center' | 'right'): Promise<void>;
  /** Sets the table indent, -1584 to 1584. */
  setTableIndent(indent: Points): Promise<void>;
  /** Applies an existing table style. */
  setTableStyle(styleId: StyleId): Promise<void>;
  /** Sets which parts of the table style apply; unset fields keep their value. */
  setTableStyleOptions(options: {
    headerRow?: boolean;
    totalRow?: boolean;
    firstColumn?: boolean;
    lastColumn?: boolean;
    bandedRows?: boolean;
    bandedColumns?: boolean;
  }): Promise<void>;
  /** Sets the table style new tables take (Set as Default), or Table Grid (`null`). */
  setDefaultTableStyle(styleId: StyleId | null): Promise<void>;
  /**
   * Positions the caret's table, so text wraps around it, as dragging its move handle does: a table without a
   * position gets one across from the margin and down from the paragraph, and each axis given is aligned, offset, or
   * (with neither) kept where it is and measured from its new reference. Unset fields keep their value; `null` puts
   * the table back in the text.
   */
  setTablePosition(position: TablePosition | null): Promise<void>;
  /** Sets the table's default cell margins (Table Options), 0 to 1584 each; unset sides keep theirs. */
  setTableCellMargins(margins: { top?: Points; left?: Points; bottom?: Points; right?: Points }): Promise<void>;
  /**
   * Inserts blank cells the size of the selected ones, shifting the selected cells right or down (Word's Insert
   * Cells; a whole row or column is `insertRows` or `insertColumns`). Tracked when tracking changes.
   */
  insertCells(shift: 'right' | 'down'): Promise<void>;
  /**
   * Sets the pointer's table tool (`view.tableTool`): while the pen or eraser is on, a press and drag on a page draws
   * or erases instead of selecting; Escape, a change of document and an edit by any other command turn it off. View.
   */
  setTableTool(tool: TableTool): Promise<void>;
  /** Sets the line the Draw Table pen draws (0.25 to 6; default a ½ pt single automatic line). Not saved. View. */
  setTablePen(line: BorderLine): Promise<void>;
  /**
   * A Draw Table pen stroke on a page (points from its top left): across or down a table it splits the cells it
   * crosses, corner to corner in a cell it draws that cell's diagonal, and off any table its rectangle becomes a new
   * table. Untracked.
   */
  drawTableStroke(stroke: { page: number; from: [Points, Points]; to: [Points, Points] }): Promise<void>;
  /** The table eraser on a page (points from its top left): merges the two cells either side of the border at `at`. */
  eraseTableBorder(options: { page: number; at: [Points, Points] }): Promise<void>;

  // Objects: the selected object
  /** Sets the object's size, 1 to 1584. */
  setObjectSize(options: {
    width?: Points;
    height?: Points;
    lockAspect?: boolean;
    /** A floating object's width as a share of a box on the page, or absolute (`'none'`). */
    relativeWidth?: RelativeSize | 'none';
    /** A floating object's height as a share of a box on the page, or absolute (`'none'`). */
    relativeHeight?: RelativeSize | 'none';
  }): Promise<void>;
  /** Resets the object to its original size. */
  resetObjectSize(): Promise<void>;
  /** Sets how text wraps around the object. */
  setObjectWrap(wrap: ObjectWrap): Promise<void>;
  /** Locks a floating object's anchor. */
  setAnchorLocked(on: boolean): Promise<void>;
  /**
   * Places the floating object: each axis given is aligned, offset, or (with neither) kept where it is and measured
   * from its new reference; `vertical: { relativeTo: 'page' }` fixes it on the page, `'paragraph'` moves it with text.
   * Unset fields keep their value.
   */
  setObjectPosition(options: {
    horizontal?: HorizontalPosition;
    vertical?: VerticalPosition;
    allowOverlap?: boolean;
    layoutInCell?: boolean;
  }): Promise<void>;
  /** Sets the sides text wraps on around the floating object and its distance from the text, 0 to 1584; unset fields keep their value. */
  setObjectWrapOptions(options: { side?: WrapSide; top?: Points; bottom?: Points; left?: Points; right?: Points }): Promise<void>;
  /** Sets the alternative text, or marks the object decorative. */
  setAltText(options: { text: string; decorative?: boolean }): Promise<void>;
  /** Deletes the object. */
  deleteObject(): Promise<void>;
  /** Changes the shape's preset geometry; its text, fill, outline and effects stay and its adjust values reset. */
  setShapePreset(preset: ShapePreset): Promise<void>;
  /** Sets one of the shape's adjust values. */
  setShapeAdjust(options: ShapeAdjust): Promise<void>;
  /** Sets the selected objects' fill; `fromStyle` gives them the style reference's again. */
  setShapeFill(fill: FillValue): Promise<void>;
  /** Sets the selected objects' outline; unset fields keep their value. */
  setShapeLine(change: LineChange): Promise<void>;
  /** Sets the selected objects' shadows, glow, soft edge and blur; unset fields keep their value, `null` removes. */
  setShapeEffects(change: EffectsChange): Promise<void>;
  /** Sets the shape's text insets, anchor, autofit, wrapping and direction; unset fields keep their value. */
  setShapeText(change: ShapeTextChange): Promise<void>;
  /** Sets the picture's transparency, greyscale, brightness, contrast and transparent colour; unset fields keep their value. */
  setPictureAdjust(change: PictureAdjustChange): Promise<void>;
  /**
   * Crops the selected picture by fractions of its image from each edge (0 to 1; negative pads it): its box follows
   * the part shown, at the scale it is drawn. Unset fields keep their value.
   */
  setPictureCrop(crop: { left?: number; top?: number; right?: number; bottom?: number }): Promise<void>;
  /** Replaces the selected picture's image, keeping its width, effects and place; its height follows the new image. */
  changePicture(data: Uint8Array): Promise<void>;
  /** Sets the selected objects' rotation, 0 to 360 degrees clockwise. */
  setObjectRotation(degrees: Degrees): Promise<void>;
  /** Flips the selected objects. */
  flipObject(axis: FlipAxis): Promise<void>;
  /** Moves the floating object in the paint order, or behind or in front of the text. */
  arrangeObject(where: Arrange): Promise<void>;
  /**
   * Aligns or distributes the selected floating objects across their page, its margins, or the box around them (the
   * default for two or more; else the margins).
   */
  alignObjects(options: {
    align: 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom' | 'distributeHorizontally' | 'distributeVertically';
    relativeTo?: 'page' | 'margin' | 'selectedObjects';
  }): Promise<void>;
  /** Groups the selected floating objects: two or more, in one story. The group is selected. */
  groupObjects(): Promise<void>;
  /** Ungroups the selected group. Its children are selected. */
  ungroupObjects(): Promise<void>;
  /** Groups again the objects the last {@link Commands.ungroupObjects} ungrouped, wherever they moved. */
  regroupObjects(): Promise<void>;
  /**
   * Shows the selected floating object's wrap polygon for editing (`view.editingWrapPoints`): dragging a vertex moves it,
   * dragging an edge adds one, Ctrl+click removes one, each one undo step that makes the wrap tight. Esc ends it.
   */
  editWrapPoints(): Promise<void>;
  /** Selects the next or previous child of the selected group. View. */
  selectGroupChild(direction: 'next' | 'previous'): Promise<void>;
  /** Names the selected object (1 to 255 characters after trimming), as the Selection Pane lists it. */
  setObjectName(name: string): Promise<void>;
  /** Sets the selected object's alternative text title (at most 255 characters; empty clears it). */
  setObjectTitle(title: string): Promise<void>;
  /** Hides or shows a floating object: hidden, it isn't drawn and text doesn't wrap around it. */
  setObjectHidden(options: { object: ObjectId; hidden: boolean }): Promise<void>;
  /** Selects a floating object or a group's object, as the Selection Pane does; `extend` adds it to (or takes it from) the selected ones. View. */
  selectObject(options: { object: ObjectId; extend?: boolean }): Promise<void>;
  /** Moves a floating object in its page's stack: just in front of `inFrontOf`, or behind every other when absent. It keeps its layer. */
  restackObject(options: { object: ObjectId; inFrontOf?: ObjectId }): Promise<void>;
  /** Whether resizing the selected object keeps its aspect ratio (Lock aspect ratio), saved with it. */
  setLockAspectRatio(on: boolean): Promise<void>;
  /** Sets the selected pictures' recolouring (Picture Color, Recolor). */
  setPictureRecolor(recolor: Recolor): Promise<void>;
  /**
   * Clears the selected picture's adjustments, colour, transparency, effects, outline, shape, rotation and flips,
   * keeping its crop (Reset Picture); `size` also clears the crop and sizes it at its image's own size.
   */
  resetPicture(options: { size?: boolean }): Promise<void>;
  /** Sets the selected floating object's wrap polygon (3 to 1024 points, −16 to 17 box sizes), marked edited; a wrap that is neither tight nor through becomes tight. */
  setWrapPoints(points: BoxPoint[]): Promise<void>;
  /** Makes the selected floating object's wrap polygon again from its outline, no longer edited. */
  resetWrapPoints(): Promise<void>;
  /** Links the selected text box (the last in its chain) to an empty text box in no chain, in the same story (Create Link). */
  linkTextBox(target: ObjectId): Promise<void>;
  /** Starts linking the selected text box with the pointer (`view.linkingTextBox`): the next click on an empty text box links it; elsewhere or Escape cancels. View. */
  beginTextBoxLink(): Promise<void>;
  /** Breaks the link from the selected text box to the next (Break Link): the boxes after it are a chain of their own, empty. */
  unlinkTextBox(): Promise<void>;

  // Fields
  /** Changes a field's code or display. */
  setField(options: { id: ElementId; code?: string; display?: string | RichText }): Promise<void>;
  /** Supplies field results; `token` joins the step a `fieldsUpdateRequested` event started. */
  setFieldResults(options: { results: { id: ElementId; display: string | RichText }[]; token?: number }): Promise<void>;
  /** Updates fields (F9); emits `fieldsUpdateRequested` for the fields the application computes. */
  updateFields(options?: FieldTargets): Promise<void>;
  /**
   * Locks or unlocks fields (Ctrl+F11, Ctrl+Shift+F11). Without `ids`, the selection's, with the table of contents (or
   * other field across paragraphs) it touches, or the one at the caret when no other field is there.
   */
  setFieldsLocked(options: { locked: boolean } & FieldTargets): Promise<void>;
  /**
   * Replaces fields by their results (Ctrl+Shift+F9). Without `ids`, as `setFieldsLocked`: a table of contents (or other
   * field across paragraphs) becomes text with the fields in it; a locked one stays, with every field in it.
   */
  unlinkFields(options?: FieldTargets): Promise<void>;
  /**
   * Shows field codes or results (Alt+F9, Shift+F9). Without `ids`, every field, tables of contents included; with an
   * empty `ids`, the table of contents (or other field across paragraphs) at the selection, which has no id. A view:
   * no undo step, the document isn't modified.
   */
  showFieldCodes(options: { show: boolean } & FieldTargets): Promise<void>;
  /** What design mode shows for each field code. View. */
  setFieldLabels(options: { labels: Record<string, string>; merge?: boolean }): Promise<void>;
  /** Which fields are sub-template references: replaces the rules; an empty list turns sub-templates off. View. */
  setSubTemplateRules(options: { rules: ReferenceRule[] }): Promise<void>;
  /**
   * The application's own field types (a code's first word, compared without case), replacing those set before:
   * their fields keep their results, show their labels in design mode ({@link Commands.setFieldLabels}, else
   * «first argument»), are listed by `fieldsUpdateRequested` and are never read as bookmark references
   * ("Error! Bookmark not defined."). `[]` clears them. Set it after each `open`, as the rules, or once in the
   * session options. View.
   */
  setApplicationFieldTypes(options: { types: string[] }): Promise<void>;
  /** Shows references' sub-template bodies in their place, or their fields (default: every reference). View. */
  setSubTemplateView(options: { ids?: ElementId[]; expanded: boolean }): Promise<void>;
  /** Edits a sub-template's body in this editor, on its own pages. View. */
  editSubTemplate(options: { name: string; category?: string }): Promise<void>;
  /** Back to the template from a sub-template, at the template's selection from before. View. */
  closeSubTemplate(): Promise<void>;
  /**
   * A new sub-template, empty, with a DOCX's body, or a copy of the sub-template `copyOf` in the same category
   * (`category`: default the first rule's).
   */
  createSubTemplate(options: { name: string; category?: string; from?: Uint8Array; copyOf?: string }): Promise<void>;
  /** Renames a sub-template, and with `updateReferences` every reference to it. */
  renameSubTemplate(options: { name: string; to: string; category?: string; updateReferences?: boolean }): Promise<void>;
  /** Removes a sub-template; its references no longer resolve. */
  removeSubTemplate(options: { name: string; category?: string }): Promise<void>;
  /**
   * The template's body becomes the DOCX `from`'s body, on its last section's page; its sub-templates, custom XML
   * data parts, settings, styles, headers and footers stay. Closes a sub-template being edited first.
   */
  replaceBody(options: { from: Uint8Array }): Promise<void>;

  // References
  /**
   * A table of contents in place of the selection (in place of the table the selection is in): with no options,
   * Word's (heading levels 1 to 3, outline levels, hyperlinks, hidden in a web view). Its entries' styles (TOC 1 to 9)
   * are added when missing; its page numbers are the finished layout's. Body only.
   */
  insertTableOfContents(options?: { options?: TableOfContentsOptions; layout?: TocLayout; frame?: TocFrame }): Promise<void>;
  /** A table of figures of the captions labelled `label`, in place of the selection (or of the table it is in). */
  insertTableOfFigures(options: {
    label: string;
    /** Entries keep the label and number. Default true. */
    includeLabelAndNumber?: boolean;
    /** Its other options (hyperlinks and hidden in a web view when unset). */
    options?: TableOfContentsOptions;
    layout?: TocLayout;
    frame?: TocFrame;
  }): Promise<void>;
  /** Word's Manual Table: typed entries in a building block control, at the start of a body paragraph. */
  insertManualTableOfContents(): Promise<void>;
  /**
   * Updates the table of contents or figures the selection is in (else the first; `index`: that one of
   * `references.tablesOfContents`): its page numbers, or every entry again.
   */
  updateTableOfContents(options: { mode: TocUpdateMode; index?: number }): Promise<void>;
  /** Updates every table of contents and of figures but the locked ones (as before printing); how many. */
  updateAllTablesOfContents(mode: TocUpdateMode): Promise<{ changed: number }>;
  /** Removes the table of contents the selection is in (else the first; `index`: that one), with its control and title. */
  removeTableOfContents(options?: { index?: number }): Promise<void>;
  /** Marks a table of contents entry after the selection (Mark Entry, Alt+Shift+O): a hidden table entry field. */
  markTableOfContentsEntry(options: {
    text: string;
    /** 1 to 9. */
    level: number;
    /** The tables that collect it: those taking entry fields of this identifier (`C` by default). */
    identifier?: string;
    /** It shows a page number. Default true. */
    pageNumber?: boolean;
  }): Promise<{ id: ElementId }>;
  /** A caption for the selected table, picture or object (or the caret's paragraph); the label's other captions renumber. */
  insertCaption(options: CaptionOptions & { text?: string }): Promise<void>;
  /** Captions every table inserted from now on (`null`: off). A session setting, never saved; also when read only. */
  setAutoCaption(caption: CaptionOptions | null): Promise<void>;
  /** A cross-reference to the `target.index`-th item `references.crossReferenceTargets(target.kind)` lists, at the selection. */
  insertCrossReference(options: {
    target: { kind: CrossReferenceKind; index: number };
    content: CrossReferenceContent;
    /** It links to its target. Default true. */
    hyperlink?: boolean;
    /** "above" or "below" follows it. */
    aboveBelow?: boolean;
  }): Promise<{ id: ElementId }>;

  // Content controls and regions
  /** Inserts a content control, optionally around the selection. */
  insertControl(options: {
    kind: ControlKind;
    tag?: string;
    title?: string;
    lock?: 'none' | 'control' | 'content' | 'both';
    placeholder?: string;
    binding?: DataBinding;
    options?: ControlOptions;
    /**
     * A picture control's placeholder picture (PNG, JPEG, GIF, BMP or SVG): what the editor, the saved DOCX and a
     * PDF show until it is filled. An SVG is drawn with the picture's box as its viewport, so one with percentage
     * geometry and no `viewBox` adapts to every size. Default: the engine's.
     */
    placeholderImage?: PlaceholderImage;
    wrapSelection?: boolean;
    /**
     * Floats a picture control, positioned and wrapped as `setObjectWrap` and `setObjectPosition` place an object;
     * `wrap` can't be `inline`.
     */
    float?: { wrap: ObjectWrap; horizontal?: HorizontalPosition; vertical?: VerticalPosition };
  }): Promise<{ id: ElementId }>;
  /**
   * Changes a content control; a `null` binding removes it. `placeholderImage` replaces a picture control's
   * placeholder picture while it shows it (else it rejects with `unavailable`).
   */
  setControl(options: {
    id: ElementId;
    tag?: string;
    title?: string;
    lock?: 'none' | 'control' | 'content' | 'both';
    placeholder?: string;
    binding?: DataBinding | null;
    options?: ControlOptions;
    placeholderImage?: PlaceholderImage;
    /** How it shows its edges. */
    appearance?: 'boundingBox' | 'tags' | 'hidden';
    /** Its colour, or the default (`'none'`). */
    color?: Color | 'none';
    /** It goes when its content is edited, leaving the content. */
    temporary?: boolean;
    /** The character style text typed into it takes, or none (`'none'`). */
    style?: StyleId | 'none';
  }): Promise<void>;
  /** Removes a content control, optionally keeping its content. */
  removeControl(options: { id: ElementId; keepContent?: boolean }): Promise<void>;
  /**
   * Fills a content control with exactly one of text, an image (fitted to its box), DOCX content, a building block
   * gallery's entry (its `buildingBlocks`), or a date control's date (ISO 8601: `YYYY-MM-DD`, with a time and zone
   * or without; its text that date in its format).
   */
  fillControl(options: {
    id: ElementId;
    text?: string;
    image?: Uint8Array;
    docx?: Uint8Array;
    buildingBlock?: string;
    date?: string;
  }): Promise<void>;
  /** Checks or unchecks a checkbox control. */
  setCheckbox(options: { id: ElementId; checked: boolean }): Promise<void>;
  /** Wraps the selected paragraphs or table rows in a repeating section. */
  wrapInRepeatingSection(options?: { tag?: string; title?: string }): Promise<{ id: ElementId }>;
  /**
   * Adds an item to a repeating section: a copy of one of its items (the last by default), with its content, after it
   * or `before` it. The caret goes to the copy's start.
   */
  addRepeatingItem(options: { id: ElementId; item?: ElementId; before?: boolean }): Promise<{ id: ElementId }>;
  /** Removes a field, or a control with its content. */
  removeElement(id: ElementId): Promise<void>;
  /** Replaces text in element codes, tags or titles: case-sensitive, every occurrence. */
  replaceInElements(options: {
    find: string;
    replace: string;
    in: 'code' | 'tag' | 'title';
    kind?: 'field' | 'control';
    ids?: ElementId[];
  }): Promise<{ changed: number }>;
  /**
   * Rewrites a font family's name in the document itself, so the saved file names `to`: one undo step. In document
   * scope with no style filter it rewrites the text (runs, paragraph marks, symbols) and the tables below it (styles,
   * document defaults, numbering levels, the theme's font slots and the font table). In selection scope, or with a
   * filter, it rewrites only the text, and a matching run whose family comes from a style or the theme gets `to` as
   * its own. With tracked changes on, the text's changes are formatting revisions. Text inside locked content
   * controls is skipped and counted.
   */
  replaceFont(options: { from: string; to: string; scope?: ReplaceScope; bold?: boolean; italic?: boolean }): Promise<{
    changed: number;
    skipped: number;
  }>;
  /** Draws wavy marks with a tooltip on elements. View. */
  markElements(options: {
    marks: { id: ElementId; severity: 'error' | 'warning' | 'info'; message: string }[];
    clear?: boolean;
  }): Promise<void>;
  /**
   * Sets the tag prefixes (compared without case) whose picture controls keep their pictures: editing can't delete,
   * paste over or replace such a picture; `fillControl` and `setControl` still change it, and in design mode deleting
   * it removes its whole control. Replaces the prefixes set before; an empty list protects none. Not saved.
   */
  setProtectedPictureControls(options: { tagPrefixes: string[] }): Promise<void>;
  /** Selects a picture control (the selected one when none is named) and emits `pictureControlActivated`, in any mode. */
  activatePictureControl(options?: { element?: ElementId }): Promise<void>;

  // Data parts
  /** Writes a custom XML data part (well-formed). */
  writeDataPart(options: { namespace: string; itemId?: string; xml: string }): Promise<{ itemId: string }>;
  /** Removes a custom XML data part. */
  removeDataPart(options: { namespace: string; itemId?: string }): Promise<void>;

  // Review
  /** Turns change tracking on or off (Ctrl+Shift+E). */
  setTrackChanges(on: boolean): Promise<void>;
  /** Sets how tracked changes are shown. View. */
  setMarkupView(view: MarkupView): Promise<void>;
  /** Which kinds of markup show (Show Markup); unset fields keep their value. View. */
  setShowMarkup(options: { insertionsDeletions?: boolean; formatting?: boolean; comments?: boolean }): Promise<void>;
  /** The reviewers whose revisions and comments don't show, by name; empty: all reviewers show. View. */
  setHiddenReviewers(options: { hidden: string[] }): Promise<void>;
  /** Where revisions show in All Markup. View. */
  setBalloonMode(mode: BalloonMode): Promise<void>;
  /** How revisions draw (Track Changes Options); unset fields keep their value. View. */
  setMarkupStyle(options: MarkupStyleChange): Promise<void>;
  /** Whether tracking records moves as moves and formatting changes; unset fields keep their value. Saved. */
  setTrackOptions(options: { moves?: boolean; formatting?: boolean }): Promise<void>;
  /** Accepts a revision; a move resolves as a pair. */
  acceptRevision(id: RevisionId): Promise<void>;
  /** Rejects a revision; a move resolves as a pair. */
  rejectRevision(id: RevisionId): Promise<void>;
  /** Accepts the revisions in the selection. */
  acceptRevisionsInSelection(): Promise<void>;
  /** Rejects the revisions in the selection. */
  rejectRevisionsInSelection(): Promise<void>;
  /** Accepts every revision, one author's, or (`shown`) those Show Markup shows. */
  acceptAllRevisions(options?: { author?: string; shown?: boolean }): Promise<void>;
  /** Rejects every revision, one author's, or (`shown`) those Show Markup shows. */
  rejectAllRevisions(options?: { author?: string; shown?: boolean }): Promise<void>;
  /** Accepts every revision and turns tracking off (Accept All Changes and Stop Tracking). */
  acceptAllAndStopTracking(): Promise<void>;
  /** Selects the next or previous revision. */
  selectRevision(direction: 'next' | 'previous'): Promise<void>;
  /** Comments on the selection, or the word at the caret (Ctrl+Alt+M). */
  addComment(body: string | RichText): Promise<{ id: CommentId }>;
  /** Replies to a comment. */
  replyToComment(options: { to: CommentId; body: string | RichText }): Promise<{ id: CommentId }>;
  /** Changes a comment's body. */
  editComment(options: { id: CommentId; body: string | RichText }): Promise<void>;
  /** Deletes a reply, or the whole thread when it's the first comment. */
  deleteComment(id: CommentId): Promise<void>;
  /** Deletes every comment, resolved ones too, or (`shownOnly`) the threads Show Markup shows. */
  deleteAllComments(options?: { shownOnly?: boolean }): Promise<void>;
  /** Resolves or reopens a thread. */
  resolveComment(options: { id: CommentId; done: boolean }): Promise<void>;
  /** Sets the active thread. View. */
  setActiveComment(id: CommentId | null): Promise<void>;
  /** Selects and reveals a comment's range. */
  selectComment(id: CommentId): Promise<void>;
  /** Sets the colour an author's changes are marked with. View. */
  setAuthorColor(options: { author: string; color: Color }): Promise<void>;

  // View: never change the document
  /** Sets the zoom: 10 to 500 percent, or a fit. */
  setZoom(zoom: number | 'pageWidth' | 'wholePage'): Promise<void>;
  /**
   * Sets view options: formatting marks (Ctrl+Shift+8), gridlines, text boundaries, live layout, the chrome's theme
   * and accent, the page's appearance, the measuring detail and the object grid.
   */
  setViewOptions(options: ViewOptions): Promise<void>;
  /** Switches between editing and design mode. */
  setMode(mode: 'editing' | 'design'): Promise<void>;
  /** Turns Select Objects on or off (`view.selectingObjects`): a drag off every object selects the objects wholly inside its rectangle. Escape ends it. View. */
  setSelectObjects(on: boolean): Promise<void>;
  /** Sets what a click on the ruler adds; answers the tab type popover. View. */
  setRulerTabType(type: TabSelector): Promise<void>;
  /** Asks for the tab type popover (`popoverRequested`). View. */
  showTabTypes(): Promise<void>;
  /** Asks for the options of the tab stop nearest the caret (`popoverRequested`). View. */
  showTabStop(): Promise<void>;

  // AutoCorrect
  /** Sets what typing corrects: the rules given switch, the lists given replace the shipped ones. A session setting: no undo. */
  setAutoCorrect(change: AutoCorrectChange): Promise<void>;
  /** Puts back what was typed in place of the correction just made: undoes it when it is the last step, else one undo step. */
  revertAutoCorrect(): Promise<void>;
  /** Reverts the correction just made, as `revertAutoCorrect` does, and never makes it again (adds it to `autoCorrect.never`). */
  neverAutoCorrect(): Promise<void>;
  /** Asks for the options of the correction just made (`popoverRequested`), Alt+Shift+F10. View. */
  showAutoCorrectOptions(): Promise<void>;
  /** Dismisses the popover asked for (`popoverClosed`), for an application that closes its own on Esc or a press elsewhere; with none open, nothing. View. */
  closePopover(): Promise<void>;
  /** Says the formatting at the caret to screen readers (an `announcement` of kind `formatting`). Shift+F1. */
  describeFormatting(): Promise<void>;
}

/** A command id: the method name, the batch op's `type`, the catalog id and the key in `state.commands`. */
export type CommandId = keyof Commands;

/**
 * An op's fields besides `type`. A single argument (a primitive, {@link RichText}, bytes, an array, a
 * {@link FillValue}, whose `type` would clash with the op's, or a union with a primitive) goes in `value`; an options
 * object, and a tagged value such as {@link LineSpacing} or {@link TableWidth}, is spread into the op.
 */
export type OpArgs<K extends CommandId> = Parameters<Commands[K]> extends []
  ? {}
  : K extends SectionValueCommandId
    ? { value: Parameters<Commands[K]>[0]; applyTo?: ApplyTo }
    : [Exclude<Parameters<Commands[K]>[0], undefined>] extends [string | number | boolean | null]
    ? { value: Exclude<Parameters<Commands[K]>[0], undefined> }
    : [Exclude<Parameters<Commands[K]>[0], undefined>] extends [RichText | Uint8Array | FillValue | Watermark | Recolor | ObjectId | BorderLine | readonly unknown[]]
      ? { value: Exclude<Parameters<Commands[K]>[0], undefined> }
      : [Exclude<Parameters<Commands[K]>[0], undefined>] extends [object]
        ? Exclude<Parameters<Commands[K]>[0], undefined>
        : { value: Exclude<Parameters<Commands[K]>[0], undefined> };

/** The page setup commands whose value travels in `value`, with their `applyTo` beside it. */
type SectionValueCommandId = 'setOrientation' | 'setPageVerticalAlignment' | 'setLineNumbering' | 'setSectionBreakType' | 'setSectionTextDirection';

/** How a command's argument travels in an op, read off {@link OpArgs}. */
type Encoding<K extends CommandId> = Parameters<Commands[K]> extends []
  ? 'none'
  : K extends SectionValueCommandId
    ? 'section'
    : [OpArgs<K>] extends [{ value: Exclude<Parameters<Commands[K]>[0], undefined> }]
      ? 'value'
      : 'spread';

/**
 * A command inside another, as the formatting a style or the defaults take: an {@link Op}'s shape (`{ type, ... }`),
 * which the engine checks.
 */
export type NestedOp = { type: CommandId } & Record<string, unknown>;

/** A batch op: the command id as `type`, plus its arguments. A command with one argument puts it in `value`. */
export type Op = { [K in CommandId]: { type: K } & OpArgs<K> }[CommandId];

/** A command that takes no arguments: `commands[id]()` runs it. */
export type NoArgumentCommandId = { [K in CommandId]: Parameters<Commands[K]> extends [] ? K : never }[CommandId];

/**
 * A command's catalog entry. `takesArguments` tells the two kinds apart, so an entry filtered on
 * `!takesArguments` has a {@link NoArgumentCommandId} and runs as `commands[entry.id]()`.
 */
export type CommandInfo =
  | (CommandEntry & { id: NoArgumentCommandId; takesArguments: false })
  | (CommandEntry & { id: Exclude<CommandId, NoArgumentCommandId>; takesArguments: true });

/** What every catalog entry has. */
export interface CommandEntry {
  /** The command. */
  id: CommandId;
  /** The English label. */
  label: string;
  /** The group it belongs to, for building menus and ribbons. */
  group:
    | 'text'
    | 'clipboard'
    | 'history'
    | 'selection'
    | 'character'
    | 'paragraph'
    | 'tabs'
    | 'page'
    | 'headerFooter'
    | 'insert'
    | 'table'
    | 'object'
    | 'field'
    | 'control'
    | 'data'
    | 'references'
    | 'review'
    | 'view';
  /** Display strings: `'Ctrl+B'`, `'⌘B'`. */
  shortcuts: { windows: string[]; mac: string[] };
  /** Whether it takes arguments. */
  takesArguments: boolean;
  /** Whether it changes the document. */
  mutates: boolean;
  /** Its undo grouping. */
  undo: 'step' | 'typing' | 'view';
}

/** How each command's argument travels: checked against {@link Encoding} by the compiler. */
const encodings: { [K in CommandId]: Encoding<K> } = {
  insertText: 'value',
  insertParagraph: 'none',
  insertLineBreak: 'spread',
  insertPageBreak: 'none',
  insertColumnBreak: 'none',
  insertSectionBreak: 'value',
  insertStyleSeparator: 'none',
  insertTab: 'none',
  insertSymbol: 'spread',
  toggleCharacterCode: 'none',
  delete: 'spread',
  cut: 'none',
  copy: 'none',
  paste: 'spread',
  pasteText: 'value',
  repaste: 'value',
  dismissPasteOptions: 'none',
  setPasteOptions: 'spread',
  insertFile: 'spread',
  moveText: 'none',
  copyText: 'none',
  undo: 'spread',
  redo: 'spread',
  repeat: 'none',
  selectAll: 'none',
  select: 'value',
  moveSelection: 'spread',
  moveToCell: 'value',
  setSelectionMode: 'value',
  goBack: 'none',
  escape: 'none',
  selectNextObject: 'value',
  selectField: 'value',
  setSelection: 'spread',
  shrinkSelection: 'none',
  selectSimilarFormatting: 'none',
  selectStyleInstances: 'value',
  goToPage: 'value',
  goToBookmark: 'value',
  goTo: 'spread',
  setBold: 'value',
  setItalic: 'value',
  setUnderline: 'value',
  setStrikethrough: 'value',
  setDoubleStrikethrough: 'value',
  setSubscript: 'value',
  setSuperscript: 'value',
  setSmallCaps: 'value',
  setAllCaps: 'value',
  setHidden: 'value',
  setFont: 'value',
  setFontSize: 'value',
  growFont: 'none',
  shrinkFont: 'none',
  growFontOnePoint: 'none',
  shrinkFontOnePoint: 'none',
  setFontColor: 'value',
  setHighlight: 'value',
  setCharacterSpacing: 'value',
  setCharacterPosition: 'value',
  applyCharacterStyle: 'value',
  setTypography: 'spread',
  setFitText: 'value',
  setLanguage: 'spread',
  setCharacterEffects: 'spread',
  setTextEffects: 'spread',
  setUnderlineColor: 'value',
  clearFormatting: 'none',
  clearAllFormatting: 'none',
  changeCase: 'value',
  copyFormatting: 'none',
  pasteFormatting: 'none',
  setAlignment: 'value',
  setLineSpacing: 'spread',
  setParagraphSpacing: 'spread',
  setIndents: 'spread',
  increaseIndent: 'none',
  decreaseIndent: 'none',
  increaseHangingIndent: 'none',
  decreaseHangingIndent: 'none',
  clearParagraphFormatting: 'none',
  moveParagraphs: 'value',
  setParagraphStyle: 'value',
  updateStyleToSelection: 'value',
  deleteStyle: 'value',
  setStyleMeta: 'spread',
  setDocumentDefaults: 'spread',
  setDefaultFormatting: 'value',
  createStyle: 'spread',
  modifyStyle: 'spread',
  setList: 'value',
  setListLevel: 'value',
  setParagraphFlow: 'spread',
  setOutline: 'spread',
  promoteOutline: 'spread',
  demoteOutline: 'spread',
  promoteToHeading1: 'spread',
  demoteToBodyText: 'none',
  promoteHeading: 'spread',
  demoteHeading: 'spread',
  moveHeading: 'spread',
  deleteHeading: 'value',
  insertHeadingBefore: 'value',
  insertHeadingAfter: 'value',
  insertSubheading: 'value',
  selectHeadingContent: 'value',
  setParagraphTextDirection: 'value',
  setParagraphBorders: 'value',
  setParagraphShading: 'value',
  setParagraphBorderLines: 'spread',
  setShading: 'spread',
  setTextBorder: 'spread',
  setPageBorders: 'spread',
  setPageColor: 'value',
  setWatermark: 'value',
  removeWatermark: 'none',
  setFrame: 'spread',
  removeFrame: 'none',
  setDropCap: 'spread',
  setListDefinition: 'spread',
  setNumberingValue: 'spread',
  restartNumbering: 'none',
  continueNumbering: 'none',
  adjustListIndents: 'spread',
  applyListEntry: 'value',
  addTabStop: 'spread',
  moveTabStop: 'spread',
  deleteTabStop: 'value',
  clearTabStops: 'none',
  setDefaultTabStop: 'value',
  setAutoHyphenation: 'value',
  setHyphenationZone: 'value',
  setConsecutiveHyphenLimit: 'value',
  setHyphenateCaps: 'value',
  setMargins: 'spread',
  setOrientation: 'section',
  setPaperSize: 'spread',
  setColumns: 'spread',
  setHeaderFooterDistance: 'spread',
  setDifferentFirstPage: 'value',
  setDifferentOddEvenPages: 'value',
  setPageNumbering: 'spread',
  setPageVerticalAlignment: 'section',
  setLineNumbering: 'section',
  setSectionBreakType: 'section',
  setSectionTextDirection: 'section',
  setMultiplePages: 'spread',
  moveColumnBoundary: 'spread',
  editHeader: 'none',
  editFooter: 'none',
  closeHeaderFooter: 'none',
  insertPageNumber: 'spread',
  setHeaderFooterLinked: 'spread',
  removeHeaderFooter: 'spread',
  insertAlignmentTab: 'spread',
  insertTable: 'spread',
  insertPicture: 'spread',
  insertTextBox: 'none',
  insertShape: 'spread',
  insertFootnote: 'none',
  insertEndnote: 'none',
  insertNote: 'spread',
  setNoteProperties: 'spread',
  convertNotes: 'value',
  setNoteSeparator: 'spread',
  insertHyperlink: 'spread',
  editHyperlink: 'spread',
  removeHyperlink: 'none',
  insertBookmark: 'value',
  deleteBookmark: 'value',
  setDocumentProtection: 'spread',
  addEditableRange: 'spread',
  setEditableRangeEditor: 'spread',
  selectNextEditableRange: 'value',
  selectEditableRanges: 'none',
  setEditingGroups: 'value',
  removeEditableRanges: 'none',
  insertFormField: 'value',
  setFormField: 'spread',
  fillFormField: 'spread',
  selectNextFormField: 'value',
  setFormFieldShading: 'value',
  setDocumentProperties: 'spread',
  setDocumentVariable: 'spread',
  setFileName: 'value',
  insertField: 'spread',
  insertRows: 'spread',
  insertColumns: 'spread',
  deleteRows: 'none',
  deleteColumns: 'none',
  deleteTable: 'none',
  deleteCells: 'value',
  mergeCells: 'none',
  splitCells: 'spread',
  splitTable: 'none',
  convertTextToTable: 'spread',
  convertTableToText: 'spread',
  distributeRows: 'none',
  distributeColumns: 'none',
  moveRows: 'value',
  autoFitColumns: 'value',
  setCellAlignment: 'value',
  setCellShading: 'value',
  setCellBorders: 'spread',
  setCellMargins: 'spread',
  setCellProperties: 'spread',
  setRowProperties: 'spread',
  setTableBandSizes: 'spread',
  setTableCellSpacing: 'value',
  setTableAltText: 'spread',
  setHeaderRow: 'value',
  setRowHeight: 'value',
  setRowCanBreak: 'value',
  setColumnWidth: 'value',
  moveTableColumnBoundary: 'spread',
  moveTableRowBoundary: 'spread',
  setTableWidth: 'spread',
  setTableAlignment: 'value',
  setTableIndent: 'value',
  setTableStyle: 'value',
  setTableStyleOptions: 'spread',
  setDefaultTableStyle: 'value',
  setTablePosition: 'value',
  setTableCellMargins: 'spread',
  insertCells: 'value',
  setTableTool: 'value',
  setTablePen: 'value',
  drawTableStroke: 'spread',
  eraseTableBorder: 'spread',
  setObjectSize: 'spread',
  resetObjectSize: 'none',
  setObjectWrap: 'value',
  setAnchorLocked: 'value',
  setObjectPosition: 'spread',
  setObjectWrapOptions: 'spread',
  setAltText: 'spread',
  deleteObject: 'none',
  setShapePreset: 'value',
  setShapeAdjust: 'spread',
  setShapeFill: 'value',
  setShapeLine: 'spread',
  setShapeEffects: 'spread',
  setShapeText: 'spread',
  setPictureAdjust: 'spread',
  setPictureCrop: 'spread',
  changePicture: 'value',
  setObjectRotation: 'value',
  flipObject: 'value',
  arrangeObject: 'value',
  alignObjects: 'spread',
  groupObjects: 'none',
  ungroupObjects: 'none',
  regroupObjects: 'none',
  editWrapPoints: 'none',
  selectGroupChild: 'value',
  setObjectName: 'value',
  setObjectTitle: 'value',
  setObjectHidden: 'spread',
  selectObject: 'spread',
  restackObject: 'spread',
  setLockAspectRatio: 'value',
  setPictureRecolor: 'value',
  resetPicture: 'spread',
  setWrapPoints: 'value',
  resetWrapPoints: 'none',
  linkTextBox: 'value',
  beginTextBoxLink: 'none',
  unlinkTextBox: 'none',
  setField: 'spread',
  setFieldResults: 'spread',
  updateFields: 'spread',
  setFieldsLocked: 'spread',
  unlinkFields: 'spread',
  showFieldCodes: 'spread',
  setFieldLabels: 'spread',
  setSubTemplateRules: 'spread',
  setApplicationFieldTypes: 'spread',
  setSubTemplateView: 'spread',
  editSubTemplate: 'spread',
  closeSubTemplate: 'none',
  createSubTemplate: 'spread',
  renameSubTemplate: 'spread',
  removeSubTemplate: 'spread',
  replaceBody: 'spread',
  insertTableOfContents: 'spread',
  insertTableOfFigures: 'spread',
  insertManualTableOfContents: 'none',
  updateTableOfContents: 'spread',
  updateAllTablesOfContents: 'value',
  removeTableOfContents: 'spread',
  markTableOfContentsEntry: 'spread',
  insertCaption: 'spread',
  setAutoCaption: 'value',
  insertCrossReference: 'spread',
  insertControl: 'spread',
  setControl: 'spread',
  removeControl: 'spread',
  fillControl: 'spread',
  setCheckbox: 'spread',
  wrapInRepeatingSection: 'spread',
  addRepeatingItem: 'spread',
  removeElement: 'value',
  replaceInElements: 'spread',
  replaceFont: 'spread',
  markElements: 'spread',
  setProtectedPictureControls: 'spread',
  activatePictureControl: 'spread',
  writeDataPart: 'spread',
  removeDataPart: 'spread',
  setTrackChanges: 'value',
  setMarkupView: 'value',
  setShowMarkup: 'spread',
  setHiddenReviewers: 'spread',
  setBalloonMode: 'value',
  setMarkupStyle: 'spread',
  setTrackOptions: 'spread',
  acceptRevision: 'value',
  rejectRevision: 'value',
  acceptRevisionsInSelection: 'none',
  rejectRevisionsInSelection: 'none',
  acceptAllRevisions: 'spread',
  rejectAllRevisions: 'spread',
  acceptAllAndStopTracking: 'none',
  selectRevision: 'value',
  addComment: 'value',
  replyToComment: 'spread',
  editComment: 'spread',
  deleteComment: 'value',
  deleteAllComments: 'spread',
  resolveComment: 'spread',
  setActiveComment: 'value',
  selectComment: 'value',
  setAuthorColor: 'spread',
  setZoom: 'value',
  setViewOptions: 'spread',
  setMode: 'value',
  setSelectObjects: 'value',
  setRulerTabType: 'value',
  showTabTypes: 'none',
  showTabStop: 'none',
  setAutoCorrect: 'spread',
  revertAutoCorrect: 'none',
  neverAutoCorrect: 'none',
  showAutoCorrectOptions: 'none',
  closePopover: 'none',
  describeFormatting: 'none',
};

/** Builds a command's op from its method arguments. */
export function toOp(id: CommandId, arg: unknown, options?: unknown): Op {
  const how: string = encodings[id];
  if (how === 'none' || arg === undefined) return { type: id } as Op;
  if (how === 'value') return { type: id, value: arg } as Op;
  if (how === 'section') return { type: id, value: arg, ...(options as object | undefined) } as Op;
  return { type: id, ...(arg as object) } as Op;
}

/** The inverse of {@link toOp}: the command id and its method argument. */
export function fromOp(op: Op): [CommandId, unknown, unknown?] {
  const { type, ...fields } = op as { type: CommandId } & Record<string, unknown>;
  const how: string = encodings[type];
  if (how === 'none') return [type, undefined];
  if (how === 'value') return [type, fields.value];
  if (how === 'section') {
    const { value, ...options } = fields;
    return Object.keys(options).length ? [type, value, options] : [type, value];
  }
  return [type, fields];
}

/** Builds the `commands` object: every method turns its arguments into an op and runs it. */
export function createCommands(run: (op: Op) => Promise<unknown>): Commands {
  const commands: Record<string, (arg?: unknown, options?: unknown) => Promise<unknown>> = {};
  for (const id of Object.keys(encodings) as CommandId[]) commands[id] = (arg, options) => run(toOp(id, arg, options));
  return Object.freeze(commands) as unknown as Commands;
}

/** Where `replaceFont` rewrites: the whole document (every story, and the tables below the text) or the selected text. */
export type ReplaceScope = 'document' | 'selection';

/** How a paste takes the clipboard: Word's Paste Options, Paste Special's formats and the table choices. */
export type PasteAs =
  /** As the paste options say for where it came from. */
  | 'formatted'
  /** Keep Source Formatting. */
  | 'keepSource'
  /** Merge Formatting: the look of the text it lands in, keeping emphasis applied to part of it. */
  | 'merge'
  /** Use Destination Styles. */
  | 'destinationStyles'
  /** Keep Text Only: list numbers and bullets as text with `keepBullets`. */
  | 'textOnly'
  /** The clipboard's HTML. */
  | 'html'
  /** The clipboard's plain text as it is. */
  | 'text'
  /** The clipboard's picture, in line. */
  | 'picture'
  /** Copied cells as a table nested in the cell. */
  | 'nestTable'
  /** Copied cells as rows above the caret's row, made like this table's. */
  | 'mergeTable'
  /** Copied cells as rows above the caret's row, as they were copied. */
  | 'insertRows'
  /** Copied cells over the cells from the caret's, adding rows the table lacks. */
  | 'overwriteCells';

/** The smart cut and paste adjustments. */
export interface SmartPaste {
  /** Adjust sentence and word spacing. */
  wordSpacing: boolean;
  /** Adjust paragraph spacing on paste. */
  paragraphSpacing: boolean;
  /** Adjust table formatting and alignment: copied cells go into a table as the selection suggests. */
  tables: boolean;
  /** Smart style behavior. */
  styles: boolean;
  /** Merge pasted lists with surrounding lists. */
  mergeLists: boolean;
  /** Merge formatting when pasting from PowerPoint. */
  fromPowerPoint: boolean;
  /** Adjust formatting when pasting from Excel. */
  fromExcel: boolean;
}

/** The cut, copy and paste options (the state's `pasteOptions`); Word's defaults until changed. */
export interface PasteSettings {
  /** Pasting within the same document: `keepSource`, `merge` or `textOnly`. */
  within: PasteAs;
  /** Pasting between documents. */
  between: PasteAs;
  /** Pasting between documents when style definitions conflict: also `destinationStyles`. */
  conflict: PasteAs;
  /** Pasting from other programs. */
  other: PasteAs;
  /** Keep bullets and numbers when pasting with Keep Text Only. */
  keepBullets: boolean;
  /** Show the Paste Options button when content is pasted. */
  showButton: boolean;
  /** Use smart cut and paste: the `smart` adjustments are made only while it is on. */
  smartCutPaste: boolean;
  /** The smart cut and paste adjustments. */
  smart: SmartPaste;
}

/** `setPasteOptions`: the options that change. A default that isn't one its option offers fails `invalid`. */
export type PasteOptionsChange = Partial<Omit<PasteSettings, 'smart'>> & { smart?: Partial<SmartPaste> };

/** What Go To goes to. */
export type GoToWhat =
  | 'page'
  | 'section'
  | 'line'
  | 'bookmark'
  | 'comment'
  | 'footnote'
  | 'endnote'
  | 'field'
  | 'table'
  | 'graphic'
  | 'equation'
  | 'object'
  | 'heading';

/** Which one Go To goes to: the n-th (from 1), n on (back, negative; not 0), the next or the previous. */
export type GoToTarget = { absolute: number } | { relative: number } | 'next' | 'previous';

/** A range of heading or entry levels, 1 to 9. */
export interface TocLevels {
  /** The first level. */
  from: number;
  /** The last level, `from` or more. */
  to: number;
}

/** What a table of contents collects and how its entries read; each one left out is off. */
export interface TableOfContentsOptions {
  /** Paragraphs in the built-in heading styles of these levels. */
  headingLevels?: TocLevels;
  /** Paragraphs with an outline level, as their level (within `headingLevels` when set). */
  outlineLevels?: boolean;
  /** Paragraphs in these styles, at these levels (1 to 9). */
  styleLevels?: { style: StyleId; level: number }[];
  /** Table entry fields (Mark Entry's): of this identifier (`C` by default), of these levels. */
  entryFields?: { identifier?: string; levels?: TocLevels };
  /** Captions of a label: a table of figures. */
  captions?: { label: string; includeLabelAndNumber: boolean };
  /** Only what this bookmark marks. */
  bookmark?: string;
  /** Entries link to their source. */
  hyperlinks?: boolean;
  /** Which entries show a page number (all by default). */
  pageNumbers?: 'all' | 'none' | { omitLevels: TocLevels };
  /** What goes between an entry and its page number in place of a tab. */
  separator?: string;
  /** Page numbers led by a sequence's number and a separator (`-` by default). */
  chapterNumbers?: { sequence: string; separator?: string };
  /** Tab leaders and page numbers hidden in a web view. */
  hideInWebView?: boolean;
  /** Tabs in entries kept (else spaces). */
  keepTabs?: boolean;
  /** Line breaks in entries kept (else spaces). */
  keepLineBreaks?: boolean;
}

/** How a new table's entries are laid out. */
export interface TocLayout {
  /** Page numbers at a right tab stop at the right margin. Default true. */
  rightAlignPageNumbers?: boolean;
  /** That tab stop's leader. Default dots. */
  tabLeader?: TabLeader;
}

/** A new table's title and building block control (Word's Automatic Table 1: `{ title: 'Contents', buildingBlock: true }`). */
export interface TocFrame {
  /** A title paragraph before it, in the TOC Heading style. */
  title?: string;
  /** A building block content control around the title and the table. */
  buildingBlock?: boolean;
}

/** What updating a table of contents rewrites. */
export type TocUpdateMode = 'pageNumbers' | 'entireTable';

/** A caption's label and numbering, as `insertCaption` and `setAutoCaption` take them. */
export interface CaptionOptions {
  /** The label (`Figure`): a letter, then letters, digits, spaces and underscores. */
  label: string;
  /** Above or below the item (above a table, below anything else, by default). */
  position?: 'above' | 'below';
  /** The number without the label before it. */
  excludeLabel?: boolean;
  /** The number's format: decimal (the default), letters or roman numerals. */
  format?: 'decimal' | 'lowerLetter' | 'upperLetter' | 'lowerRoman' | 'upperRoman';
  /** A chapter number before it: the number of the last heading of `level` (1 to 9) before it. */
  chapter?: { level: number; separator: 'hyphen' | 'period' | 'colon' | 'emDash' | 'enDash' };
}

/** What a cross-reference can point to. */
export type CrossReferenceKind = 'numberedItem' | 'heading' | 'bookmark' | 'footnote' | 'endnote' | { caption: string };

/** What a cross-reference shows of its target. */
export type CrossReferenceContent =
  | 'text'
  | 'pageNumber'
  | 'number'
  | 'numberNoContext'
  | 'numberFullContext'
  | 'aboveBelow'
  | 'noteNumber'
  | 'noteNumberFormatted'
  | 'labelAndNumber'
  | 'captionText';
