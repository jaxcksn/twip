import type { CommandId } from './commands.js';
import type { DialogKind } from './services/dialogs.js';
import type { ControlKind } from './services/elements.js';
import type { RulerModel, RulerUnit, TabSelector } from './services/ruler.js';
import type { Color, CommentId, ElementId, Points, RevisionId, StyleId } from './types.js';

/** An on/off value over the selection: `'mixed'` where it differs, `null` where it doesn't apply. */
export type Tri = boolean | 'mixed' | null;

/** A value over the selection: `'mixed'` where it differs, `null` where it doesn't apply. */
export type Mixed<T> = T | 'mixed' | null;

/** The editor's UI snapshot: small, immutable, and cached on the calling side. */
export interface EditorState {
  /** Rises by one per committed transaction. */
  revision: number;
  /** Rises by one per observable change, selection included. */
  sequence: number;
  /** Whether a document is opening, ready, or still being laid out. */
  load: 'opening' | 'ready' | 'layingOut';
  /** Editing or the merge designer's design mode. */
  mode: 'editing' | 'design';
  /** Whether edits are refused. */
  readOnly: boolean;
  /** Changed since the last open or DOCX export; undoing back clears it. */
  modified: boolean;
  /** What is selected. */
  selection: SelectionSummary;
  /** The selection's character formatting. */
  formatting: CharacterFormatting;
  /** The selection's paragraph formatting. */
  paragraph: ParagraphFormatting;
  /** The selection's section. */
  page: PageFormatting;
  /** Each command's availability and value, keyed by command id. */
  commands: { readonly [K in CommandId]: CommandState<K> };
  /** Each dialog's availability: whether `dialogs.open` opens it now. */
  dialogs: { readonly [K in DialogKind]: boolean };
  /** Undo and redo. */
  history: { canUndo: boolean; canRedo: boolean; undoLabel: string | null; redoLabel: string | null };
  /**
   * The pages laid out so far and the caret's page. Background layout adds pages until `complete`; each change is a
   * new state with the same revision.
   */
  pages: { count: number; current: number; complete: boolean };
  /** View settings. */
  view: ViewState;
  /** Track changes and comments. */
  review: ReviewState;
  /** Spell checking. */
  spelling: { enabled: boolean; language: string; misspellings: number };
  /** Fonts being fetched and fonts that could not be found. */
  fonts: { pending: string[]; unresolved: string[] };
  /** The ruler model for the selection, in points, with no viewport mapping. */
  ruler: RulerModel;
  /** Whether formatting is held for `pasteFormatting`. */
  formatPainter: boolean;
}

/** The value each command reports in state, for the commands that report one. */
export interface CommandValues {
  /** The undo label. */
  undo: string | null;
  /** The redo label. */
  redo: string | null;
  /** The selection mode. */
  setSelectionMode: SelectionSummary['mode'];
  /** Bold. */
  setBold: Tri;
  /** Italic. */
  setItalic: Tri;
  /** The underline style. */
  setUnderline: Mixed<UnderlineStyle>;
  /** Strikethrough. */
  setStrikethrough: Tri;
  /** Double strikethrough. */
  setDoubleStrikethrough: Tri;
  /** Subscript. */
  setSubscript: Tri;
  /** Superscript. */
  setSuperscript: Tri;
  /** Small caps. */
  setSmallCaps: Tri;
  /** All caps. */
  setAllCaps: Tri;
  /** Hidden text. */
  setHidden: Tri;
  /** The document's font family. */
  setFont: Mixed<string>;
  /** The font size. */
  setFontSize: Mixed<Points>;
  /** The font colour. */
  setFontColor: Mixed<Color | 'auto'>;
  /** The highlight. */
  setHighlight: Mixed<HighlightColor | 'none'>;
  /** The character spacing. */
  setCharacterSpacing: Mixed<Points>;
  /** The raised or lowered position. */
  setCharacterPosition: Mixed<Points>;
  /** Whether formatting is held. */
  copyFormatting: boolean;
  /** Whether formatting is held. */
  pasteFormatting: boolean;
  /** The alignment. */
  setAlignment: ParagraphFormatting['alignment'];
  /** The line spacing. */
  setLineSpacing: Mixed<LineSpacing>;
  /** The space before and after. */
  setParagraphSpacing: { before: Mixed<Points | 'auto'>; after: Mixed<Points | 'auto'> };
  /** The indents. */
  setIndents: ParagraphFormatting['indents'];
  /** The paragraph style. */
  setParagraphStyle: Mixed<StyleId>;
  /** The list kind. */
  setList: ParagraphFormatting['list'];
  /** The list level. */
  setListLevel: Mixed<number>;
  /** The pagination flags. */
  setParagraphFlow: ParagraphFormatting['flow'];
  /** The paragraph borders. */
  setParagraphBorders: ParagraphFormatting['borders'];
  /** The paragraph shading. */
  setParagraphShading: Mixed<Color | 'none'>;
  /** The margins. */
  setMargins: PageFormatting['margins'];
  /** The orientation. */
  setOrientation: PageFormatting['orientation'];
  /** The paper size. */
  setPaperSize: PageFormatting['size'];
  /** The text columns. */
  setColumns: PageFormatting['columns'];
  /** The header and footer distances. */
  setHeaderFooterDistance: { header: Points; footer: Points };
  /** A different first page. */
  setDifferentFirstPage: boolean;
  /** Different odd and even pages. */
  setDifferentOddEvenPages: boolean;
  /** Whether changes are tracked. */
  setTrackChanges: boolean;
  /** The markup view. */
  setMarkupView: MarkupView;
  /** The active comment. */
  setActiveComment: CommentId | null;
  /** The zoom, in percent. */
  setZoom: number;
  /** The view options. */
  setViewOptions: { formattingMarks: boolean; gridlines: boolean; textBoundaries: boolean };
  /** The mode. */
  setMode: EditorState['mode'];
}

/** A command's entry in {@link EditorState.commands}. */
export interface CommandState<K extends CommandId = CommandId> {
  /** Whether running it now would succeed. */
  enabled: boolean;
  /** For on/off commands: whether it is on. */
  checked?: boolean;
  /** The command's state value, where it has one. */
  value?: K extends keyof CommandValues ? CommandValues[K] : never;
}

/** What is selected. No document position is public. */
export interface SelectionSummary {
  /** A caret, text, table cells, an object, or an element. */
  kind: 'caret' | 'text' | 'cells' | 'object' | 'element';
  /** The story the selection is in. */
  story: 'body' | 'header' | 'footer' | 'footnote' | 'endnote' | 'textBox';
  /** The page of the caret. */
  page: number;
  /** The selection mode. */
  mode: 'normal' | 'extend' | 'block';
  /** Nothing is selected (a caret). */
  empty: boolean;
  /**
   * The selection as plain text, paragraphs separated by `\n` (`''` for a caret), or `null` when it is longer than
   * 10,000 characters.
   */
  text: string | null;
  /** The table around the selection, if any. */
  table: TableSelection | null;
  /** The selected object, if any. */
  object: ObjectSelection | null;
  /** Innermost first: the element the selection is in or covers, then its parents. */
  context: ElementContext[];
  /** The link at the selection, if any. */
  link: { address: string; text: string } | null;
  /** The comment thread at the caret. */
  comment: CommentId | null;
  /** The revision at the caret. */
  revision: RevisionId | null;
  /** Set while a header or footer is being edited. */
  headerFooter: {
    kind: 'header' | 'footer';
    type: 'default' | 'first' | 'even';
    section: number;
    page: number;
    /** For example `'First Page Header -Section 2-'`. */
    label: string;
  } | null;
}

/** The table around the selection. */
export interface TableSelection {
  /** The row count. */
  rows: number;
  /** The column count. */
  columns: number;
  /** The current cell's row, 0-based. */
  row: number;
  /** The current cell's column, 0-based. */
  column: number;
  /** Whether the selected cells can merge. */
  canMerge: boolean;
  /** Repeat as header row. */
  headerRow: Tri;
  /** Rows can break across pages. */
  rowCanBreak: Tri;
  /** The vertical cell alignment. */
  cellAlignment: Mixed<'top' | 'center' | 'bottom'>;
  /** The cell shading. */
  cellShading: Mixed<Color | 'none'>;
  /** The row height. */
  rowHeight: Mixed<RowHeight>;
  /** The column width. */
  columnWidth: Mixed<Points>;
  /** The table style. */
  style: StyleId | null;
  /** The table style options. */
  styleOptions: TableStyleOptions;
  /** The table width. */
  width: TableWidth;
  /** The table alignment. */
  alignment: 'left' | 'center' | 'right';
  /** The table indent. */
  indent: Points;
}

/** A row height. */
export type RowHeight = { height: Points; rule: 'atLeast' | 'exact' } | 'auto';

/** Which parts of a table style apply. */
export interface TableStyleOptions {
  /** The header row. */
  headerRow: boolean;
  /** The total row. */
  totalRow: boolean;
  /** The first column. */
  firstColumn: boolean;
  /** The last column. */
  lastColumn: boolean;
  /** Banded rows. */
  bandedRows: boolean;
  /** Banded columns. */
  bandedColumns: boolean;
}

/** A table width. */
export type TableWidth = { kind: 'auto' } | { kind: 'points'; value: Points } | { kind: 'percent'; value: number };

/** The selected picture or text box. */
export interface ObjectSelection {
  /** A picture or a text box. */
  kind: 'picture' | 'textBox';
  /** How text wraps around it. */
  wrap: ObjectWrap;
  /** The width. */
  width: Points;
  /** The height. */
  height: Points;
  /** The alternative text. */
  altText: string;
  /** Marked decorative. */
  decorative: boolean;
  /** Whether a floating object's anchor is locked; `null` when inline. */
  anchorLocked: boolean | null;
}

/** How text wraps around an object. */
export type ObjectWrap = 'inline' | 'square' | 'tight' | 'through' | 'topAndBottom' | 'behindText' | 'inFrontOfText';

/** An element the selection is in or covers. */
export interface ElementContext {
  /** The element. */
  id: ElementId;
  /** Its kind. */
  kind: 'field' | ControlKind | 'repeatingItem';
  /** The selection covers exactly this element. */
  exact: boolean;
  /** A field's code. */
  code?: string;
  /** A control's tag. */
  tag?: string;
  /** A control's title. */
  title?: string;
}

/** The selection's character formatting. */
export interface CharacterFormatting {
  /** The document's family, never the substituted one. */
  font: Mixed<string>;
  /** The size. */
  size: Mixed<Points>;
  /** Bold. */
  bold: Tri;
  /** Italic. */
  italic: Tri;
  /** The underline style. */
  underline: Mixed<UnderlineStyle>;
  /** Strikethrough. */
  strikethrough: Tri;
  /** Double strikethrough. */
  doubleStrikethrough: Tri;
  /** Subscript. */
  subscript: Tri;
  /** Superscript. */
  superscript: Tri;
  /** Small caps. */
  smallCaps: Tri;
  /** All caps. */
  allCaps: Tri;
  /** Hidden. */
  hidden: Tri;
  /** The colour. */
  color: Mixed<Color | 'auto'>;
  /** The highlight. */
  highlight: Mixed<HighlightColor | 'none'>;
  /** The character spacing. */
  characterSpacing: Mixed<Points>;
  /** The raised (positive) or lowered position. */
  position: Mixed<Points>;
}

/** The selection's paragraph formatting. */
export interface ParagraphFormatting {
  /** The paragraph style. */
  style: Mixed<StyleId>;
  /** The alignment. */
  alignment: Mixed<'left' | 'center' | 'right' | 'justify'>;
  /** The line spacing. */
  lineSpacing: Mixed<LineSpacing>;
  /** The space before. */
  spaceBefore: Mixed<Points | 'auto'>;
  /** The space after. */
  spaceAfter: Mixed<Points | 'auto'>;
  /** The indents. */
  indents: { left: Mixed<Points>; right: Mixed<Points>; firstLine: Mixed<Points>; hanging: Mixed<Points> };
  /** The list kind. */
  list: Mixed<'bullet' | 'numbered' | 'none'>;
  /** The list level. */
  listLevel: Mixed<number>;
  /** The pagination flags. */
  flow: { keepWithNext: Tri; keepLinesTogether: Tri; pageBreakBefore: Tri; widowControl: Tri };
  /** The borders. */
  borders: Mixed<'box' | 'bottom' | 'none' | 'other'>;
  /** The shading. */
  shading: Mixed<Color | 'none'>;
}

/** The selection's section. */
export interface PageFormatting {
  /** The paper size. */
  size: { width: Points; height: Points };
  /** The orientation. */
  orientation: 'portrait' | 'landscape';
  /** The margins. */
  margins: { top: Points; bottom: Points; left: Points; right: Points; gutter: Points };
  /** The text columns. */
  columns: { count: number; spacing: Points; separator: boolean };
  /** The header distance from the page edge. */
  headerDistance: Points;
  /** The footer distance from the page edge. */
  footerDistance: Points;
  /** A different first page. */
  differentFirstPage: boolean;
  /** Different odd and even pages. */
  differentOddEven: boolean;
}

/** View settings. */
export interface ViewState {
  /** Percent, after any fit. */
  zoom: number;
  /** Formatting marks are shown. */
  formattingMarks: boolean;
  /** Table gridlines are shown. */
  gridlines: boolean;
  /** Text boundaries are shown. */
  textBoundaries: boolean;
  /** Field codes are shown. */
  fieldCodes: boolean;
  /** Which built-in rulers are shown. */
  rulers: { horizontal: boolean; vertical: boolean };
  /** The ruler unit. */
  rulerUnit: RulerUnit;
  /** What a click on the ruler adds. */
  tabSelector: TabSelector;
  /** The room beside the pages for balloons, CSS px. */
  markupPaneWidth: number;
  /** The editor background around the pages. */
  background: Color;
}

/** Track changes and comments. */
export interface ReviewState {
  /** Changes are tracked. */
  tracking: boolean;
  /** Document protection keeps tracking on. */
  trackingLocked: boolean;
  /** The markup view. */
  markupView: MarkupView;
  /** The revision count. */
  revisions: number;
  /** The comment count. */
  comments: number;
  /** The active comment thread. */
  activeComment: CommentId | null;
}

/** How tracked changes are shown. */
export type MarkupView = 'all' | 'simple' | 'noMarkup' | 'original';

/** A line spacing. */
export type LineSpacing = { rule: 'multiple'; lines: number } | { rule: 'exact' | 'atLeast'; size: Points };

/** An underline style. */
export type UnderlineStyle = 'none' | 'single' | 'words' | 'double' | 'thick' | 'dotted' | 'dash' | 'dotDash' | 'wave';

/** A highlight colour. */
export type HighlightColor =
  | 'yellow'
  | 'brightGreen'
  | 'turquoise'
  | 'pink'
  | 'blue'
  | 'red'
  | 'darkBlue'
  | 'teal'
  | 'green'
  | 'violet'
  | 'darkRed'
  | 'darkYellow'
  | 'gray50'
  | 'gray25'
  | 'black';
