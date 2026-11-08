// Browser GPU frames: imports the page's WebGPU device and canvas textures, or a WebGL 2 context, into the engine
// module and draws a session's frame through them. WebGPU first, then WebGL 2; CPU pixels are the fallback. A lost
// device or context is replaced: a new device is attached when one is lost, a WebGL context when it is restored. Past
// a few lost devices, CPU pixels are copied into the canvas instead.
import type { TwipModule } from '../transport/module.js';

/** The parts of a WebGPU device the engine imports. */
export interface GpuDevice {
  queue: { writeTexture(destination: { texture: unknown }, data: Uint8Array, layout: { bytesPerRow: number }, size: { width: number; height: number }): void };
  /** Settles when the device is lost: destroyed, or the GPU reset. */
  lost: Promise<unknown>;
}

/** The parts of a WebGPU canvas context used here. */
interface GpuCanvasContext {
  configure(options: { device: GpuDevice; format: string; alphaMode: string; usage: number }): void;
  getCurrentTexture(): unknown;
}

/** A canvas configured for WebGPU frames. */
export interface WebGpuTarget {
  canvas: HTMLCanvasElement | OffscreenCanvas;
  context: GpuCanvasContext;
  bgra: boolean;
  /** CPU pixels are copied into the texture with this device, which the engine didn't adopt. */
  cpu?: GpuDevice;
}

/** A canvas with the engine's WebGL 2 context. */
export interface WebGlTarget {
  canvas: HTMLCanvasElement | OffscreenCanvas;
  handle: number;
}

/** Adopts a WebGPU device for every session of the engine. False if the engine can't use it: use WebGL then. */
export function attachWebGpu(m: TwipModule, device: GpuDevice): boolean {
  if (!m.WebGPU) return false;
  const instance = m._twip_gpu_instance();
  const imported = m.WebGPU.importJsDevice(device, instance);
  return m._twip_attach_webgpu(instance, imported, m.WebGPU.importJsQueue(device.queue, instance)) === 0;
}

/** Configures `canvas` for WebGPU frames with `device`; for CPU pixels copied in, with `cpu`. */
export function webGpuTarget(canvas: HTMLCanvasElement | OffscreenCanvas, device: GpuDevice, cpu = false): WebGpuTarget {
  const context = canvas.getContext('webgpu' as '2d') as unknown as GpuCanvasContext;
  if (cpu) {
    // COPY_DST: the engine's pixels are premultiplied RGBA.
    context.configure({ device, format: 'rgba8unorm', alphaMode: 'premultiplied', usage: 0x02 });
    return { canvas, context, bgra: false, cpu: device };
  }
  const gpu = (globalThis.navigator as { gpu?: { getPreferredCanvasFormat(): string } } | undefined)?.gpu;
  const format = gpu?.getPreferredCanvasFormat() ?? 'rgba8unorm';
  // COPY_SRC | COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT: what Graphite needs of the canvas texture.
  context.configure({ device, format, alphaMode: 'premultiplied', usage: 0x01 | 0x02 | 0x04 | 0x10 });
  return { canvas, context, bgra: format === 'bgra8unorm' };
}

/** The page's WebGPU device, or null when it has none. */
export async function gpuDevice(): Promise<GpuDevice | null> {
  const gpu = (globalThis.navigator as { gpu?: { requestAdapter(): Promise<{ requestDevice(): Promise<GpuDevice> } | null> } } | undefined)?.gpu;
  try {
    return (await (await gpu?.requestAdapter())?.requestDevice()) ?? null;
  } catch {
    return null;
  }
}

/** Draws a session's frame into the target's current texture; the frame's `TwipFrameInfo` goes to `info`. */
export function frameWebGpu(m: TwipModule, engine: number, session: number, target: WebGpuTarget, info: number): boolean {
  if (!m.WebGPU) return false;
  const texture = m.WebGPU.importJsTexture(target.context.getCurrentTexture());
  return m._twip_frame_webgpu(engine, session, texture, target.bgra ? 1 : 0, target.canvas.height, info) === 0;
}

/** Copies CPU pixels (premultiplied RGBA) into a CPU target's current texture. */
export function writeWebGpu(target: WebGpuTarget, pixels: Uint8Array, width: number, height: number): void {
  target.cpu?.queue.writeTexture({ texture: target.context.getCurrentTexture() }, pixels, { bytesPerRow: width * 4 }, { width, height });
}

/** Creates a WebGL 2 context on `canvas` for a session (one per canvas), or null when WebGL 2 is unavailable. */
export function webGlTarget(m: TwipModule, engine: number, session: number, canvas: HTMLCanvasElement | OffscreenCanvas): WebGlTarget | null {
  if (!m.GL) return null;
  const attributes = { majorVersion: 2, minorVersion: 0, alpha: 1, depth: 0, stencil: 8, antialias: 0, premultipliedAlpha: 1 };
  const handle = m.GL.createContext(canvas, attributes);
  if (!handle) return null;
  m.GL.makeContextCurrent(handle);
  return m._twip_attach_webgl(engine, session) === 0 ? { canvas, handle } : null;
}

/** The session's WebGL target again once its lost context is restored: a new context handle, attached anew. */
export function restoreWebGl(m: TwipModule, engine: number, session: number, target: WebGlTarget): WebGlTarget | null {
  m.GL?.deleteContext(target.handle);
  return webGlTarget(m, engine, session, target.canvas);
}

/** Keeps a lost WebGL context restorable (by default the browser drops it), calling `lost` then `restored`. */
export function watchWebGl(canvas: HTMLCanvasElement | OffscreenCanvas, lost: () => void, restored: () => void): () => void {
  const onLost = (e: Event) => {
    e.preventDefault();
    lost();
  };
  canvas.addEventListener('webglcontextlost', onLost);
  canvas.addEventListener('webglcontextrestored', restored);
  return () => {
    canvas.removeEventListener('webglcontextlost', onLost);
    canvas.removeEventListener('webglcontextrestored', restored);
  };
}

/** Draws a session's frame into its WebGL canvas; the frame's `TwipFrameInfo` goes to `info`. */
export function frameWebGl(m: TwipModule, engine: number, session: number, target: WebGlTarget, info: number): boolean {
  if (!m.GL) return false;
  m.GL.makeContextCurrent(target.handle);
  return m._twip_frame_webgl(engine, session, target.canvas.width, target.canvas.height, info) === 0;
}
