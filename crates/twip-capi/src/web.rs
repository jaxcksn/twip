//! The browser module's host: the engine's callbacks go to the page's `globalThis.twipHost.emit(kind, session, ptr,
//! len)` (the SDK sets it before it starts the module; the payload is readable in the module's memory during the call),
//! and the engine's clocks are the page's `performance.now()` and `Date.now()`. The GPU screen draws into the page's
//! canvases: [`twip_gpu_start`] makes the WebGPU device, [`twip_attach_canvas`] gives a session its canvas, and
//! `twip_frame_window` draws into it. Without WebGPU, [`twip_webgl_context`] makes a canvas its WebGL 2 context and
//! [`twip_attach_webgl`] gives it to the session.

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
    let Some(target) = surface_target(canvas) else { return status(ErrorCode::InvalidArgument) };
    match guard(engine as *mut TwipEngine, session, |e| {
        e.session(session)?.host().attach_canvas(target).ok_or_else(|| Error::unavailable("no GPU to draw with"))
    }) {
        Ok(backend) => backend as i32,
        Err(e) => status(e.code),
    }
}

/// For a page without WebGPU: a WebGL 2 context of `canvas`'s own (an `HTMLCanvasElement` or `OffscreenCanvas`), as a
/// handle for [`twip_attach_webgl`]; 0 when the canvas has no hardware WebGL 2. It touches no engine, so it may wait on
/// the browser while the engine runs other calls.
#[wasm_bindgen]
pub async fn twip_webgl_context(canvas: JsValue) -> u32 {
    match surface_target(canvas) {
        Some(target) => window::webgl_context(target).await,
        None => 0,
    }
}

/// Draws the session's frames with WebGL 2 into the canvas of `handle` ([`twip_webgl_context`]) from now on: the backend
/// (6 WebGL), or `unavailable` for a handle that isn't pending. A handle the session doesn't take is let go of.
#[wasm_bindgen]
pub fn twip_attach_webgl(engine: usize, session: u32, handle: u32) -> i32 {
    match guard(engine as *mut TwipEngine, session, |e| {
        e.session(session)?.host().attach_webgl(handle).ok_or_else(|| Error::unavailable("no WebGL context to draw with"))
    }) {
        Ok(backend) => backend as i32,
        Err(e) => {
            window::forget_webgl(handle);
            status(e.code)
        }
    }
}

/// Lets go of the WebGL context of `handle` ([`twip_webgl_context`]) when no session is to take it.
#[wasm_bindgen]
pub fn twip_webgl_forget(handle: u32) {
    window::forget_webgl(handle);
}

/// A canvas from the page as a surface target: an `HTMLCanvasElement` or an `OffscreenCanvas`.
fn surface_target(canvas: JsValue) -> Option<wgpu::SurfaceTarget<'static>> {
    match canvas.dyn_into::<web_sys::HtmlCanvasElement>() {
        Ok(c) => Some(wgpu::SurfaceTarget::Canvas(c)),
        Err(other) => other.dyn_into::<web_sys::OffscreenCanvas>().ok().map(wgpu::SurfaceTarget::OffscreenCanvas),
    }
}
