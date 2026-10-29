// Graphite on native Dawn (Windows): the one device the process draws with, and a swap chain surface per window.
// Backends in order of preference: D3D12, D3D11, Vulkan. cpp/dawn.cpp makes the Graphite context and recorder.

#include <atomic>
#include <memory>
#include <vector>

#include <windows.h>

#include "dawn/dawn_proc.h"
#include "dawn/native/DawnNative.h"
#include "webgpu/webgpu_cpp.h"

namespace {
std::unique_ptr<dawn::native::Instance> gInstance;
wgpu::Adapter gAdapter;
wgpu::Device gDevice;
std::atomic<bool> gLost{false};
bool gEverLost = false;

wgpu::DawnTogglesDescriptor Toggles(const char* const* toggles, size_t count) {
    wgpu::DawnTogglesDescriptor desc;
    desc.enabledToggleCount = count;
    desc.enabledToggles = toggles;
    return desc;
}

// The optional features Graphite takes advantage of (Skia's tools/graphite/dawn/GraphiteDawnToggles.cpp).
std::vector<wgpu::FeatureName> Features(const wgpu::Adapter& adapter) {
    std::vector<wgpu::FeatureName> features;
    for (auto f : {wgpu::FeatureName::BufferMapExtendedUsages, wgpu::FeatureName::DawnAllowUndefinedLoadStoreOp,
                   wgpu::FeatureName::DawnLoadResolveTexture, wgpu::FeatureName::DawnPartialLoadResolveTexture,
                   wgpu::FeatureName::DawnTexelCopyBufferRowAlignment, wgpu::FeatureName::DualSourceBlending,
                   wgpu::FeatureName::FramebufferFetch, wgpu::FeatureName::ImplicitDeviceSynchronization,
                   wgpu::FeatureName::MSAARenderToSingleSampled, wgpu::FeatureName::ShaderF16,
                   wgpu::FeatureName::TextureCompressionBC, wgpu::FeatureName::TextureFormatsTier1,
                   wgpu::FeatureName::TransientAttachments, wgpu::FeatureName::Unorm16TextureFormats,
                   wgpu::FeatureName::RenderPassRenderArea}) {
        if (adapter.HasFeature(f)) features.push_back(f);
    }
    return features;
}
}  // namespace

// Creates the device on the first call: the first hardware adapter of the preferred backends. Writes borrowed
// instance, device and queue handles (the process keeps them) and returns the backend: 1 D3D12, 2 D3D11, 3 Vulkan;
// 0 when there is none (no GPU, or only a software adapter, which the CPU raster beats). After a loss, the next call
// lets go of the lost device, its adapter and instance (the caller released what it made with them) and starts over,
// and so does each call while none can be made again.
extern "C" int twip_dawn_device(WGPUInstance* instance, WGPUDevice* device, WGPUQueue* queue) {
    static int backend = -1;
    if (gLost || (gEverLost && backend == 0)) {
        gDevice = nullptr;
        gAdapter = nullptr;
        gInstance.reset();
        gEverLost = true;
        gLost = false;
        backend = -1;
    }
    if (backend < 0) {
        backend = 0;
        dawnProcSetProcs(&dawn::native::GetProcs());
        static const char* const kInstance[] = {"allow_unsafe_apis"};
        auto instanceToggles = Toggles(kInstance, std::size(kInstance));
        static const auto kTimedWaitAny = wgpu::InstanceFeatureName::TimedWaitAny;
        wgpu::InstanceDescriptor desc;
        desc.requiredFeatureCount = 1;
        desc.requiredFeatures = &kTimedWaitAny;
        desc.nextInChain = &instanceToggles;
        gInstance = std::make_unique<dawn::native::Instance>(&desc);
        static const char* const kAdapter[] = {"skip_validation", "disable_lazy_clear_for_mapped_at_creation_buffer",
                                               "disable_robustness"};
        auto adapterToggles = Toggles(kAdapter, std::size(kAdapter));
        const std::pair<wgpu::BackendType, int> order[] = {
                {wgpu::BackendType::D3D12, 1}, {wgpu::BackendType::D3D11, 2}, {wgpu::BackendType::Vulkan, 3}};
        for (auto [type, id] : order) {
            wgpu::RequestAdapterOptions options;
            options.backendType = type;
            options.powerPreference = wgpu::PowerPreference::HighPerformance;
            options.featureLevel = wgpu::FeatureLevel::Core;
            options.nextInChain = &adapterToggles;
            for (const auto& candidate : gInstance->EnumerateAdapters(&options)) {
                wgpu::Adapter adapter = candidate.Get();
                wgpu::AdapterInfo info;
                adapter.GetInfo(&info);
                if (info.backendType != type || info.adapterType == wgpu::AdapterType::CPU) continue;
                auto features = Features(adapter);
                wgpu::Limits limits = {};
                adapter.GetLimits(&limits);
                wgpu::DeviceDescriptor dd;
                dd.requiredFeatureCount = features.size();
                dd.requiredFeatures = features.data();
                dd.requiredLimits = &limits;
                dd.nextInChain = &adapterToggles;
                // A driver reset, update or removal. Not the device's release, a dropped instance or a failed create.
                dd.SetDeviceLostCallback(wgpu::CallbackMode::AllowSpontaneous,
                                         [](const wgpu::Device&, wgpu::DeviceLostReason reason, wgpu::StringView) {
                                             if (reason == wgpu::DeviceLostReason::Unknown) gLost = true;
                                         });
                dd.SetUncapturedErrorCallback([](const wgpu::Device&, wgpu::ErrorType, wgpu::StringView) {});
                wgpu::Device created = adapter.CreateDevice(&dd);
                if (!created) continue;
                gAdapter = adapter;
                gDevice = created;
                backend = id;
                break;
            }
            if (backend) break;
        }
    }
    if (!backend || gLost) return 0;
    *instance = gInstance->Get();
    *device = gDevice.Get();
    *queue = gDevice.GetQueue().Get();
    return backend;
}

// Whether the device was lost (a driver reset or update): the windows draw with the CPU until the next attach makes a
// new one.
extern "C" bool twip_dawn_lost() { return gLost; }

// Testing: loses the device as a driver reset would.
extern "C" void twip_dawn_test_lose() {
    if (!gDevice) return;
    gDevice.ForceLoss(wgpu::DeviceLostReason::Unknown, "a test loss");
    gLost = true;
}

// A surface on the window (the caller releases it with twip_dawn_surface_release).
extern "C" WGPUSurface twip_dawn_surface(HWND hwnd) {
    if (!gDevice) return nullptr;
    wgpu::SurfaceSourceWindowsHWND source;
    source.hinstance = GetModuleHandleW(nullptr);
    source.hwnd = hwnd;
    wgpu::SurfaceDescriptor desc;
    desc.nextInChain = &source;
    return wgpu::Instance(gInstance->Get()).CreateSurface(&desc).MoveToCHandle();
}

extern "C" void twip_dawn_surface_release(WGPUSurface surface) { wgpuSurfaceRelease(surface); }

// Sizes the surface's swap chain; `bgra` says whether its textures are BGRA (else RGBA).
extern "C" bool twip_dawn_configure(WGPUSurface handle, uint32_t width, uint32_t height, bool* bgra) {
    wgpu::Surface surface(handle);
    wgpu::SurfaceCapabilities caps;
    if (surface.GetCapabilities(gAdapter, &caps) != wgpu::Status::Success || caps.formatCount == 0) return false;
    wgpu::TextureFormat format = caps.formats[0];
    for (size_t i = 0; i < caps.formatCount; i++) {
        if (caps.formats[i] == wgpu::TextureFormat::BGRA8Unorm) format = caps.formats[i];
    }
    if (format != wgpu::TextureFormat::BGRA8Unorm && format != wgpu::TextureFormat::RGBA8Unorm) return false;
    // Mailbox doesn't block the engine thread on the display's refresh; FIFO is always there.
    wgpu::PresentMode mode = wgpu::PresentMode::Fifo;
    for (size_t i = 0; i < caps.presentModeCount; i++) {
        if (caps.presentModes[i] == wgpu::PresentMode::Mailbox) mode = wgpu::PresentMode::Mailbox;
    }
    wgpu::SurfaceConfiguration config;
    config.device = gDevice;
    config.format = format;
    config.usage = wgpu::TextureUsage::RenderAttachment | (caps.usages & (wgpu::TextureUsage::TextureBinding |
                                                                          wgpu::TextureUsage::CopySrc |
                                                                          wgpu::TextureUsage::CopyDst));
    config.width = width;
    config.height = height;
    config.presentMode = mode;
    config.alphaMode = wgpu::CompositeAlphaMode::Opaque;
    surface.Configure(&config);
    *bgra = format == wgpu::TextureFormat::BGRA8Unorm;
    return true;
}

// The texture to draw the next frame into (the caller releases it), or null when the swap chain needs configuring
// again (the window changed size, or the display mode changed).
extern "C" WGPUTexture twip_dawn_current(WGPUSurface handle) {
    wgpu::SurfaceTexture texture;
    wgpu::Surface(handle).GetCurrentTexture(&texture);
    if (texture.status != wgpu::SurfaceGetCurrentTextureStatus::SuccessOptimal &&
        texture.status != wgpu::SurfaceGetCurrentTextureStatus::SuccessSuboptimal) {
        return nullptr;
    }
    return texture.texture.MoveToCHandle();
}

extern "C" bool twip_dawn_present(WGPUSurface handle) {
    return wgpu::Surface(handle).Present() == wgpu::Status::Success;
}
