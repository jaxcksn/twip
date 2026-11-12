// Browser GPU frames: the engine draws each session's frame into its canvas itself, on the page's WebGPU (wgpu,
// crates/twip/src/render/window.rs). The SDK starts the engine's device, attaches canvases to it, and after a lost
// device starts a new one and attaches the canvases again. Past a few lost devices, CPU pixels are copied into the
// canvas with a device of the page's own instead.
import type { TwipModule } from '../transport/module.js';

/** The parts of a WebGPU device the CPU path uses. */
export interface GpuDevice {
  queue: { writeTexture(destination: { texture: unknown }, data: Uint8Array, layout: { bytesPerRow: number }, size: { width: number; height: number }): void };
}

/** The parts of a WebGPU canvas context the CPU path uses. */
interface GpuCanvasContext {
  configure(options: { device: GpuDevice; format: string; alphaMode: string; usage: number }): void;
  getCurrentTexture(): unknown;
}

/** A canvas CPU pixels are copied into with a device of the page's. */
export interface CpuGpuTarget {
  context: GpuCanvasContext;
  device: GpuDevice;
}

/** What `twip_gpu_start` reports: no device, the one there kept, or a new one. */
export const GpuStart = { None: 0, Kept: 1, New: 2 } as const;

/** Whether the page has WebGPU at all. */
export function hasWebGpu(): boolean {
  return typeof (globalThis.navigator as { gpu?: unknown } | undefined)?.gpu === 'object';
}

/** Gives the session its canvas for GPU frames: true when the engine draws into it. */
export function attachGpu(m: TwipModule, engine: number, session: number, canvas: HTMLCanvasElement | OffscreenCanvas): boolean {
  return m.twip_attach_canvas(engine, session, canvas) > 0;
}

/** Draws a session's frame into its canvas on the GPU; the frame's `TwipFrameInfo` goes to `info`. */
export function frameGpu(m: TwipModule, engine: number, session: number, width: number, height: number, info: number): boolean {
  return m._twip_frame_window(engine, session, width, height, info) === 0;
}

/** A device of the page's own, for copying CPU pixels into a canvas that has a WebGPU context; null without one. */
export async function cpuDevice(): Promise<GpuDevice | null> {
  const gpu = (globalThis.navigator as { gpu?: { requestAdapter(): Promise<{ requestDevice(): Promise<GpuDevice> } | null> } } | undefined)?.gpu;
  try {
    return (await (await gpu?.requestAdapter())?.requestDevice()) ?? null;
  } catch {
    return null;
  }
}

/** Configures `canvas` for CPU pixels copied in with `device` (COPY_DST: the engine's pixels are premultiplied RGBA). */
export function cpuGpuTarget(canvas: HTMLCanvasElement | OffscreenCanvas, device: GpuDevice): CpuGpuTarget {
  const context = canvas.getContext('webgpu' as '2d') as unknown as GpuCanvasContext;
  context.configure({ device, format: 'rgba8unorm', alphaMode: 'premultiplied', usage: 0x02 });
  return { context, device };
}

/** Copies CPU pixels (premultiplied RGBA) into the target's current texture. */
export function writeGpu(target: CpuGpuTarget, pixels: Uint8Array, width: number, height: number): void {
  target.device.queue.writeTexture({ texture: target.context.getCurrentTexture() }, pixels, { bytesPerRow: width * 4 }, { width, height });
}
