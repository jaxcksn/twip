import type { RichText } from './review.js';
import type { ControlKind, ControlOptions, DataBinding } from './services/elements.js';
import type { TabAlignment, TabLeader } from './services/ruler.js';
import type {
  HighlightColor,
  LineSpacing,
  MarkupView,
  ObjectWrap,
  RowHeight,
  TableWidth,
  UnderlineStyle,
} from './state.js';
import type { Color, CommentId, ElementId, Points, RevisionId, StyleId } from './types.js';

/** Which elements a field command acts on: the given ids, or the selection's by default. */
export interface FieldTargets {
  /** The fields; default: the selection's. */
  ids?: ElementId[];
}

/** How a picture placeholder looks before it is filled. */
export interface PlaceholderLook {
  /** Which placeholders: one, those with a tag or a key prefix, or the default for the rest. */
  match: { id: ElementId } | { tag: string } | { prefix: string } | 'default';
  /** A fill, label and icon, or an image. */
  look: { fill?: Color; label?: string; iconSvg?: string } | { image: Uint8Array; mime: string };
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
  /** Inserts a line break (Shift+Enter). */
  insertLineBreak(): Promise<void>;
  /** Inserts a page break (Ctrl+Enter); in the body, not in a table. */
  insertPageBreak(): Promise<void>;
  /** Inserts a column break (Ctrl+Shift+Enter); in the body, not in a table. */
  insertColumnBreak(): Promise<void>;
  /** Inserts a section break; in the body, not in a table. */
  insertSectionBreak(kind: 'nextPage' | 'continuous' | 'evenPage' | 'oddPage'): Promise<void>;
  /** Inserts a tab character. Typing. */
  insertTab(): Promise<void>;
  /** Inserts one character, optionally in a symbol font. */
  insertSymbol(options: { char: string; font?: string }): Promise<void>;
  /** Deletes backward or forward, by character (the default) or word. Typing. */
  delete(options: { direction: 'backward' | 'forward'; unit?: 'character' | 'word' }): Promise<void>;

  // Clipboard and history
  /** Cuts the selection to the clipboard. */
  cut(): Promise<void>;
  /** Copies the selection to the clipboard; also when read only. */
  copy(): Promise<void>;
  /** Pastes from the system clipboard, formatted (the default) or as text. Fails `not-allowed` where the browser denies reading it. */
  paste(options?: { as?: 'formatted' | 'text' }): Promise<void>;
  /** Pastes the given text. */
  pasteText(text: string): Promise<void>;
  /** Undoes the last step. */
  undo(): Promise<void>;
  /** Redoes the last undone step. */
  redo(): Promise<void>;
  /** Repeats the last edit (F4). */
  repeat(): Promise<void>;

  // Selection and navigation: never change the document
  /** Selects the whole story. */
  selectAll(): Promise<void>;
  /** Selects the unit around the selection; the table units in a table. */
  select(unit: 'word' | 'sentence' | 'paragraph' | 'cell' | 'row' | 'column' | 'table'): Promise<void>;
  /** Moves or extends the selection. */
  moveSelection(options: {
    by: 'character' | 'word' | 'line' | 'paragraph' | 'lineEdge' | 'document' | 'screen' | 'page' | 'rowEdge' | 'columnEdge';
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
  /** Moves the caret to a page, from 1. */
  goToPage(page: number): Promise<void>;

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
  /** Sets the font family (non-empty). */
  setFont(family: string): Promise<void>;
  /** Sets the font size, 1 to 1638 in 0.5 steps. */
  setFontSize(size: Points): Promise<void>;
  /** Grows the font to the next standard size (Ctrl+Shift+&gt;). */
  growFont(): Promise<void>;
  /** Shrinks the font to the previous standard size (Ctrl+Shift+&lt;). */
  shrinkFont(): Promise<void>;
  /** Sets the font colour. */
  setFontColor(color: Color | 'auto'): Promise<void>;
  /** Sets or removes (`null`) the highlight. */
  setHighlight(color: HighlightColor | null): Promise<void>;
  /** Sets the character spacing, -1584 to 1584. */
  setCharacterSpacing(spacing: Points): Promise<void>;
  /** Raises (positive) or lowers the text, -1584 to 1584. */
  setCharacterPosition(position: Points): Promise<void>;
  /** Clears character formatting (Ctrl+Space). */
  clearFormatting(): Promise<void>;
  /** Changes the case of the selection (Shift+F3 cycles). */
  changeCase(to: 'lower' | 'upper' | 'sentence' | 'title' | 'toggle'): Promise<void>;
  /** Holds the selection's formatting for {@link Commands.pasteFormatting} (Ctrl+Shift+C). */
  copyFormatting(): Promise<void>;
  /** Applies the held formatting to the selection (Ctrl+Shift+V). */
  pasteFormatting(): Promise<void>;

  // Paragraph formatting
  /** Sets the alignment (Ctrl+L, E, R, J). */
  setAlignment(alignment: 'left' | 'center' | 'right' | 'justify'): Promise<void>;
  /** Sets the line spacing: 0.06 to 132 lines, or 0.7 to 1584 points. */
  setLineSpacing(spacing: LineSpacing): Promise<void>;
  /** Sets the space before and after, 0 to 1584; unset fields keep their value. */
  setParagraphSpacing(options: { before?: Points | 'auto'; after?: Points | 'auto' }): Promise<void>;
  /** Sets the indents, -1584 to 1584; `firstLine` and `hanging` are exclusive. Unset fields keep their value. */
  setIndents(options: { left?: Points; right?: Points; firstLine?: Points; hanging?: Points }): Promise<void>;
  /** Increases the left indent (Ctrl+M). */
  increaseIndent(): Promise<void>;
  /** Decreases the left indent (Ctrl+Shift+M). */
  decreaseIndent(): Promise<void>;
  /** Applies an existing paragraph style. */
  setParagraphStyle(styleId: StyleId): Promise<void>;
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
  }): Promise<void>;
  /** Sets the paragraph borders (a half-point single line). */
  setParagraphBorders(preset: 'box' | 'bottom' | 'none'): Promise<void>;
  /** Sets or removes (`null`) the paragraph shading. */
  setParagraphShading(color: Color | null): Promise<void>;

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

  // Page layout: the selection's sections
  /** Sets the margins, 0 to 1584; unset fields keep their value. */
  setMargins(options: { top?: Points; bottom?: Points; left?: Points; right?: Points; gutter?: Points }): Promise<void>;
  /** Sets the orientation. */
  setOrientation(orientation: 'portrait' | 'landscape'): Promise<void>;
  /** Sets the paper size, in portrait, 36 to 1584. */
  setPaperSize(options: { width: Points; height: Points }): Promise<void>;
  /** Sets the text columns: 1 to 45, with spacing and a separator line. */
  setColumns(options: { count: number; spacing?: Points; separator?: boolean }): Promise<void>;
  /** Sets the header and footer distances; unset fields keep their value. */
  setHeaderFooterDistance(options: { header?: Points; footer?: Points }): Promise<void>;
  /** Sets a different first page header and footer. */
  setDifferentFirstPage(on: boolean): Promise<void>;
  /** Sets different odd and even page headers and footers. */
  setDifferentOddEvenPages(on: boolean): Promise<void>;
  /** Moves a section text column's boundary. */
  moveColumnBoundary(options: { column: number; to: Points }): Promise<void>;

  // Headers and footers
  /** Edits the caret page's header, creating it when the page has none. */
  editHeader(): Promise<void>;
  /** Edits the caret page's footer, creating it when the page has none. */
  editFooter(): Promise<void>;
  /** Returns to the body (Esc). */
  closeHeaderFooter(): Promise<void>;
  /** Inserts a page number at the caret, or in the header or footer. */
  insertPageNumber(options: {
    format: 'page' | 'pageOfTotal' | 'total';
    where?: 'caret' | 'header' | 'footer';
    alignment?: 'left' | 'center' | 'right';
  }): Promise<void>;

  // Insert
  /** Inserts a table of 1 to 32767 rows and 1 to 63 columns. */
  insertTable(options: { rows: number; columns: number }): Promise<void>;
  /** Inserts a picture (PNG, JPEG, GIF, BMP or TIFF). */
  insertPicture(options: { data: Uint8Array; altText?: string }): Promise<void>;
  /** Inserts a text box. */
  insertTextBox(): Promise<void>;
  /** Inserts a footnote. */
  insertFootnote(): Promise<void>;
  /** Inserts an endnote. */
  insertEndnote(): Promise<void>;
  /** Inserts a link; `text` replaces the selection (default: the selection, or the address). */
  insertHyperlink(options: { address: string; text?: string }): Promise<void>;
  /** Changes the link at the selection. */
  editHyperlink(options: { address?: string; text?: string }): Promise<void>;
  /** Removes the link at the selection, keeping its text. */
  removeHyperlink(): Promise<void>;
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
  /** Sets or removes (`null`) the cell shading. */
  setCellShading(color: Color | null): Promise<void>;
  /** Sets the cell borders on some edges; no border removes them. */
  setCellBorders(options: {
    edges: 'all' | 'outside' | 'inside' | 'insideHorizontal' | 'insideVertical' | 'top' | 'bottom' | 'left' | 'right' | 'none';
    border?: { style: 'single' | 'double' | 'dotted' | 'dashed' | 'thick'; width: Points; color: Color | 'auto' };
  }): Promise<void>;
  /** Sets the cell margins, 0 to 1584; unset fields keep their value. */
  setCellMargins(options: { top?: Points; bottom?: Points; left?: Points; right?: Points }): Promise<void>;
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

  // Pictures and text boxes: the selected object
  /** Sets the object's size, 1 to 1584. */
  setObjectSize(options: { width?: Points; height?: Points; lockAspect?: boolean }): Promise<void>;
  /** Resets the object to its original size. */
  resetObjectSize(): Promise<void>;
  /** Sets how text wraps around the object. */
  setObjectWrap(wrap: ObjectWrap): Promise<void>;
  /** Locks a floating object's anchor. */
  setAnchorLocked(on: boolean): Promise<void>;
  /** Sets the alternative text, or marks the object decorative. */
  setAltText(options: { text: string; decorative?: boolean }): Promise<void>;
  /** Deletes the object. */
  deleteObject(): Promise<void>;

  // Fields
  /** Changes a field's code or display. */
  setField(options: { id: ElementId; code?: string; display?: string | RichText }): Promise<void>;
  /** Supplies field results; `token` joins the step a `fieldsUpdateRequested` event started. */
  setFieldResults(options: { results: { id: ElementId; display: string | RichText }[]; token?: number }): Promise<void>;
  /** Updates fields (F9); emits `fieldsUpdateRequested` for the fields the application computes. */
  updateFields(options?: FieldTargets): Promise<void>;
  /** Locks or unlocks fields (Ctrl+F11, Ctrl+Shift+F11). */
  setFieldsLocked(options: { locked: boolean } & FieldTargets): Promise<void>;
  /** Replaces fields by their results (Ctrl+Shift+F9). */
  unlinkFields(options?: FieldTargets): Promise<void>;
  /** Shows field codes or results (Alt+F9, Shift+F9). View. */
  showFieldCodes(options: { show: boolean } & FieldTargets): Promise<void>;
  /** What design mode shows for each field code. View. */
  setFieldLabels(options: { labels: Record<string, string>; merge?: boolean }): Promise<void>;

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
    wrapSelection?: boolean;
  }): Promise<{ id: ElementId }>;
  /** Changes a content control; a `null` binding removes it. */
  setControl(options: {
    id: ElementId;
    tag?: string;
    title?: string;
    lock?: 'none' | 'control' | 'content' | 'both';
    placeholder?: string;
    binding?: DataBinding | null;
    options?: ControlOptions;
  }): Promise<void>;
  /** Removes a content control, optionally keeping its content. */
  removeControl(options: { id: ElementId; keepContent?: boolean }): Promise<void>;
  /** Fills a content control with exactly one of text, an image (fitted to its box) or DOCX content. */
  fillControl(options: { id: ElementId; text?: string; image?: Uint8Array; docx?: Uint8Array }): Promise<void>;
  /** Checks or unchecks a checkbox control. */
  setCheckbox(options: { id: ElementId; checked: boolean }): Promise<void>;
  /** Wraps the selected paragraphs or table rows in a repeating section. */
  wrapInRepeatingSection(options?: { tag?: string; title?: string }): Promise<{ id: ElementId }>;
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
  /** Draws wavy marks with a tooltip on elements. View. */
  markElements(options: {
    marks: { id: ElementId; severity: 'error' | 'warning' | 'info'; message: string }[];
    clear?: boolean;
  }): Promise<void>;
  /** Sets how picture placeholders look. View. */
  setPlaceholderLooks(options: { looks: PlaceholderLook[]; merge?: boolean }): Promise<void>;
  /** Fills the selected placeholder, or every unfilled one with `key`. */
  fillPlaceholder(options: { key?: string; image: Uint8Array }): Promise<void>;

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
  /** Accepts a revision; a move resolves as a pair. */
  acceptRevision(id: RevisionId): Promise<void>;
  /** Rejects a revision; a move resolves as a pair. */
  rejectRevision(id: RevisionId): Promise<void>;
  /** Accepts the revisions in the selection. */
  acceptRevisionsInSelection(): Promise<void>;
  /** Rejects the revisions in the selection. */
  rejectRevisionsInSelection(): Promise<void>;
  /** Accepts every revision, or one author's. */
  acceptAllRevisions(options?: { author?: string }): Promise<void>;
  /** Rejects every revision, or one author's. */
  rejectAllRevisions(options?: { author?: string }): Promise<void>;
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
  /** Shows or hides formatting marks (Ctrl+Shift+8), gridlines and text boundaries. */
  setViewOptions(options: { formattingMarks?: boolean; gridlines?: boolean; textBoundaries?: boolean }): Promise<void>;
  /** Switches between editing and design mode. */
  setMode(mode: 'editing' | 'design'): Promise<void>;
}

/** A command id: the method name, the batch op's `type`, the catalog id and the key in `state.commands`. */
export type CommandId = keyof Commands;

/**
 * An op's fields besides `type`. A single argument (a primitive, {@link RichText}, bytes, an array, or a union with a
 * primitive) goes in `value`; an options object, and a tagged value such as {@link LineSpacing} or
 * {@link TableWidth}, is spread into the op.
 */
export type OpArgs<K extends CommandId> = Parameters<Commands[K]> extends []
  ? {}
  : [Exclude<Parameters<Commands[K]>[0], undefined>] extends [string | number | boolean | null]
    ? { value: Exclude<Parameters<Commands[K]>[0], undefined> }
    : [Exclude<Parameters<Commands[K]>[0], undefined>] extends [RichText | Uint8Array | readonly unknown[]]
      ? { value: Exclude<Parameters<Commands[K]>[0], undefined> }
      : [Exclude<Parameters<Commands[K]>[0], undefined>] extends [object]
        ? Exclude<Parameters<Commands[K]>[0], undefined>
        : { value: Exclude<Parameters<Commands[K]>[0], undefined> };

/** How a command's argument travels in an op, read off {@link OpArgs}. */
type Encoding<K extends CommandId> = Parameters<Commands[K]> extends []
  ? 'none'
  : [OpArgs<K>] extends [{ value: Exclude<Parameters<Commands[K]>[0], undefined> }]
    ? 'value'
    : 'spread';

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
  insertLineBreak: 'none',
  insertPageBreak: 'none',
  insertColumnBreak: 'none',
  insertSectionBreak: 'value',
  insertTab: 'none',
  insertSymbol: 'spread',
  delete: 'spread',
  cut: 'none',
  copy: 'none',
  paste: 'spread',
  pasteText: 'value',
  undo: 'none',
  redo: 'none',
  repeat: 'none',
  selectAll: 'none',
  select: 'value',
  moveSelection: 'spread',
  moveToCell: 'value',
  setSelectionMode: 'value',
  goBack: 'none',
  escape: 'none',
  selectNextObject: 'value',
  goToPage: 'value',
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
  setFontColor: 'value',
  setHighlight: 'value',
  setCharacterSpacing: 'value',
  setCharacterPosition: 'value',
  clearFormatting: 'none',
  changeCase: 'value',
  copyFormatting: 'none',
  pasteFormatting: 'none',
  setAlignment: 'value',
  setLineSpacing: 'spread',
  setParagraphSpacing: 'spread',
  setIndents: 'spread',
  increaseIndent: 'none',
  decreaseIndent: 'none',
  setParagraphStyle: 'value',
  setList: 'value',
  setListLevel: 'value',
  setParagraphFlow: 'spread',
  setParagraphBorders: 'value',
  setParagraphShading: 'value',
  addTabStop: 'spread',
  moveTabStop: 'spread',
  deleteTabStop: 'value',
  clearTabStops: 'none',
  setDefaultTabStop: 'value',
  setMargins: 'spread',
  setOrientation: 'value',
  setPaperSize: 'spread',
  setColumns: 'spread',
  setHeaderFooterDistance: 'spread',
  setDifferentFirstPage: 'value',
  setDifferentOddEvenPages: 'value',
  moveColumnBoundary: 'spread',
  editHeader: 'none',
  editFooter: 'none',
  closeHeaderFooter: 'none',
  insertPageNumber: 'spread',
  insertTable: 'spread',
  insertPicture: 'spread',
  insertTextBox: 'none',
  insertFootnote: 'none',
  insertEndnote: 'none',
  insertHyperlink: 'spread',
  editHyperlink: 'spread',
  removeHyperlink: 'none',
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
  distributeRows: 'none',
  distributeColumns: 'none',
  moveRows: 'value',
  autoFitColumns: 'value',
  setCellAlignment: 'value',
  setCellShading: 'value',
  setCellBorders: 'spread',
  setCellMargins: 'spread',
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
  setObjectSize: 'spread',
  resetObjectSize: 'none',
  setObjectWrap: 'value',
  setAnchorLocked: 'value',
  setAltText: 'spread',
  deleteObject: 'none',
  setField: 'spread',
  setFieldResults: 'spread',
  updateFields: 'spread',
  setFieldsLocked: 'spread',
  unlinkFields: 'spread',
  showFieldCodes: 'spread',
  setFieldLabels: 'spread',
  insertControl: 'spread',
  setControl: 'spread',
  removeControl: 'spread',
  fillControl: 'spread',
  setCheckbox: 'spread',
  wrapInRepeatingSection: 'spread',
  removeElement: 'value',
  replaceInElements: 'spread',
  markElements: 'spread',
  setPlaceholderLooks: 'spread',
  fillPlaceholder: 'spread',
  writeDataPart: 'spread',
  removeDataPart: 'spread',
  setTrackChanges: 'value',
  setMarkupView: 'value',
  acceptRevision: 'value',
  rejectRevision: 'value',
  acceptRevisionsInSelection: 'none',
  rejectRevisionsInSelection: 'none',
  acceptAllRevisions: 'spread',
  rejectAllRevisions: 'spread',
  selectRevision: 'value',
  addComment: 'value',
  replyToComment: 'spread',
  editComment: 'spread',
  deleteComment: 'value',
  resolveComment: 'spread',
  setActiveComment: 'value',
  selectComment: 'value',
  setAuthorColor: 'spread',
  setZoom: 'value',
  setViewOptions: 'spread',
  setMode: 'value',
};

/** Builds a command's op from its method arguments. */
export function toOp(id: CommandId, arg: unknown): Op {
  const how: string = encodings[id];
  if (how === 'none' || arg === undefined) return { type: id } as Op;
  if (how === 'value') return { type: id, value: arg } as Op;
  return { type: id, ...(arg as object) } as Op;
}

/** The inverse of {@link toOp}: the command id and its method argument. */
export function fromOp(op: Op): [CommandId, unknown] {
  const { type, ...fields } = op as { type: CommandId } & Record<string, unknown>;
  const how: string = encodings[type];
  if (how === 'none') return [type, undefined];
  if (how === 'value') return [type, fields.value];
  return [type, fields];
}

/** Builds the `commands` object: every method turns its arguments into an op and runs it. */
export function createCommands(run: (op: Op) => Promise<unknown>): Commands {
  const commands: Record<string, (arg?: unknown) => Promise<unknown>> = {};
  for (const id of Object.keys(encodings) as CommandId[]) commands[id] = (arg) => run(toOp(id, arg));
  return Object.freeze(commands) as unknown as Commands;
}
