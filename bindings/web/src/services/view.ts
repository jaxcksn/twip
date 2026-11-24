import type { Op, PasteAs } from '../commands.js';
import type { Session } from '../editor.js';
import type { PressAction } from '../events.js';
import type { ElementContext } from '../state.js';
import { type Cursor, Method } from '../transport/protocol.js';
import type { ClientPoint, ClientRect, Color, FrameStats, OverlayColors, Points } from '../types.js';
import type { MenuTarget } from './menus.js';
import type { TabAlignment, TabLeader, TabSelector } from './ruler.js';

/** The view of a mounted editor. Zoom and the other view settings are the view commands. */
export interface View {
  /** Scrolls the selection into view. */
  revealSelection(): Promise<void>;
  /**
   * The background around the pages, instead of the theme's desk colour. The page shadow and the markup pane follow
   * it. Until the application sets a theme, a dark background also gives the dark theme. `null` puts back the theme's.
   */
  setBackground(color: Color | null): Promise<void>;
  /**
   * The chrome's colour scheme (the desk, rulers, handles, guides and readouts), and its accent
   * when given. Under the platform's forced-colours mode the editor keeps the high-contrast scheme and applies this
   * once the mode ends. `setViewOptions` with `theme` and `accent`.
   */
  setTheme(theme: ChromeTheme, accent?: Color): Promise<void>;
  /**
   * The application's colours for the light and dark schemes, to match its own UI: each scheme given replaces the
   * last one set for it, and `{}` puts its own colours back. `setViewOptions` with `themeColors`.
   */
  setThemeColors(colors: ThemeColors): Promise<void>;
  /**
   * How the page is drawn on screen: in the document's colours, dark, or dark chrome around a page in its own
   * colours. Saving, export and printing always use the document's colours. Under forced colours the page stays light
   * until the mode ends. `setViewOptions` with `appearance`.
   */
  setAppearance(appearance: DocumentAppearance): Promise<void>;
  /** How much the editor measures while you drag and hover. `setViewOptions` with `detail`. */
  setDetail(detail: MeasureDetail): Promise<void>;
  /** The pages on screen, for a page navigator or a thumbnails strip. */
  visiblePages(): { page: number; rect: ClientRect }[];
  /** What the frames have cost so far: for benchmarks and tests. */
  frameStats(): Promise<FrameStats>;
  /**
   * How the caret blinks: `ms` per phase (0 for a steady caret), and stop blinking, the caret left visible, after
   * `stopAfterMs` without input. The editor stops following the platform's reduced-motion setting.
   */
  setCaretBlink(ms: number, stopAfterMs?: number): Promise<void>;
  /**
   * The colours of what the editor draws over the document; the ones not given keep theirs. The editor stops following
   * the platform's forced-colours mode. `null` puts back the theme's, which follow it again.
   */
  setOverlayColors(colors: Partial<OverlayColors> | null): Promise<void>;
}

/** The view options of `setViewOptions`; the ones not given keep theirs. */
export interface ViewOptions {
  /** Formatting marks (Ctrl+Shift+8). */
  formattingMarks?: boolean;
  /** Table gridlines. */
  gridlines?: boolean;
  /** Text boundaries. */
  textBoundaries?: boolean;
  /** Text wraps around an object again as it is dragged; otherwise a preview shows until the release. */
  liveLayout?: boolean;
  /** The chrome's colour scheme. */
  theme?: ChromeTheme;
  /** The application's colours for the light and dark schemes. */
  themeColors?: ThemeColors;
  /** The chrome's accent: selection, handles, guides and active markers. Any colour; the dark scheme lightens it. */
  accent?: Color;
  /** How the page is drawn on screen. */
  appearance?: DocumentAppearance;
  /** How much the editor measures while you drag and hover. */
  detail?: MeasureDetail;
  /** When the dot grid objects snap to shows (unrelated to `gridlines`, the table gridlines). */
  objectGrid?: ObjectGrid;
  /**
   * Labels content controls and shown sub-templates while the caret is in them or the pointer is over them. Default
   * `true`. A control whose `w15:appearance` is `hidden` is never labelled.
   */
  controlLabels?: boolean;
  /** When fields are shaded. Default `'whenSelected'`. */
  fieldShading?: FieldShading;
  /** Shows bookmarks as brackets around their text (an I-beam where they hold none); hidden ones never show. */
  bookmarks?: boolean;
  /** Pointer shapes over elements instead of the editor's own, replacing those set before; the first match wins. */
  cursorOverrides?: CursorOverride[];
  /** Shows tab characters as arrows, whatever `formattingMarks` says. */
  tabCharacters?: boolean;
  /** Shows spaces as dots and non-breaking spaces as degree signs. */
  spaces?: boolean;
  /** Shows paragraph marks, breaks, and end-of-cell and end-of-row marks. */
  paragraphMarks?: boolean;
  /** Shows hidden text, laid out and dotted underneath (the document lays out again; exports leave it out). */
  hiddenText?: boolean;
  /** Shows optional hyphens, laid out as not signs. */
  optionalHyphens?: boolean;
  /** Shows the selected floating object's anchor. Default: on. */
  objectAnchors?: boolean;
  /** Click and Type: a double-click in blank space puts the caret there, adding what it needs. Default: on. */
  clickAndType?: boolean;
  /** Selected text can be dragged and dropped, with either button and out of the view. Default: on. */
  textDragAndDrop?: boolean;
  /** Hides the white space between pages: each page shows only its body. A double-click between pages toggles it. */
  hideWhiteSpace?: boolean;
  /** Under enforced protection, highlights the regions the author may edit and brackets their ends. Default: on. */
  showEditableRegions?: boolean;
}

/** The pointer shape to show over some elements instead of the editor's own. */
export interface CursorOverride {
  /** The elements: of a kind, or those a press on does an action. */
  on: { kind: ElementContext['kind'] } | { action: PressAction };
  /** The shape. */
  cursor: Cursor;
}

export type { Cursor };

/**
 * The editor chrome's colour scheme: the desk, rulers, handles, guides and readouts. `highContrast` takes the
 * platform's colours; a mounted editor sets it itself under forced colours.
 */
export type ChromeTheme = 'light' | 'dark' | { highContrast: SystemColors };

/**
 * The application's colours for the light and dark chrome schemes, to match its own UI. The high-contrast scheme
 * always takes the platform's.
 */
export interface ThemeColors {
  /** For the light scheme. */
  light?: ChromeColors;
  /** For the dark scheme. */
  dark?: ChromeColors;
}

/** Chrome colours in place of a scheme's own; each one left out keeps the scheme's. */
export interface ChromeColors {
  /** Around the pages and the rulers' strips (`setBackground` wins around the pages). */
  desk?: Color;
  /** The rulers' strips, and the readouts' and labels' backgrounds. */
  surface?: Color;
  /** Text, and the ruler numbers and markers. */
  text?: Color;
  /** The rulers' unit ticks, and secondary text. */
  secondaryText?: Color;
  /** The rulers' fine ticks, the object grid, and faint text. */
  faintText?: Color;
  /** The rulers' outlines and the margins' hatching. */
  line?: Color;
}

/** The platform's colours in high contrast mode, as the CSS system colours name them. */
export interface SystemColors {
  /** The window background (`Canvas`). */
  canvas: Color;
  /** Text on it (`CanvasText`). */
  canvasText: Color;
  /** Selected items' background (`Highlight`). */
  highlight: Color;
  /** Text on it (`HighlightText`). */
  highlightText: Color;
  /** Disabled and secondary text (`GrayText`). */
  grayText: Color;
}

/**
 * How the page is drawn on screen: `light` in the document's colours, `dark` a dark page with the document's colours
 * adjusted to it, `darkWithLightPage` dark chrome around a page in its own colours. Saving, export and printing always
 * use the document's colours.
 */
export type DocumentAppearance = 'light' | 'dark' | 'darkWithLightPage';

/**
 * How much the editor measures while you drag and hover: `quiet` a chip with the value, `standard` dimension lines with
 * the value (everything while Alt is held), `full` everything, with the change from the start, table sizes and status
 * lines.
 */
export type MeasureDetail = 'quiet' | 'standard' | 'full';

/** When the dot grid objects snap to shows. */
export type ObjectGrid = 'off' | 'whileDragging' | 'always';

/**
 * When fields are shaded: a grey backdrop behind each field's text on screen, so a field's result can be told from
 * the text around it. It's never printed, exported or saved. `whenSelected` shades the field the caret or selection is
 * in or touches.
 */
export type FieldShading = 'never' | 'always' | 'whenSelected';

/**
 * The `popoverRequested` event: a popover to show by `anchor` (client px), and what answers it. One is open at a
 * time; `popoverClosed` says the engine dismissed it, and `closePopover` dismisses it from the application. The
 * application draws it: the editor draws none.
 */
export type Popover =
  | {
      /** The tab type picker: answered with `setRulerTabType`. */
      kind: 'tabType';
      /** The tab type box. */
      anchor: ClientRect;
      /** The type a click on the ruler adds now. */
      current: TabSelector;
      /** Every type, in order. */
      choices: TabTypeChoice[];
    }
  | {
      /** One tab stop's options: `addTabStop` at its position changes the alignment or leader, `deleteTabStop` clears it. */
      kind: 'tabStop';
      /** The stop's mark on the ruler. */
      anchor: ClientRect;
      /** Its position, from the paragraph's left text edge. */
      position: Points;
      /** Its alignment. */
      alignment: TabAlignment;
      /** Its leader. */
      leader: TabLeader;
    }
  | {
      /** The options of the correction just made. */
      kind: 'autoCorrect';
      /** The corrected text. */
      anchor: ClientRect;
      /** What was typed. */
      from: string;
      /** What it became. */
      to: string;
    }
  | {
      /**
       * The options of the AutoFormat change just made: `revertAutoCorrect` undoes it (Undo Hyperlink, Undo Border Line,
       * Undo Automatic Indent), `neverAutoCorrect` undoes it and turns its option off (Stop Automatically Creating
       * Hyperlinks or Border Lines, Stop Setting Indent on Tabs and Backspaces); your AutoCorrect dialog is Control
       * AutoFormat Options.
       */
      kind: 'autoFormat';
      /** The change's options button, or what it made. */
      anchor: ClientRect;
      /** What it made. */
      change: 'hyperlink' | 'borderLine' | 'indent';
      /** What was typed: the address, or the characters a border line replaced; empty for an indent. */
      text: string;
    }
  | {
      /**
       * The Paste Options button of the paste just made, shown while nothing has changed since (typing, another edit,
       * undo or Esc closes it): `commands.repaste` pastes it again another way, `commands.dismissPasteOptions` hides
       * it. A lone Ctrl press opens its menu in Word.
       */
      kind: 'pasteOptions';
      /** The button: below the end of the paste, at its right. */
      anchor: ClientRect;
      /** The end of the paste's caret box. */
      end: ClientRect;
      /** The ways it can paste again, in Word's order; `current` among them. */
      choices: PasteAs[];
      /** The way it was pasted. */
      current: PasteAs;
    };

/** One entry of the tab type picker. */
export interface TabTypeChoice {
  /** The type. */
  value: TabSelector;
  /** Its name (`Decimal`). */
  name: string;
  /** What it does, in one line (`Numbers line up on the decimal point`). */
  description: string;
}

/** What the engine's pointer gestures do, on a mounted editor. */
export interface Gestures {
  /** Every gesture's binding. */
  bindings(): GestureBinding[];
  /** Rebinds a gesture. */
  set(gesture: GestureId, action: GestureAction): Promise<void>;
  /** Back to the default (all of them when no id is given). */
  reset(gesture?: GestureId): Promise<void>;
}

/** A pointer gesture the engine recognises. */
export type GestureId =
  | 'headerAreaDoubleClick'
  | 'footerAreaDoubleClick'
  | 'bodyDoubleClickInHeaderFooter'
  | 'textDoubleClick'
  | 'textTripleClick'
  | 'textCtrlClick'
  | 'linkActivate'
  | 'marginClick'
  | 'marginDoubleClick'
  | 'tableBorderDoubleClick'
  | 'tableBorderDrag'
  | 'objectDrag'
  | 'textDrag'
  | 'elementDoubleClick'
  | 'pictureControlClick'
  | 'pictureControlDoubleClick'
  | 'rulerClick'
  | 'rulerDrag'
  | 'rulerDragOff'
  | 'rulerDoubleClick'
  | 'contextClick'
  | 'objectDoubleClick'
  | 'objectResizeDrag'
  | 'shapeHandleDrag'
  | 'tableMoveDrag'
  | 'tableZoneClick'
  | 'tableInsertClick'
  | 'noteDoubleClick'
  | 'clickAndType'
  | 'pageGapDoubleClick'
  /** A click on an enabled drop-down form field where forms are filled in: the host opens its list. */
  | 'formFieldClick'
  /** A double-click on a legacy form field outside forms protection: the host opens its options. */
  | 'formFieldDoubleClick';

/** What a gesture does: its default, an event, nothing, or an op. */
export type GestureAction = 'default' | 'emit' | 'none' | { op: Op };

/** A gesture's binding. */
export interface GestureBinding {
  /** The gesture. */
  gesture: GestureId;
  /** What it does. */
  action: GestureAction;
  /** For example `'Double-click the header area'`. */
  label: string;
}

/** The `gesture` event: a gesture bound to `'emit'` happened. */
export interface GestureEvent {
  /** The gesture. */
  gesture: GestureId;
  /** Where. */
  at: ClientPoint;
  /** The page. */
  page: number;
  /** What is under the point. */
  target: MenuTarget | { kind: 'headerArea' | 'footerArea' | 'margin' | 'ruler' | 'tableBorder' | 'pageGap' };
}

/** The gestures over a session; `bindings` starts as `initial` and is read again after each change. */
export function createGestures(s: Session, initial: GestureBinding[]): Gestures {
  let bindings = initial;
  const refresh = async () => {
    bindings = await s.call<GestureBinding[]>(Method.GesturesBindings);
  };
  return {
    bindings: () => bindings,
    set: async (gesture, action) => {
      await s.call(Method.GesturesSet, { gesture, action });
      await refresh();
    },
    reset: async (gesture) => {
      await s.call(Method.GesturesReset, gesture ?? null);
      await refresh();
    },
  };
}
