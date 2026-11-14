import type { Op } from '../commands.js';
import type { Session } from '../editor.js';
import type { PressAction } from '../events.js';
import type { ElementContext } from '../state.js';
import { type Cursor, Method } from '../transport/protocol.js';
import type { ClientPoint, ClientRect, Color, FrameStats, OverlayColors, Points } from '../types.js';
import type { MenuTarget } from './menus.js';
import type { TabAlignment, TabLeader, TabSelector } from './ruler.js';
import type {
  ChromeTheme,
  DocumentAppearance,
  GestureAction,
  GestureBinding,
  GestureId,
  MeasureDetail,
  ThemeColors,
} from '../wire.generated.js';

export type {
  ChromeColors,
  ChromeTheme,
  CursorOverride,
  DocumentAppearance,
  FieldShading,
  GestureAction,
  GestureBinding,
  GestureEvent,
  GestureId,
  MeasureDetail,
  ObjectGrid,
  Popover,
  SystemColors,
  TabTypeChoice,
  ThemeColors,
  ViewOptions,
} from '../wire.generated.js';

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

export type { Cursor };

/** What the engine's pointer gestures do, on a mounted editor. */
export interface Gestures {
  /** Every gesture's binding. */
  bindings(): GestureBinding[];
  /** Rebinds a gesture. */
  set(gesture: GestureId, action: GestureAction): Promise<void>;
  /** Back to the default (all of them when no id is given). */
  reset(gesture?: GestureId): Promise<void>;
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
