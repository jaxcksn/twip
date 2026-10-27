// Replaces Diplomat's loader (which instantiates a bare wasm32-unknown-unknown module): the engine is an emscripten
// module, so its exports live on the emscripten Module as `_name`, and its memory can grow. Copied over the
// generated diplomat-wasm.mjs by `moon run web:generate`.
import createTwip from './twip-wasm.js';

const module = await createTwip();

/** The emscripten module itself: its `WebGPU` and `GL` helpers import browser GPU objects (gpu.mjs). */
export const emscripten = module;

const wasm = new Proxy({}, {
    get(_, name) {
        if (name === 'memory') return { get buffer() { return module.HEAPU8.buffer; } };
        return module[`_${String(name)}`];
    },
});

export default wasm;
