import type {
  BorderLine,
  CaptionOptions,
  ChapterNumbers,
  ColumnWidth,
  CommandId,
  DropCap,
  FrameProperties,
  TableTool,
  LigatureFlags,
  Ligatures,
  LineNumbers,
  MultiplePagesMode,
  NoteSettings,
  NumberForm,
  NumberFormat,
  NumberSpacing,
  PasteSettings,
  RangeEditorRef,
  SectionBreakKind,
  ShadingPattern,
  TextFlow,
  TextInputType,
  ThemeFont,
} from './commands.js';
import type { FontsState } from './fonts/library.js';
import type { BalloonMode, MarkupStyle, ReviewCounts, ShowMarkup } from './review.js';
import type { DialogKind } from './services/dialogs.js';
import type { CalendarType, ControlKind, DateStorage, ReferenceRule } from './services/elements.js';
import type { RulerModel, RulerUnit, TabSelector, TabStopInfo } from './services/ruler.js';
import type { ChromeTheme, CursorOverride, DocumentAppearance, FieldShading, MeasureDetail, ObjectGrid, ThemeColors } from './services/view.js';
import type {
  BoxPoint,
  CellBlock,
  Color,
  ColourValue,
  CommentId,
  Degrees,
  ElementId,
  FillValue,
  ObjectId,
  PictureAdjustState,
  Points,
  RevisionId,
  ShapeState,
  StyleId,
  TextEffects,
  TextRange,
  Watermark,
} from './types.js';

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
  /** What typing corrects (`setAutoCorrect`). */
  autoCorrect: AutoCorrectOptions;
  /** The cut, copy and paste options (`setPasteOptions`). */
  pasteOptions: PasteSettings;
  /** The caption inserted tables get (`setAutoCaption`); absent when off. */
  autoCaption?: CaptionOptions;
  /** The sub-template being edited; absent while the template is. */
  editing?: EditingTarget;
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
  dialogs: { readonly [K in Exclude<DialogKind, 'autoCorrect'>]: boolean };
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
  /** The document's fonts that draw with a stand-in, and the characters no font has. */
  fonts: FontsState;
  /** The ruler model for the selection, in points, with no viewport mapping. */
  ruler: RulerModel;
  /** Whether formatting is held for `pasteFormatting`. */
  formatPainter: boolean;
  /**
   * The document's 12 theme colours, in `ThemeColour` order (text1, background1, text2, background2, accent1 to
   * accent6, hyperlink, followedHyperlink), for an application's colour picker.
   */
  themeColours: Color[];
  /** The theme's heading (`+Headings`) and body (`+Body`) font families. */
  themeFonts: { heading: string; body: string };
}

/** A border line as the editor reads it back: its colour as written, its distance and its shadow. */
export type BorderReadback = BorderLine & { color: ColourValue | 'auto'; space: Points; shadow: boolean };

/** Shading as the editor reads it back: a fill, and a pattern in the pattern colour over it. */
export interface Shading {
  /** The fill, or none. */
  fill: ColourValue | 'none';
  /** The pattern. */
  pattern: ShadingPattern;
  /** The pattern's colour, or automatic. */
  color: ColourValue | 'auto';
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
  /** Automatic hyphenation. */
  setAutoHyphenation: boolean;
  /** The hyphenation zone. */
  setHyphenationZone: Mixed<Points>;
  /** The most consecutive hyphenated lines, 0 for no limit. */
  setConsecutiveHyphenLimit: Mixed<number>;
  /** Words in capitals are hyphenated. */
  setHyphenateCaps: boolean;
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
  setViewOptions: { formattingMarks: boolean; gridlines: boolean; textBoundaries: boolean; liveLayout: boolean };
  /** The mode. */
  setMode: EditorState['mode'];
  /** What a click on the ruler adds. */
  setRulerTabType: TabSelector;
  /** The caret's table's position, every field set; absent for a table in the text. */
  setTablePosition: Required<TablePosition>;
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
  story: 'body' | 'header' | 'footer' | 'footnote' | 'endnote' | 'textBox' | 'subTemplate';
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
  /**
   * Every selected range of text in document order: one for a text selection, one per line for a column block,
   * several for a discontiguous selection (`ranges.length > 1`); empty for a caret, cells or an object. At most 1,000.
   */
  ranges: TextRange[];
  /** The selection has more ranges than `ranges` lists. */
  rangesTruncated: boolean;
  /**
   * The selected blocks of cells of a discontiguous selection, in the order they were added; the selection's own block
   * is the last when `kind` is `'cells'`. Empty when at most one block is selected (`table` describes it).
   */
  cellBlocks: CellBlock[];
  /** The table around the selection, if any. */
  table: TableSelection | null;
  /** The selected object, if any. */
  object: ObjectSelection | null;
  /** Innermost first: the element the selection is in or covers, then its parents. */
  context: ElementContext[];
  /** The link at the selection, if any. */
  link: LinkInfo | null;
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
  /**
   * Set while F2 (`moveText`) or Shift+F2 (`copyText`) waits for its place: a click or the arrow keys choose it
   * (`placed`), Enter moves the text there, Esc gives up.
   */
  pendingMove?: { copy: boolean; placed: boolean };
  /**
   * Set while a list's numbers are selected (a click on a number): character formatting commands then format that
   * list level's numbers, and any selection move ends it.
   */
  listNumbers?: { list: number; level: number };
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
  /** The cells' preferred width. */
  cellWidth: Mixed<TableWidth>;
  /** The direction the cells' text runs in. */
  textDirection: Mixed<TextFlow>;
  /** The cells' text wraps. */
  wrapText: Tri;
  /** The cells' text is squeezed to fit. */
  fitText: Tri;
  /** The rows' end marks are hidden. */
  rowHidden: Tri;
  /** The space between the table's cells; absent for none. */
  cellSpacing?: Points;
  /** The table's alternative text title; absent when empty. */
  title?: string;
  /** The table's alternative text description; absent when empty. */
  description?: string;
  /** The rows and columns in each band, as it lays out, and the table's own (absent: its style's). */
  bands: { rows: number; columns: number; ownRows?: number; ownColumns?: number };
  /** The selected cells' borders, edge by edge: each cell's own line, else its row's or table's for that edge. */
  cellBorders: CellBorderLines;
  /** The table's own borders. */
  tableBorders: TableBorderLines;
  /** The selected cells' shading with its pattern (`cellShading` is its fill alone). */
  cellShadingValue: Mixed<Shading | 'none'>;
  /** The table's shading (behind spaced cells); absent for none. */
  tableShading?: Shading;
  /** The selected cells' margins on each side. */
  cellMargins: { top: Mixed<Points>; left: Mixed<Points>; bottom: Mixed<Points>; right: Mixed<Points> };
  /** The table's default cell margins (Table Options). */
  defaultCellMargins: { top: Points; left: Points; bottom: Points; right: Points };
  /** The selected cells' end marks take no height. */
  hideMark: Tri;
  /** The selected rows' exceptions to the table's properties; `'none'` where a row takes the table's. */
  rowExceptions: {
    any: Tri;
    alignment: Mixed<'left' | 'center' | 'right' | 'none'>;
    indent: Mixed<Points | 'none'>;
    cellSpacing: Mixed<Points | 'none'>;
    shading: Mixed<Shading | 'none'>;
    borders: Mixed<TableBorderLines | 'none'>;
  };
  /** How the table's columns are sized. */
  fit: TableFit;
  /** The columns the current cell spans. */
  columnSpan: number;
  /** The rows the current cell spans (more than one: merged down). */
  rowSpan: number;
}

/** How a table's columns are sized: fixed widths, fitted to their contents, or filling the width it is in. */
export type TableFit = 'fixed' | 'contents' | 'window';

/** The selected cells' edges, each the same line over them, `'mixed'`, `'none'`, or `null` where it doesn't apply. */
export interface CellBorderLines {
  /** The selection's top edge. */
  top: Mixed<BorderReadback | 'none'>;
  /** Its left edge. */
  left: Mixed<BorderReadback | 'none'>;
  /** Its bottom edge. */
  bottom: Mixed<BorderReadback | 'none'>;
  /** Its right edge. */
  right: Mixed<BorderReadback | 'none'>;
  /** The lines between the selected rows (`null` with one row). */
  insideHorizontal: Mixed<BorderReadback | 'none'>;
  /** The lines between the selected columns (`null` with one column). */
  insideVertical: Mixed<BorderReadback | 'none'>;
  /** The diagonal from top left to bottom right. */
  diagonalDown: Mixed<BorderReadback | 'none'>;
  /** The diagonal from bottom left to top right. */
  diagonalUp: Mixed<BorderReadback | 'none'>;
}

/** A table's own borders; absent for none. */
export interface TableBorderLines {
  /** The top edge. */
  top?: BorderReadback;
  /** The left edge. */
  left?: BorderReadback;
  /** The bottom edge. */
  bottom?: BorderReadback;
  /** The right edge. */
  right?: BorderReadback;
  /** The lines between rows. */
  insideHorizontal?: BorderReadback;
  /** The lines between columns. */
  insideVertical?: BorderReadback;
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

/** Which of a text box's lines text around it wraps tightly to. */
export type TightWrap = 'none' | 'allLines' | 'firstAndLastLine' | 'firstLineOnly' | 'lastLineOnly';

/** The selected object. */
export interface ObjectSelection {
  /** A picture, a text box, a shape, a line or connector (a shape with no filled path), a group, or a frame. */
  kind: ObjectKind;
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
  /** Where a floating object is and how text keeps from it; `null` when inline. */
  placement: ObjectPlacement | null;
  /** The shape's properties as written; `null` for objects without shape properties (frames) and kept graphics. */
  shape: ShapeState | null;
  /** Its rotation, clockwise. */
  rotation: Degrees;
  /** Flipped left to right. */
  flipH: boolean;
  /** Flipped top to bottom. */
  flipV: boolean;
  /** The selected child's place in its group; empty unless a group child is selected. */
  childPath: number[];
  /** How many objects are selected: 2 or more for a multi-selection, the other fields describing the first. */
  count: number;
  /** A picture's crop, by fractions of its image from each edge; absent when uncropped. */
  crop?: { left: number; top: number; right: number; bottom: number };
  /** A picture's Set Transparent Colour; absent for none. */
  transparentColor?: Color;
  /** A floating object's width as a share of a box on the page; absent for an absolute width. */
  relativeWidth?: RelativeSize;
  /** Its height as a share of a box on the page; absent for an absolute height. */
  relativeHeight?: RelativeSize;
  /** A floating object's id (a group child's when one is selected); absent for inline objects and frames. */
  id?: ObjectId;
  /** Its name, as the Selection Pane lists it. */
  name: string;
  /** Its alternative text's title; absent when empty. */
  title?: string;
  /** Hidden: not drawn, and text doesn't wrap around it. */
  hidden: boolean;
  /** Resizing it keeps its aspect ratio (Lock aspect ratio). */
  lockAspect: boolean;
  /** A picture's image at its own size; absent for other objects. */
  originalSize?: { width: Points; height: Points };
  /** Its place in the stack of its page's floating objects, 0 at the front; absent for inline objects and frames. */
  stackIndex?: number;
  /** A picture's adjustments; absent for other objects. */
  pictureAdjust?: PictureAdjustState;
  /** A text box's place in a chain of linked text boxes; absent when it is in none. */
  textBoxLink?: { index: number; count: number; previous?: ObjectId; next?: ObjectId };
  /**
   * A floating object's wrap polygon, unclosed, and whether it was edited by hand (kept when the object changes);
   * absent when it has none and its wrapping is neither tight nor through.
   */
  wrapPolygon?: { points: BoxPoint[]; edited: boolean };
  /** A frame's placement and whether other frames and objects may overlap it; absent for other objects. */
  frame?: { properties: FrameProperties; allowOverlap: boolean };
}

/** The kinds of selectable object. */
export type ObjectKind = 'picture' | 'textBox' | 'shape' | 'line' | 'group' | 'frame';

/** A floating object as the Selection Pane lists it (`objectList`). */
export interface ObjectListEntry {
  /** The object. */
  object: ObjectId;
  /** Its name. */
  name: string;
  /** What it is. */
  kind: ObjectKind;
  /** Hidden. */
  hidden: boolean;
  /** A group's objects, front first; absent for other objects. */
  children?: ObjectListEntry[];
}

/** A floating object's size as a share of a box on the page: 1 to 1000 percent. */
export interface RelativeSize {
  /** The share. */
  percent: number;
  /** The box: margins, page, or a margin (widths: left, right, inside, outside; heights: top, bottom). */
  relativeTo: 'margin' | 'page' | 'leftMargin' | 'rightMargin' | 'insideMargin' | 'outsideMargin' | 'topMargin' | 'bottomMargin';
}

/** How text wraps around an object. */
export type ObjectWrap = 'inline' | 'square' | 'tight' | 'through' | 'topAndBottom' | 'behindText' | 'inFrontOfText';

/** What a floating object's horizontal position is measured from. */
export type HorizontalReference =
  | 'column'
  | 'margin'
  | 'page'
  | 'character'
  | 'leftMargin'
  | 'rightMargin'
  | 'insideMargin'
  | 'outsideMargin';

/** What a floating object's vertical position is measured from: `paragraph` and `line` move it with the text. */
export type VerticalReference =
  | 'paragraph'
  | 'line'
  | 'margin'
  | 'page'
  | 'topMargin'
  | 'bottomMargin'
  | 'insideMargin'
  | 'outsideMargin';

/** A floating object aligned across its horizontal reference. */
export type ObjectHorizontalAlignment = 'left' | 'center' | 'right' | 'inside' | 'outside';

/** A floating object aligned across its vertical reference. */
export type ObjectVerticalAlignment = 'top' | 'center' | 'bottom' | 'inside' | 'outside';

/** Which sides of a floating object text wraps on. */
export type WrapSide = 'bothSides' | 'left' | 'right' | 'largest';

/**
 * A floating object's horizontal place: aligned across its reference, or an offset from the reference's left edge.
 * Set with neither, the object stays where it is, measured from the new reference.
 */
export interface HorizontalPosition {
  /** What it is measured from. */
  relativeTo: HorizontalReference;
  /** Aligned across the reference. */
  align?: ObjectHorizontalAlignment;
  /** Right of the reference's left edge, −1584 to 1584. */
  offset?: Points;
  /** Right of the reference's left edge by a share of its width, −1000 to 1000 percent. */
  percent?: number;
}

/**
 * A floating object's vertical place: aligned across its reference, or an offset below the reference's top. Set with
 * neither, the object stays where it is, measured from the new reference.
 */
export interface VerticalPosition {
  /** What it is measured from. */
  relativeTo: VerticalReference;
  /** Aligned across the reference. */
  align?: ObjectVerticalAlignment;
  /** Below the reference's top, −1584 to 1584. */
  offset?: Points;
  /** Below the reference's top by a share of its height, −1000 to 1000 percent. */
  percent?: number;
}

/** How far text keeps from a floating object. */
export interface TextDistance {
  /** Above it. */
  top: Points;
  /** Below it. */
  bottom: Points;
  /** On its left. */
  left: Points;
  /** On its right. */
  right: Points;
}

/** A floating object's place and its wrapping options. */
export interface ObjectPlacement {
  /** The horizontal place: an alignment or an offset. */
  horizontal: HorizontalPosition;
  /** The vertical place: an alignment or an offset. */
  vertical: VerticalPosition;
  /** Other floating objects may overlap it. */
  allowOverlap: boolean;
  /** In a table cell, it is kept within the cell. */
  layoutInCell: boolean;
  /** The sides text wraps on. */
  wrapSide: WrapSide;
  /** The distance from the text. */
  distance: TextDistance;
}

/**
 * A positioned table's place (`setTablePosition`): each axis as {@link HorizontalPosition} and
 * {@link VerticalPosition} give an object's, and its distance from the text around it. A table is measured from the
 * `column`, `margin` or `page` across, and from the `paragraph`, `margin` or `page` down; another reference is an
 * invalid argument. Given to the command, what isn't set is kept; in the state, every field is set.
 */
export interface TablePosition {
  /** The horizontal place: an alignment or an offset. */
  horizontal?: HorizontalPosition;
  /** The vertical place: an alignment or an offset. */
  vertical?: VerticalPosition;
  /** How far the text around it keeps, 0 to 1584. */
  wrapDistance?: TextDistance;
  /** Other positioned tables may overlap it. */
  allowOverlap?: boolean;
}

/** An element the selection is in or covers. */
export interface ElementContext {
  /** The element. */
  id: ElementId;
  /** Its kind. */
  kind: 'field' | ControlKind | 'repeatingItem';
  /** The selection covers exactly this element. */
  exact: boolean;
  /** Locked against editing: a locked field, or a control whose content can't be edited. */
  locked: boolean;
  /** A field shows its code rather than its result. */
  showingCode: boolean;
  /** A field's result is out of date: the next field update refreshes it. */
  dirty: boolean;
  /** A field's code. */
  code?: string;
  /** A control's tag. */
  tag?: string;
  /** A control's title. */
  title?: string;
  /** A picture control's picture size (inline or floating). */
  size?: { width: Points; height: Points };
  /** A legacy form field's options and value. */
  form?: FormFieldInfo;
  /** A content control's look and options. */
  control?: ControlInfo;
}

/** The link at the selection. */
export interface LinkInfo {
  /** Where it goes: its target, then `#` and the bookmark when it names one. */
  address: string;
  /** Its display text. */
  text: string;
  /** The target without its bookmark: empty for a place in this document. */
  target: string;
  /** The place in the target it goes to (a bookmark name). */
  bookmark?: string;
  /** The ScreenTip shown when it is hovered. */
  screenTip?: string;
  /** The window or frame it opens in. */
  frame?: string;
}

/** A content control's look and options; what doesn't apply to its kind is absent. */
export interface ControlInfo {
  /** How it shows its edges. */
  appearance: 'boundingBox' | 'tags' | 'hidden';
  /** Its colour; absent for the default. */
  color?: Color;
  /** It goes when its content is edited. */
  temporary: boolean;
  /** The character style text typed into it takes. */
  style?: StyleId;
  /** A check box's checked symbol. */
  checkedSymbol?: string;
  /** A check box's unchecked symbol. */
  uncheckedSymbol?: string;
  /** What a repeating section's items are called. */
  sectionTitle?: string;
  /** A repeating section's items can be added and removed. */
  allowInsertDelete?: boolean;
  /** A building block gallery's entries: the package's building blocks in its gallery and category, by name. */
  buildingBlocks?: string[];
  /** A building block gallery's (or building block's) gallery, by Word's name for it; absent: any. */
  gallery?: string;
  /** A building block gallery's (or building block's) category; absent: any. */
  category?: string;
  /** A date control's date (`YYYY-MM-DDTHH:MM:SSZ`); absent while it shows its placeholder or no date. */
  date?: string;
  /** A date control's calendar. */
  calendar?: CalendarType;
  /** What a date control stores in its data. */
  storeAs?: DateStorage;
}

/** A legacy form field's options and value; what doesn't apply to its kind is absent. */
export interface FormFieldInfo {
  /** Its kind. */
  kind: 'text' | 'checkBox' | 'dropDown';
  /** Its bookmark name. */
  name: string;
  /** It can be filled in. */
  enabled: boolean;
  /** The document's fields update when it is left. */
  calculateOnExit: boolean;
  /** The help F1 shows. */
  helpText?: string;
  /** The help the status bar shows. */
  statusText?: string;
  /** `helpText` names an AutoText entry whose text is the help. */
  helpIsAutoText: boolean;
  /** `statusText` names an AutoText entry whose text is the help. */
  statusIsAutoText: boolean;
  /** The macro run on entry. */
  entryMacro?: string;
  /** The macro run on exit. */
  exitMacro?: string;
  /** What a text field takes. */
  inputType?: TextInputType;
  /** A text field's default text. */
  defaultText?: string;
  /** The most characters a text field takes (0: any). */
  maxLength?: number;
  /** A text field's text format (`Uppercase`, a number or date picture). */
  format?: string;
  /** A check box is checked. */
  checked?: boolean;
  /** A check box's size, or its text's (`'auto'`). */
  size?: Points | 'auto';
  /** A check box is checked by default. */
  checkedByDefault?: boolean;
  /** A drop-down's entries. */
  entries?: string[];
  /** The entry a drop-down shows. */
  entry?: number;
  /** The entry a drop-down shows by default, from 0. */
  defaultEntry?: number;
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
  /** The character style, or `'none'`. */
  characterStyle: Mixed<StyleId | 'none'>;
  /** The horizontal scale, percent. */
  scale: Mixed<number>;
  /** Kerning at or above this size; 0: off. */
  kerning: Mixed<Points>;
  /** The ligatures used (a combination Word's dialog doesn't offer reads as mixed). */
  ligatures: Mixed<Ligatures>;
  /** The digits' widths. */
  numberSpacing: Mixed<NumberSpacing>;
  /** The digits' forms. */
  numberForm: Mixed<NumberForm>;
  /** The stylistic set (1…20), or 0 for the font's default. */
  stylisticSet: Mixed<number>;
  /** Contextual alternates. */
  contextualAlternates: Tri;
  /** The language (a BCP 47 tag). */
  language: Mixed<string>;
  /** Spelling and grammar are not checked. */
  noProof: Tri;
  /** Emboss. */
  emboss: Tri;
  /** Engrave. */
  engrave: Tri;
  /** Outline. */
  outline: Tri;
  /** Shadow. */
  shadow: Tri;
  /** The underline's colour, or the text's (`'auto'`). */
  underlineColor: Mixed<Color | 'auto'>;
  /** The text effects (fill, outline, shadow, glow and reflection), through the character style. */
  textEffects: Mixed<TextEffects>;
  /** The theme font the font is, or `'none'` (a family by name). */
  fontTheme: Mixed<ThemeFont | 'none'>;
  /** The colour as written: RGB, or a theme colour with its tint or shade (`color` is what it shows). */
  colorValue: Mixed<ColourValue | 'auto'>;
  /** The underline's colour as written, or the text's (`'auto'`). */
  underlineColorValue: Mixed<ColourValue | 'auto'>;
  /** The text's border, or `'none'`. */
  textBorder: Mixed<BorderReadback | 'none'>;
  /** The text's shading, or `'none'`. */
  textShading: Mixed<Shading | 'none'>;
  /** The width the text is fitted to (Fit Text), or `'none'`. */
  fitText: Mixed<Points | 'none'>;
  /** Each kind of OpenType ligature on or off. */
  ligatureFlags: Mixed<Required<LigatureFlags>>;
  /** The OpenType stylistic sets on (1 to 20), in order. */
  stylisticSets: Mixed<number[]>;
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
  indents: { left: Mixed<Points>; right: Mixed<Points>; firstLine: Mixed<Points>; hanging: Mixed<Points>; mirror: Tri };
  /** The list kind. */
  list: Mixed<'bullet' | 'numbered' | 'none'>;
  /** The list level. */
  listLevel: Mixed<number>;
  /** The pagination flags. */
  flow: {
    keepWithNext: Tri;
    keepLinesTogether: Tri;
    pageBreakBefore: Tri;
    widowControl: Tri;
    dontHyphenate: Tri;
    suppressLineNumbers: Tri;
    textboxTightWrap: Mixed<TightWrap>;
  };
  /** The borders. */
  borders: Mixed<'box' | 'bottom' | 'none' | 'other'>;
  /** The shading. */
  shading: Mixed<Color | 'none'>;
  /** No space between paragraphs of the same style. */
  contextualSpacing: Tri;
  /** The outline level (1 to 9), or body text (0). */
  outlineLevel: Mixed<number>;
  /** Collapsed when the document opens. */
  collapsed: Tri;
  /** The text direction. */
  textDirection: Mixed<TextFlow>;
  /** The selection's paragraph's drop cap, when it has one. */
  dropCap?: DropCap;
  /** The top border, or `'none'`. */
  borderTop: Mixed<BorderReadback | 'none'>;
  /** The left border. */
  borderLeft: Mixed<BorderReadback | 'none'>;
  /** The bottom border. */
  borderBottom: Mixed<BorderReadback | 'none'>;
  /** The right border. */
  borderRight: Mixed<BorderReadback | 'none'>;
  /** The line between paragraphs with the same borders. */
  borderBetween: Mixed<BorderReadback | 'none'>;
  /** The shading in full (`shading` is its fill as shown), or `'none'`. */
  shadingValue: Mixed<Shading | 'none'>;
  /** The tab stops, own and inherited, in position order: the same for every selected paragraph, or mixed. */
  tabs: Mixed<TabStopInfo[]>;
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
  /** The text columns; `widths` is empty when they are `equal`. */
  columns: { count: number; spacing: Points; separator: boolean; widths: ColumnWidth[]; equal: boolean };
  /** The header distance from the page edge. */
  headerDistance: Points;
  /** The footer distance from the page edge. */
  footerDistance: Points;
  /** A different first page. */
  differentFirstPage: boolean;
  /** Different odd and even pages. */
  differentOddEven: boolean;
  /** The page number format. */
  pageNumberFormat: NumberFormat;
  /** The section's first page number, or `null` when it continues from the section before. */
  pageNumberStart: number | null;
  /** Chapter numbers in page numbers, or `null`. */
  chapterNumbers: ChapterNumbers | null;
  /** Where text sits between the top and bottom margins. */
  verticalAlignment: 'top' | 'center' | 'justified' | 'bottom';
  /** The line numbering, or `null`. */
  lineNumbers: LineNumbers | null;
  /** The break the section starts with. */
  breakType: SectionBreakKind;
  /** The section's text direction. */
  textDirection: TextFlow;
  /** How the document's pages pair. */
  multiplePages: MultiplePagesMode;
  /** The gutter is at the top. */
  gutterAtTop: boolean;
  /** Which of the section's headers take the section before's (Link to Previous). */
  headerLinks: { default: boolean; first: boolean; even: boolean };
  /** Which of its footers do. */
  footerLinks: { default: boolean; first: boolean; even: boolean };
  /** The section's footnote settings. */
  footnotes: NoteSettings;
  /** The section's endnote settings. */
  endnotes: NoteSettings;
  /** The footnote area's columns; 0 for the section's own. */
  footnoteColumns: number;
  /** The page colour, gradient, pattern or picture (its image empty), or absent for none. */
  background?: FillValue;
  /** The watermark the pages show (a picture's image empty), or absent. */
  watermark?: Watermark;
  /** The section's page borders: each edge's line (absent: none), what the distances are measured from, and more. */
  pageBorders: {
    top?: BorderReadback;
    left?: BorderReadback;
    bottom?: BorderReadback;
    right?: BorderReadback;
    offsetFrom: 'page' | 'text';
    display: 'allPages' | 'firstPage' | 'notFirstPage';
    surroundHeader: boolean;
    surroundFooter: boolean;
    behindText: boolean;
  };
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
  /** Text wraps around an object again as it is dragged; otherwise a preview shows until the release. */
  liveLayout: boolean;
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
  /** The background around the pages: the application's colour, or the theme's desk. */
  background: Color;
  /** The chrome's colour scheme. */
  theme: ChromeTheme;
  /** The application's colours for the light and dark schemes. */
  themeColors: ThemeColors;
  /** The chrome's accent as drawn (the dark scheme lightens it): for popovers and other chrome that matches the editor's. */
  accent: Color;
  /** Text and glyphs drawn on the accent. */
  accentOn: Color;
  /** How the page is drawn on screen. */
  appearance: DocumentAppearance;
  /** How much the editor measures while you drag and hover. */
  detail: MeasureDetail;
  /** When the objects' dot grid shows. */
  objectGrid: ObjectGrid;
  /** Content controls and shown sub-templates are labelled while selected or hovered. */
  controlLabels: boolean;
  /** When fields are shaded. */
  fieldShading: FieldShading;
  /** Bookmarks are shown. */
  bookmarks: boolean;
  /** Tab characters are shown (or every mark is). */
  tabCharacters: boolean;
  /** Spaces are shown. */
  spaces: boolean;
  /** Paragraph marks and breaks are shown. */
  paragraphMarks: boolean;
  /** Hidden text is shown. */
  hiddenText: boolean;
  /** Optional hyphens are shown. */
  optionalHyphens: boolean;
  /** The selected floating object's anchor is shown. */
  objectAnchors: boolean;
  /** Click and Type is on. */
  clickAndType: boolean;
  /** Selected text can be dragged and dropped. */
  textDragAndDrop: boolean;
  /** The white space between pages is hidden. It opens as the document was saved. */
  hideWhiteSpace: boolean;
  /** Under enforced protection, the regions the author may edit are highlighted. */
  showEditableRegions: boolean;
  /** The selected floating object's wrap points are being edited. */
  editingWrapPoints: boolean;
  /** Highlight All's highlights show (`find.highlightAll`); they go when the document changes. */
  findHighlights: boolean;
  /** The highlighter's colour while Highlight with nothing selected made the pointer one; absent when off. */
  highlighter?: HighlightColor;
  /** The pointer shapes shown over elements instead of the editor's own; absent when none are set. */
  cursorOverrides?: CursorOverride[];
  /** Which fields are sub-template references (`setSubTemplateRules`); absent when none are. */
  subTemplateRules?: ReferenceRule[];
  /** Create Link's pointer is on: the next click on an empty text box links the selected one to it. */
  linkingTextBox: boolean;
  /** Select Objects is on. */
  selectingObjects: boolean;
  /** The Select Objects rectangle being dragged: its page and box (left, top, right, bottom, points); absent when none. */
  objectMarquee?: { page: number; rect: [Points, Points, Points, Points] };
  /** The pointer's table tool. */
  tableTool: TableTool;
  /** The line the Draw Table pen draws. */
  tablePen: BorderReadback;
  /**
   * What the Draw Table pen's stroke being dragged would draw, in page points, for the host to show dashed in the pen's
   * colour: a line or diagonal from `from` to `to`, or a new table's outline (`from` its top left, `to` its bottom
   * right). Absent when no stroke is dragged.
   */
  tableToolFeedback?: { page: number; kind: 'line' | 'diagonal' | 'outline'; from: [Points, Points]; to: [Points, Points] };
  /** The border under the pointer the eraser would remove, in page points, for the host to highlight; absent when none. */
  tableEraserTarget?: { page: number; from: [Points, Points]; to: [Points, Points] };
}

/** What the editor is on: the template, or one of its sub-templates. */
export type EditingTarget = { kind: 'template' } | { kind: 'subTemplate'; name: string; category: string };

/** Track changes and comments. */
export interface ReviewState {
  /** Changes are tracked. */
  tracking: boolean;
  /** Document protection keeps tracking on. */
  trackingLocked: boolean;
  /** What the document's protection allows, when it has one. */
  protection?: {
    kind: 'none' | 'readOnly' | 'comments' | 'trackedChanges' | 'forms';
    enforced: boolean;
    formatting: boolean;
    /** Stopping it needs a password. */
    password: boolean;
    /** The selection is in a range the author may edit. */
    inEditableRange: boolean;
    /** Filling in forms: the sections (by index) anyone may edit. */
    unprotectedSections: number[];
    /** The editing restriction takes exceptions: editable ranges and who may edit them (read only, comments). */
    exceptionsAllowed: boolean;
    /** The editing restriction leaves chosen sections open (filling in forms). */
    sectionsAllowed: boolean;
    /** AutoFormat as you type may override the formatting restriction. */
    autoFormatOverride: boolean;
    /** The formatting restriction keeps the theme from changing. */
    blockThemeSwitching: boolean;
    /** The formatting restriction keeps the style set from changing. */
    blockStyleSetSwitching: boolean;
    /** Who may edit the editable ranges holding the selection. */
    editorsHere: RangeEditorRef[];
    /** The number of editable ranges. */
    rangeCount: number;
  };
  /** The markup view. */
  markupView: MarkupView;
  /** The revision count. */
  revisions: number;
  /** The comment count. */
  comments: number;
  /** The active comment thread. */
  activeComment: CommentId | null;
  /** Which kinds of markup show, and whose. It opens as the document was saved. */
  showMarkup: ShowMarkup;
  /** Where revisions show in All Markup. */
  balloonMode: BalloonMode;
  /** How revisions draw. */
  markupStyle: MarkupStyle;
  /** Tracking records moves as moves. */
  trackMoves: boolean;
  /** Tracking records formatting changes. */
  trackFormatting: boolean;
  /** The revisions and threads by kind: all, and those Show Markup shows. */
  counts: ReviewCounts;
}

/** What typing corrects (AutoCorrect): each rule on or off, and the texts never corrected again. All on by default. */
export interface AutoCorrectOptions {
  /** `THe` → `The`. */
  twoInitialCapitals: boolean;
  /** The first letter of a sentence. */
  sentenceCapitals: boolean;
  /** The first letter of a table cell. */
  cellCapitals: boolean;
  /** `monday` → `Monday`. */
  dayNames: boolean;
  /** The replacement list and the symbols (`(c)` → `©`). */
  replaceText: boolean;
  /** Straight quotes to curly ones. */
  smartQuotes: boolean;
  /** `1st` with the suffix raised. */
  ordinals: boolean;
  /** `1/2` → `½`. */
  fractions: boolean;
  /** Hyphens to en and em dashes. */
  dashes: boolean;
  /** `1.` or `1)` then a space or tab at a paragraph's start makes it a numbered list. */
  numberedLists: boolean;
  /** `*` or `-` then a space or tab at a paragraph's start makes it a bulleted list. */
  bulletedLists: boolean;
  /** AutoFormat: three or more `-`, `_`, `=`, `*`, `~` or `#` then Enter make the paragraph a bottom border line. */
  borderLines: boolean;
  /** AutoFormat: an Internet or network path (`www.example.com`, `name@example.com`) then a space or Enter becomes a link. */
  internetPaths: boolean;
  /** AutoFormat: Tab and Backspace at a paragraph's start set its indents (and Tab at a list's first item moves the list). */
  tabIndents: boolean;
  /** The texts Never correct added (or `setAutoCorrect` set): never corrected again. Absent when empty. */
  never?: string[];
}

/** `setAutoCorrect`'s change: the rules given switch, and the lists given replace the shipped ones. */
export interface AutoCorrectChange extends Partial<Omit<AutoCorrectOptions, 'never'>> {
  /** The replacement list, in place of the shipped one. */
  entries?: AutoCorrectEntry[];
  /** Abbreviations after which a sentence doesn't start (`e.g.`), in place of the shipped ones. */
  firstLetterExceptions?: string[];
  /** Words whose two initial capitals stay (`IDs`), in place of the shipped ones. */
  initialCapsExceptions?: string[];
  /** The texts never corrected. */
  never?: string[];
}

/** One replacement-list entry. */
export interface AutoCorrectEntry {
  /** What's typed. */
  from: string;
  /** What it becomes. */
  to: string;
}

/** How tracked changes are shown. */
export type MarkupView = 'all' | 'simple' | 'noMarkup' | 'original';

/** A line spacing. */
export type LineSpacing = { rule: 'multiple'; lines: number } | { rule: 'exact' | 'atLeast'; size: Points };

/** An underline style. */
export type UnderlineStyle = 'none' | 'single' | 'words' | 'double' | 'thick' | 'dotted' | 'dash' | 'dotDash' | 'wave' | 'dottedHeavy' | 'dashedHeavy' | 'dashLong' | 'dashLongHeavy' | 'dashDotHeavy' | 'dotDotDash' | 'dashDotDotHeavy' | 'wavyHeavy' | 'wavyDouble';

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
