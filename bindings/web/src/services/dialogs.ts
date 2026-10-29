import type { Session } from '../editor.js';
import type { LineSpacing, Mixed, RowHeight, TableWidth, Tri, UnderlineStyle } from '../state.js';
import { Method } from '../transport/protocol.js';
import type { Color, Points } from '../types.js';
import type { TabAlignment, TabLeader } from './ruler.js';

/** The editor's dialogs as typed models: the application draws them. */
export interface Dialogs {
  /** Opens a dialog over the selection's current values. */
  open<K extends DialogKind>(kind: K): Promise<DialogSessionOf<K>>;
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
  | 'accessibility';

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
}

/** An open dialog as the engine sends it: its values are tagged with the kind. */
interface DialogModel<K extends DialogKind> {
  kind: K;
  values: DialogValues[K] & { kind: K };
  fields: DialogSession<K>['fields'];
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
    return (kind === 'accessibility' ? Object.assign(dialog, { select }) : dialog) as DialogSessionOf<K>;
  };
  return { open: async <K extends DialogKind>(kind: K) => open(await s.call<DialogModel<K>>(Method.DialogOpen, kind)) };
}
