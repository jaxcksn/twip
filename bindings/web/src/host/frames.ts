// Frames: the engine asks for one (`request_frame`), the next animation frame draws it into the canvas through the
// best backend the transport could attach (WebGPU, else CPU pixels), at devicePixelRatio. The
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
  /** Resolves once a frame started after this call was drawn; at once while the editor has no size or the page is hidden. */
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
  /** Frames started so far. */
  let started = 0;
  /** What waits for a frame started after `after` frames (one in flight when it asked may show what came before). */
  let waiting: { after: number; resolve: () => void }[] = [];
  const settle = (all = false) => {
    const done = waiting.filter((w) => all || w.after < started);
    waiting = waiting.filter((w) => !done.includes(w));
    for (const w of done) w.resolve();
  };

  const draw = () => {
    cancel = undefined;
    if (busy) {
      again = true;
      return;
    }
    const { width, height, dpr } = surface;
    if (!width || !height) return settle(true);
    busy = true;
    started++;
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
  // A hidden page draws no animation frames: what waits for one doesn't wait for the page to show.
  const page = dom.scroller.ownerDocument;
  ledger.listen(page, 'visibilitychange', () => (page.hidden ? settle(true) : request()));
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
      if (!surface.width || !surface.height || page.hidden || s.destroyed) return Promise.resolve();
      request();
      return new Promise((resolve) => waiting.push({ after: started, resolve }));
    },
  };
}
