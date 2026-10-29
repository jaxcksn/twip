// Browser GPU frames: imports the page's WebGPU device and canvas textures, or a WebGL 2 context, into the engine
// module and draws a session's frame through them. WebGPU first, then WebGL 2; CPU pixels are the fallback.
import type { TwipModule } from '../transport/module.js';

/** The parts of a WebGPU device the engine imports. */
export interface GpuDevice {
  queue: unknown;
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

/** Configures `canvas` for WebGPU frames with `device`. */
export function webGpuTarget(canvas: HTMLCanvasElement | OffscreenCanvas, device: GpuDevice): WebGpuTarget {
  const context = canvas.getContext('webgpu' as '2d') as unknown as GpuCanvasContext;
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

/** Creates a WebGL 2 context on `canvas` for a session (one per canvas), or null when WebGL 2 is unavailable. */
export function webGlTarget(m: TwipModule, engine: number, session: number, canvas: HTMLCanvasElement | OffscreenCanvas): WebGlTarget | null {
  if (!m.GL) return null;
  const attributes = { majorVersion: 2, minorVersion: 0, alpha: 1, depth: 0, stencil: 8, antialias: 0, premultipliedAlpha: 1 };
  const handle = m.GL.createContext(canvas, attributes);
  if (!handle) return null;
  m.GL.makeContextCurrent(handle);
  return m._twip_attach_webgl(engine, session) === 0 ? { canvas, handle } : null;
}

/** Draws a session's frame into its WebGL canvas; the frame's `TwipFrameInfo` goes to `info`. */
export function frameWebGl(m: TwipModule, engine: number, session: number, target: WebGlTarget, info: number): boolean {
  if (!m.GL) return false;
  m.GL.makeContextCurrent(target.handle);
  return m._twip_frame_webgl(engine, session, target.canvas.width, target.canvas.height, info) === 0;
}
