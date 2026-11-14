//! The browser module's host: the engine's callbacks go to the page's `globalThis.twipHost.emit(kind, session, ptr,
//! len)` (the SDK sets it before it starts the module; the payload is readable in the module's memory during the call),
//! and the engine's clocks are the page's `performance.now()` and `Date.now()`. The GPU screen draws into the page's
//! canvases: [`twip_gpu_start`] makes the device, [`twip_attach_canvas`] gives a session its canvas, and
//! `twip_frame_window` draws into it.

use engine::api::error::{Error, ErrorCode};
use engine::internal::render::window::{self, wgpu};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

use crate::{TwipEngine, guard, status};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = twipHost, js_name = emit)]
    fn host_emit(kind: u32, session: u32, ptr: usize, len: usize);
    /// The engine's GPU device was lost: the page starts a new one (or draws CPU pixels).
    #[wasm_bindgen(js_namespace = twipHost, js_name = gpuLost)]
    fn host_gpu_lost();
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now() -> f64;
}

/// Hands a callback's payload to the page.
pub fn emit(kind: u32, session: u32, payload: &[u8]) {
    host_emit(kind, session, payload.as_ptr() as usize, payload.len());
}

/// Gives the engine the page's clocks.
pub fn clocks() {
    engine::internal::clock::set(performance_now, date_now);
}

/// Makes the page's WebGPU device when there is none, or it was lost: 0 when there is none to draw with, 1 when the one
/// there is kept, 2 when a new one was made. Canvases are attached after it, and attached again after a new one.
#[wasm_bindgen]
pub async fn twip_gpu_start() -> u32 {
    window::start(host_gpu_lost).await
}

/// Draws the session's frames into `canvas` (an `HTMLCanvasElement` or `OffscreenCanvas`) with the GPU from now on: the
/// backend (5 WebGPU), or `unavailable` before [`twip_gpu_start`] made a device or when the canvas can't be drawn into
/// (draw with `twip_frame_rgba` then). `engine` is from `twip_engine_new`, as every call takes it.
#[wasm_bindgen]
pub fn twip_attach_canvas(engine: usize, session: u32, canvas: JsValue) -> i32 {
    let target = match canvas.dyn_into::<web_sys::HtmlCanvasElement>() {
        Ok(c) => wgpu::SurfaceTarget::Canvas(c),
        Err(other) => match other.dyn_into::<web_sys::OffscreenCanvas>() {
            Ok(c) => wgpu::SurfaceTarget::OffscreenCanvas(c),
            Err(_) => return status(ErrorCode::InvalidArgument),
        },
    };
    match guard(engine as *mut TwipEngine, session, |e| {
        e.session(session)?.host().attach_canvas(target).ok_or_else(|| Error::unavailable("no GPU to draw with"))
    }) {
        Ok(backend) => backend as i32,
        Err(e) => status(e.code),
    }
}
