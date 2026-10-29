/**
 * The Twip DOCX editor for the browser and Node: a managed, framework-independent editor. Mount it in a sized
 * container with {@link Editor.mount}, or create a session with no DOM with {@link HeadlessEditor.create}.
 *
 * @packageDocumentation
 */

export { Editor, HeadlessEditor } from './editor.js';
export type { ExportResult, MountOptions, SessionOptions } from './editor.js';
export type {
  CommandEntry,
  CommandId,
  CommandInfo,
  Commands,
  FieldTargets,
  NoArgumentCommandId,
  Op,
  OpArgs,
  PlaceholderLook,
} from './commands.js';
export type {
  CharacterFormatting,
  CommandState,
  CommandValues,
  EditorState,
  ElementContext,
  HighlightColor,
  LineSpacing,
  MarkupView,
  Mixed,
  ObjectSelection,
  ObjectWrap,
  PageFormatting,
  ParagraphFormatting,
  ReviewState,
  RowHeight,
  SelectionSummary,
  TableSelection,
  TableStyleOptions,
  TableWidth,
  Tri,
  UnderlineStyle,
  ViewState,
} from './state.js';
export type { HostCommand, Tooltip, TransactionRecord, TwipEvents } from './events.js';
export { EditorError } from './errors.js';
export type { ErrorCode } from './errors.js';
export { FontLibrary } from './fonts/library.js';
export type { FaceInfo, FontRequest, SubstitutionRule } from './fonts/library.js';
export type {
  BalloonPlacement,
  Comment,
  CommentAnchor,
  CommentThread,
  MountedReview,
  Person,
  Review,
  RevisionInfo,
  RichRun,
  RichText,
} from './review.js';
export type { ContextMenu, ContextMenuEvent, MenuAction, MenuItem, MenuTarget, Menus } from './services/menus.js';
export type {
  AccessibilityDialogSession,
  DialogKind,
  DialogSession,
  DialogSessionOf,
  DialogValues,
  Dialogs,
  FieldInfo,
} from './services/dialogs.js';
export type { Find, FindMatch, FindQuery, FindResult } from './services/find.js';
export type { Dictionary, Spelling } from './services/spelling.js';
export type {
  Block,
  ControlKind,
  ControlOptions,
  DataBinding,
  DataParts,
  ElementDetail,
  ElementInfo,
  Elements,
  MountedElements,
  PlaceholderInfo,
} from './services/elements.js';
export type { Accessibility, AccessibleNode, Shortcuts } from './services/accessibility.js';
export type { Heading, Navigation, StyleInfo, Styles } from './services/navigation.js';
export type {
  Ruler,
  RulerModel,
  RulerModelSource,
  RulerUnit,
  RulerView,
  TabAlignment,
  TabLeader,
  TabSelector,
} from './services/ruler.js';
export type { GestureAction, GestureBinding, GestureEvent, GestureId, Gestures, View } from './services/view.js';
export { cm, inches, mm, pt } from './types.js';
export type {
  Author,
  ClientPoint,
  ClientRect,
  Color,
  CommentId,
  ContentPoint,
  ElementId,
  FrameStats,
  HeadingId,
  OverlayColors,
  MatchId,
  Points,
  RevisionId,
  StyleId,
  Warning,
  WordCount,
} from './types.js';
