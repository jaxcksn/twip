// Browser GPU painting: imports the page's WebGPU device and canvas textures, or a WebGL 2 context, into the engine
// module and paints an editor through them. Hosts call these instead of the raw handle methods on Editor.
import { emscripten } from './diplomat-wasm.mjs';
import { Editor } from './Editor.mjs';

/** Adopts a GPUDevice for every editor; false if the engine can't use it (paint with WebGL instead). */
export function attachWebGpu(device) {
  const instance = Editor.webgpuInstance();
  const { WebGPU } = emscripten;
  return Editor.attachWebgpu(instance, WebGPU.importJsDevice(device, instance), WebGPU.importJsQueue(device.queue, instance));
}

/** Configures `canvas` for WebGPU painting with `device`; pass the result to `paintWebGpu`. */
export function webGpuTarget(canvas, device) {
  const context = canvas.getContext('webgpu');
  const format = navigator.gpu.getPreferredCanvasFormat();
  // COPY_SRC | COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT: what Graphite needs of the canvas texture.
  context.configure({ device, format, alphaMode: 'premultiplied', usage: 0x01 | 0x02 | 0x04 | 0x10 });
  return { canvas, context, bgra: format === 'bgra8unorm' };
}

/** Paints `editor` into the target's current texture; the page stack's top-left at (`originX`, `originY`) CSS px. */
export function paintWebGpu(editor, target, originX, originY, dpr = globalThis.devicePixelRatio ?? 1) {
  const texture = emscripten.WebGPU.importJsTexture(target.context.getCurrentTexture());
  return editor.paintWebgpu(texture, target.bgra, originX, originY, target.canvas.height, dpr);
}

/** Creates a WebGL 2 context on `canvas` for `editor` (one per canvas); pass the result to `paintWebGl`. */
export function webGlTarget(editor, canvas) {
  const { GL } = emscripten;
  const handle = GL.createContext(canvas, { majorVersion: 2, minorVersion: 0, alpha: 1, depth: 0, stencil: 8, antialias: 0, premultipliedAlpha: 1 });
  if (!handle) throw new Error('WebGL 2 is unavailable');
  GL.makeContextCurrent(handle);
  if (!editor.attachWebgl()) throw new Error('the engine could not use the WebGL context');
  return { canvas, handle };
}

/** Paints `editor` into its WebGL canvas; the page stack's top-left at (`originX`, `originY`) CSS px. */
export function paintWebGl(editor, target, originX, originY, dpr = globalThis.devicePixelRatio ?? 1) {
  emscripten.GL.makeContextCurrent(target.handle);
  return editor.paintWebgl(originX, originY, target.canvas.width, target.canvas.height, dpr);
}
