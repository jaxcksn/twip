import type { CommandId } from './commands.js';
import type { DialogKind } from './services/dialogs.js';
import type { ControlKind, ReferenceRule } from './services/elements.js';
import type { RulerModel, RulerUnit, TabSelector } from './services/ruler.js';
import type { ChromeTheme, DocumentAppearance, MeasureDetail, ObjectGrid, ThemeColors } from './services/view.js';
import type { Color, CommentId, Degrees, ElementId, Points, RevisionId, ShapeState, StyleId } from './types.js';

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
  /** Fonts being fetched and fonts that could not be found. */
  fonts: { pending: string[]; unresolved: string[] };
  /** The ruler model for the selection, in points, with no viewport mapping. */
  ruler: RulerModel;
  /** Whether formatting is held for `pasteFormatting`. */
  formatPainter: boolean;
  /**
   * The document's 12 theme colours, in `ThemeColour` order (text1, background1, text2, background2, accent1 to
   * accent6, hyperlink, followedHyperlink), for an application's colour picker.
   */
  themeColours: Color[];
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

/** The selected object. */
export interface ObjectSelection {
  /** A picture, a text box, a shape, a line or connector (a shape with no filled path), or a group. */
  kind: 'picture' | 'textBox' | 'shape' | 'line' | 'group';
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
}

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
  /** A picture control's picture size (inline or floating). */
  size?: { width: Points; height: Points };
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
  /** Which fields are sub-template references (`setSubTemplateRules`); absent when none are. */
  subTemplateRules?: ReferenceRule[];
}

/** What the editor is on: the template, or one of its sub-templates. */
export type EditingTarget = { kind: 'template' } | { kind: 'subTemplate'; name: string; category: string };

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
