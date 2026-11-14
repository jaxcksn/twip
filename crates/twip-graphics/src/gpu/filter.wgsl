// A layer filter's pass along rows or columns, in the CPU backend's integer arithmetic on 8-bit values, so both
// backends filter alike. Beyond the layer's edges is transparent.
//
// - Box (0): each pixel the rounded average of the pixels from `left` before it to `right` after.
// - Kernel (1): a symmetric kernel of 16-bit weights summing to 65536, `left` either side.
// - Min (2): each channel's minimum within `left` either side; a window reaching past an edge gives 0.
//
// A wide box or min reads a few pixels per pixel instead of its whole window, in more passes, to the same bytes:
//
// - Block sums (3, `block_sums`): each pixel's sum of the `left` pixels from it (to the edge), into a 16-bit texture
//   (exact up to 257 pixels).
// - Blocks (4, `blocks`): a box (`left`, `right`) as the block sums (`weights[0]` long) its window holds whole, plus
//   the pixels its blocks leave over: about 3√span reads.
// - Min doubling (5): each pixel the minimum of itself and the pixel `left` on (to the edge); `k` of them from a layer
//   give each pixel the minimum of the 2^k from it.
// - Min final (6): a min of reach `left` as the two 2^k-runs (`right` = 2^k) covering its window.

struct Pass {
    // 1 along rows, 0 along columns.
    horizontal: u32,
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
// Only `blocks` reads it (its pipeline's layout has it).
@group(0) @binding(2) var sums: texture_2d<u32>;

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

// The fragment's line: its first pixel, the step along it, the fragment's place on it and its length.
struct Line {
    start: vec2<i32>,
    along: vec2<i32>,
    i: i32,
    n: i32,
}

fn line(position: vec4<f32>) -> Line {
    let p = vec2<i32>(floor(position.xy));
    let size = vec2<i32>(params.size);
    let along = select(vec2<i32>(0, 1), vec2<i32>(1, 0), params.horizontal == 1u);
    let i = select(p.y, p.x, params.horizontal == 1u);
    return Line(p - along * i, along, i, select(size.y, size.x, params.horizontal == 1u));
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let l = line(position);
    let start = l.start;
    let along = l.along;
    let i = l.i;
    let n = l.n;
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
    } else if params.kind == 2u {
        let r = i32(params.left);
        if i >= r && i + r < n {
            out = vec4<u32>(255u);
            for (var j = i - r; j <= i + r; j++) {
                out = min(out, texel(start + along * j));
            }
        }
    } else if params.kind == 5u {
        let s = i32(params.left);
        out = texel(start + along * i);
        if i + s < n {
            out = min(out, texel(start + along * (i + s)));
        }
    } else {
        let r = i32(params.left);
        let s = i32(params.right);
        if i >= r && i + r < n {
            out = min(texel(start + along * (i - r)), texel(start + along * (i + r - s + 1)));
        }
    }
    return vec4<f32>(out) / 255.0;
}

@fragment
fn block_sums(@builtin(position) position: vec4<f32>) -> @location(0) vec4<u32> {
    let l = line(position);
    var sum = vec4<u32>(0u);
    for (var j = l.i; j < min(l.i + i32(params.left), l.n); j++) {
        sum += texel(l.start + l.along * j);
    }
    return sum;
}

@fragment
fn blocks(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let l = line(position);
    let start = l.start;
    let along = l.along;
    let n = l.n;
    let b = i32(weight(0u));
    let span = i32(params.left + params.right + 1u);
    let first = l.i - i32(params.left);
    let whole = span / b;
    var sum = vec4<u32>(0u);
    for (var k = 0; k < whole; k++) {
        let j = first + k * b;
        if j >= n {
            break;
        }
        if j >= 0 {
            sum += textureLoad(sums, start + along * j, 0);
        } else {
            // A block over the line's start: its pixels on the line (none when it ends before it).
            for (var t = 0; t < j + b; t++) {
                sum += texel(start + along * t);
            }
        }
    }
    for (var t = max(first + whole * b, 0); t < min(first + span, n); t++) {
        sum += texel(start + along * t);
    }
    let out = (sum + vec4<u32>(u32(span) / 2u)) / u32(span);
    return vec4<f32>(out) / 255.0;
}
