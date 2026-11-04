// What an application builds its chrome on: the balloon layer and attached balloons, the markup pane, the
// background, the view (reveal, visible pages), and hit testing and external drops at client points.
import type { Op } from '../commands.js';
import type { Session } from '../editor.js';
import { stackBalloons, type BalloonPlacement, type CommentAnchor } from '../review.js';
import type { ChromeTheme, DocumentAppearance, Popover, View, ViewOptions } from '../services/view.js';
import type { ViewState } from '../state.js';
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

  // The overlay colours: the theme's until the application sets them (the light theme's are the base it changes).
  const THEME: OverlayColors = { caret: '#1C1D1F', selection: '#9DC7F4', object: '#0A74E4', drop: '#62656C', marks: '#0A74E4', leaders: '#62656C' };
  let overlay = THEME;
  const system = (css: string): Color => {
    const probe = Object.assign(document.createElement('span'), { hidden: true });
    probe.style.color = css;
    document.body.append(probe);
    const [r, g, b] = (getComputedStyle(probe).color.match(/\d+/g) ?? ['0', '0', '0']).map(Number);
    probe.remove();
    return `#${[r, g, b].map((v) => (v ?? 0).toString(16).padStart(2, '0')).join('').toUpperCase()}`;
  };
  const paint = (colors: OverlayColors | null) => s.call(Method.HostSetOverlayPalette, colors).then(() => frames.request());

  // The theme and the page's appearance: the application's, or, while the platform forces its colours, the
  // high-contrast theme from the system colours with a light page; the application's again when that mode ends.
  const look: { theme: ChromeTheme; appearance: DocumentAppearance } = { theme: 'light', appearance: 'light' };
  const forced: MediaQueryList | undefined = globalThis.matchMedia?.('(forced-colors: active)');
  const setLook = async ({ theme, appearance, ...rest }: ViewOptions) => {
    if (theme) look.theme = theme;
    if (appearance) look.appearance = appearance;
    const mine = forced?.matches ? {} : { ...(theme && { theme }), ...(appearance && { appearance }) };
    await s.run({ type: 'setViewOptions', ...rest, ...mine });
  };
  if (forced) {
    const follow = () => {
      const highContrast = () => ({
        canvas: system('Canvas'),
        canvasText: system('CanvasText'),
        highlight: system('Highlight'),
        highlightText: system('HighlightText'),
        grayText: system('GrayText'),
      });
      const options = forced.matches ? { theme: { highContrast: highContrast() }, appearance: 'light' as const } : look;
      s.run({ type: 'setViewOptions', ...options }).catch(report);
    };
    ledger.listen(forced, 'change', follow);
    if (forced.matches) follow();
  }
  // The area around the pages: the theme's desk, or the application's background.
  s.disposers.push(s.subscribe((state) => (dom.scroller.style.background = state.view.background)));

  const set = async (method: Method, value: unknown) => {
    await s.call(method, value);
    frames.request();
  };
  const view: View = {
    revealSelection: () => s.call(Method.HostRevealSelection),
    setBackground: async (color: Color | null) => {
      await set(Method.HostSetBackground, color);
      dom.scroller.style.background = color ?? s.state.view.background;
    },
    setTheme: (theme, accent) => setLook({ theme, ...(accent && { accent }) }),
    setAppearance: (appearance) => setLook({ appearance }),
    setDetail: (detail) => setLook({ detail }),
    visiblePages: () => {
      const r = dom.canvas.getBoundingClientRect();
      return pages.map(({ page, rect }) => ({ page, rect: { ...rect, left: rect.left + r.left, top: rect.top + r.top } }));
    },
    frameStats: () => s.call<FrameStats>(Method.HostFrameStats),
    setOverlayColors: (colors) => {
      overlay = colors ? { ...overlay, ...colors } : THEME;
      return paint(colors && overlay);
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

/** The popovers' colours by theme: the chrome's tokens, as CSS variables. */
function tokens(v: ViewState): Record<string, string> {
  const t = v.theme;
  const c =
    typeof t === 'object'
      ? { pop: t.highContrast.canvas, ink: t.highContrast.canvasText, ink2: t.highContrast.canvasText, ink3: t.highContrast.grayText, line: t.highContrast.canvasText, line2: 'transparent', shadow: 'none' }
      : t === 'dark'
        ? { pop: '#2b2c31', ink: '#e7e7ea', ink2: '#a0a3aa', ink3: '#5f626a', line: 'rgba(255,255,255,.12)', line2: 'rgba(255,255,255,.045)', shadow: '0 1px 2px rgba(0,0,0,.5),0 12px 32px rgba(0,0,0,.55)' }
        : { pop: '#ffffff', ink: '#1c1d1f', ink2: '#62656c', ink3: '#a4a6ac', line: 'rgba(20,22,26,.12)', line2: 'rgba(20,22,26,.05)', shadow: '0 1px 2px rgba(0,0,0,.08),0 10px 28px rgba(0,0,0,.12)' };
  return { ...c, accent: v.accent, 'on-accent': v.accentOn };
}

/** A tab type's mark, as the ruler draws it: 14 px, in the current colour. */
function glyph(type: string): string {
  const r = (x: number, y: number, w: number, h: number) => `<rect x="${x}" y="${y}" width="${w}" height="${h}"/>`;
  const shapes: Record<string, string> = {
    left: r(6, 3, 2, 9) + r(6, 10, 7, 2),
    center: r(6, 3, 2, 9) + r(2, 10, 10, 2),
    right: r(6, 3, 2, 9) + r(1, 10, 7, 2),
    decimal: r(6, 3, 2, 9) + r(2, 10, 10, 2) + r(10, 5, 2, 2),
    bar: r(6, 1, 2, 12),
    firstLineIndent: '<path d="M2 3h10l-5 5z"/>',
    hangingIndent: '<path d="M2 11h10l-5-5z"/>',
  };
  return `<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true" style="flex:none">${shapes[type] ?? ''}</svg>`;
}

/** A position in the ruler's unit: `2.75″`. */
function measure(pt: number, unit: ViewState['rulerUnit']): string {
  const n = (v: number, d: number) => String(+v.toFixed(d));
  switch (unit) {
    case 'centimeters':
      return `${n((pt / 72) * 2.54, 2)} cm`;
    case 'millimeters':
      return `${n((pt / 72) * 25.4, 1)} mm`;
    case 'points':
      return `${n(pt, 1)} pt`;
    case 'picas':
      return `${n(pt / 12, 2)} pi`;
    default:
      return `${n(pt / 72, 2)}″`;
  }
}

const POPOVER_CSS = `
[data-twip=popover]{position:fixed;inset:auto;margin:0;box-sizing:border-box;background:var(--pop);color:var(--ink);border:1px solid var(--line);border-radius:4px;box-shadow:var(--shadow);font:400 13px/1 system-ui,sans-serif;overflow:visible}
[data-twip=popover] .h{font:600 10px/1 ui-monospace,monospace;color:var(--ink3);letter-spacing:.04em}
[data-twip=popover] button{all:unset;box-sizing:border-box;display:flex;align-items:center;cursor:pointer;color:var(--ink)}
[data-twip=popover] button:hover,[data-twip=popover] button:focus-visible{background:var(--line2);box-shadow:inset 2px 0 0 var(--accent)}
[data-twip=popover] .g{display:grid;gap:4px;padding:0 12px 6px}
[data-twip=popover] .g button{justify-content:center;border:1px solid var(--line);font:500 10px/1 system-ui,sans-serif}
[data-twip=popover] .g button[aria-checked=true]{background:var(--accent);color:var(--on-accent);box-shadow:none}
[data-twip=popover] .g button:focus-visible{outline:2px solid var(--accent);outline-offset:1px}
`;

/** A choice in a popover's grid: its value, label, whether it's the current one, and what picking it does. */
type Choice = [value: string, label: string, on: boolean, pick: () => void];

/**
 * The engine's popover requests as HTML popovers in the chrome's colours, unless the application listens for
 * `popoverRequested`: the tab type picker, a tab stop's options and the AutoCorrect menu. Arrows move, Enter picks,
 * Esc closes; focus goes back to the editor.
 */
export function startPopovers(s: Session, dom: Dom, ledger: Ledger, report: (e: unknown) => void, focusEditor: () => void): void {
  const el = ledger.node(dom.scroller.parentElement ?? document.body, document.createElement('div'));
  el.dataset.twip = 'popover';
  el.popover = 'manual';
  el.setAttribute('role', 'menu');
  el.append(Object.assign(document.createElement('style'), { textContent: POPOVER_CSS }));
  const body = el.appendChild(document.createElement('div'));
  /** The engine's popover is open too: closing the host's on its own (Esc, a press outside the editor) closes it. */
  let engineOpen = false;
  let shown: Popover | undefined;

  const close = (refocus: boolean) => {
    if (!shown) return;
    shown = undefined;
    el.hidePopover();
    body.replaceChildren();
    // Its `popoverClosed` then finds nothing open.
    if (engineOpen) s.run({ type: 'closePopover' }).catch(report);
    engineOpen = false;
    if (refocus) focusEditor();
  };
  /** Runs a popover's answer, which closes the engine's popover itself. */
  const answer = (op: Op, keepOpen = false) => {
    if (!keepOpen) engineOpen = false;
    s.run(op).catch(report);
    if (!keepOpen) close(true);
  };
  const node = (tag: string, css: string, text = '', cls = '') => {
    const n = document.createElement(tag);
    n.style.cssText = css;
    n.textContent = text;
    n.className = cls;
    return n;
  };
  const item = (role: 'menuitem' | 'menuitemradio', css: string, pick: () => void, checked?: boolean) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.tabIndex = -1;
    b.setAttribute('role', role);
    if (checked !== undefined) b.setAttribute('aria-checked', String(checked));
    b.style.cssText = css;
    b.addEventListener('click', pick);
    return b;
  };

  const render = (p: Popover) => {
    body.replaceChildren();
    if (p.kind === 'tabType') {
      el.style.width = '260px';
      el.style.padding = '4px';
      el.setAttribute('aria-label', 'Tab type for new stops');
      body.append(node('div', 'padding:8px 8px 6px', 'TAB TYPE FOR NEW STOPS', 'h'));
      for (const c of p.choices) {
        const on = c.value === p.current;
        const current = 'background:var(--line2);box-shadow:inset 2px 0 0 var(--accent);color:var(--accent);';
        const b = item('menuitemradio', `gap:12px;padding:8px;width:100%;${on ? current : ''}`, () => answer({ type: 'setRulerTabType', value: c.value }), on);
        b.innerHTML = glyph(c.value);
        const text = node('div', 'display:flex;flex-direction:column;gap:4px');
        text.append(node('span', 'font:500 13px/1 system-ui,sans-serif;color:var(--ink)', c.name), node('span', 'font:400 11.5px/1.2 system-ui,sans-serif;color:var(--ink2)', c.description));
        b.append(text);
        body.append(b);
      }
      body.append(node('div', 'padding:8px 8px 6px;border-top:1px solid var(--line);margin-top:4px;font:400 11.5px/1.35 system-ui,sans-serif;color:var(--ink2)', 'Then click the ruler to place it.'));
    } else if (p.kind === 'tabStop') {
      el.style.width = '240px';
      el.style.padding = '0';
      const name = { left: 'Left', center: 'Center', right: 'Right', decimal: 'Decimal', bar: 'Bar' };
      const title = `${name[p.alignment]} tab · ${measure(p.position, s.state.view.rulerUnit)}`;
      el.setAttribute('aria-label', title);
      body.append(node('div', 'padding:10px 12px;border-bottom:1px solid var(--line);font:600 11px/1 ui-monospace,monospace', title));
      // A change keeps the popover open, showing the stop's new options.
      const change = (next: { alignment?: typeof p.alignment; leader?: typeof p.leader }) => {
        const q = { ...p, ...next };
        answer({ type: 'addTabStop', position: q.position, alignment: q.alignment, leader: q.leader }, true);
        shown = q;
        render(q);
        body.querySelector<HTMLElement>(next.alignment ? '.g [aria-checked=true]' : '.g:last-of-type [aria-checked=true]')?.focus();
      };
      const grid = (label: string, columns: number, choices: Choice[], css: string) => {
        body.append(node('div', 'padding:10px 12px 6px', label, 'h'));
        const g = node('div', `grid-template-columns:repeat(${columns},1fr);${css}`, '', 'g');
        g.setAttribute('role', 'group');
        g.setAttribute('aria-label', label);
        for (const [value, text, on, pick] of choices) {
          const b = item('menuitemradio', columns === 4 ? 'height:40px;flex-direction:column;gap:4px' : 'height:28px;font-size:11px', pick, on);
          if (columns === 4) b.innerHTML = glyph(value);
          b.append(text);
          g.append(b);
        }
        body.append(g);
      };
      grid('ALIGNMENT', 4, (['left', 'center', 'right', 'decimal'] as const).map((a): Choice => [a, name[a], a === p.alignment, () => change({ alignment: a })]), '');
      grid('LEADER', 2, (['none', 'dots'] as const).map((l): Choice => [l, l === 'none' ? 'None' : 'Dots ·····', l === p.leader, () => change({ leader: l })]), 'padding-bottom:10px');
      const clear = item('menuitem', 'height:32px;width:100%;padding:0 12px;border-top:1px solid var(--line);color:var(--ink2)', () => answer({ type: 'deleteTabStop', value: p.position }));
      clear.textContent = 'Clear tab stop';
      body.append(clear);
    } else {
      el.style.width = '232px';
      el.style.padding = '0';
      el.setAttribute('aria-label', `AutoCorrect: ${p.from} to ${p.to}`);
      const head = node('div', 'display:flex;align-items:center;gap:8px;padding:10px 12px;border-bottom:1px solid var(--line);font:600 11px/1 ui-monospace,monospace');
      head.append(
        node('span', 'color:var(--ink3);text-decoration:line-through', p.from),
        node('span', 'color:var(--ink3)', '→'),
        node('span', 'color:var(--accent)', p.to),
        node('span', 'margin-left:auto;font-weight:500;color:var(--ink3)', 'autocorrect'),
      );
      const list = node('div', 'padding:4px;display:flex;flex-direction:column');
      const entry = (text: string, pick: () => void, hint = '', css = '') => {
        const b = item('menuitem', `justify-content:space-between;height:28px;padding:0 8px;${css}`, pick);
        b.append(node('span', '', text));
        if (hint) b.append(node('span', 'font:500 10px/1 ui-monospace,monospace;color:var(--ink2)', hint));
        list.append(b);
      };
      const mac = /Mac|iP(hone|ad)/.test(globalThis.navigator?.platform ?? '');
      entry(`Revert to ${p.from}`, () => answer({ type: 'revertAutoCorrect' }), mac ? '⌘Z' : 'Ctrl+Z');
      entry(`Never correct ${p.from}`, () => answer({ type: 'neverAutoCorrect' }));
      // The rules are the application's own dialog.
      const rules = () => {
        close(true);
        s.emit('dialogRequested', { kind: 'autoCorrect' });
      };
      entry('AutoCorrect rules…', rules, '', 'color:var(--ink2)');
      body.append(head, list);
    }
  };

  const place = (anchor: ClientRect) => {
    const [w, h] = [el.offsetWidth, el.offsetHeight];
    const left = shown?.kind === 'tabStop' ? anchor.left + anchor.width / 2 - w / 2 : anchor.left;
    el.style.left = `${Math.max(4, Math.min(left, innerWidth - w - 4))}px`;
    el.style.top = `${Math.max(4, Math.min(anchor.top + anchor.height + 4, innerHeight - h - 4))}px`;
  };

  s.fallbacks.set('popoverRequested', (p: Popover) => {
    close(false);
    shown = p;
    engineOpen = true;
    for (const [k, v] of Object.entries(tokens(s.state.view))) el.style.setProperty(`--${k}`, v);
    render(p);
    el.showPopover();
    place(p.anchor);
    (body.querySelector<HTMLElement>('[aria-checked=true]') ?? body.querySelector<HTMLElement>('button'))?.focus();
  });
  s.disposers.push(
    () => s.fallbacks.delete('popoverRequested'),
    s.on('popoverClosed', () => {
      engineOpen = false;
      close(el.contains(document.activeElement));
    }),
  );

  ledger.listen(el, 'keydown', (e: KeyboardEvent) => {
    const items = [...body.querySelectorAll('button')];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const step = ({ ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 } as Record<string, number>)[e.key];
    if (step) items[(at + step + items.length) % items.length]?.focus();
    else if (e.key === 'Home') items[0]?.focus();
    else if (e.key === 'End') items.at(-1)?.focus();
    else if (e.key === 'Escape' || e.key === 'Tab') close(true);
    else return;
    e.preventDefault();
    e.stopPropagation();
  });
  // A press elsewhere closes it; on the editor, the engine's own dismissal follows.
  ledger.listen(
    document,
    'pointerdown',
    (e: Event) => {
      if (!shown || el.contains(e.target as Node)) return;
      if (dom.scroller.contains(e.target as Node)) engineOpen = false;
      close(false);
    },
    { capture: true },
  );
}
