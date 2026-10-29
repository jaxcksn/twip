//! Browser GPU targets. The page owns the WebGPU device or the WebGL 2 context: it attaches one, then paints each
//! frame through it. WebGPU draws with Graphite on Dawn (emdawnwebgpu, `cpp/dawn.cpp`); WebGL 2 with Ganesh.

use std::cell::RefCell;
use std::ffi::c_void;

use skia_safe::gpu::{self, DirectContext, graphite};
use skia_safe::{Canvas, ColorType, Surface};

unsafe extern "C" {
    // cpp/dawn.cpp
    fn editor_graphite_make_dawn(instance: *mut c_void, device: *mut c_void, queue: *mut c_void) -> *mut c_void;
    fn editor_graphite_dawn_texture(out: *mut graphite::BackendTexture, texture: *mut c_void);
    fn editor_graphite_make_recorder(context: *mut c_void) -> *mut c_void;
    // emdawnwebgpu
    fn wgpuCreateInstance(descriptor: *const c_void) -> *mut c_void;
    fn wgpuTextureRelease(texture: *mut c_void);
}

struct Graphite {
    context: graphite::Context,
    recorder: graphite::Recorder,
}

struct Ganesh {
    context: DirectContext,
    surface: Option<Surface>,
}

/// One Graphite context for the page's WebGPU device, and a Ganesh context per WebGL canvas, by key (an editor).
#[derive(Default)]
struct State {
    graphite: Option<Graphite>,
    ganesh: std::collections::HashMap<u64, Ganesh>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// A WebGPU instance for `Module.WebGPU.importJsDevice/importJsQueue` (emdawnwebgpu requires imported objects to
/// belong to one).
pub fn webgpu_instance() -> usize {
    unsafe { wgpuCreateInstance(std::ptr::null()) as usize }
}

/// Adopts the page's WebGPU device for every editor: emdawnwebgpu handles from `importJsDevice/importJsQueue` and
/// the instance from [`webgpu_instance`]. False when Graphite can't use them (the page falls back to WebGL).
/// # Safety
/// The handles must be live emdawnwebgpu objects.
pub unsafe fn attach_webgpu(instance: usize, device: usize, queue: usize) -> bool {
    let (instance, device, queue) = (instance as *mut c_void, device as *mut c_void, queue as *mut c_void);
    // SAFETY: graphite::Context is a repr(transparent) owning pointer to skgpu::graphite::Context (null -> None).
    let context: Option<graphite::Context> = unsafe { std::mem::transmute(editor_graphite_make_dawn(instance, device, queue)) };
    let Some(context) = context else { return false };
    // SAFETY: as above, for graphite::Recorder. Its image provider uploads pictures (cpp/dawn.cpp).
    let recorder: Option<graphite::Recorder> = unsafe { std::mem::transmute(editor_graphite_make_recorder(std::mem::transmute_copy(&context))) };
    let Some(recorder) = recorder else { return false };
    let lost = STATE.with_borrow_mut(|s| s.graphite.replace(Graphite { context, recorder }));
    // shortcut: a lost device's context keeps work that can never finish, and Skia aborts on freeing it then; it is
    // leaked instead, a context's memory for each device loss of the page (the web SDK adopts no new device after 3).
    std::mem::forget(lost);
    true
}

/// Draws a frame with `draw` into a WebGPU texture imported with `importJsTexture` (released here).
/// # Safety
/// `texture` must be a live emdawnwebgpu texture, not used again.
pub unsafe fn webgpu_frame(texture: usize, bgra: bool, draw: impl FnOnce(&Canvas)) -> bool {
    let texture = texture as *mut c_void;
    let painted = STATE.with_borrow_mut(|s| {
        let Some(g) = &mut s.graphite else { return false };
        let mut target = graphite::BackendTexture::new();
        unsafe { editor_graphite_dawn_texture(&mut target, texture) };
        let color = if bgra { ColorType::BGRA8888 } else { ColorType::RGBA8888 };
        let Some(mut surface) = graphite::surfaces::wrap_backend_texture(&mut g.recorder, &target, color, None, None) else {
            return false;
        };
        draw(surface.canvas());
        drop(surface);
        let Some(mut recording) = g.recorder.snap() else { return false };
        g.context.insert_recording(&graphite::InsertRecordingInfo::new(&mut recording)) == graphite::InsertStatus::Success && g.context.submit(None)
    });
    unsafe { wgpuTextureRelease(texture) };
    painted
}

/// Adopts the current WebGL 2 context (made current by the page) for `key`'s canvas. Again after a context loss: the
/// lost context's objects went with it, so its Ganesh context is abandoned, not freed.
pub fn attach_gl(key: u64) -> bool {
    let Some(context) = gpu::gl::Interface::new_native().and_then(|i| gpu::direct_contexts::make_gl(i, None)) else {
        return false;
    };
    let old = STATE.with_borrow_mut(|s| s.ganesh.insert(key, Ganesh { context, surface: None }));
    if let Some(mut old) = old {
        old.context.abandon();
    }
    true
}

/// Frees the WebGL state of `key`'s canvas.
pub fn detach_gl(key: u64) {
    STATE.with_borrow_mut(|s| s.ganesh.remove(&key));
}

/// Draws a frame with `draw` into the default framebuffer of `key`'s WebGL canvas (`width` x `height` pixels; its
/// context current).
pub fn gl_frame(key: u64, width: i32, height: i32, draw: impl FnOnce(&Canvas)) -> bool {
    STATE.with_borrow_mut(|s| {
        let Some(g) = s.ganesh.get_mut(&key) else { return false };
        if g.surface.as_ref().is_none_or(|surface| (surface.width(), surface.height()) != (width, height)) {
            let fb = gpu::gl::FramebufferInfo { fboid: 0, format: gpu::gl::Format::RGBA8.into(), protected: gpu::Protected::No };
            let target = gpu::backend_render_targets::make_gl((width, height), 0, 8, fb);
            g.surface =
                gpu::surfaces::wrap_backend_render_target(&mut g.context, &target, gpu::SurfaceOrigin::BottomLeft, ColorType::RGBA8888, None, None);
        }
        let Some(surface) = &mut g.surface else { return false };
        draw(surface.canvas());
        g.context.flush_and_submit_surface(surface, None);
        true
    })
}
