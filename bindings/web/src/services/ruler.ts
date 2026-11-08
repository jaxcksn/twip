import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';
import type { Points } from '../types.js';

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

/** A ruler unit. */
export type RulerUnit = 'inches' | 'centimeters' | 'millimeters' | 'points' | 'picas';
/** What a click on the ruler adds. */
export type TabSelector = TabAlignment | 'firstLineIndent' | 'hangingIndent';
/** A tab stop's alignment. */
export type TabAlignment = 'left' | 'center' | 'right' | 'decimal' | 'bar';
/** A tab stop's leader. */
export type TabLeader = 'none' | 'dots' | 'dashes' | 'underline' | 'heavy' | 'middleDot';

/** The ruler model for the selection, in points. */
export interface RulerModel {
  /** The selected paragraphs differ: the first one's values are shown. */
  mixed: boolean;
  /** The horizontal ruler. */
  horizontal: {
    pageWidth: Points;
    /** The margins, or the cell or text column around the caret. */
    text: { start: Points; end: Points };
    /** `firstLine` is relative to `left`; negative is hanging. */
    indents: { left: Points; right: Points; firstLine: Points };
    tabs: { position: Points; alignment: TabAlignment; leader: TabLeader; inherited: boolean }[];
    defaultTabStop: Points;
    /** The section's text columns. */
    columns: { start: Points; end: Points }[];
    /** The cell boundaries of the caret's row. */
    tableColumns: Points[];
  };
  /** The vertical ruler. */
  vertical: {
    pageHeight: Points;
    /** The top and bottom margins. */
    text: { start: Points; end: Points };
    /** The row boundaries of the caret's table on this page. */
    tableRows: Points[];
  };
}

/** The ruler model mapped to the viewport. */
export interface RulerView {
  /** The model. */
  model: RulerModel;
  /** The page the rulers follow (the caret's). */
  page: number;
  /** The client x of the page's left edge, and CSS px per point. */
  horizontal: { origin: number; scale: number };
  /** The client y of the page's top edge, and CSS px per point. */
  vertical: { origin: number; scale: number };
  /** The unit shown. */
  unit: RulerUnit;
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
