import type { CommandId } from './commands.js';
import type { FontsState } from './fonts/library.js';
import type { DialogKind } from './services/dialogs.js';
import type { ControlKind, ReferenceRule } from './services/elements.js';
import type { RulerModel, RulerUnit, TabSelector } from './services/ruler.js';
import type { ChromeTheme, CursorOverride, DocumentAppearance, FieldShading, MeasureDetail, ObjectGrid, ThemeColors } from './services/view.js';
import type { Color, CommentId, Degrees, ElementId, Points, RevisionId, ShapeState, StyleId } from './types.js';
import type {
  AutoCorrectEntry,
  AutoCorrectOptions,
  CharacterFormatting,
  EditingTarget,
  HighlightColor,
  LineSpacing,
  MarkupView,
  PageFormatting,
  ParagraphFormatting,
  ReviewState,
  SelectionSummary,
  TablePosition,
  UnderlineStyle,
  ViewState,
} from './wire.generated.js';

export type {
  AutoCorrectEntry,
  AutoCorrectOptions,
  CharacterFormatting,
  EditingTarget,
  ElementContext,
  HighlightColor,
  HorizontalPosition,
  HorizontalReference,
  LineSpacing,
  MarkupView,
  ObjectHorizontalAlignment,
  ObjectPlacement,
  ObjectSelection,
  ObjectVerticalAlignment,
  ObjectWrap,
  PageFormatting,
  ParagraphFormatting,
  ReviewState,
  SelectionSummary,
  TablePosition,
  TableSelection,
  TableStyleOptions,
  TableWidth,
  TextDistance,
  UnderlineStyle,
  VerticalPosition,
  VerticalReference,
  ViewState,
  WrapSide,
} from './wire.generated.js';

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

/** A row height. */
export type RowHeight = { height: Points; rule: 'atLeast' | 'exact' } | 'auto';

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
