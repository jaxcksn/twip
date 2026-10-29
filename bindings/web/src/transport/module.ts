// Loading the engine module: the one place that knows where the WebAssembly build is and what it exports.
// The build copies the emscripten output (twip-wasm.js and twip_wasm.wasm) next to dist/index.js.

/** Pointers and sizes are 32-bit numbers in wasm32. */
type Ptr = number;

/** The emscripten module of the engine: the C ABI as `_name` exports, plus the heap and the GPU helpers. */
export interface TwipModule {
  HEAPU8: Uint8Array;
  HEAPU32: Uint32Array;
  /** Set by the SDK: `twip-host.js` calls it for every engine callback. The payload lives in HEAPU8 during the call. */
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
  _twip_key(engine: Ptr, session: number, key: number, mods: number): number;
  _twip_text(engine: Ptr, session: number, utf8: Ptr, len: number): void;
  _twip_composition(engine: Ptr, session: number, phase: number, utf8: Ptr, len: number): void;
  _twip_pointer(engine: Ptr, session: number, kind: number, x: number, y: number, button: number, clicks: number, mods: number): void;
  _twip_wheel(engine: Ptr, session: number, dx: number, dy: number, mods: number): void;
  _twip_focus(engine: Ptr, session: number, focused: number): void;
  _twip_surface(engine: Ptr, session: number, width: number, height: number, dpr: number): void;
  _twip_scroll(engine: Ptr, session: number, x: number, y: number): void;
  _twip_frame_rgba(engine: Ptr, session: number, pixels: Ptr, width: number, height: number, info: Ptr): number;
  _twip_timer_fired(engine: Ptr, session: number, timer: number): void;
  _twip_gpu_instance(): Ptr;
  _twip_attach_webgpu(instance: Ptr, device: Ptr, queue: Ptr): number;
  _twip_frame_webgpu(engine: Ptr, session: number, texture: Ptr, bgra: number, height: number, info: Ptr): number;
  _twip_attach_webgl(engine: Ptr, session: number): number;
  _twip_frame_webgl(engine: Ptr, session: number, width: number, height: number, info: Ptr): number;
  /** emscripten's WebGPU import helpers (emdawnwebgpu). */
  WebGPU?: {
    importJsDevice(device: unknown, instance: Ptr): Ptr;
    importJsQueue(queue: unknown, instance: Ptr): Ptr;
    importJsTexture(texture: unknown): Ptr;
  };
  /** emscripten's WebGL context helpers. */
  GL?: {
    createContext(canvas: unknown, attributes: Record<string, number>): number;
    makeContextCurrent(handle: number): boolean;
  };
}

/**
 * Where the engine module and its WebAssembly are, relative to this file in dist/transport/. Both are literal
 * `new URL(..., import.meta.url)`s, so a bundler copies both files into the application's build.
 */
const moduleUrl = new URL('../twip-wasm.js', import.meta.url);
const wasmUrl = new URL('../twip_wasm.wasm', import.meta.url);

/** Instantiates the engine module, with `dispatch` registered before any engine call can call back. */
export async function loadModule(dispatch: NonNullable<TwipModule['twipDispatch']>): Promise<TwipModule> {
  const { default: create } = (await import(moduleUrl.href)) as {
    default: (overrides: object) => Promise<TwipModule>;
  };
  // In Node the module finds its file itself; in a browser it's wherever the bundler put it.
  const locateFile =
    moduleUrl.protocol === 'file:' ? undefined : (file: string) => (file.endsWith('.wasm') ? wasmUrl : new URL(file, moduleUrl)).href;
  const module = await create({ twipDispatch: dispatch, locateFile });
  module.twipDispatch = dispatch;
  return module;
}
