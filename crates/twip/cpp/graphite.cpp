// Graphite's recorder with an image provider, for every GPU backend (Dawn and Metal): skia-safe binds no
// ImageProvider.

#include <unordered_map>

#include "include/core/SkImage.h"
#include "include/gpu/graphite/Context.h"
#include "include/gpu/graphite/Image.h"
#include "include/gpu/graphite/ImageProvider.h"
#include "include/gpu/graphite/Recorder.h"

using namespace skgpu::graphite;

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