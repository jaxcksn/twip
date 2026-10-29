// The browser host: mounting an editor in the application's container. It builds the editor's DOM (host/dom.ts)
// and starts frames, scrolling, keyboard and IME input, the clipboard, pointer input, the accessibility DOM and the
// chrome, everything through the ledger, so destroy and a failed mount release all of it.
import type { MountOptions, Session } from '../editor.js';
import { EditorError } from '../errors.js';
import type { RulerView } from '../services/ruler.js';
import type { ClientPoint } from '../types.js';
import { startA11y } from './a11y.js';
import { type Chrome, startChrome } from './chrome.js';
import { startClipboard, type Clipboard } from './clipboard.js';
import { createDom, Ledger } from './dom.js';
import { startFrames } from './frames.js';
import { startIme } from './ime.js';
import { startInput } from './input.js';
import { startPointer } from './pointer.js';
import { startScroll } from './scroll.js';

/** What a mounted editor gets from its host. */
export interface Host extends Chrome, Clipboard {
  focus(): void;
  /** Resolves once the first viewport is drawn, or at once when the container is hidden or has no size. */
  ready(): Promise<void>;
}

/** Builds the host for `session` in `container`. `claims` is `shortcuts.lookup`, as a yes or no. */
export async function mountHost(container: HTMLElement, s: Session, options: MountOptions, claims: (chord: string) => boolean): Promise<Host> {
  const ledger = new Ledger();
  s.disposers.push(() => ledger.release());
  const dom = createDom(container, ledger);
  const report = (e: unknown) => {
    if (s.destroyed) return;
    s.emit('error', e instanceof EditorError ? e : new EditorError('internal', String(e), { cause: e }));
  };
  // Client points (the API's) to viewport points (the engine's): the canvas covers the viewport.
  const toViewport = (at: { clientX: number; clientY: number }) => {
    const r = dom.canvas.getBoundingClientRect();
    return { x: at.clientX - r.left, y: at.clientY - r.top };
  };
  // The engine's events carry viewport points; the API's are client points.
  s.adjust = (type, event) => {
    const r = dom.canvas.getBoundingClientRect();
    const client = (p: ClientPoint) => ({ clientX: p.clientX + r.left, clientY: p.clientY + r.top });
    if ((type === 'contextMenu' || type === 'gesture') && event) return { ...(event as { at: ClientPoint }), at: client((event as { at: ClientPoint }).at) };
    if (type === 'tooltip' && event) {
      const t = event as { at: ClientPoint; rect: { left: number; top: number } };
      return { ...t, at: client(t.at), rect: { ...t.rect, left: t.rect.left + r.left, top: t.rect.top + r.top } };
    }
    if (type === 'rulerChanged' && event) {
      const v = event as RulerView;
      return { ...v, horizontal: { ...v.horizontal, origin: v.horizontal.origin + r.left }, vertical: { ...v.vertical, origin: v.vertical.origin + r.top } };
    }
    return event;
  };

  let chrome: Chrome | undefined;
  const frames = await startFrames(s, dom, ledger, report, (info) => {
    scroll.layout(info);
    chrome?.framed(info);
  });
  const scroll = startScroll(s, dom, ledger, frames, report);
  chrome = startChrome(s, dom, ledger, frames, report, toViewport);
  const input = startInput(s, dom, ledger, report, claims);
  startIme(s, input.element, ledger, report);
  const clipboard = startClipboard(s, dom, input.element, ledger, report, toViewport);
  const balloons = chrome.balloonLayer;
  startPointer(s, dom, ledger, scroll, report, input.focus, (t) => t instanceof Node && balloons.contains(t) && t !== balloons);
  startA11y(s, dom, ledger);

  await chrome.view.setBackground(options.background ?? '#E6E6E6');
  if (options.markupPaneWidth) await chrome.setMarkupPaneWidth(options.markupPaneWidth);
  if (options.zoom !== undefined && options.zoom !== 100) await s.run({ type: 'setZoom', value: options.zoom });
  return { ...chrome, ...clipboard, focus: input.focus, ready: () => frames.drawn() };
}
