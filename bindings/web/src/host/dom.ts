// The editor's DOM inside the application's container, and the ledger of everything a mount creates: listeners,
// observers, timers and animation frames are made through it, so destroy (or a failed mount) undoes all of it.
import { EditorError } from '../errors.js';

/** What a mount owns: `container > scroller > [canvas, content > layer]`, each marked with `data-twip`. */
export interface Dom {
  /** Scrolls natively over the content; the canvas stays in view. */
  scroller: HTMLDivElement;
  /** The viewport's pixels. */
  canvas: HTMLCanvasElement;
  /** As large as the document at the zoom: it gives the scroller its extent. ContentPoint coordinates. */
  content: HTMLDivElement;
  /** Over the content: the input element, the accessibility DOM and the balloon layer. */
  layer: HTMLDivElement;
}

/** What every live ledger holds, and the engine worker (transport/worker.ts), for the tests (`globalThis[resources]()`). */
export const live = { nodes: 0, listeners: 0, observers: 0, timers: 0, frames: 0, workers: 0 };

/** The symbol the resource counts are read through. Internal: for the SDK's tests. */
export const resources = Symbol.for('twip.resources');
(globalThis as Record<symbol, unknown>)[resources] = () => ({ ...live });

/** The resources of one mount. */
export class Ledger {
  readonly #undo: (() => void)[] = [];
  readonly #timers = new Set<ReturnType<typeof setTimeout>>();
  readonly #frames = new Set<number>();
  #released = false;

  /** `addEventListener`, removed on release. */
  listen<K extends keyof HTMLElementEventMap>(
    target: HTMLElement,
    type: K,
    listener: (e: HTMLElementEventMap[K]) => void,
    options?: AddEventListenerOptions,
  ): void;
  listen(target: EventTarget, type: string, listener: (e: Event) => void, options?: AddEventListenerOptions): void;
  listen(target: EventTarget, type: string, listener: (e: never) => void, options?: AddEventListenerOptions): void {
    const l = listener as EventListener;
    target.addEventListener(type, l, options);
    live.listeners++;
    this.#undo.push(() => {
      target.removeEventListener(type, l, options);
      live.listeners--;
    });
  }

  /** An observer, disconnected on release. */
  observe<T extends { disconnect(): void }>(observer: T): T {
    live.observers++;
    this.#undo.push(() => {
      observer.disconnect();
      live.observers--;
    });
    return observer;
  }

  /** A node appended to `parent`, removed on release. */
  node<T extends Element>(parent: Element, node: T): T {
    parent.append(node);
    live.nodes++;
    this.#undo.push(() => {
      node.remove();
      live.nodes--;
    });
    return node;
  }

  /** A one-shot timer, cleared on release. Returns its cancel. */
  timeout(run: () => void, ms: number): () => void {
    if (this.#released) return () => {};
    const handle = setTimeout(() => {
      this.#timers.delete(handle);
      live.timers--;
      run();
    }, ms);
    this.#timers.add(handle);
    live.timers++;
    return () => {
      if (this.#timers.delete(handle)) {
        clearTimeout(handle);
        live.timers--;
      }
    };
  }

  /** One animation frame, cancelled on release. Returns its cancel. */
  frame(run: () => void): () => void {
    if (this.#released) return () => {};
    const handle = requestAnimationFrame(() => {
      this.#frames.delete(handle);
      live.frames--;
      run();
    });
    this.#frames.add(handle);
    live.frames++;
    return () => {
      if (this.#frames.delete(handle)) {
        cancelAnimationFrame(handle);
        live.frames--;
      }
    };
  }

  /** Undoes everything, last made first. Idempotent. */
  release(): void {
    this.#released = true;
    for (const handle of this.#timers) clearTimeout(handle);
    for (const handle of this.#frames) cancelAnimationFrame(handle);
    live.timers -= this.#timers.size;
    live.frames -= this.#frames.size;
    this.#timers.clear();
    this.#frames.clear();
    for (const undo of this.#undo.splice(0).reverse()) undo();
  }
}

/** Containers with an editor, or one mounting. */
const occupied = new WeakSet<HTMLElement>();

/** Claims `container` for one editor, or throws `unavailable`. Returns the release. */
export function claim(container: HTMLElement): () => void {
  if (occupied.has(container) || container.querySelector(':scope > [data-twip]')) {
    throw new EditorError('unavailable', 'the container already holds an editor');
  }
  occupied.add(container);
  return () => occupied.delete(container);
}

function element<K extends 'div' | 'canvas'>(tag: K, name: string, css: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.dataset.twip = name;
  e.style.cssText = css;
  return e;
}

/** Builds the editor's DOM in `container`; the ledger removes it. */
export function createDom(container: HTMLElement, ledger: Ledger): Dom {
  // One grid cell holds the canvas and the content: the sticky canvas can then stay in view over the content's whole
  // extent, in both directions.
  const scroller = element('div', 'editor', 'position:relative;width:100%;height:100%;overflow:auto;display:grid;outline:none;');
  const canvas = element('canvas', 'canvas', 'grid-area:1/1;position:sticky;left:0;top:0;align-self:start;justify-self:start;display:block;touch-action:none;');
  const content = element('div', 'content', 'grid-area:1/1;position:relative;pointer-events:none;');
  const layer = element('div', 'layer', 'position:absolute;inset:0;overflow:hidden;');
  content.append(layer);
  scroller.append(canvas, content);
  ledger.node(container, scroller);
  return { scroller, canvas, content, layer };
}
