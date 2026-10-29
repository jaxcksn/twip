// Frames: the engine asks for one (`request_frame`), the next animation frame draws it into the canvas through the
// best backend the transport could attach (WebGPU, then WebGL 2, then CPU pixels), at devicePixelRatio. The
// viewport's size, the pixel density and visibility are watched here; a hidden or zero-sized editor draws nothing
// until it has a size again. The engine's `set_timer` timers are armed by the transport.
import type { Session } from '../editor.js';
import { type FrameInfo, Kind, type Message } from '../transport/protocol.js';
import type { Dom, Ledger } from './dom.js';

/** The frame loop of a mounted editor. */
export interface Frames {
  /** The last frame's content size. */
  readonly info: FrameInfo;
  /** Draws a frame at the next animation frame. */
  request(): void;
  /** Resolves once a frame was drawn after this call; at once while the editor has no size. */
  drawn(): Promise<void>;
}

/** Starts the frame loop; `onFrame` runs after each frame drawn. */
export async function startFrames(s: Session, dom: Dom, ledger: Ledger, report: (e: unknown) => void, onFrame: (info: FrameInfo) => void): Promise<Frames> {
  await s.guard(() => s.transport.attachCanvas(s.id, dom.canvas));
  let info: FrameInfo = { contentWidth: 0, contentHeight: 0, pages: 0 };
  let surface = { width: 0, height: 0, dpr: 0 };
  let cancel: (() => void) | undefined;
  let busy = false;
  let again = false;
  const waiting: (() => void)[] = [];
  const settle = () => {
    for (const resolve of waiting.splice(0)) resolve();
  };

  const draw = () => {
    cancel = undefined;
    if (busy) {
      again = true;
      return;
    }
    const { width, height, dpr } = surface;
    if (!width || !height) return settle();
    busy = true;
    s.frame(Math.round(width * dpr), Math.round(height * dpr))
      .then((drawn) => {
        if (drawn) {
          info = drawn;
          onFrame(drawn);
        }
      }, report)
      .finally(() => {
        busy = false;
        settle();
        if (again) {
          again = false;
          request();
        }
      });
  };
  const request = () => {
    if (!cancel && !s.destroyed) cancel = ledger.frame(draw);
  };

  // The viewport's size and density go to the engine as the surface; the canvas covers the viewport.
  const resize = () => {
    const next = { width: dom.scroller.clientWidth, height: dom.scroller.clientHeight, dpr: globalThis.devicePixelRatio || 1 };
    if (next.width !== surface.width || next.height !== surface.height || next.dpr !== surface.dpr) {
      surface = next;
      dom.canvas.style.width = `${next.width}px`;
      dom.canvas.style.height = `${next.height}px`;
      s.input({ type: 'surface', ...next }).catch(report);
    }
    request();
  };
  // A density change (browser zoom, another monitor) is a `resolution` media query that stops matching.
  const watchDensity = () => {
    const query = matchMedia(`(resolution: ${globalThis.devicePixelRatio || 1}dppx)`);
    ledger.listen(query, 'change', () => {
      resize();
      watchDensity();
    }, { once: true });
  };
  watchDensity();
  ledger.observe(new ResizeObserver(resize)).observe(dom.scroller);
  // Shown again (display, visibility, scrolled into the page): draw.
  ledger.observe(new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && resize())).observe(dom.scroller);
  const onMessage = (message: Message) => {
    if (message.kind === Kind.RequestFrame) request();
  };
  s.hostListeners.add(onMessage);
  s.disposers.push(() => s.hostListeners.delete(onMessage));
  resize();

  return {
    get info() {
      return info;
    },
    request,
    drawn: () => {
      if (!surface.width || !surface.height || s.destroyed) return Promise.resolve();
      request();
      return new Promise((resolve) => waiting.push(resolve));
    },
  };
}
