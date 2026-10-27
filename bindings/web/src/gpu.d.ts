import type { Editor } from './Editor';

export interface WebGpuTarget { canvas: HTMLCanvasElement; context: GPUCanvasContext; bgra: boolean }
export interface WebGlTarget { canvas: HTMLCanvasElement; handle: number }

/** Adopts a GPUDevice for every editor; false if the engine can't use it (paint with WebGL instead). */
export function attachWebGpu(device: GPUDevice): boolean;
/** Configures `canvas` for WebGPU painting with `device`. */
export function webGpuTarget(canvas: HTMLCanvasElement, device: GPUDevice): WebGpuTarget;
/** Paints `editor` into the target's current texture; the page stack's top-left at (`originX`, `originY`) CSS px. */
export function paintWebGpu(editor: Editor, target: WebGpuTarget, originX: number, originY: number, dpr?: number): boolean;
/** Creates a WebGL 2 context on `canvas` for `editor` (one per canvas). */
export function webGlTarget(editor: Editor, canvas: HTMLCanvasElement): WebGlTarget;
/** Paints `editor` into its WebGL canvas; the page stack's top-left at (`originX`, `originY`) CSS px. */
export function paintWebGl(editor: Editor, target: WebGlTarget, originX: number, originY: number, dpr?: number): boolean;
