// The instance pipeline: one quad per instance, drawn as a four-vertex triangle strip.
//
// Coverage: an instance covers its quad either analytically (a rect: each pixel's area inside the rect's fractional
// edges, the box filter the CPU backend uses) or from the coverage atlas (one texel per pixel, unfiltered).
//
// Paint: a solid premultiplied colour, a gradient (linear, two-point conical or sweep) looked up in this frame's
// ramps, the CPU backend's 256-entry rows, at the nearest entry, an image sampled from one mip level then its colour
// filters, or a layer's pixels then its colour filters and opacity. A paint is evaluated at the pixel's centre mapped
// to the paint's space, as the CPU backend's shaders evaluate it.
//
// The output is the paint times the coverage; the pipeline's fixed-function blend does the rest. For an erase
// (destination-in, clear) the output's alpha is what erases. The other blend modes (`general`) read a copy of the
// destination and blend as the CPU backend's general blend does: the W3C Compositing and Blending formulas on 8-bit
// premultiplied colour, then the destination moved towards the result by the coverage.

struct Globals {
    // The pass's target size, pixels.
    size: vec2<f32>,
    // The device pixel at the target's top left (a layer's region's).
    origin: vec2<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var atlas: texture_2d_array<f32>;
// This frame's gradient ramps: a row each, 256 entries.
@group(0) @binding(2) var ramps: texture_2d<f32>;
// This frame's colour filters: a row each. Texel 0 is the kind (0 a matrix, 1 a table); a matrix's rows follow in
// texels 1 to 4 (four multipliers each) and its offsets in texel 5; a table's 256 values (0 to 255) follow four to a
// texel.
@group(0) @binding(3) var filters: texture_2d<f32>;
// Colour glyphs' masks: premultiplied RGBA.
@group(0) @binding(4) var colour_atlas: texture_2d_array<f32>;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
// The destination under a general blend's quad, from its top left.
@group(2) @binding(0) var destination: texture_2d<f32>;

struct Instance {
    @builtin(vertex_index) corner: u32,
    // The pixels drawn: x0, y0, x1, y1, device pixels.
    @location(0) quad: vec4<f32>,
    // A rect: its fractional edges. A mask: its quad's top left pixel (xy) and that pixel's atlas texel (zw).
    @location(1) edges: vec4<f32>,
    @location(2) color: vec4<f32>,
    // 0: a rect. 1: a mask on atlas page `page`.
    @location(3) kind: u32,
    @location(4) page: u32,
    // The paint's kind (bits 0-3), its extend (4-5), the output (8-9: 0 the paint, 1 destination-in's erase, 2
    // clear's), and its ramp row or image mip level (16-31).
    @location(5) paint: u32,
    // Device pixels to the paint's space (a, b, c, d; then e, f), and two more values.
    @location(6) xform0: vec4<f32>,
    @location(7) xform1: vec4<f32>,
    @location(8) params: vec4<f32>,
    // The first colour filter's row, and (top 8 bits) how many.
    @location(9) chain: u32,
}

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) edges: vec4<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) @interpolate(flat) kind: u32,
    @location(3) @interpolate(flat) page: u32,
    @location(4) @interpolate(flat) paint: u32,
    @location(5) @interpolate(flat) xform0: vec4<f32>,
    @location(6) @interpolate(flat) xform1: vec4<f32>,
    @location(7) @interpolate(flat) params: vec4<f32>,
    @location(8) @interpolate(flat) chain: u32,
    @location(9) @interpolate(flat) quad: vec2<f32>,
}

@vertex
fn vertex(i: Instance) -> Varyings {
    let corner = vec2<f32>(f32(i.corner & 1u), f32(i.corner >> 1u));
    let p = mix(i.quad.xy, i.quad.zw, corner) - globals.origin;
    var out: Varyings;
    out.position = vec4<f32>(p.x / globals.size.x * 2.0 - 1.0, 1.0 - p.y / globals.size.y * 2.0, 0.0, 1.0);
    out.edges = i.edges;
    out.color = i.color;
    out.kind = i.kind;
    out.page = i.page;
    out.paint = i.paint;
    out.xform0 = i.xform0;
    out.xform1 = i.xform1;
    out.params = i.params;
    out.chain = i.chain;
    out.quad = i.quad.xy;
    return out;
}

// `t` folded into 0..1 by the extend mode: clamp, repeat, mirror.
fn fold(t: f32, extend: u32) -> f32 {
    if extend == 1u {
        return t - floor(t);
    }
    if extend == 2u {
        let f = t - 2.0 * floor(t * 0.5);
        return select(f, 2.0 - f, f > 1.0);
    }
    return clamp(t, 0.0, 1.0);
}

fn ramp(t: f32, extend: u32, row: u32) -> vec4<f32> {
    let i = u32(fold(t, extend) * 255.0 + 0.5);
    return textureLoad(ramps, vec2<i32>(i32(min(i, 255u)), i32(row)), 0);
}

// The two-point conical gradient's t at `p`: the largest root of |p - c(t)| = r(t) with r(t) >= 0. `y` is 0 where no
// circle reaches the point.
fn conical(p: vec2<f32>, c0: vec2<f32>, r0: f32, cd: vec2<f32>, dr: f32) -> vec2<f32> {
    let q = p - c0;
    let a = dot(cd, cd) - dr * dr;
    let b = dot(q, cd) + r0 * dr;
    let c = dot(q, q) - r0 * r0;
    if abs(a) < 1e-9 {
        if b == 0.0 {
            return vec2<f32>(0.0, 0.0);
        }
        let t = c / (2.0 * b);
        return vec2<f32>(t, select(0.0, 1.0, r0 + t * dr >= 0.0));
    }
    let disc = b * b - a * c;
    if disc < 0.0 {
        return vec2<f32>(0.0, 0.0);
    }
    let root = sqrt(disc);
    let t1 = (b + root) / a;
    let t2 = (b - root) / a;
    let hi = max(t1, t2);
    let lo = min(t1, t2);
    if r0 + hi * dr >= 0.0 {
        return vec2<f32>(hi, 1.0);
    }
    if r0 + lo * dr >= 0.0 {
        return vec2<f32>(lo, 1.0);
    }
    return vec2<f32>(0.0, 0.0);
}

// A table filter's value for `v` (0..1) on filter row `row`.
fn look(v: f32, row: i32) -> f32 {
    let i = u32(v * 255.0 + 0.5);
    let texel = textureLoad(filters, vec2<i32>(1 + i32(i / 4u), row), 0);
    return texel[i % 4u] / 255.0;
}

// `px` (premultiplied) through `count` colour filters from row `first`: on unpremultiplied colour, each result clamped
// before the next, then premultiplied again at 8 bits, as the CPU backend filters.
fn filtered(px: vec4<f32>, first: u32, count: u32) -> vec4<f32> {
    var alpha = round(px.a * 255.0) / 255.0;
    var c = select(vec3<f32>(0.0), min(round(px.rgb * 255.0) / 255.0 / alpha, vec3<f32>(1.0)), alpha > 0.0);
    for (var i = 0u; i < count; i++) {
        let row = i32(first + i);
        if textureLoad(filters, vec2<i32>(0, row), 0).x < 0.5 {
            let v = vec4<f32>(c, alpha);
            let o = textureLoad(filters, vec2<i32>(5, row), 0);
            let r = vec4<f32>(
                dot(textureLoad(filters, vec2<i32>(1, row), 0), v) + o.x,
                dot(textureLoad(filters, vec2<i32>(2, row), 0), v) + o.y,
                dot(textureLoad(filters, vec2<i32>(3, row), 0), v) + o.z,
                dot(textureLoad(filters, vec2<i32>(4, row), 0), v) + o.w,
            );
            let k = clamp(r, vec4<f32>(0.0), vec4<f32>(1.0));
            c = k.xyz;
            alpha = k.w;
        } else {
            c = vec3<f32>(look(c.x, row), look(c.y, row), look(c.z, row));
            alpha = look(alpha, row);
        }
    }
    let a8 = round(alpha * 255.0) / 255.0;
    return vec4<f32>(round(c * 255.0) / 255.0 * a8, a8);
}

// `p`'s pixel of the layer whose region's top left pixel is `origin`, through its colour filters, times its
// opacity (0 to 255) as the CPU backend multiplies it.
fn layer(p: vec2<f32>, origin: vec2<f32>, opacity: u32, chain: u32) -> vec4<f32> {
    var px = textureLoad(image, vec2<i32>(floor(p) - origin), 0);
    let count = chain >> 24u;
    if count > 0u {
        px = filtered(px, chain & 0xFFFFFFu, count);
    }
    return scaled(px, opacity);
}

// `px` (8-bit premultiplied) times `by` (0 to 255), each channel rounded as the CPU backend's `mul_div255`.
fn scaled(px: vec4<f32>, by: u32) -> vec4<f32> {
    if by == 255u {
        return px;
    }
    let t = vec4<u32>(round(px * 255.0)) * by + vec4<u32>(128u);
    return vec4<f32>((t * 257u) >> vec4<u32>(16u)) / 255.0;
}

// `p` is the pixel's centre, device pixels.
fn paint(v: Varyings, p: vec2<f32>) -> vec4<f32> {
    let kind = v.paint & 15u;
    if kind == 0u {
        return v.color;
    }
    if kind == 5u {
        return layer(p, v.params.xy, u32(v.params.z), v.chain);
    }
    if kind == 6u {
        // A colour glyph: its mask's top left pixel (params.xy) is at texel params.zw of colour atlas page `row`, and
        // the text colour's alpha modulates it, as the CPU backend multiplies it.
        let texel = vec2<i32>(floor(p) - v.params.xy + v.params.zw);
        let px = textureLoad(colour_atlas, texel, i32(v.paint >> 16u), 0);
        return scaled(px, u32(round(v.color.a * 255.0)));
    }
    // The pixel's centre in the paint's space.
    let q = vec2<f32>(v.xform0.x * p.x + v.xform0.z * p.y + v.xform1.x, v.xform0.y * p.x + v.xform0.w * p.y + v.xform1.y);
    let extend = (v.paint >> 4u) & 3u;
    let row = v.paint >> 16u;
    if kind == 1u {
        let t = dot(q - v.params.xy, v.params.zw) * v.xform1.z;
        return ramp(t, extend, row);
    }
    if kind == 2u {
        let t = conical(q, v.params.xy, v.xform1.z, v.params.zw, v.xform1.w);
        return select(vec4<f32>(0.0), ramp(t.x, extend, row), t.y > 0.5);
    }
    if kind == 3u {
        let d = q - v.params.xy;
        var angle = degrees(atan2(d.y, d.x));
        angle = angle - 360.0 * floor(angle / 360.0);
        let t = select(0.0, (angle - v.params.z) / v.params.w, v.params.w != 0.0);
        return ramp(t, extend, row);
    }
    // An image: the level's pixels are the image's times its scale; sampled at the level the paint chose.
    let uv = q * v.xform1.zw / v.params.xy;
    let px = textureSampleLevel(image, image_sampler, uv, f32(row));
    let count = v.chain >> 24u;
    if count == 0u {
        return px;
    }
    return filtered(px, v.chain & 0xFFFFFFu, count);
}

// The instance's coverage of the pixel centred at `p` (device pixels).
fn coverage(v: Varyings, p: vec2<f32>) -> f32 {
    // The pixel's centre is at a half: its area spans half a pixel either way.
    if v.kind == 0u {
        let lo = max(p - 0.5, v.edges.xy);
        let hi = min(p + 0.5, v.edges.zw);
        let span = clamp(hi - lo, vec2<f32>(0.0), vec2<f32>(1.0));
        return span.x * span.y;
    }
    let texel = vec2<i32>(floor(p) - v.edges.xy + v.edges.zw);
    return textureLoad(atlas, texel, i32(v.page), 0).r;
}

fn screen(a: f32, b: f32) -> f32 {
    return a + b - a * b;
}

fn hard_light(s: f32, d: f32) -> f32 {
    if s <= 0.5 {
        return d * (2.0 * s);
    }
    return screen(d, 2.0 * s - 1.0);
}

// A separable blend function (by `BlendMode`'s order) on unpremultiplied channels.
fn separable(mode: u32, s: f32, d: f32) -> f32 {
    switch mode {
        case 13u: { return screen(s, d); }
        case 14u: { return hard_light(d, s); }
        case 15u: { return min(s, d); }
        case 16u: { return max(s, d); }
        case 17u: {
            if d == 0.0 {
                return 0.0;
            }
            if s >= 1.0 {
                return 1.0;
            }
            return min(d / (1.0 - s), 1.0);
        }
        case 18u: {
            if d >= 1.0 {
                return 1.0;
            }
            if s <= 0.0 {
                return 0.0;
            }
            return 1.0 - min((1.0 - d) / s, 1.0);
        }
        case 19u: { return hard_light(s, d); }
        case 20u: {
            if s <= 0.5 {
                return d - (1.0 - 2.0 * s) * d * (1.0 - d);
            }
            let dd = select(sqrt(d), ((16.0 * d - 12.0) * d + 4.0) * d, d <= 0.25);
            return d + (2.0 * s - 1.0) * (dd - d);
        }
        case 21u: { return abs(s - d); }
        case 22u: { return s + d - 2.0 * s * d; }
        default: { return s * d; }
    }
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var out = c;
    if n < 0.0 && l - n != 0.0 {
        out = l + (out - l) * l / (l - n);
    }
    if x > 1.0 && x - l != 0.0 {
        out = l + (out - l) * (1.0 - l) / (x - l);
    }
    return out;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return clip_color(c + (l - lum(c)));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    if x > n {
        return (c - n) * s / (x - n);
    }
    return vec3<f32>(0.0);
}

// A non-separable blend function (hue, saturation, colour, luminosity) on unpremultiplied colour.
fn non_separable(mode: u32, s: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 24u: { return set_lum(set_sat(s, sat(d)), lum(d)); }
        case 25u: { return set_lum(set_sat(d, sat(s)), lum(d)); }
        case 26u: { return set_lum(s, lum(d)); }
        default: { return set_lum(d, lum(s)); }
    }
}

fn unpremultiply(c: vec3<f32>, a: f32) -> vec3<f32> {
    return select(vec3<f32>(0.0), min(c / a, vec3<f32>(1.0)), a > 0.0);
}

// Source `s` blended with destination `d` (8-bit premultiplied) by `mode`, as 8-bit premultiplied colour.
fn general(s: vec4<u32>, d: vec4<u32>, mode: u32) -> vec4<u32> {
    let sf = vec4<f32>(s) / 255.0;
    let df = vec4<f32>(d) / 255.0;
    let sc = sf.rgb;
    let sa = sf.a;
    let dc = df.rgb;
    let da = df.a;
    var rgb: vec3<f32>;
    var a: f32;
    switch mode {
        case 0u: { rgb = vec3<f32>(0.0); a = 0.0; }
        case 1u: { rgb = sc; a = sa; }
        case 2u: { rgb = dc; a = da; }
        case 3u: { rgb = sc + dc * (1.0 - sa); a = sa + da * (1.0 - sa); }
        case 4u: { rgb = dc + sc * (1.0 - da); a = da + sa * (1.0 - da); }
        case 5u: { rgb = sc * da; a = sa * da; }
        case 6u: { rgb = dc * sa; a = da * sa; }
        case 7u: { rgb = sc * (1.0 - da); a = sa * (1.0 - da); }
        case 8u: { rgb = dc * (1.0 - sa); a = da * (1.0 - sa); }
        case 9u: { rgb = sc * da + dc * (1.0 - sa); a = da; }
        case 10u: { rgb = dc * sa + sc * (1.0 - da); a = sa; }
        case 11u: { rgb = sc * (1.0 - da) + dc * (1.0 - sa); a = sa * (1.0 - da) + da * (1.0 - sa); }
        case 12u: { rgb = min(sc + dc, vec3<f32>(1.0)); a = min(sa + da, 1.0); }
        case 24u, 25u, 26u, 27u: {
            let b = non_separable(mode, unpremultiply(sc, sa), unpremultiply(dc, da));
            rgb = sc * (1.0 - da) + dc * (1.0 - sa) + sa * da * b;
            a = sa + da - sa * da;
        }
        default: {
            let us = unpremultiply(sc, sa);
            let ud = unpremultiply(dc, da);
            let b = vec3<f32>(separable(mode, us.r, ud.r), separable(mode, us.g, ud.g), separable(mode, us.b, ud.b));
            rgb = sc * (1.0 - da) + dc * (1.0 - sa) + sa * da * b;
            a = sa + da - sa * da;
        }
    }
    let qa = u32(clamp(a, 0.0, 1.0) * 255.0 + 0.5);
    // Premultiplied: no channel above alpha.
    return vec4<u32>(min(vec3<u32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5), vec3<u32>(qa)), qa);
}

// `d` moved towards `r` by coverage `c` (0 to 255), each channel `d + c·(r - d)` rounded away from `d`'s side.
fn towards(d: vec4<u32>, r: vec4<u32>, c: u32) -> vec4<u32> {
    if c == 255u {
        return r;
    }
    let di = vec4<i32>(d);
    let ri = vec4<i32>(r);
    let bias = select(vec4<i32>(-127), vec4<i32>(127), ri >= di);
    return vec4<u32>(di + ((ri - di) * i32(c) + bias) / 255);
}

// A general blend: the instance's paint blended with the destination's copy, replacing the destination.
@fragment
fn blend(v: Varyings) -> @location(0) vec4<f32> {
    let p = v.position.xy + globals.origin;
    let c = u32(round(coverage(v, p) * 255.0));
    let s = vec4<u32>(round(paint(v, p) * 255.0));
    let d = vec4<u32>(round(textureLoad(destination, vec2<i32>(floor(p) - v.quad), 0) * 255.0));
    return vec4<f32>(towards(d, general(s, d, (v.paint >> 10u) & 63u), c)) / 255.0;
}

@fragment
fn fragment(v: Varyings) -> @location(0) vec4<f32> {
    let p = v.position.xy + globals.origin;
    let cov = coverage(v, p);
    var color = paint(v, p);
    let output = (v.paint >> 8u) & 3u;
    if output == 1u {
        color = vec4<f32>(0.0, 0.0, 0.0, 1.0 - color.a);
    } else if output == 2u {
        color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    return color * cov;
}
