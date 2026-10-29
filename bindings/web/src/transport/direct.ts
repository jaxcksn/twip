// The engine on the calling thread. One module instance and one engine per page, shared by every editor, so fonts,
// dictionaries and the internal clipboard are shared. Every call goes through one ordered queue that drains in a
// microtask: a command called from a listener runs after the current dispatch.
import { EditorError, type ErrorCode } from '../errors.js';
import type { FaceInfo } from '../fonts/library.js';
import type { Warning } from '../types.js';
import {
  attachWebGpu,
  frameWebGl,
  frameWebGpu,
  gpuDevice,
  type GpuDevice,
  restoreWebGl,
  watchWebGl,
  webGlTarget,
  webGpuTarget,
  writeWebGpu,
  type WebGlTarget,
  type WebGpuTarget,
} from '../host/gpu.js';
import { loadModule, type TwipModule } from './module.js';
import { type Backend, type FrameInfo, Kind, type Input, type Message, Method, type Transport, unwrap } from './protocol.js';

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** Status codes of the C ABI, from 1, for responses that carry no error JSON. */
const statusCodes: ErrorCode[] = [
  'destroyed',
  'invalid-argument',
  'unavailable',
  'read-only',
  'import-failed',
  'export-failed',
  'aborted',
  'not-allowed',
  'unsupported',
  'internal',
];

/** A `TwipBuf` is `{ ptr, len }`: two 32-bit words in wasm32. `TwipFrameInfo` is five f64 then two u32. */
const BUF_SIZE = 8;
const FRAME_INFO_SIZE = 48;
/** Lost WebGPU devices replaced by new ones the engine adopts. Each leaks the engine's GPU context for the old one
 * (Skia can't free a lost device's context), so after these the canvases show CPU pixels copied in. */
const MAX_DEVICE_LOSSES = 3;

/** A session's canvas, on the backend it got. A lost GPU one draws nothing until it has a device or context again. */
type Canvas =
  | { backend: 'webgpu'; canvas: HTMLCanvasElement; target: WebGpuTarget; lost?: boolean }
  | { backend: 'webgl'; canvas: HTMLCanvasElement; target: WebGlTarget; lost?: boolean; unwatch: () => void }
  | { backend: 'cpu'; canvas: HTMLCanvasElement; context: CanvasRenderingContext2D; lost?: undefined };

let shared: Promise<DirectTransport> | undefined;

/** The page's one direct transport, loading the engine module on first use. */
export function directTransport(): Promise<DirectTransport> {
  shared ??= DirectTransport.load().catch((cause: unknown) => {
    shared = undefined;
    throw new EditorError('internal', `the engine module failed to load: ${String(cause)}`, { cause });
  });
  return shared;
}

/** The {@link Transport} that calls the engine module on this thread. */
export class DirectTransport implements Transport {
  readonly #m: TwipModule;
  readonly #engine: number;
  readonly #queue: (() => void)[] = [];
  #draining = false;
  /** Messages from the engine during the running operation, delivered after it returns. */
  #pending: Message[] = [];
  readonly #listeners = new Map<number, Set<(message: Message) => void>>();
  readonly #timers = new Map<number, Set<ReturnType<typeof setTimeout>>>();
  readonly #canvases = new Map<number, Canvas>();
  /** The page's WebGPU device once the engine adopted it; null without one. */
  #gpu: Promise<GpuDevice | null> | undefined;
  #losses = 0;
  /** Set when the module trapped: it can't be trusted again. */
  #fault: EditorError | undefined;

  private constructor(module: TwipModule, engine: number, pending: Message[]) {
    this.#m = module;
    this.#engine = engine;
    this.#pending = pending;
  }

  /** Loads the module and creates the page's engine. */
  static async load(): Promise<DirectTransport> {
    const pending: Message[] = [];
    let module: TwipModule | undefined;
    let transport: DirectTransport | undefined;
    module = await loadModule((kind, sessionId, ptr, len) => {
      // The payload is valid only during the callback: decode (copy) it now, act on it after the operation.
      const payload = kind === Kind.RequestFrame || len === 0 ? undefined : JSON.parse(decoder.decode(module!.HEAPU8.subarray(ptr, ptr + len)));
      (transport ? transport.#pending : pending).push({ kind, sessionId, payload });
    });
    const options = encoder.encode('{}');
    const at = module._twip_alloc(options.length);
    module.HEAPU8.set(options, at);
    const engine = module._twip_engine_new(at, options.length, 0, 0);
    module._twip_free(at, options.length);
    if (!engine) throw new Error('twip_engine_new failed');
    transport = new DirectTransport(module, engine, pending);
    return transport;
  }

  createSession(options: object): Promise<number> {
    return this.#enqueue(() => this.#withBytes(encoder.encode(JSON.stringify(options)), (p, n) => this.#m._twip_session_new(this.#engine, p, n)));
  }

  closeSession(session: number): Promise<void> {
    return this.#enqueue(() => {
      for (const t of this.#timers.get(session) ?? []) clearTimeout(t);
      this.#timers.delete(session);
      const c = this.#canvases.get(session);
      if (c?.backend === 'webgl') c.unwatch();
      this.#canvases.delete(session);
      this.#m._twip_session_free(this.#engine, session);
    }).finally(() => this.#listeners.delete(session));
  }

  call(session: number, method: Method, request?: unknown): Promise<unknown> {
    const json = encoder.encode(JSON.stringify(request ?? null));
    return this.#enqueue(() => {
      let status = 0;
      const out = this.#withBytes(json, (p, n) => this.#readBuf((buf) => (status = this.#m._twip_call(this.#engine, session, method, p, n, buf))));
      return envelope(status, out);
    });
  }

  open(session: number, bytes: Uint8Array, signal?: AbortSignal): Promise<Warning[]> {
    return this.#enqueue(() => {
      signal?.throwIfAborted();
      let status = 0;
      const out = this.#withBytes(bytes, (p, n) =>
        this.#withBytes(encoder.encode('{}'), (o, on) => this.#readBuf((buf) => (status = this.#m._twip_open(this.#engine, session, p, n, o, on, buf)))),
      );
      return (envelope(status, out) as Warning[] | undefined) ?? [];
    }, signal);
  }

  openPrepare(session: number, bytes: Uint8Array): Promise<number> {
    return this.#enqueue(() => {
      let status = 0;
      const out = this.#withBytes(bytes, (p, n) =>
        this.#withBytes(encoder.encode('{}'), (o, on) =>
          this.#readBuf((buf) => (status = this.#m._twip_open_prepare(this.#engine, session, p, n, o, on, buf))),
        ),
      );
      return envelope(status, out) as number;
    });
  }

  async openCommit(session: number, token: number): Promise<Warning[]> {
    return ((await this.call(session, Method.OpenCommit, token)) as Warning[] | undefined) ?? [];
  }

  async openDiscard(session: number, token: number): Promise<void> {
    await this.call(session, Method.OpenDiscard, token);
  }

  export(session: number, options: { format: 'docx' | 'pdf' }, signal?: AbortSignal) {
    return this.#enqueue(() => {
      signal?.throwIfAborted();
      let status = 0;
      let meta: Uint8Array = new Uint8Array();
      const bytes = this.#withBytes(encoder.encode(JSON.stringify(options)), (o, on) =>
        this.#readBuf((bytesBuf) => {
          meta = this.#readBuf((metaBuf) => (status = this.#m._twip_export(this.#engine, session, o, on, bytesBuf, metaBuf)));
        }),
      );
      const { revision, warnings } = envelope(status, meta) as { revision: number; warnings: Warning[] };
      return { bytes, revision, warnings };
    }, signal);
  }

  addFont(bytes: Uint8Array): Promise<FaceInfo[]> {
    return this.#enqueue(() => {
      let status = 0;
      const out = this.#withBytes(bytes, (p, n) => this.#readBuf((buf) => (status = this.#m._twip_font_add(this.#engine, p, n, buf))));
      return envelope(status, out) as FaceInfo[];
    });
  }

  input(session: number, input: Input): Promise<number> {
    return this.#enqueue(() => {
      const [m, e] = [this.#m, this.#engine];
      switch (input.type) {
        case 'key':
          return m._twip_key(e, session, input.key, input.mods);
        case 'text':
          this.#withBytes(encoder.encode(input.text), (p, n) => m._twip_text(e, session, p, n));
          break;
        case 'composition':
          this.#withBytes(encoder.encode(input.text), (p, n) => m._twip_composition(e, session, input.phase, p, n));
          break;
        case 'focus':
          m._twip_focus(e, session, input.focused ? 1 : 0);
          break;
        case 'surface':
          m._twip_surface(e, session, input.width, input.height, input.dpr);
          break;
        case 'scroll':
          m._twip_scroll(e, session, input.x, input.y);
          break;
        case 'wheel':
          m._twip_wheel(e, session, input.dx, input.dy, input.mods);
          break;
        case 'pointer':
          m._twip_pointer(e, session, input.kind, input.x, input.y, input.button, input.clicks, input.mods);
          break;
      }
      return 0;
    });
  }

  async attachCanvas(session: number, canvas: HTMLCanvasElement): Promise<Backend> {
    const device = await this.#device();
    return this.#enqueue(() => {
      const m = this.#m;
      let attached: Canvas | undefined;
      if (device) attached = { backend: 'webgpu', canvas, target: webGpuTarget(canvas, device) };
      const gl = attached ? null : webGlTarget(m, this.#engine, session, canvas);
      if (gl) {
        const c: Canvas = { backend: 'webgl', canvas, target: gl, unwatch: () => {} };
        c.unwatch = watchWebGl(canvas, () => (c.lost = true), () => this.#restoreGl(session, c));
        attached = c;
      }
      const context = attached ? null : canvas.getContext('2d');
      if (context) attached = { backend: 'cpu', canvas, context };
      if (!attached) throw new EditorError('unsupported', 'the canvas has no WebGPU, WebGL 2 or 2D context');
      this.#canvases.set(session, attached);
      return attached.backend;
    });
  }

  frame(session: number, width: number, height: number): Promise<FrameInfo | null> {
    return this.#enqueue(() => {
      const c = this.#canvases.get(session);
      if (!c || c.lost || width === 0 || height === 0) return null;
      if (c.canvas.width !== width) c.canvas.width = width;
      if (c.canvas.height !== height) c.canvas.height = height;
      const m = this.#m;
      const info = m._twip_alloc(FRAME_INFO_SIZE);
      try {
        m.HEAPU8.fill(0, info, info + FRAME_INFO_SIZE);
        let drawn: boolean;
        if (c.backend === 'webgpu') {
          const target = c.target;
          if (target.cpu) drawn = this.#frameRgba(session, width, height, info, (p) => writeWebGpu(target, p, width, height));
          else drawn = frameWebGpu(m, this.#engine, session, target, info);
        } else if (c.backend === 'webgl') drawn = frameWebGl(m, this.#engine, session, c.target, info);
        else {
          const context = c.context;
          drawn = this.#frameRgba(session, width, height, info, (p) => context.putImageData(new ImageData(new Uint8ClampedArray(p.buffer), width, height), 0, 0));
        }
        if (!drawn) return null;
        // TwipFrameInfo: content_width and content_height are its first two f64s, pages its last u32.
        const view = new DataView(m.HEAPU8.buffer, info, FRAME_INFO_SIZE);
        return { contentWidth: view.getFloat64(0, true), contentHeight: view.getFloat64(8, true), pages: view.getUint32(44, true) };
      } finally {
        m._twip_free(info, FRAME_INFO_SIZE);
      }
    });
  }

  /** The page's WebGPU device the engine adopted, or null without one. A lost device is replaced by a new one, which
   * the engine doesn't adopt past `MAX_DEVICE_LOSSES`. */
  #device(): Promise<GpuDevice | null> {
    return (this.#gpu ??= gpuDevice().then((d) => {
      if (!d || (this.#losses <= MAX_DEVICE_LOSSES && !attachWebGpu(this.#m, d))) return null;
      d.lost.then(() => this.#deviceLost(), () => {});
      return d;
    }));
  }

  /** The WebGPU canvases wait for a new device, are configured for it, then draw again. */
  #deviceLost(): void {
    this.#gpu = undefined;
    this.#losses++;
    for (const c of this.#canvases.values()) if (c.backend === 'webgpu') c.lost = true;
    // shortcut: without a new device the canvases stay blank; moving the editor to a new canvas on WebGL or CPU pixels
    // would recover them.
    void this.#device().then((device) =>
      this.#enqueue(() => {
        if (!device) return;
        for (const [session, c] of this.#canvases) {
          if (c.backend !== 'webgpu' || !c.lost) continue;
          c.target = webGpuTarget(c.canvas, device, this.#losses > MAX_DEVICE_LOSSES);
          c.lost = false;
          this.#pending.push({ kind: Kind.RequestFrame, sessionId: session, payload: undefined });
        }
      }),
    );
  }

  /** A session's WebGL context is back: a new context handle for the engine, then a frame. */
  #restoreGl(session: number, c: Extract<Canvas, { backend: 'webgl' }>): void {
    this.#enqueue(() => {
      if (this.#canvases.get(session) !== c) return;
      const target = restoreWebGl(this.#m, this.#engine, session, c.target);
      if (!target) return;
      c.target = target;
      c.lost = false;
      this.#pending.push({ kind: Kind.RequestFrame, sessionId: session, payload: undefined });
    }).catch(() => {});
  }

  /** Draws CPU pixels and hands `put` a copy of them. */
  #frameRgba(session: number, width: number, height: number, info: number, put: (pixels: Uint8Array<ArrayBuffer>) => void): boolean {
    const size = width * height * 4;
    const pixels = this.#m._twip_alloc(size);
    try {
      if (this.#m._twip_frame_rgba(this.#engine, session, pixels, width, height, info) !== 0) return false;
      put(this.#m.HEAPU8.slice(pixels, pixels + size));
      return true;
    } finally {
      this.#m._twip_free(pixels, size);
    }
  }

  listen(session: number, listener: (message: Message) => void): () => void {
    let set = this.#listeners.get(session);
    if (!set) this.#listeners.set(session, (set = new Set()));
    set.add(listener);
    return () => set.delete(listener);
  }

  /** Runs `op` after every earlier operation, then delivers the messages it caused, then settles. */
  #enqueue<T>(op: () => T, signal?: AbortSignal): Promise<T> {
    if (signal?.aborted) return Promise.reject(aborted());
    return new Promise<T>((resolve, reject) => {
      this.#queue.push(() => {
        if (this.#fault) return reject(this.#fault);
        let result: T;
        try {
          result = op();
        } catch (e) {
          this.#deliver();
          return reject(this.#failure(e));
        }
        this.#deliver();
        resolve(result);
      });
      if (!this.#draining) {
        this.#draining = true;
        queueMicrotask(() => {
          // Listeners may enqueue more: they run in this same drain, after the current dispatch.
          for (let next = this.#queue.shift(); next; next = this.#queue.shift()) next();
          this.#draining = false;
        });
      }
    });
  }

  #deliver(): void {
    const messages = this.#pending;
    this.#pending = [];
    for (const message of messages) {
      if (message.kind === Kind.SetTimer) {
        this.#arm(message.sessionId, message.payload as { timer: number; ms: number });
        continue;
      }
      for (const listener of [...(this.#listeners.get(message.sessionId) ?? [])]) {
        try {
          listener(message);
        } catch (e) {
          globalThis.reportError?.(e);
        }
      }
    }
  }

  #arm(session: number, { timer, ms }: { timer: number; ms: number }): void {
    let set = this.#timers.get(session);
    if (!set) this.#timers.set(session, (set = new Set()));
    const handle = setTimeout(() => {
      set.delete(handle);
      this.#enqueue(() => this.#m._twip_timer_fired(this.#engine, session, timer)).catch(() => {});
    }, ms);
    set.add(handle);
  }

  #failure(e: unknown): EditorError {
    if (e instanceof EditorError) return e;
    if (e instanceof DOMException && e.name === 'AbortError') return aborted();
    // A trap: the module's state is unknown from here on.
    this.#fault = new EditorError('internal', `the engine stopped: ${String(e)}`, { fatal: true, cause: e });
    return this.#fault;
  }

  /** Copies `bytes` into the module for the duration of `use`. */
  #withBytes<T>(bytes: Uint8Array, use: (ptr: number, len: number) => T): T {
    const ptr = this.#m._twip_alloc(Math.max(bytes.length, 1));
    this.#m.HEAPU8.set(bytes, ptr);
    try {
      return use(ptr, bytes.length);
    } finally {
      this.#m._twip_free(ptr, Math.max(bytes.length, 1));
    }
  }

  /** Passes a fresh `TwipBuf` to `fill`, then copies its bytes out and frees it. */
  #readBuf(fill: (buf: number) => unknown): Uint8Array {
    const buf = this.#m._twip_alloc(BUF_SIZE);
    this.#m.HEAPU32.fill(0, buf >> 2, (buf >> 2) + 2);
    try {
      fill(buf);
      const ptr = this.#m.HEAPU32[buf >> 2] ?? 0;
      const len = this.#m.HEAPU32[(buf >> 2) + 1] ?? 0;
      const bytes = this.#m.HEAPU8.slice(ptr, ptr + len);
      if (ptr) this.#m._twip_buf_free(buf);
      return bytes;
    } finally {
      this.#m._twip_free(buf, BUF_SIZE);
    }
  }
}

function aborted(): EditorError {
  return new EditorError('aborted', 'the operation was aborted');
}

/** A response: its JSON envelope when there is one, else the status code alone. */
function envelope(status: number, bytes: Uint8Array): unknown {
  if (bytes.length) return unwrap(JSON.parse(decoder.decode(bytes)));
  if (status === 0) return undefined;
  const code = statusCodes[Math.abs(status) - 1] ?? 'internal';
  throw new EditorError(code, `the engine failed with status ${status}`);
}
