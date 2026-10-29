//! Windows GPU target: Graphite on native Dawn, presenting to the host's window through a swap chain. One device and
//! Graphite context for the process (`cpp/dawn_native.cpp`: D3D12, then D3D11, then Vulkan), a surface per window.
//! The engine thread draws and presents; every call here is on it. After a device loss, frames fail until the next
//! attach, which lets go of the context and every window's surface and makes them again on a new device.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;

use skia_safe::gpu::graphite;
use skia_safe::{Canvas, ColorType};

unsafe extern "C" {
    // cpp/dawn.cpp
    fn editor_graphite_make_dawn(instance: *mut c_void, device: *mut c_void, queue: *mut c_void) -> *mut c_void;
    fn editor_graphite_dawn_texture(out: *mut graphite::BackendTexture, texture: *mut c_void);
    fn editor_graphite_make_recorder(context: *mut c_void) -> *mut c_void;
    // cpp/dawn_native.cpp
    fn twip_dawn_device(instance: *mut *mut c_void, device: *mut *mut c_void, queue: *mut *mut c_void) -> i32;
    fn twip_dawn_lost() -> bool;
    fn twip_dawn_test_lose();
    fn twip_dawn_surface(hwnd: *mut c_void) -> *mut c_void;
    fn twip_dawn_surface_release(surface: *mut c_void);
    fn twip_dawn_configure(surface: *mut c_void, width: u32, height: u32, bgra: *mut bool) -> bool;
    fn twip_dawn_current(surface: *mut c_void) -> *mut c_void;
    fn twip_dawn_present(surface: *mut c_void) -> bool;
    // Dawn
    fn wgpuTextureRelease(texture: *mut c_void);
}

/// The GPU backend a window draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    D3D12 = 1,
    D3D11 = 2,
    Vulkan = 3,
}

/// The recorder drops before its context.
struct Graphite {
    recorder: graphite::Recorder,
    context: graphite::Context,
    backend: Backend,
}

struct Window {
    surface: *mut c_void,
    size: (u32, u32),
    bgra: bool,
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: the surface is this window's own reference.
        unsafe { twip_dawn_surface_release(self.surface) };
    }
}

#[derive(Default)]
struct State {
    /// `None` until the first attach, and again after a device loss; `Some(None)` when there is no usable GPU.
    graphite: Option<Option<Graphite>>,
    windows: HashMap<u64, Window>,
    /// A device was lost: an attach while there is no usable GPU tries to make one again.
    lost: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

fn graphite(state: &mut State) -> Option<&mut Graphite> {
    // SAFETY: no pointers; asks the device's flag.
    let lost = unsafe { twip_dawn_lost() };
    if lost || (state.lost && matches!(state.graphite, Some(None))) {
        // What was made with the lost device goes before it does: the next device is made below.
        state.windows.clear();
        state.graphite = None;
        state.lost = true;
    }
    state
        .graphite
        .get_or_insert_with(|| {
            let (mut instance, mut device, mut queue) = (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            // SAFETY: plain out-pointers.
            let backend = match unsafe { twip_dawn_device(&mut instance, &mut device, &mut queue) } {
                1 => Backend::D3D12,
                2 => Backend::D3D11,
                3 => Backend::Vulkan,
                _ => return None,
            };
            // SAFETY: graphite::Context is a repr(transparent) owning pointer to skgpu::graphite::Context (null ->
            // None); the handles are the process's live device.
            let context: Option<graphite::Context> = unsafe { std::mem::transmute(editor_graphite_make_dawn(instance, device, queue)) };
            let context = context?;
            // SAFETY: as above, for graphite::Recorder. Its image provider uploads pictures (cpp/dawn.cpp).
            let recorder: Option<graphite::Recorder> =
                unsafe { std::mem::transmute(editor_graphite_make_recorder(std::mem::transmute_copy(&context))) };
            Some(Graphite { context, recorder: recorder?, backend })
        })
        .as_mut()
}

/// Testing: loses the GPU device as a driver reset would. Frames fail `unavailable` until the next attach.
pub fn lose_device() {
    // SAFETY: no pointers.
    unsafe { twip_dawn_test_lose() };
}

/// Draws `key`'s frames into the window `hwnd` from now on. The backend, or `None` with no usable GPU (draw with the
/// CPU then).
/// # Safety
/// `hwnd` is a live window that outlives the attach ([`detach`], or the session's end).
pub unsafe fn attach(key: u64, hwnd: *mut c_void) -> Option<Backend> {
    STATE.with_borrow_mut(|s| {
        let backend = graphite(s)?.backend;
        // SAFETY: the host's promise: a live window handle.
        let surface = unsafe { twip_dawn_surface(hwnd) };
        if surface.is_null() {
            return None;
        }
        s.windows.insert(key, Window { surface, size: (0, 0), bgra: true });
        Some(backend)
    })
}

/// Stops drawing `key`'s frames into its window.
pub fn detach(key: u64) {
    STATE.with_borrow_mut(|s| s.windows.remove(&key));
}

/// Draws a frame with `draw` into `key`'s window, `width` x `height` pixels, and presents it. False when the window
/// isn't attached or the GPU can't draw (the device was lost): draw with the CPU then.
pub fn frame(key: u64, width: u32, height: u32, draw: impl FnOnce(&Canvas)) -> bool {
    STATE.with_borrow_mut(|s| {
        let State { graphite: g, windows, .. } = s;
        let (Some(Some(g)), Some(w)) = (g.as_mut(), windows.get_mut(&key)) else { return false };
        // SAFETY: no pointers.
        if unsafe { twip_dawn_lost() } || width == 0 || height == 0 {
            return false;
        }
        // SAFETY: the window's live surface, and a plain out-pointer.
        if w.size != (width, height) && !unsafe { twip_dawn_configure(w.surface, width, height, &mut w.bgra) } {
            return false;
        }
        w.size = (width, height);
        // SAFETY: as above.
        let mut texture = unsafe { twip_dawn_current(w.surface) };
        if texture.is_null() {
            // The swap chain went stale (a display change): configure it again, once.
            // SAFETY: as above.
            if !unsafe { twip_dawn_configure(w.surface, width, height, &mut w.bgra) } {
                return false;
            }
            texture = unsafe { twip_dawn_current(w.surface) };
            if texture.is_null() {
                return false;
            }
        }
        let mut target = graphite::BackendTexture::new();
        // SAFETY: a live texture this call owns.
        unsafe { editor_graphite_dawn_texture(&mut target, texture) };
        let color = if w.bgra { ColorType::BGRA8888 } else { ColorType::RGBA8888 };
        let drawn = match graphite::surfaces::wrap_backend_texture(&mut g.recorder, &target, color, None, None) {
            Some(mut surface) => {
                draw(surface.canvas());
                drop(surface);
                g.recorder.snap().is_some_and(|mut recording| {
                    g.context.insert_recording(&graphite::InsertRecordingInfo::new(&mut recording)) == graphite::InsertStatus::Success
                        && g.context.submit(None)
                })
            }
            None => false,
        };
        // SAFETY: as above; presenting hands the texture back to the swap chain.
        let presented = drawn && unsafe { twip_dawn_present(w.surface) };
        unsafe { wgpuTextureRelease(texture) };
        presented
    })
}
