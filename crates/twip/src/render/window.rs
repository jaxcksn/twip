//! Native GPU target: twip-graphics on wgpu, presenting to the host's window. On Windows into an `HWND` (D3D12, else
//! Vulkan); on macOS into the host's `CAMetalLayer` (Metal). One device for the process; a surface, and a renderer for
//! its format, per window. The state is the process's, behind a lock, and never dropped: wgpu can't let go of a device
//! while a thread's locals are being destroyed. After a device loss, frames fail until the next attach, which lets go
//! of the device and every window's surface and makes them again on a new device.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use twip_graphics::gpu::{Context, Renderer, block_on, wgpu};

use super::graphics::Graphics;

/// The GPU backend a window draws with (2 was D3D11, which wgpu doesn't have).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    D3D12 = 1,
    Vulkan = 3,
    Metal = 4,
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
    /// `None` until the first attach, and again after a device loss; `Some(None)` when there is no usable GPU.
    context: Option<Option<Context>>,
    windows: HashMap<u64, Window>,
    /// Offscreen frames' renderers ([`pixels`]), by session.
    offscreen: HashMap<u64, Renderer>,
    /// A device was lost: an attach while there is no usable GPU tries to make one again.
    lost: bool,
}

impl State {
    /// The process's context, lost or not; made (fitting `surface`, if given) when there is none yet.
    fn context(&mut self, surface: Option<&wgpu::Surface<'_>>, instance: impl FnOnce() -> wgpu::Instance) -> Option<&Context> {
        let lost = matches!(&self.context, Some(Some(c)) if c.is_lost());
        if lost || (self.lost && matches!(self.context, Some(None))) {
            // What was made with the lost device goes before it does: the next device is made below.
            self.windows.clear();
            self.offscreen.clear();
            self.context = None;
            self.lost = true;
        }
        self.context.get_or_insert_with(|| block_on(Context::new(instance(), surface)).ok()).as_ref()
    }
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Mutex::default);

/// The state, locked. A panic while it was held (a bug, in a debug build) leaves it as it was.
fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Testing: loses the GPU device as a driver reset would. Frames fail `unavailable` until the next attach.
pub fn lose_device() {
    if let Some(Some(c)) = &state().context {
        c.device.destroy();
        // The loss is reported while the device is polled.
        let _ = c.device.poll(wgpu::PollType::wait_indefinitely());
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
/// `window` is a live window or layer that outlives the attach ([`detach`], or the session's end).
pub unsafe fn attach(key: u64, window: *mut c_void) -> Option<Backend> {
    let s = &mut *state();
    let instance = match &s.context {
        Some(Some(c)) if !c.is_lost() => c.instance.clone(),
        _ => Context::instance(),
    };
    // SAFETY: the host's promise.
    let surface = unsafe { surface(&instance, window) }?;
    let ctx = s.context(Some(&surface), || instance)?;
    if !ctx.adapter.is_surface_supported(&surface) {
        return None;
    }
    let backend = match ctx.adapter.get_info().backend {
        wgpu::Backend::Dx12 => Backend::D3D12,
        wgpu::Backend::Metal => Backend::Metal,
        _ => Backend::Vulkan,
    };
    s.windows.insert(key, Window { surface, size: (0, 0), renderer: None });
    Some(backend)
}

/// Stops drawing `key`'s frames into its window, and forgets its offscreen frames' renderer.
pub fn detach(key: u64) {
    let mut s = state();
    s.windows.remove(&key);
    s.offscreen.remove(&key);
}

/// Draws `graphics`' recorded frame into `key`'s window, `width` x `height` pixels, and presents it; the next frame is
/// recorded afresh either way. False when the window isn't attached or the GPU can't draw (the device was lost): draw
/// with the CPU then.
pub fn frame(key: u64, width: u32, height: u32, graphics: &mut Graphics) -> bool {
    graphics.finish(|res, frame| {
        let s = &mut *state();
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
}

/// Tests and tools: `graphics`' recorded frame drawn on the GPU at `width` x `height` and read back (premultiplied
/// RGBA, rows packed), with `key`'s own renderer; `None` with no usable GPU, or when the device was lost.
pub fn pixels(key: u64, width: u32, height: u32, graphics: &mut Graphics) -> Option<Vec<u8>> {
    graphics.finish(|res, frame| {
        let s = &mut *state();
        s.context(None, Context::instance)?;
        let State { context: Some(Some(ctx)), offscreen, .. } = s else { return None };
        let renderer = offscreen.entry(key).or_insert_with(|| Renderer::new(ctx, wgpu::TextureFormat::Rgba8Unorm));
        renderer.draw_pixels(ctx, res, frame, (width, height)).ok()
    })
}
