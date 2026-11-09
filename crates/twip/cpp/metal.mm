// Graphite on Metal (macOS): the device the process draws with, and the host's CAMetalLayer as each window's swap
// chain. src/render/window.rs makes the Graphite context with skia-safe; cpp/graphite.cpp makes its recorder.
// Built with ARC. Every call is on the engine thread, which has no run loop: the layer changes go in an explicit
// transaction, and the caller drains an autorelease pool around each frame.

#import <Metal/Metal.h>
#import <QuartzCore/QuartzCore.h>

// A new device and command queue, each +1 (the caller releases them with twip_metal_release), or false with no GPU.
// The system default device is the high-performance GPU on a Mac with two.
extern "C" bool twip_metal_device(void** device, void** queue) {
    @autoreleasepool {
        id<MTLDevice> made = MTLCreateSystemDefaultDevice();
        id<MTLCommandQueue> commands = [made newCommandQueue];
        if (!commands) return false;
        *device = (void*)CFBridgingRetain(made);
        *queue = (void*)CFBridgingRetain(commands);
        return true;
    }
}

extern "C" void twip_metal_release(void* object) {
    if (object) CFRelease(object);
}

// Takes the host's layer for drawing on `device`: the layer, +1, or null when it isn't a CAMetalLayer.
extern "C" void* twip_metal_layer(void* handle, void* device) {
    @autoreleasepool {
        id object = (__bridge id)handle;
        if (![object isKindOfClass:[CAMetalLayer class]]) return nullptr;
        CAMetalLayer* layer = object;
        [CATransaction begin];
        [CATransaction setDisableActions:YES];
        layer.device = (__bridge id<MTLDevice>)device;
        layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
        // Graphite may sample the target (a blend that reads what's under it).
        layer.framebufferOnly = NO;
        layer.opaque = YES;
        // Presents as soon as the GPU is done rather than at the display's refresh, so nextDrawable doesn't hold the
        // engine thread for a vsync (as Dawn's mailbox did).
        layer.displaySyncEnabled = NO;
        [CATransaction commit];
        return (void*)CFBridgingRetain(layer);
    }
}

extern "C" void twip_metal_resize(void* handle, uint32_t width, uint32_t height) {
    CAMetalLayer* layer = (__bridge CAMetalLayer*)handle;
    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    layer.drawableSize = CGSizeMake(width, height);
    [CATransaction commit];
}

// The next drawable, +1, and its texture and size (the texture is alive while the drawable is), or null when none came
// in time.
extern "C" void* twip_metal_next(void* handle, void** texture, uint32_t* width, uint32_t* height) {
    id<CAMetalDrawable> drawable = [(__bridge CAMetalLayer*)handle nextDrawable];
    if (!drawable) return nullptr;
    *texture = (__bridge void*)drawable.texture;
    *width = uint32_t(drawable.texture.width);
    *height = uint32_t(drawable.texture.height);
    return (void*)CFBridgingRetain(drawable);
}

// Presents `drawable` after the work Graphite submitted to `queue`: a command buffer on the same queue runs after it.
extern "C" bool twip_metal_present(void* queue, void* drawable) {
    id<MTLCommandBuffer> buffer = [(__bridge id<MTLCommandQueue>)queue commandBuffer];
    if (!buffer) return false;
    [buffer presentDrawable:(__bridge id<CAMetalDrawable>)drawable];
    [buffer commit];
    return true;
}
