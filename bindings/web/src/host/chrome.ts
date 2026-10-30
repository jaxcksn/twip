// What an application builds its chrome on: the balloon layer and attached balloons, the markup pane, the
// background, the view (reveal, visible pages), and hit testing and external drops at client points.
import type { Op } from '../commands.js';
import type { Session } from '../editor.js';
import { stackBalloons, type BalloonPlacement, type CommentAnchor } from '../review.js';
import type { View } from '../services/view.js';
import { type FrameInfo, Method } from '../transport/protocol.js';
import type { ClientPoint, ClientRect, Color, CommentId, ElementId, FrameStats, OverlayColors } from '../types.js';
import type { Dom, Ledger } from './dom.js';
import type { Frames } from './frames.js';

/** The space between a page and its balloons, and between balloons, CSS px. */
const GAP = 8;

export interface Chrome {
  view: View;
  balloonLayer: HTMLElement;
  attachBalloon(thread: CommentId, card: HTMLElement): () => void;
  setMarkupPaneWidth(width: number): Promise<void>;
  hitTest(at: ClientPoint): Promise<{ element: ElementId | null; editable: boolean }>;
  dragOver(at: ClientPoint): void;
  dragLeave(): void;
  drop(at: ClientPoint, op: Op): Promise<void>;
  /** After each frame: the visible pages, and the balloons for the anchors it reported. */
  framed(info: FrameInfo): void;
  /** The engine draws the balloons' leader lines (the `drawLeaders` mount option). */
  drawLeaders(on: boolean): void;
}

export function startChrome(
  s: Session,
  dom: Dom,
  ledger: Ledger,
  frames: Frames,
  report: (e: unknown) => void,
  toViewport: (at: ClientPoint) => { x: number; y: number },
): Chrome {
  const layer = ledger.node(dom.content, document.createElement('div'));
  layer.dataset.twip = 'balloons';
  layer.style.cssText = 'position:absolute;inset:0;pointer-events:none;';

  // Balloons: each attached card beside its thread's anchor, stacked, and placed again after every anchors change.
  const cards = new Map<CommentId, HTMLElement>();
  let anchors: CommentAnchor[] = [];
  let pane = 0;
  let leaders = false;
  const place = () => {
    const shown = anchors.filter((a) => cards.has(a.thread));
    for (const [thread, card] of cards) card.style.display = shown.some((a) => a.thread === thread) ? '' : 'none';
    if (!shown.length && !cards.size) return;
    const heights = shown.map((a) => cards.get(a.thread)!.offsetHeight);
    const tops = stackBalloons(shown, heights, GAP);
    const placements: BalloonPlacement[] = shown.map((a, i) => {
      const left = pane > 0 ? frames.info.contentWidth - pane + GAP : a.pageEdge + GAP;
      const top = tops[i]!;
      const card = cards.get(a.thread)!;
      card.style.left = `${left}px`;
      card.style.top = `${top}px`;
      return { thread: a.thread, top, left, leader: [a.start, { x: a.pageEdge, y: a.start.y }, { x: left, y: top + Math.min(heights[i]!, a.lineBottom - a.lineTop) / 2 }] };
    });
    s.emit('balloonsLaidOut', placements);
    if (leaders) s.call(Method.HostSetLeaders, placements.map((p) => p.leader)).then(() => frames.request(), report);
  };
  s.disposers.push(
    s.on('commentAnchors', (a) => {
      anchors = a;
      place();
    }),
  );

  // The pages on screen, viewport px from the engine after each frame; client px when read.
  let pages: { page: number; rect: ClientRect }[] = [];
  const framed = () => {
    if (s.destroyed) return;
    s.call<typeof pages>(Method.HostVisiblePages).then((p) => (pages = p), report);
  };

  // The caret's blink follows the platform's reduced-motion setting until the application sets it.
  const blink = async (ms: number, stopAfterMs?: number) => {
    await s.call(Method.HostSetCaretBlink, { intervalMs: ms > 0 ? ms : null, stopAfterMs: stopAfterMs ?? null });
  };
  let motion: MediaQueryList | undefined = globalThis.matchMedia?.('(prefers-reduced-motion: reduce)');
  if (motion) {
    const follow = () => {
      if (motion) blink(motion.matches ? 0 : 530).catch(report);
    };
    ledger.listen(motion, 'change', follow);
    if (motion.matches) follow();
  }

  // The overlay colours: the engine's, or the system's in forced-colours mode until the application sets them.
  const defaults: OverlayColors = { caret: '#000000', selection: '#B3D7FF', object: '#606060', drop: '#404040', marks: '#2F6FB5', leaders: '#767676' };
  let overlay = defaults;
  const system = (css: string): Color => {
    const probe = Object.assign(document.createElement('span'), { hidden: true });
    probe.style.color = css;
    document.body.append(probe);
    const [r, g, b] = (getComputedStyle(probe).color.match(/\d+/g) ?? ['0', '0', '0']).map(Number);
    probe.remove();
    return `#${[r, g, b].map((v) => (v ?? 0).toString(16).padStart(2, '0')).join('').toUpperCase()}`;
  };
  const paint = (colors: OverlayColors) => s.call(Method.HostSetOverlayPalette, colors).then(() => frames.request());
  let forced: MediaQueryList | undefined = globalThis.matchMedia?.('(forced-colors: active)');
  if (forced) {
    const follow = () => {
      if (!forced) return;
      const text = system('CanvasText');
      overlay = forced.matches ? { caret: text, selection: system('Highlight'), object: text, drop: text, marks: system('GrayText'), leaders: system('GrayText') } : defaults;
      paint(overlay).catch(report);
    };
    ledger.listen(forced, 'change', follow);
    if (forced.matches) follow();
  }

  const set = async (method: Method, value: unknown) => {
    await s.call(method, value);
    frames.request();
  };
  const view: View = {
    revealSelection: () => s.call(Method.HostRevealSelection),
    setBackground: async (color: Color) => {
      await set(Method.HostSetBackground, color);
      dom.scroller.style.background = color;
    },
    visiblePages: () => {
      const r = dom.canvas.getBoundingClientRect();
      return pages.map(({ page, rect }) => ({ page, rect: { ...rect, left: rect.left + r.left, top: rect.top + r.top } }));
    },
    frameStats: () => s.call<FrameStats>(Method.HostFrameStats),
    setOverlayColors: (colors) => {
      forced = undefined;
      overlay = { ...overlay, ...colors };
      return paint(overlay);
    },
    setCaretBlink: (ms, stopAfterMs) => {
      motion = undefined;
      return blink(ms, stopAfterMs);
    },
  };

  return {
    view,
    balloonLayer: layer,
    drawLeaders: (on) => {
      leaders = on;
      if (on) place();
      else s.call(Method.HostSetLeaders, []).then(() => frames.request(), report);
    },
    attachBalloon: (thread, card) => {
      cards.get(thread)?.remove();
      card.style.position = 'absolute';
      card.style.pointerEvents = 'auto';
      cards.set(thread, card);
      layer.append(card);
      place();
      return () => {
        if (cards.get(thread) !== card) return;
        cards.delete(thread);
        card.remove();
        place();
      };
    },
    setMarkupPaneWidth: async (width) => {
      await set(Method.HostSetMarkupPaneWidth, width);
      pane = Number.isFinite(width) ? Math.max(0, width) : 0;
    },
    hitTest: (at) => s.call(Method.HostHitTest, toViewport(at)),
    dragOver: (at) => void s.call(Method.HostDragOver, toViewport(at)).then(frames.request, report),
    dragLeave: () => void s.call(Method.HostDragLeave).then(frames.request, report),
    drop: (at, op) => s.commit(Method.HostDrop, { at: toViewport(at), command: op }),
    framed,
  };
}
