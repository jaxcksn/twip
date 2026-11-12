//! GPU targets: twip-graphics on wgpu, presenting to the host's window or the page's canvas. Natively into an `HWND` on
//! Windows (D3D12, else Vulkan) or the host's `CAMetalLayer` on macOS (Metal); in the browser into a canvas, on the
//! page's WebGPU. One device; a surface, and a renderer for its format, per window. Natively the state is the
//! process's, behind a lock, and never dropped (wgpu can't let go of a device while a thread's locals are being
//! destroyed); in the browser, whose WebGPU objects stay on their thread, it is the thread's. After a device loss,
//! frames fail until a new device: natively the next attach makes one, letting go of every window's surface; in the
//! browser [`start`] does, and the page attaches its canvases again.

use std::collections::HashMap;

/// The wgpu the screen draws with: the browser's host hands its canvases over as its surface targets.
pub use twip_graphics::gpu::wgpu;
use twip_graphics::gpu::{Context, Renderer};

use super::graphics::Graphics;

/// The GPU backend a window draws with (2 was D3D11, which wgpu doesn't have).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    D3D12 = 1,
    Vulkan = 3,
    Metal = 4,
    /// The browser's WebGPU.
    WebGpu = 5,
}

struct Window {
    surface: wgpu::Surface<'static>,
    /// The size the surface is configured at; `(0, 0)` until the first frame, and when it is to be configured again.
    size: (u32, u32),
    /// The renderer for the surface's format, made when it is first configured.
    renderer: Option<(wgpu::TextureFormat, Renderer)>,
}

impl Window {
    /// Configures the surface at `width × height`: false when the adapter can't draw to it.
    fn configure(&mut self, ctx: &Context, width: u32, height: u32) -> bool {
        let Some(config) = ctx.surface_config(&self.surface, width, height) else { return false };
        self.surface.configure(&ctx.device, &config);
        if self.renderer.as_ref().is_none_or(|(f, _)| *f != config.format) {
            self.renderer = Some((config.format, Renderer::new(ctx, config.format)));
        }
        self.size = (width, height);
        true
    }
}

#[derive(Default)]
struct State {
    /// `None` until the first device, and again after a device loss; `Some(None)` when there is no usable GPU.
    context: Option<Option<Context>>,
    windows: HashMap<u64, Window>,
    /// Offscreen frames' renderers ([`pixels`]), by session.
    #[cfg_attr(target_family = "wasm", allow(dead_code, reason = "offscreen frames are native"))]
    offscreen: HashMap<u64, Renderer>,
    /// A device was lost: an attach while there is no usable GPU tries to make one again.
    lost: bool,
}

impl State {
    /// Lets go of what was made with a lost device, so the next device can be made; whether one was lost.
    fn forget_lost(&mut self) -> bool {
        let lost = matches!(&self.context, Some(Some(c)) if c.is_lost());
        if lost || (self.lost && matches!(self.context, Some(None))) {
            self.windows.clear();
            self.offscreen.clear();
            self.context = None;
            self.lost = true;
        }
        self.context.is_none()
    }

    fn backend(&self) -> Option<Backend> {
        let Some(Some(ctx)) = &self.context else { return None };
        Some(match ctx.adapter.get_info().backend {
            wgpu::Backend::Dx12 => Backend::D3D12,
            wgpu::Backend::Metal => Backend::Metal,
            wgpu::Backend::BrowserWebGpu => Backend::WebGpu,
            _ => Backend::Vulkan,
        })
    }

    /// Keeps `surface` for `key`'s frames when the device can draw to it: the backend.
    fn attach(&mut self, key: u64, surface: wgpu::Surface<'static>) -> Option<Backend> {
        let Some(Some(ctx)) = &self.context else { return None };
        if !ctx.adapter.is_surface_supported(&surface) {
            return None;
        }
        self.windows.insert(key, Window { surface, size: (0, 0), renderer: None });
        self.backend()
    }
}

#[cfg(not(target_family = "wasm"))]
mod platform {
    use std::ffi::c_void;
    use std::sync::{LazyLock, Mutex, PoisonError};

    use twip_graphics::gpu::{Context, block_on, wgpu};

    use super::{Backend, State};

    static STATE: LazyLock<Mutex<State>> = LazyLock::new(Mutex::default);

    /// Runs `f` on the state, locked. A panic while it was held (a bug, in a debug build) leaves it as it was.
    pub(super) fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut STATE.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// The device, made (fitting `surface`, if given, from `instance`) when there is none.
    pub(super) fn ensure(s: &mut State, surface: Option<&wgpu::Surface<'_>>, instance: impl FnOnce() -> wgpu::Instance) {
        if s.forget_lost() {
            s.context = Some(block_on(Context::new(instance(), surface)).ok());
        }
    }

    /// A surface on `window` (an `HWND` on Windows, a `CAMetalLayer` on macOS), made with `instance`.
    /// # Safety
    /// `window` is a live window or layer that outlives the surface.
    unsafe fn surface(instance: &wgpu::Instance, window: *mut c_void) -> Option<wgpu::Surface<'static>> {
        #[cfg(windows)]
        let target = {
            use wgpu::rwh::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};
            let hwnd = std::num::NonZeroIsize::new(window as isize)?;
            wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
                raw_window_handle: RawWindowHandle::Win32(Win32WindowHandle::new(hwnd)),
            }
        };
        #[cfg(target_os = "macos")]
        let target = wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(window);
        // SAFETY: the caller's promise: a live window or layer, outliving the surface.
        unsafe { instance.create_surface_unsafe(target) }.ok()
    }

    /// Draws `key`'s frames into `window` from now on: an `HWND` on Windows, a `CAMetalLayer` on macOS. The backend, or
    /// `None` with no usable GPU (draw with the CPU then).
    /// # Safety
    /// `window` is a live window or layer that outlives the attach ([`super::detach`], or the session's end).
    pub unsafe fn attach(key: u64, window: *mut c_void) -> Option<Backend> {
        with_state(|s| {
            let instance = match &s.context {
                Some(Some(c)) if !c.is_lost() => c.instance.clone(),
                _ => Context::instance(),
            };
            // SAFETY: the host's promise.
            let surface = unsafe { surface(&instance, window) }?;
            ensure(s, Some(&surface), || instance);
            s.attach(key, surface)
        })
    }

    /// Testing: loses the GPU device as a driver reset would. Frames fail `unavailable` until the next attach.
    pub fn lose_device() {
        with_state(|s| {
            if let Some(Some(c)) = &s.context {
                c.device.destroy();
                // The loss is reported while the device is polled.
                let _ = c.device.poll(wgpu::PollType::wait_indefinitely());
            }
        });
    }
}

#[cfg(target_family = "wasm")]
mod platform {
    use std::cell::RefCell;

    use twip_graphics::gpu::{Context, wgpu};

    use super::{Backend, State};

    thread_local! {
        static STATE: RefCell<State> = RefCell::default();
    }

    pub(super) fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with_borrow_mut(f)
    }

    /// Makes the page's device when there is none (or it was lost): 0 when there is none to draw with, 1 when the one
    /// there is kept, 2 when a new one was made (the page attaches its canvases again then).
    /// `on_lost` is called when the new device is lost.
    pub async fn start(on_lost: fn()) -> u32 {
        if !with_state(State::forget_lost) {
            return with_state(|s| u32::from(matches!(s.context, Some(Some(_)))));
        }
        let context = Context::new(Context::instance(), None).await.ok();
        if let Some(c) = &context {
            c.on_lost(on_lost);
        }
        with_state(|s| {
            let made = if context.is_some() { 2 } else { 0 };
            s.context = Some(context);
            made
        })
    }

    /// Draws `key`'s frames into `canvas` from now on (an `HTMLCanvasElement` or `OffscreenCanvas`); `None` before
    /// [`start`] made a device, or when the canvas has no WebGPU context to give.
    pub fn attach_canvas(key: u64, canvas: wgpu::SurfaceTarget<'static>) -> Option<Backend> {
        with_state(|s| {
            let Some(Some(ctx)) = &s.context else { return None };
            let surface = ctx.instance.create_surface(canvas).ok()?;
            s.attach(key, surface)
        })
    }

    /// Testing: loses the GPU device as a driver reset would. Frames fail `unavailable` until [`start`] again.
    pub fn lose_device() {
        with_state(|s| {
            if let Some(Some(c)) = &s.context {
                c.device.destroy();
            }
        });
    }
}

pub use platform::*;

use platform::with_state;

/// Stops drawing `key`'s frames into its window, and forgets its offscreen frames' renderer.
pub fn detach(key: u64) {
    with_state(|s| {
        s.windows.remove(&key);
        s.offscreen.remove(&key);
    });
}

/// Draws `graphics`' recorded frame into `key`'s window, `width` x `height` pixels, and presents it; the next frame is
/// recorded afresh either way. False when the window isn't attached or the GPU can't draw (the device was lost): draw
/// with the CPU then.
pub fn frame(key: u64, width: u32, height: u32, graphics: &mut Graphics) -> bool {
    graphics.finish(|res, frame| {
        with_state(|s| {
            let State { context: Some(Some(ctx)), windows, .. } = s else { return false };
            let Some(w) = windows.get_mut(&key) else { return false };
            if ctx.is_lost() || width == 0 || height == 0 {
                return false;
            }
            if w.size != (width, height) && !w.configure(ctx, width, height) {
                return false;
            }
            use wgpu::CurrentSurfaceTexture as Got;
            let texture = match w.surface.get_current_texture() {
                Got::Success(t) => t,
                // Drawn, and configured again for the next frame.
                Got::Suboptimal(t) => {
                    w.size = (0, 0);
                    t
                }
                // The swap chain went stale (a display change): configure it again, once.
                Got::Outdated | Got::Lost => {
                    if !w.configure(ctx, width, height) {
                        return false;
                    }
                    match w.surface.get_current_texture() {
                        Got::Success(t) | Got::Suboptimal(t) => t,
                        _ => return false,
                    }
                }
                // No texture in time (the window is hidden, the display asleep), or a texture wgpu refused.
                Got::Timeout | Got::Occluded | Got::Validation => return false,
            };
            let Some((_, renderer)) = w.renderer.as_mut() else { return false };
            let Ok(prepared) = renderer.prepare(ctx, res, frame, (width, height)) else { return false };
            let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            renderer.render(&mut encoder, &view, &prepared);
            ctx.queue.submit([encoder.finish()]);
            ctx.queue.present(texture);
            !ctx.is_lost()
        })
    })
}

/// Tests and tools: `graphics`' recorded frame drawn on the GPU at `width` x `height` and read back (premultiplied
/// RGBA, rows packed), with `key`'s own renderer; `None` with no usable GPU, or when the device was lost.
#[cfg(not(target_family = "wasm"))]
pub fn pixels(key: u64, width: u32, height: u32, graphics: &mut Graphics) -> Option<Vec<u8>> {
    graphics.finish(|res, frame| {
        with_state(|s| {
            platform::ensure(s, None, Context::instance);
            let State { context: Some(Some(ctx)), offscreen, .. } = s else { return None };
            let renderer = offscreen.entry(key).or_insert_with(|| Renderer::new(ctx, wgpu::TextureFormat::Rgba8Unorm));
            renderer.draw_pixels(ctx, res, frame, (width, height)).ok()
        })
    })
}
