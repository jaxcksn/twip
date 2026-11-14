import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { Points } from '../types.js';
import type { RulerModel, RulerUnit, RulerView, TabSelector } from '../wire.generated.js';

export type { RulerModel, RulerUnit, RulerView, TabAlignment, TabLeader, TabSelector } from '../wire.generated.js';

/** The ruler model, on every editor. */
export interface RulerModelSource {
  /** The ruler model for the selection, in points: the same value as `getState().ruler`. */
  model(): RulerModel;
}

/** The rulers of a mounted editor: the built-in rulers, and the model mapped to the viewport. */
export interface Ruler extends RulerModelSource {
  /** Shows or hides the built-in rulers. */
  setVisible(rulers: { horizontal?: boolean; vertical?: boolean }): Promise<void>;
  /** The unit the rulers show. */
  setUnit(unit: RulerUnit): Promise<void>;
  /** What a click on the ruler adds; the corner box cycles it. */
  setTabSelector(kind: TabSelector): Promise<void>;
  /** The model with its mapping to the viewport, for an application drawing its own ruler. */
  view(): RulerView;
}

/** The mounted rulers over a session; `view` starts as `initial` and follows `rulerChanged`. */
export function createRuler(s: Session, initial: RulerView): Ruler {
  let view = initial;
  s.on('rulerChanged', (v) => (view = v));
  return {
    model: () => s.state.ruler,
    setVisible: ({ horizontal, vertical }) => {
      const now = s.state.view.rulers;
      return s.call(Method.HostSetRulers, { horizontal: horizontal ?? now.horizontal, vertical: vertical ?? now.vertical });
    },
    setUnit: (unit) => s.call(Method.HostSetRulerUnit, unit),
    setTabSelector: (kind) => s.call(Method.HostSetTabSelector, kind),
    view: () => view,
  };
}
