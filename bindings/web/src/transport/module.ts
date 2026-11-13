// Loading the engine module: the one place that knows where the WebAssembly build is and what it exports.
// The build copies the module (twip-wasm.js, wasm-bindgen's glue, and twip_wasm.wasm) next to dist/index.js.
import { EditorError } from '../errors.js';

/** Pointers and sizes are 32-bit numbers in wasm32. */
type Ptr = number;

/** The engine module: the C ABI as `_name` functions, the memory they share as `HEAPU8` and `HEAPU32`, and the GPU
 * screen's start and canvas functions. */
export interface TwipModule {
  readonly HEAPU8: Uint8Array;
  readonly HEAPU32: Uint32Array;
  /** Set by the SDK: the engine hands it every callback. The payload lives in HEAPU8 during the call. */
  twipDispatch?: (kind: number, session: number, ptr: Ptr, len: number) => void;
  _twip_alloc(len: number): Ptr;
  _twip_free(ptr: Ptr, len: number): void;
  _twip_buf_free(buf: Ptr): void;
  _twip_engine_new(options: Ptr, len: number, callback: Ptr, context: Ptr): Ptr;
  _twip_engine_free(engine: Ptr): void;
  _twip_session_new(engine: Ptr, options: Ptr, len: number): number;
  _twip_session_free(engine: Ptr, session: number): void;
  _twip_call(engine: Ptr, session: number, method: number, request: Ptr, len: number, out: Ptr): number;
  _twip_open(engine: Ptr, session: number, bytes: Ptr, len: number, options: Ptr, optionsLen: number, out: Ptr): number;
  _twip_open_prepare(engine: Ptr, session: number, bytes: Ptr, len: number, options: Ptr, optionsLen: number, out: Ptr): number;
  _twip_export(engine: Ptr, session: number, options: Ptr, optionsLen: number, bytes: Ptr, meta: Ptr): number;
  _twip_font_add(engine: Ptr, bytes: Ptr, len: number, out: Ptr): number;
  _twip_font_symbol_fallback(engine: Ptr, bytes: Ptr, len: number, out: Ptr): number;
  _twip_key(engine: Ptr, session: number, key: number, mods: number): number;
  _twip_text(engine: Ptr, session: number, utf8: Ptr, len: number): void;
  _twip_composition(engine: Ptr, session: number, phase: number, utf8: Ptr, len: number): void;
  _twip_pointer(engine: Ptr, session: number, kind: number, x: number, y: number, button: number, clicks: number, mods: number): void;
  _twip_wheel(engine: Ptr, session: number, dx: number, dy: number, mods: number): void;
  _twip_modifiers(engine: Ptr, session: number, mods: number): void;
  _twip_focus(engine: Ptr, session: number, focused: number): void;
  _twip_surface(engine: Ptr, session: number, width: number, height: number, dpr: number): void;
  _twip_scroll(engine: Ptr, session: number, x: number, y: number): void;
  _twip_frame_rgba(engine: Ptr, session: number, pixels: Ptr, width: number, height: number, info: Ptr): number;
  _twip_frame_window(engine: Ptr, session: number, width: number, height: number, info: Ptr): number;
  _twip_detach_window(engine: Ptr, session: number): void;
  _twip_test_lose_gpu(engine: Ptr, session: number): void;
  _twip_timer_fired(engine: Ptr, session: number, timer: number): void;
  /** Makes the page's WebGPU device when there is none or it was lost: 0 none, 1 the one there kept, 2 a new one. */
  twip_gpu_start(): Promise<number>;
  /** Gives a session its canvas for GPU frames: the backend (5 WebGPU), or a negative status. */
  twip_attach_canvas(engine: Ptr, session: number, canvas: HTMLCanvasElement | OffscreenCanvas): number;
  /** Without WebGPU: makes `canvas` a WebGL 2 context of its own, as a handle for `twip_attach_webgl` (0: none). */
  twip_webgl_context(canvas: HTMLCanvasElement | OffscreenCanvas): Promise<number>;
  /** Gives a session the WebGL canvas of `handle`: the backend (6 WebGL), or a negative status. */
  twip_attach_webgl(engine: Ptr, session: number, handle: number): number;
  /** Lets go of the WebGL context of `handle` when no session is to take it. */
  twip_webgl_forget(handle: number): void;
}

/**
 * Where the engine module and its WebAssembly are by default: relative to this file in dist/transport/. Both are
 * literal `new URL(..., import.meta.url)`s, so a bundler that follows them copies both files into the application's
 * build. {@link configure}'s `assets` replaces them.
 */
const defaultModuleUrl = new URL('../twip-wasm.js', import.meta.url);
const defaultWasmUrl = new URL('../twip_wasm.wasm', import.meta.url);

/** The folder of the package's dist/ files, when the application set one. */
let assets: URL | undefined;
/** Whether this thread has read `assets` to start the engine: it can't change after. */
let used = false;

/**
 * Sets where the package's runtime files (dist/: worker.js, twip-wasm.js, twip_wasm.wasm, transport/ and worker/) are
 * served from, for an application whose bundler doesn't follow `new URL(..., import.meta.url)`. Call it once, before
 * anything starts the engine: the first {@link FontLibrary} call, {@link Editor.mount} or
 * {@link HeadlessEditor.create}. The engine is shared by every session, so it's not a per-session option.
 *
 * `assets` is the folder, resolved against the document's base URL. It may be on another origin (a CDN), which must
 * then send CORS headers for every file; see the package README.
 *
 * @throws {@link EditorError} `not-allowed` when the engine has already loaded from another folder.
 */
export function configure(options: { assets: string | URL }): void {
  const url = new URL(options.assets, globalThis.document?.baseURI ?? globalThis.location?.href);
  if (!url.pathname.endsWith('/')) url.pathname += '/';
  if (used && url.href !== assets?.href) {
    throw new EditorError('not-allowed', 'configure() must be called before the first FontLibrary call or editor: the engine has already loaded');
  }
  assets = url;
}

/** The configured assets folder, if any, read to start the engine: it can't change after this. */
export function useAssets(): URL | undefined {
  used = true;
  return assets;
}

/** Instantiates the engine module, with `dispatch` registered before any engine call can call back. */
export async function loadModule(dispatch: NonNullable<TwipModule['twipDispatch']>): Promise<TwipModule> {
  const assets = useAssets();
  const moduleUrl = assets ? new URL('twip-wasm.js', assets) : defaultModuleUrl;
  const wasmUrl = assets ? new URL('twip_wasm.wasm', assets) : defaultWasmUrl;
  // The engine calls the host through this global (crates/twip-capi/src/web.rs); each worker has its own.
  (globalThis as { twipHost?: unknown }).twipHost = { emit: dispatch, gpuLost: () => {} };
  const glue = (await import(moduleUrl.href)) as Record<string, unknown> & {
    default: (options: { module_or_path: unknown }) => Promise<Record<string, unknown>>;
  };
  // In Node the module's bytes are read from its file; in a browser it's fetched from wherever the bundler, or
  // `assets`, put it. (The specifier is built so bundlers don't follow it.)
  const fs = 'node:fs/promises';
  const source = wasmUrl.protocol === 'file:' ? await ((await import(fs)) as { readFile(path: URL): Promise<Uint8Array> }).readFile(wasmUrl) : wasmUrl;
  const exports = await glue.default({ module_or_path: source });
  const memory = exports.memory as WebAssembly.Memory;
  const module: Record<string, unknown> = {
    // Views made on each use: the memory's buffer changes when it grows.
    get HEAPU8() {
      return new Uint8Array(memory.buffer);
    },
    get HEAPU32() {
      return new Uint32Array(memory.buffer);
    },
    twipDispatch: dispatch,
    twip_gpu_start: glue.twip_gpu_start,
    twip_attach_canvas: glue.twip_attach_canvas,
    twip_webgl_context: glue.twip_webgl_context,
    twip_attach_webgl: glue.twip_attach_webgl,
    twip_webgl_forget: glue.twip_webgl_forget,
  };
  for (const [name, value] of Object.entries(exports)) if (name.startsWith('twip_')) module[`_${name}`] = value;
  if (typeof module._twip_engine_new !== 'function') throw new EditorError('internal', 'the engine module has no C ABI exports');
  return module as unknown as TwipModule;
}
