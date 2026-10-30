// Pointer input: raw pointer events at viewport points go to the engine (`twip_pointer`), which recognises every
// gesture, internal drags of text and objects included. Here: pointer capture, click counts, the cursor from the
// `cursor` request, autoscroll while dragging past the viewport, two-finger pinch and pan on touch, and the browser's
// context menu held back while the application listens for `contextMenu`.
import type { Session } from '../editor.js';
import { type Cursor, Kind, type HostRequest, type Message } from '../transport/protocol.js';
import { CELL_SELECT, COLUMN_SELECT, ROW_SELECT } from './cursors.js';
import type { Dom, Ledger } from './dom.js';
import { modsOf } from './keys.js';
import type { Scroll } from './scroll.js';

const PointerKind = { down: 0, move: 1, up: 2, cancel: 3, leave: 4 } as const;

/** DOM `PointerEvent.button` to the engine's: primary 0, secondary 1, middle 2. */
const buttons: Readonly<Record<number, number>> = { 0: 0, 2: 1, 1: 2 };

/** A `.cur` cursor, over the browser's own `fallback` where it can't show one. */
const cur = (c: { url: string; x: number; y: number }, fallback: string): string => `url("${c.url}") ${c.x} ${c.y}, ${fallback}`;

/** The browser's cursors, and the selection arrows it has none of (`cursors.ts`, the WinForms host's `.cur` files). */
const cursors: Readonly<Record<Cursor, string>> = {
  default: 'default',
  text: 'text',
  pointer: 'pointer',
  move: 'move',
  resizeHorizontal: 'col-resize',
  resizeVertical: 'row-resize',
  resizeNorthWestSouthEast: 'nwse-resize',
  resizeNorthEastSouthWest: 'nesw-resize',
  crosshair: 'crosshair',
  notAllowed: 'not-allowed',
  copy: 'copy',
  cellSelect: cur(CELL_SELECT, 'cell'),
  columnSelect: cur(COLUMN_SELECT, 'default'),
  rowSelect: cur(ROW_SELECT, 'default'),
};

/** Presses closer than this in time (ms) and distance (CSS px) count as one multi-click. */
const CLICK_TIME = 500;
const CLICK_SLOP = 4;

export function startPointer(
  s: Session,
  dom: Dom,
  ledger: Ledger,
  scroll: Scroll,
  report: (e: unknown) => void,
  focus: () => void,
  ignore: (target: EventTarget | null) => boolean,
): void {
  const { scroller } = dom;
  const point = (e: { clientX: number; clientY: number }) => {
    const r = dom.canvas.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const send = (kind: number, e: PointerEvent | { clientX: number; clientY: number }, button: number, mods: number) =>
    s.input({ type: 'pointer', kind, ...point(e), button, clicks, mods }).catch(report);

  let clicks = 0;
  let last = { time: -Infinity, x: 0, y: 0, button: -1 };
  /** The pointer of the press being dragged. */
  let pressed: { id: number; at: PointerEvent } | undefined;
  /** Where a mouse or pen last moved over the document, until it leaves. */
  let over: { clientX: number; clientY: number } | undefined;
  /** Touches down, for pinch and pan. */
  const touches = new Map<number, { x: number; y: number }>();
  let pinch: { distance: number; x: number; y: number } | undefined;
  let stopAutoscroll: (() => void) | undefined;

  const twoTouches = () => {
    const [a, b] = [...touches.values()] as [{ x: number; y: number }, { x: number; y: number }];
    return { distance: Math.hypot(a.x - b.x, a.y - b.y), x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
  };

  ledger.listen(scroller, 'pointerdown', (e: PointerEvent) => {
    if (ignore(e.target)) return;
    const at = point(e);
    // A press on a scrollbar scrolls.
    if (at.x >= scroller.clientWidth || at.y >= scroller.clientHeight) return;
    const button = buttons[e.button];
    if (button === undefined) return;
    if (e.pointerType === 'touch') {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (touches.size === 2) {
        // A second finger: the first one's press is withdrawn, and the two pinch and pan.
        if (pressed) send(PointerKind.cancel, pressed.at, 0, 0);
        pressed = undefined;
        pinch = twoTouches();
        return;
      }
      if (touches.size > 2) return;
    }
    // A press takes the focus (the browser would give it to no element: the canvas takes none).
    if (button !== 2) {
      e.preventDefault();
      focus();
    }
    const near = Math.hypot(e.clientX - last.x, e.clientY - last.y) <= CLICK_SLOP;
    clicks = button === last.button && near && e.timeStamp - last.time <= CLICK_TIME ? clicks + 1 : 1;
    last = { time: e.timeStamp, x: e.clientX, y: e.clientY, button };
    if (button === 0) {
      pressed = { id: e.pointerId, at: e };
      scroller.setPointerCapture(e.pointerId);
    }
    send(PointerKind.down, e, button, modsOf(e));
  });

  ledger.listen(scroller, 'pointermove', (e: PointerEvent) => {
    if (touches.has(e.pointerId)) {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (pinch && touches.size === 2) {
        const now = twoTouches();
        if (pinch.distance > 0 && Math.abs(now.distance / pinch.distance - 1) > 0.02) {
          scroll.zoomBy(now.distance / pinch.distance);
          pinch.distance = now.distance;
        }
        scroller.scrollBy(pinch.x - now.x, pinch.y - now.y);
        pinch = { ...pinch, x: now.x, y: now.y };
        return;
      }
      if (pinch) return;
    }
    if (pressed && pressed.id !== e.pointerId) return;
    if (!pressed && ignore(e.target)) return;
    if (pressed) {
      pressed.at = e;
      autoscroll(e);
    }
    send(PointerKind.move, e, 0, modsOf(e));
  });

  const end = (kind: number) => (e: PointerEvent) => {
    touches.delete(e.pointerId);
    if (pinch) {
      if (touches.size === 0) pinch = undefined;
      return;
    }
    if (pressed?.id === e.pointerId) {
      pressed = undefined;
      stopAutoscroll?.();
      if (scroller.hasPointerCapture(e.pointerId)) scroller.releasePointerCapture(e.pointerId);
    } else if (ignore(e.target)) return;
    const button = buttons[e.button] ?? 0;
    send(kind, e, button, modsOf(e));
  };
  ledger.listen(scroller, 'pointerup', end(PointerKind.up));
  ledger.listen(scroller, 'pointercancel', end(PointerKind.cancel));
  ledger.listen(scroller, 'pointerleave', (e: PointerEvent) => {
    if (!pressed && e.pointerType !== 'touch') send(PointerKind.leave, e, 0, modsOf(e));
    over = undefined;
  });

  // Ctrl, Shift or Meta pressed or released with the pointer still: the same move again with the new modifiers, so
  // the shape follows what a press would now do (the hand over a link with Ctrl, copy while dragging).
  ledger.listen(scroller, 'pointermove', (e: PointerEvent) => {
    if (e.pointerType !== 'touch') over = { clientX: e.clientX, clientY: e.clientY };
  });
  const modifier = (event: Event) => {
    const e = event as KeyboardEvent;
    if (!over || e.repeat || !['Control', 'Shift', 'Meta'].includes(e.key)) return;
    send(PointerKind.move, pressed?.at ?? over, 0, modsOf(e));
  };
  const doc = scroller.ownerDocument;
  ledger.listen(doc, 'keydown', modifier, { capture: true });
  ledger.listen(doc, 'keyup', modifier, { capture: true });

  // Autoscroll: a drag past the viewport's edge scrolls each frame, faster further out, and extends the drag.
  const autoscroll = (e: PointerEvent) => {
    const step = () => {
      if (!pressed) return;
      const at = point(pressed.at);
      const out = (v: number, size: number) => (v < 0 ? v : v > size ? v - size : 0);
      const dx = out(at.x, scroller.clientWidth);
      const dy = out(at.y, scroller.clientHeight);
      if (!dx && !dy) {
        stopAutoscroll = undefined;
        return;
      }
      const speed = (d: number) => Math.sign(d) * Math.min(40, Math.ceil(Math.abs(d) / 4));
      scroller.scrollBy(speed(dx), speed(dy));
      send(PointerKind.move, pressed.at, 0, modsOf(pressed.at));
      stopAutoscroll = ledger.frame(step);
    };
    const at = point(e);
    const inside = at.x >= 0 && at.y >= 0 && at.x <= scroller.clientWidth && at.y <= scroller.clientHeight;
    if (!inside && !stopAutoscroll) stopAutoscroll = ledger.frame(step);
  };

  // The engine's requests: the cursor, and capture for its own drags.
  const onMessage = (m: Message) => {
    if (m.kind !== Kind.HostRequest) return;
    const r = m.payload as HostRequest;
    if (r.type === 'cursor') scroller.style.cursor = cursors[r.cursor] ?? 'default';
    else if (r.type === 'capture' && pressed && !scroller.hasPointerCapture(pressed.id)) scroller.setPointerCapture(pressed.id);
    else if (r.type === 'release' && pressed && scroller.hasPointerCapture(pressed.id)) scroller.releasePointerCapture(pressed.id);
  };
  s.hostListeners.add(onMessage);
  s.disposers.push(() => s.hostListeners.delete(onMessage));

  // The engine opens its menu on a secondary press (and the menu keys): the browser's stays shut while the
  // application listens for it.
  ledger.listen(scroller, 'contextmenu', (e: MouseEvent) => {
    if (!ignore(e.target) && s.listening('contextMenu')) e.preventDefault();
  });
}
