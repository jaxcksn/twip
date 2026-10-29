// Native scrolling: the content element is as large as the document at the zoom, so the scroller scrolls natively
// and its position goes to the engine (`twip_scroll`). Content narrower than the viewport is centred. The `scroll_to`
// and `scroll` host requests scroll the scroller; Ctrl+wheel (and a trackpad's pinch, which arrives as one) zooms.
import type { Session } from '../editor.js';
import { type FrameInfo, Kind, type HostRequest, type Message } from '../transport/protocol.js';
import type { Dom, Ledger } from './dom.js';
import type { Frames } from './frames.js';

/** The scrolling of a mounted editor. */
export interface Scroll {
  /** Where content x 0 is in the scroller's scrolled box: the centring offset. */
  readonly offset: number;
  /** Sizes the content after a frame. */
  layout(info: FrameInfo): void;
  /** Zooms by a factor of the current zoom. */
  zoomBy(factor: number): void;
}

export function startScroll(s: Session, dom: Dom, ledger: Ledger, frames: Frames, report: (e: unknown) => void): Scroll {
  const { scroller, content } = dom;
  let offset = 0;
  let sent = { x: NaN, y: NaN };
  let size = { width: 0, height: 0 };
  // A `scroll` request (a zoom), applied once the next frame has sized the content for it.
  let pending: { x: number; y: number } | undefined;

  const send = () => {
    const at = { x: scroller.scrollLeft - offset, y: scroller.scrollTop };
    if (at.x === sent.x && at.y === sent.y) return;
    sent = at;
    s.input({ type: 'scroll', ...at }).catch(report);
  };
  const layout = (info: FrameInfo) => {
    if (info.contentWidth !== size.width || info.contentHeight !== size.height) {
      size = { width: info.contentWidth, height: info.contentHeight };
      content.style.width = `${size.width}px`;
      content.style.height = `${size.height}px`;
    }
    const centred = Math.max(0, Math.floor((scroller.clientWidth - size.width) / 2));
    if (centred !== offset) {
      offset = centred;
      content.style.marginLeft = `${offset}px`;
    }
    if (pending) {
      // The engine drew this frame from there already: report only where the scroller couldn't go.
      scroller.scrollLeft = pending.x + offset;
      scroller.scrollTop = pending.y;
      sent = pending;
      pending = undefined;
    }
    send();
  };
  ledger.listen(scroller, 'scroll', send, { passive: true });

  // `scroll_to`: the least scrolling that shows the rectangle (content px).
  const scrollTo = (r: { x: number; y: number; width: number; height: number }) => {
    const left = r.x + offset;
    const { clientWidth: w, clientHeight: h } = scroller;
    if (r.y < scroller.scrollTop) scroller.scrollTop = r.y;
    else if (r.y + r.height > scroller.scrollTop + h) scroller.scrollTop = r.y + r.height - h;
    if (left < scroller.scrollLeft) scroller.scrollLeft = left;
    else if (left + r.width > scroller.scrollLeft + w) scroller.scrollLeft = left + r.width - w;
    send();
  };
  const onMessage = (m: Message) => {
    const r = m.payload as HostRequest;
    if (m.kind !== Kind.HostRequest) return;
    if (r.type === 'scroll_to') scrollTo(r);
    else if (r.type === 'scroll') pending = { x: r.x, y: r.y };
  };
  s.hostListeners.add(onMessage);
  s.disposers.push(() => s.hostListeners.delete(onMessage));

  // Zoom: factors gather while a zoom is running, then apply at once.
  let factor = 1;
  let zooming = false;
  const zoomBy = (f: number) => {
    factor *= f;
    if (zooming) return;
    zooming = true;
    const zoom = Math.min(500, Math.max(10, Math.round(s.state.view.zoom * factor)));
    factor = 1;
    s.run({ type: 'setZoom', value: zoom })
      .catch(report)
      .finally(() => {
        zooming = false;
        frames.request();
        if (factor !== 1) zoomBy(1);
      });
  };
  ledger.listen(
    scroller,
    'wheel',
    (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      // 10% a wheel notch (100 px, or 3 lines); a pinch sends small deltas.
      zoomBy(1.1 ** (-e.deltaY / (e.deltaMode === 0 ? 100 : 3)));
    },
    { passive: false },
  );

  return {
    get offset() {
      return offset;
    },
    layout,
    zoomBy,
  };
}
