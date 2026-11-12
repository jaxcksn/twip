// A layer filter's pass along rows or columns, in the CPU backend's integer arithmetic on 8-bit values, so both
// backends filter alike. Beyond the layer's edges is transparent.
//
// - Box: each pixel the rounded average of the pixels from `left` before it to `right` after.
// - Kernel: a symmetric kernel of 16-bit weights summing to 65536, `left` either side.
// - Min: each channel's minimum within `left` either side; a window reaching past an edge gives 0.

struct Pass {
    // 1 along rows, 0 along columns.
    horizontal: u32,
    // 0 box, 1 kernel, 2 min.
    kind: u32,
    left: u32,
    right: u32,
    // The layer's pixels: its texture's top left `size`.
    size: vec2<u32>,
    pad: vec2<u32>,
    weights: array<vec4<u32>, 4>,
}

@group(0) @binding(0) var<uniform> params: Pass;
@group(0) @binding(1) var source: texture_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // A triangle covering the whole target.
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn texel(p: vec2<i32>) -> vec4<u32> {
    return vec4<u32>(round(textureLoad(source, p, 0) * 255.0));
}

fn weight(k: u32) -> u32 {
    return params.weights[k / 4u][k % 4u];
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(position.xy));
    let size = vec2<i32>(params.size);
    let along = select(vec2<i32>(0, 1), vec2<i32>(1, 0), params.horizontal == 1u);
    let i = select(p.y, p.x, params.horizontal == 1u);
    let n = select(size.y, size.x, params.horizontal == 1u);
    let start = p - along * i;
    var out = vec4<u32>(0u);
    if params.kind == 0u {
        let lo = max(i - i32(params.left), 0);
        let hi = min(i + i32(params.right), n - 1);
        var sum = vec4<u32>(0u);
        for (var j = lo; j <= hi; j++) {
            sum += texel(start + along * j);
        }
        let span = params.left + params.right + 1u;
        out = (sum + vec4<u32>(span / 2u)) / span;
    } else if params.kind == 1u {
        let r = i32(params.left);
        var sum = vec4<u32>(32768u);
        for (var j = max(i - r, 0); j <= min(i + r, n - 1); j++) {
            sum += texel(start + along * j) * weight(u32(j - i + r));
        }
        out = sum >> vec4<u32>(16u);
    } else {
        let r = i32(params.left);
        if i >= r && i + r < n {
            out = vec4<u32>(255u);
            for (var j = i - r; j <= i + r; j++) {
                out = min(out, texel(start + along * j));
            }
        }
    }
    return vec4<f32>(out) / 255.0;
}
