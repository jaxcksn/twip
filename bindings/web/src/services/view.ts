import type { Op } from '../commands.js';
import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { ClientPoint, ClientRect, Color } from '../types.js';
import type { MenuTarget } from './menus.js';

/** The view of a mounted editor. Zoom and the other view settings are the view commands. */
export interface View {
  /** Scrolls the selection into view. */
  revealSelection(): Promise<void>;
  /**
   * The background around the pages (dark mode). The paper stays white. The page shadow, the markup pane and the
   * rulers' colours follow it: a dark background gives dark rulers.
   */
  setBackground(color: Color): Promise<void>;
  /** The pages on screen, for a page navigator or a thumbnails strip. */
  visiblePages(): { page: number; rect: ClientRect }[];
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
  | 'placeholderClick'
  | 'rulerClick'
  | 'rulerDrag'
  | 'rulerDragOff'
  | 'rulerDoubleClick'
  | 'contextClick';

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
  target: MenuTarget | { kind: 'headerArea' | 'footerArea' | 'margin' | 'ruler' | 'tableBorder' };
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
