import type {
  BorderLine,
  BorderLineStyle,
  DocumentDefaults,
  FollowNumber,
  FrameProperties,
  ListEntryId,
  ListLevel,
  NumberFormat,
  ShadingPattern,
} from '../commands.js';
import type { Session } from '../editor.js';
import type { LineSpacing, Mixed, RowHeight, TableWidth, Tri, UnderlineStyle } from '../state.js';
import { Method } from '../transport/protocol.js';
import { type PreviewPicture, type PreviewSize, toPicture } from './navigation.js';
import type { Color, Points, StyleId } from '../types.js';
import type { TabAlignment, TabLeader } from './ruler.js';

/** The editor's dialogs as typed models: the application draws them. */
export interface Dialogs {
  /** Opens a dialog over the selection's current values. */
  open<K extends DialogKind>(kind: K): Promise<DialogSessionOf<K>>;
  /**
   * Opens the style dialog: Modify Style on a style, or Create New Style from Formatting on the selection's formatting
   * (in its short form, the name and the preview, until `shortForm` is set to `false`).
   */
  openStyle(open: 'create' | { modify: StyleId }): Promise<StyleDialogSession>;
}

/** The style dialog: a dialog session, and its Format menu. */
export interface StyleDialogSession extends DialogSession<'style'> {
  /** Opens a Format menu dialog on the style: applying it changes this dialog, not the document. */
  openFormat<K extends FormatDialogKind>(kind: K): Promise<FormatDialogSession<K>>;
  /** The preview box as PNG, with the changes not yet applied: a greyed previous paragraph, text in the style, a greyed following one. */
  preview(size: PreviewSize): Promise<PreviewPicture>;
}

/** The style dialog's Format menu: the dialogs it opens on the style. */
export type FormatDialogKind = 'font' | 'paragraph' | 'tabs' | 'bordersShading' | 'frame' | 'bulletsNumbering' | 'tableProperties' | 'banding';

/** A dialog of the style dialog's Format menu. */
export interface FormatDialogSession<K extends FormatDialogKind> extends Omit<DialogSession<K>, 'update' | 'apply'> {
  /** Changes some values; dependent values and field states are recomputed. */
  update(patch: Partial<DialogValues[K]>): Promise<FormatDialogSession<K>>;
  /** Applies it to the style dialog: resolves with the style dialog after it. */
  apply(): Promise<StyleDialogSession>;
}

/** A dialog. */
export type DialogKind =
  | 'font'
  | 'paragraph'
  | 'pageSetup'
  | 'insertTable'
  | 'tableProperties'
  | 'pictureSize'
  | 'altText'
  | 'hyperlink'
  | 'tabs'
  | 'accessibility'
  | 'autoCorrect'
  | 'style'
  | 'manageStyles'
  | 'bordersShading'
  | 'frame'
  | 'banding'
  | 'defineBullet'
  | 'defineNumberFormat'
  | 'defineMultilevelList'
  | 'setNumberingValue'
  | 'adjustListIndents'
  | 'bulletsNumbering';

/** An open dialog. */
export interface DialogSession<K extends DialogKind> {
  /** Which dialog. */
  readonly kind: K;
  /** The selection's current values, `'mixed'` where they differ. */
  readonly values: DialogValues[K];
  /** Each field's label, kind, range and state, by field name; a field may have no entry. */
  readonly fields: { readonly [F in keyof DialogValues[K]]?: FieldInfo };
  /** Changes some values; dependent values and field states are recomputed. Resolves with the new model. */
  update(patch: Partial<DialogValues[K]>): Promise<DialogSessionOf<K>>;
  /** Validates, then applies: one transaction and one undo step. */
  apply(): Promise<void>;
  /** Closes the dialog and changes nothing. */
  cancel(): void;
}

/** The accessibility dialog's session: adds selecting an issue's object. */
export interface AccessibilityDialogSession extends DialogSession<'accessibility'> {
  /** Selects the issue's object. */
  select(issueId: string): Promise<void>;
}

/**
 * A dialog kind's session: {@link AccessibilityDialogSession} for `'accessibility'`, else {@link DialogSession}. Over a
 * kind known only at run time it is a union by `kind`: code that draws any dialog from its fields can update it by
 * field name, and narrow on `kind` for the accessibility dialog's `select`.
 */
export type DialogSessionOf<K extends DialogKind = DialogKind> = K extends 'accessibility'
  ? AccessibilityDialogSession
  : K extends 'style'
    ? StyleDialogSession
    : DialogSession<K>;

/** A dialog field's metadata. */
export interface FieldInfo {
  /** The English label. */
  label: string;
  /** Whether it can be changed now. */
  enabled: boolean;
  /** The control it wants. */
  kind: 'number' | 'length' | 'text' | 'choice' | 'toggle' | 'color' | 'list';
  /** The minimum. */
  min?: number;
  /** The maximum. */
  max?: number;
  /** The step. */
  step?: number;
  /** A choice's options. */
  options?: { value: string; label: string }[];
}

/** Each dialog's values. */
export interface DialogValues {
  /** The Font dialog. */
  font: {
    font: Mixed<string>;
    style: Mixed<'regular' | 'italic' | 'bold' | 'boldItalic'>;
    size: Mixed<Points>;
    color: Mixed<Color | 'auto'>;
    underline: Mixed<UnderlineStyle>;
    underlineColor: Mixed<Color | 'auto'>;
    strikethrough: Tri;
    doubleStrikethrough: Tri;
    superscript: Tri;
    subscript: Tri;
    smallCaps: Tri;
    allCaps: Tri;
    hidden: Tri;
    characterSpacing: Mixed<Points>;
    position: Mixed<Points>;
  };
  /** The Paragraph dialog. */
  paragraph: {
    alignment: Mixed<'left' | 'center' | 'right' | 'justify'>;
    outlineLevel: Mixed<number | 'body'>;
    left: Mixed<Points>;
    right: Mixed<Points>;
    special: Mixed<'none' | 'firstLine' | 'hanging'>;
    by: Mixed<Points>;
    before: Mixed<Points | 'auto'>;
    after: Mixed<Points | 'auto'>;
    lineSpacing: Mixed<LineSpacing>;
    keepWithNext: Tri;
    keepLinesTogether: Tri;
    pageBreakBefore: Tri;
    widowControl: Tri;
  };
  /** The Page setup dialog. */
  pageSetup: {
    top: Points;
    bottom: Points;
    left: Points;
    right: Points;
    gutter: Points;
    orientation: 'portrait' | 'landscape';
    width: Points;
    height: Points;
    headerDistance: Points;
    footerDistance: Points;
    differentFirstPage: boolean;
    differentOddEven: boolean;
    applyTo: 'section' | 'wholeDocument';
  };
  /** The Insert table dialog. */
  insertTable: { rows: number; columns: number };
  /** The Table properties dialog. */
  tableProperties: {
    width: TableWidth;
    alignment: 'left' | 'center' | 'right';
    indent: Points;
    rowHeight: Mixed<RowHeight>;
    rowCanBreak: Tri;
    headerRow: Tri;
    columnWidth: Mixed<Points>;
    cellAlignment: Mixed<'top' | 'center' | 'bottom'>;
    cellMargins: { top: Points; bottom: Points; left: Points; right: Points };
  };
  /** The Picture size dialog. */
  pictureSize: { width: Points; height: Points; scaleWidth: number; scaleHeight: number; lockAspect: boolean; reset: boolean };
  /** The Alt text dialog. */
  altText: { text: string; decorative: boolean };
  /** The Hyperlink dialog. */
  hyperlink: { text: string; address: string };
  /** The Tabs dialog. */
  tabs: {
    stops: { position: Points; alignment: TabAlignment; leader: TabLeader; inherited: boolean }[];
    defaultStop: Points;
    /** The stop being edited. */
    position: Points | null;
    alignment: TabAlignment;
    leader: TabLeader;
    /** Stops to clear on apply. */
    cleared: Points[];
    /** The Set, Clear and Clear All buttons, applied by `update`. */
    action: 'none' | 'set' | 'clear' | 'clearAll';
  };
  /** The Accessibility checker (read only). */
  accessibility: { issues: { id: string; text: string; tip: boolean }[] };
  /** The AutoCorrect rules: the application's own dialog, asked for by `dialogRequested`. `open` rejects `unsupported`. */
  autoCorrect: Record<never, never>;
  /** The style dialog: opened with {@link Dialogs.openStyle}. */
  style: StyleDialogValues;
  /**
   * Manage Styles: its Edit, Recommend, Restrict and Set Defaults tabs. Modify, New Style and Delete are
   * {@link Dialogs.openStyle} and `deleteStyle`; Import/Export isn't offered.
   */
  manageStyles: {
    /** The tab shown. */
    tab: 'edit' | 'recommend' | 'restrict' | 'setDefaults';
    /** How the list sorts. */
    sort: 'asRecommended' | 'alphabetical' | 'basedOn' | 'byType';
    /** Only the styles that show in lists. */
    recommendedOnly: boolean;
    /** The styles in `sort` order: their visibility, priority and lock are what Recommend and Restrict change. */
    styles: {
      id: StyleId;
      name: string;
      type: 'paragraph' | 'character' | 'table' | 'list';
      builtIn: boolean;
      visibility: 'show' | 'hideUntilUsed' | 'hide';
      priority: number;
      locked: boolean;
    }[];
    /** The style selected in the list. */
    selected: StyleId | null;
    /** The selected style's formatting in words, written by the editor. */
    description: string;
    /** The document's default formatting (Set Defaults). */
    defaults: DocumentDefaults;
  };
  /**
   * Borders and Shading: the borders of the selected paragraphs, and the shading of the paragraphs or the text. A
   * setting (`none`, `box`, `shadow`) sets the edges with the line `style`, `color` and `width`; setting an edge makes
   * it `custom`. The Page Border tab does the same for the section's pages.
   */
  bordersShading: {
    /** The tab shown. */
    tab: 'borders' | 'pageBorder' | 'shading';
    /** The setting. */
    setting: 'none' | 'box' | 'shadow' | 'custom';
    /** The line the setting and new edges take. */
    style: BorderLineStyle;
    color: Color | 'auto';
    width: Points;
    /** The edges, or `null` for none. */
    top: BorderLine | null;
    bottom: BorderLine | null;
    left: BorderLine | null;
    right: BorderLine | null;
    /** The line between paragraphs (a table style part's inside horizontal line). */
    between: BorderLine | null;
    /** A table style part's inside vertical line. */
    insideVertical: BorderLine | null;
    /** The paragraphs, the text (a box only), or a table style part's cells. */
    applyTo: 'paragraph' | 'text' | 'cell';
    /** The shading's fill. */
    fill: Color | 'none';
    /** The shading's pattern, and its color. */
    pattern: ShadingPattern;
    patternColor: Color | 'auto';
    /** Page Border: the setting, the page's edges, what they're measured from, the pages and the sections. */
    pageSetting: 'none' | 'box' | 'shadow' | 'custom';
    pageTop: BorderLine | null;
    pageBottom: BorderLine | null;
    pageLeft: BorderLine | null;
    pageRight: BorderLine | null;
    pageOffsetFrom: 'page' | 'text';
    pageDisplay: 'allPages' | 'firstPage' | 'notFirstPage';
    pageApplyTo: 'wholeDocument' | 'thisSection';
  };
  /** Banding (a table style's, from the style dialog's Format menu): the rows and the columns in each band. */
  banding: { rowBand: number; columnBand: number };
  /**
   * The Frame dialog, on the frame the selection is in. Move with text is the vertical position relative to the
   * paragraph; `remove` is its Remove Frame button.
   */
  frame: FrameProperties & {
    /** Move with text. */
    moveWithText: boolean;
    /** Remove Frame: applying removes the frame. */
    remove: boolean;
  };
  /** Define New Bullet: the bullet of the selection's list level (or a new list's). */
  defineBullet: { text: string; font: string | null; picture: string | null; alignment: 'left' | 'center' | 'right' };
  /** Define New Number Format: the number of the selection's list level (or a new list's). */
  defineNumberFormat: { format: NumberFormat; text: string; font: string | null; alignment: 'left' | 'center' | 'right' };
  /** Define New Multilevel List: the nine levels, the level shown (0 to 8), what they apply to, and the ListNum name. */
  defineMultilevelList: {
    levels: ListLevel[];
    level: number;
    applyTo: 'wholeList' | 'thisPointForward' | 'currentParagraph';
    galleryLevel: number;
    listName: string;
  };
  /** Set Numbering Value: opens on the paragraph's number. */
  setNumberingValue: { mode: 'startNew' | 'continue'; value: number; advance: boolean };
  /** Adjust List Indents. */
  adjustListIndents: { numberPosition: Points; textIndent: Points; follow: FollowNumber; tab: Points | null };
  /** Bullets and Numbering (Numbering on the style dialog's Format menu): a gallery entry, or `null` for no list. */
  bulletsNumbering: { gallery: 'bullet' | 'numbering' | 'multilevel'; entry: ListEntryId | null };
}

/** The style dialog's values. */
export type StyleDialogValues = {
  /** Creating a style, or modifying one. */
  open: 'create' | { modify: StyleId };
  /** The short form: the name and the preview. */
  shortForm: boolean;
  /** The style's name; a built-in style renamed keeps its name and takes the new one as an alias. */
  name: string;
  /** Its type, chosen when creating. */
  type: 'paragraph' | 'character' | 'linked' | 'table' | 'list';
  /** The style it is based on. */
  basedOn: StyleId | null;
  /** The style of the paragraph after one in this style. */
  next: StyleId | null;
  /** The font. */
  font: string;
  /** The size. */
  size: Points;
  /** Bold. */
  bold: boolean;
  /** Italic. */
  italic: boolean;
  /** The underline. */
  underline: UnderlineStyle;
  /** The font color. */
  color: Color | 'auto';
  /** The alignment (paragraph and linked styles). */
  alignment: 'left' | 'center' | 'right' | 'justify';
  /** The line spacing. */
  lineSpacing: LineSpacing;
  /** The space before paragraphs. */
  spaceBefore: Points;
  /** The space after paragraphs. */
  spaceAfter: Points;
  /** The left indent. */
  leftIndent: Points;
  /** Shown in the Styles gallery. */
  quick: boolean;
  /** Direct formatting of a paragraph in the style changes the style. */
  autoUpdate: boolean;
  /** A table style: the part the formatting applies to. */
  tablePart:
    | 'wholeTable'
    | 'headerRow'
    | 'totalRow'
    | 'firstColumn'
    | 'lastColumn'
    | 'oddBandedRows'
    | 'evenBandedRows'
    | 'oddBandedColumns'
    | 'evenBandedColumns'
    | 'topLeftCell'
    | 'topRightCell'
    | 'bottomLeftCell'
    | 'bottomRightCell';
  /** A table style: the part's cell fill, or `null`. */
  fill: Color | null;
  /** A table style: the part's cells' vertical alignment. */
  cellAlignment: 'top' | 'center' | 'bottom';
  /** A table style: the part's border line, on every edge, or `null`. */
  border: BorderLine | null;
  /** A table style: the rows, and the columns, in each band. */
  rowBand: number;
  columnBand: number;
  /** A list style: its levels, and the level shown (0 to 8). */
  listLevels: ListLevel[];
  listLevel: number;
  /** The style's formatting in words, written by the editor. */
  description: string;
  /** The style as changed so far, the Format menu's dialogs included: give it back unchanged. */
  pending: string;
};

/** An open dialog as the engine sends it: its values are tagged with the kind. */
interface DialogModel<K extends DialogKind> {
  kind: K;
  values: DialogValues[K] & { kind: K };
  fields: DialogSession<K>['fields'];
  /** A Format menu dialog: the style dialog it changes. */
  onStyle?: StyleDialogValues;
}

/** The dialogs over a session. */
export function createDialogs(s: Session): Dialogs {
  const open = <K extends DialogKind>(model: DialogModel<K>): DialogSessionOf<K> => {
    const { kind, values, fields } = model;
    const dialog: DialogSession<K> = {
      kind,
      values,
      fields,
      update: async (patch) => open(await s.call<DialogModel<K>>(Method.DialogUpdate, { model, patch: { ...patch, kind } })),
      apply: () => s.commit(Method.DialogApply, model),
      cancel: () => {},
    };
    const select = (issueId: string) => s.call(Method.DialogSelectIssue, issueId);
    if (kind === 'style') {
      const { kind: _, ...style } = values as DialogValues['style'] & { kind: 'style' };
      const openFormat = async <F extends FormatDialogKind>(k: F) => format(await s.call<DialogModel<F>>(Method.DialogOpenOnStyle, { kind: k, style }));
      const preview = async (size: PreviewSize) => toPicture(await s.call(Method.DialogPreview, { model, size: { scale: 1, ...size } }));
      return Object.assign(dialog, { openFormat, preview }) as unknown as DialogSessionOf<K>;
    }
    return (kind === 'accessibility' ? Object.assign(dialog, { select }) : dialog) as DialogSessionOf<K>;
  };
  const format = <F extends FormatDialogKind>(model: DialogModel<F>): FormatDialogSession<F> => ({
    kind: model.kind,
    values: model.values,
    fields: model.fields,
    update: async (patch) => format(await s.call<DialogModel<F>>(Method.DialogUpdate, { model, patch: { ...patch, kind: model.kind } })),
    apply: async () => open(await s.call<DialogModel<'style'>>(Method.DialogApplyToStyle, model)) as StyleDialogSession,
    cancel: () => {},
  });
  return {
    open: async <K extends DialogKind>(kind: K) => open(await s.call<DialogModel<K>>(Method.DialogOpen, kind)),
    openStyle: async (target) => open(await s.call<DialogModel<'style'>>(Method.DialogOpenStyle, target)) as StyleDialogSession,
  };
}
