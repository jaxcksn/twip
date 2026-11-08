//! Native GPU target: Graphite presenting to the host's window. On Windows, Graphite on native Dawn through a swap
//! chain (`cpp/dawn_native.cpp`: D3D12, then D3D11, then Vulkan); on macOS, Graphite on Metal into the host's
//! `CAMetalLayer` (`cpp/metal.mm`). One device and Graphite context for the process, a surface per window.
//! The engine thread draws and presents; every call here is on it. After a device loss, frames fail until the next
//! attach, which lets go of the context and every window's surface and makes them again on a new device.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;

use skia_safe::gpu::graphite;
use skia_safe::{Canvas, ColorType};

unsafe extern "C" {
    // cpp/graphite.cpp
    fn editor_graphite_make_recorder(context: *mut c_void) -> *mut c_void;
}

/// The GPU backend a window draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(target_os = "macos", allow(dead_code, reason = "Dawn's backends are Windows'"))]
pub enum Backend {
    D3D12 = 1,
    D3D11 = 2,
    Vulkan = 3,
    Metal = 4,
}

/// The recorder drops before its context, and the context before the device it was made on.
struct Graphite {
    recorder: graphite::Recorder,
    context: graphite::Context,
    #[allow(dead_code, reason = "kept alive for the context")]
    device: native::Device,
    backend: Backend,
}

struct Window {
    surface: native::Surface,
    size: (u32, u32),
    bgra: bool,
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

fn lost(graphite: &Option<Option<Graphite>>) -> bool {
    native::lost() || matches!(graphite, Some(Some(g)) if g.context.is_device_lost())
}

fn graphite(state: &mut State) -> Option<&mut Graphite> {
    if lost(&state.graphite) || (state.lost && matches!(state.graphite, Some(None))) {
        // What was made with the lost device goes before it does: the next device is made below.
        state.windows.clear();
        state.graphite = None;
        state.lost = true;
    }
    state
        .graphite
        .get_or_insert_with(|| {
            let (context, backend, device) = native::device()?;
            // SAFETY: graphite::Recorder is a repr(transparent) owning pointer to skgpu::graphite::Recorder (null ->
            // None), and graphite::Context one to the live context. Its image provider uploads pictures
            // (cpp/graphite.cpp).
            let recorder: Option<graphite::Recorder> =
                unsafe { std::mem::transmute(editor_graphite_make_recorder(std::mem::transmute_copy(&context))) };
            Some(Graphite { context, recorder: recorder?, device, backend })
        })
        .as_mut()
}

/// Testing: loses the GPU device as a driver reset would. Frames fail `unavailable` until the next attach.
pub fn lose_device() {
    native::test_lose();
}

/// Draws `key`'s frames into `window` from now on: an `HWND` on Windows, a `CAMetalLayer` on macOS. The backend, or
/// `None` with no usable GPU (draw with the CPU then).
/// # Safety
/// `window` is a live window or layer that outlives the attach ([`detach`], or the session's end).
pub unsafe fn attach(key: u64, window: *mut c_void) -> Option<Backend> {
    STATE.with_borrow_mut(|s| {
        let g = graphite(s)?;
        let backend = g.backend;
        // SAFETY: the host's promise: a live window handle or layer.
        let surface = unsafe { native::Surface::new(window, &g.device) }?;
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
        if lost(&s.graphite) || width == 0 || height == 0 {
            return false;
        }
        let State { graphite: g, windows, .. } = s;
        let (Some(Some(g)), Some(w)) = (g.as_mut(), windows.get_mut(&key)) else { return false };
        let _pool = native::Pool::new();
        if w.size != (width, height) {
            let Some(bgra) = w.surface.configure(width, height) else { return false };
            w.bgra = bgra;
        }
        w.size = (width, height);
        let target = match w.surface.next() {
            Some(target) => target,
            None => {
                // The swap chain went stale (a display change): configure it again, once.
                let Some(bgra) = w.surface.configure(width, height) else { return false };
                w.bgra = bgra;
                let Some(target) = w.surface.next() else { return false };
                target
            }
        };
        let color = if w.bgra { ColorType::BGRA8888 } else { ColorType::RGBA8888 };
        let drawn = match graphite::surfaces::wrap_backend_texture(&mut g.recorder, target.texture(), color, None, None) {
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
        drawn && target.present(&w.surface, &g.device)
    })
}

/// Windows: Graphite on native Dawn (`cpp/dawn.cpp`, `cpp/dawn_native.cpp`), which keeps the process's device.
#[cfg(not(target_os = "macos"))]
mod native {
    use super::*;

    unsafe extern "C" {
        // cpp/dawn.cpp
        fn editor_graphite_make_dawn(instance: *mut c_void, device: *mut c_void, queue: *mut c_void) -> *mut c_void;
        fn editor_graphite_dawn_texture(out: *mut graphite::BackendTexture, texture: *mut c_void);
        // cpp/dawn_native.cpp
        fn twip_dawn_device(instance: *mut *mut c_void, device: *mut *mut c_void, queue: *mut *mut c_void) -> i32;
        fn twip_dawn_lost() -> bool;
        fn twip_dawn_test_lose();
        fn twip_dawn_surface(window: *mut c_void) -> *mut c_void;
        fn twip_dawn_surface_release(surface: *mut c_void);
        fn twip_dawn_configure(surface: *mut c_void, width: u32, height: u32, bgra: *mut bool) -> bool;
        fn twip_dawn_current(surface: *mut c_void) -> *mut c_void;
        fn twip_dawn_present(surface: *mut c_void) -> bool;
        // Dawn
        fn wgpuTextureRelease(texture: *mut c_void);
    }

    /// cpp/dawn_native.cpp holds the device.
    pub struct Device;

    /// Nothing to drain: Dawn has no autorelease pool.
    pub struct Pool;

    impl Pool {
        pub fn new() -> Self {
            Pool
        }
    }

    pub fn device() -> Option<(graphite::Context, Backend, Device)> {
        let (mut instance, mut device, mut queue) = (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: plain out-pointers.
        let backend = match unsafe { twip_dawn_device(&mut instance, &mut device, &mut queue) } {
            1 => Backend::D3D12,
            2 => Backend::D3D11,
            3 => Backend::Vulkan,
            4 => Backend::Metal,
            _ => return None,
        };
        // SAFETY: graphite::Context is a repr(transparent) owning pointer to skgpu::graphite::Context (null -> None);
        // the handles are the process's live device.
        let context: Option<graphite::Context> = unsafe { std::mem::transmute(editor_graphite_make_dawn(instance, device, queue)) };
        Some((context?, backend, Device))
    }

    pub fn lost() -> bool {
        // SAFETY: no pointers; asks the device's flag.
        unsafe { twip_dawn_lost() }
    }

    pub fn test_lose() {
        // SAFETY: no pointers.
        unsafe { twip_dawn_test_lose() };
    }

    pub struct Surface(*mut c_void);

    impl Surface {
        /// # Safety
        /// `window` is a live `HWND`.
        pub unsafe fn new(window: *mut c_void, _: &Device) -> Option<Self> {
            // SAFETY: the caller's promise.
            let surface = unsafe { twip_dawn_surface(window) };
            (!surface.is_null()).then_some(Surface(surface))
        }

        /// Sizes the swap chain: whether its textures are BGRA (else RGBA), or `None` when it can't be.
        pub fn configure(&mut self, width: u32, height: u32) -> Option<bool> {
            let mut bgra = true;
            // SAFETY: the window's live surface, and a plain out-pointer.
            unsafe { twip_dawn_configure(self.0, width, height, &mut bgra) }.then_some(bgra)
        }

        /// The texture to draw the next frame into, or `None` when the swap chain needs configuring again.
        pub fn next(&mut self) -> Option<Target> {
            // SAFETY: as above.
            let texture = unsafe { twip_dawn_current(self.0) };
            if texture.is_null() {
                return None;
            }
            let mut target = graphite::BackendTexture::new();
            // SAFETY: a live texture the target owns.
            unsafe { editor_graphite_dawn_texture(&mut target, texture) };
            Some(Target { texture, target })
        }
    }

    impl Drop for Surface {
        fn drop(&mut self) {
            // SAFETY: the surface is this window's own reference.
            unsafe { twip_dawn_surface_release(self.0) };
        }
    }

    pub struct Target {
        texture: *mut c_void,
        target: graphite::BackendTexture,
    }

    impl Target {
        pub fn texture(&self) -> &graphite::BackendTexture {
            &self.target
        }

        /// Presenting hands the texture back to the swap chain.
        pub fn present(self, surface: &Surface, _: &Device) -> bool {
            // SAFETY: the window's live surface.
            unsafe { twip_dawn_present(surface.0) }
        }
    }

    impl Drop for Target {
        fn drop(&mut self) {
            // SAFETY: the texture's own reference.
            unsafe { wgpuTextureRelease(self.texture) };
        }
    }
}

/// macOS: Graphite on Metal (`cpp/metal.mm`), into the host's `CAMetalLayer`.
#[cfg(target_os = "macos")]
mod native {
    use std::cell::Cell;

    use skia_safe::gpu::graphite::mtl;

    use super::*;

    unsafe extern "C" {
        // cpp/metal.mm
        fn twip_metal_device(device: *mut *mut c_void, queue: *mut *mut c_void) -> bool;
        fn twip_metal_release(object: *mut c_void);
        fn twip_metal_layer(layer: *mut c_void, device: *mut c_void) -> *mut c_void;
        fn twip_metal_resize(layer: *mut c_void, width: u32, height: u32);
        fn twip_metal_next(layer: *mut c_void, texture: *mut *mut c_void, width: *mut u32, height: *mut u32) -> *mut c_void;
        fn twip_metal_present(queue: *mut c_void, drawable: *mut c_void) -> bool;
        // The Objective-C runtime: what `@autoreleasepool` compiles to.
        fn objc_autoreleasePoolPush() -> *mut c_void;
        fn objc_autoreleasePoolPop(pool: *mut c_void);
    }

    thread_local! {
        /// [`test_lose`] was called: frames fail until the next device.
        static LOST: Cell<bool> = const { Cell::new(false) };
    }

    /// The process's device and command queue, held while the context made on them lives.
    pub struct Device {
        device: *mut c_void,
        queue: *mut c_void,
    }

    impl Drop for Device {
        fn drop(&mut self) {
            // SAFETY: this device's own references.
            unsafe {
                twip_metal_release(self.queue);
                twip_metal_release(self.device);
            }
        }
    }

    /// An autorelease pool around a frame: the engine thread has no run loop to drain what Metal autoreleases.
    pub struct Pool(*mut c_void);

    impl Pool {
        pub fn new() -> Self {
            // SAFETY: no arguments; popped in order on this thread.
            Pool(unsafe { objc_autoreleasePoolPush() })
        }
    }

    impl Drop for Pool {
        fn drop(&mut self) {
            // SAFETY: the pool this pushed, the innermost on this thread.
            unsafe { objc_autoreleasePoolPop(self.0) };
        }
    }

    /// A new device each time: after a loss, the next attach starts over.
    pub fn device() -> Option<(graphite::Context, Backend, Device)> {
        LOST.set(false);
        let (mut device, mut queue) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: plain out-pointers.
        if !unsafe { twip_metal_device(&mut device, &mut queue) } {
            return None;
        }
        let held = Device { device, queue };
        // SAFETY: a live device and queue; the backend context retains them.
        let backend = unsafe { mtl::BackendContext::new(device, queue) };
        let context = mtl::context_factory::make_metal(&backend, None)?;
        Some((context, Backend::Metal, held))
    }

    pub fn lost() -> bool {
        LOST.get()
    }

    pub fn test_lose() {
        LOST.set(true);
    }

    /// The host's layer, retained.
    pub struct Surface(*mut c_void);

    impl Surface {
        /// # Safety
        /// `window` is a live `CAMetalLayer` (anything else is refused).
        pub unsafe fn new(window: *mut c_void, device: &Device) -> Option<Self> {
            // SAFETY: the caller's promise, and the live device.
            let layer = unsafe { twip_metal_layer(window, device.device) };
            (!layer.is_null()).then_some(Surface(layer))
        }

        /// Sizes the layer's drawables: always BGRA.
        pub fn configure(&mut self, width: u32, height: u32) -> Option<bool> {
            // SAFETY: the window's live layer.
            unsafe { twip_metal_resize(self.0, width, height) };
            Some(true)
        }

        /// The next drawable, or `None` when none came in time (the display is asleep, the window hidden).
        pub fn next(&mut self) -> Option<Target> {
            let (mut texture, mut width, mut height) = (std::ptr::null_mut(), 0, 0);
            // SAFETY: the live layer, and plain out-pointers.
            let drawable = unsafe { twip_metal_next(self.0, &mut texture, &mut width, &mut height) };
            if drawable.is_null() {
                return None;
            }
            // SAFETY: the drawable's texture, alive while the target holds the drawable.
            let target = unsafe { mtl::backend_textures::make_metal((width as i32, height as i32), texture) };
            Some(Target { drawable, target })
        }
    }

    impl Drop for Surface {
        fn drop(&mut self) {
            // SAFETY: this window's own reference.
            unsafe { twip_metal_release(self.0) };
        }
    }

    pub struct Target {
        drawable: *mut c_void,
        target: graphite::BackendTexture,
    }

    impl Target {
        pub fn texture(&self) -> &graphite::BackendTexture {
            &self.target
        }

        /// Presents after the frame's GPU work.
        pub fn present(self, _: &Surface, device: &Device) -> bool {
            // SAFETY: the live queue and drawable.
            unsafe { twip_metal_present(device.queue, self.drawable) }
        }
    }

    impl Drop for Target {
        fn drop(&mut self) {
            // SAFETY: the drawable's own reference.
            unsafe { twip_metal_release(self.drawable) };
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use skia_safe::gpu::Mipmapped;
    use skia_safe::{AlphaType, Color, Data, EncodedImageFormat, Image, ImageInfo, Paint, Rect, images};

    use super::*;

    /// Graphite on Metal draws a decoded picture: pictures reach the GPU through cpp/graphite.cpp's image provider, so
    /// this fails if the shim and the prebuilt Skia disagree.
    #[test]
    fn draws_pictures_with_metal() {
        STATE.with_borrow_mut(|s| {
            let Some(g) = graphite(s) else {
                eprintln!("no Metal device: skipped");
                return;
            };
            assert_eq!(g.backend, Backend::Metal);
            let red = ImageInfo::new((2, 2), ColorType::RGBA8888, AlphaType::Premul, None);
            let raster = images::raster_from_data(&red, Data::new_copy(&[0xFF, 0, 0, 0xFF].repeat(4)), 8).expect("raster");
            let png = raster.encode(None, EncodedImageFormat::PNG, None).expect("PNG");
            let picture = Image::from_encoded(png).expect("decoded lazily");

            let info = ImageInfo::new((32, 32), ColorType::RGBA8888, AlphaType::Premul, None);
            let mut surface = graphite::surfaces::render_target(&mut g.recorder, &info, Mipmapped::No, None, None).expect("target");
            surface.canvas().clear(Color::WHITE);
            surface.canvas().draw_image_rect(&picture, None, Rect::from_wh(32.0, 32.0), &Paint::default());
            let mut recording = g.recorder.snap().expect("recording");
            assert_eq!(g.context.insert_recording(&graphite::InsertRecordingInfo::new(&mut recording)), graphite::InsertStatus::Success);
            assert!(g.context.submit_and_wait());
            let mut pixels = vec![0u8; 32 * 32 * 4];
            assert!(g.context.read_pixels(&mut surface, &info, &mut pixels, 32 * 4, (0, 0)));
            let centre = &pixels[(16 * 32 + 16) * 4..][..4];
            assert_eq!(centre, [0xFF, 0, 0, 0xFF], "the picture, not the white under it");
        });
    }
}
