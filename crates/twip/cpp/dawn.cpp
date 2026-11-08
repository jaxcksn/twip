// Graphite on Dawn for the browser: the entry points skia-safe has no binding for (it binds Graphite's Metal and
// Vulkan backends only). Handles are emdawnwebgpu's, imported from the page's JS WebGPU objects (Module.WebGPU).

#include <unordered_map>

#include "include/core/SkImage.h"
#include "include/gpu/graphite/BackendTexture.h"
#include "include/gpu/graphite/Context.h"
#include "include/gpu/graphite/ContextOptions.h"
#include "include/gpu/graphite/Image.h"
#include "include/gpu/graphite/ImageProvider.h"
#include "include/gpu/graphite/Recorder.h"
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

namespace {
// Graphite draws only texture-backed images (its default provider uploads nothing): this one uploads each decoded
// picture once and keeps the texture while the picture is drawn.
class PictureUploads final : public ImageProvider {
public:
    sk_sp<SkImage> findOrCreate(Recorder* recorder, const SkImage* image, SkImage::RequiredProperties props) override {
        uint64_t key = uint64_t(image->uniqueID()) << 1 | uint64_t(props.fMipmapped);
        if (auto it = fTextures.find(key); it != fTextures.end()) return it->second;
        // shortcut: no eviction, a reset past 256 pictures (pictures of closed documents); LRU if that bites.
        if (fTextures.size() >= 256) fTextures.clear();
        sk_sp<SkImage> texture = SkImages::TextureFromImage(recorder, image, props);
        if (texture) fTextures[key] = texture;
        return texture;
    }

private:
    std::unordered_map<uint64_t, sk_sp<SkImage>> fTextures;
};
}  // namespace

// A recorder that uploads raster and lazily decoded pictures (the caller owns it: skia-safe's graphite::Recorder).
extern "C" Recorder* editor_graphite_make_recorder(Context* context) {
    RecorderOptions options;
    options.fImageProvider = sk_make_sp<PictureUploads>();
    return context->makeRecorder(options).release();
}