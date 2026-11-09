// Graphite on Dawn, in the browser and on Windows: the entry points skia-safe has no binding for (it binds Graphite's
// Metal and Vulkan backends only). In the browser the handles are emdawnwebgpu's, imported from the page's JS WebGPU
// objects (Module.WebGPU); on Windows they are cpp/dawn_native.cpp's.

#include "include/gpu/graphite/BackendTexture.h"
#include "include/gpu/graphite/Context.h"
#include "include/gpu/graphite/ContextOptions.h"
#include "include/gpu/graphite/dawn/DawnBackendContext.h"
#include "include/gpu/graphite/dawn/DawnGraphiteTypes.h"

using namespace skgpu::graphite;

// The caller owns the context (skia-safe's graphite::Context deletes it). The wgpu wrappers add their own references.
extern "C" Context* editor_graphite_make_dawn(WGPUInstance instance, WGPUDevice device, WGPUQueue queue) {
    DawnBackendContext backend;
    backend.fInstance = wgpu::Instance(instance);
    backend.fDevice = wgpu::Device(device);
    backend.fQueue = wgpu::Queue(queue);
    return ContextFactory::MakeDawn(backend, ContextOptions()).release();
}

// Points `out` (a constructed BackendTexture, i.e. skia-safe's graphite::BackendTexture) at a canvas texture.
extern "C" void editor_graphite_dawn_texture(BackendTexture* out, WGPUTexture texture) {
    *out = BackendTextures::MakeDawn(texture);
}
