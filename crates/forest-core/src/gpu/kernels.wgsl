// Shading kernels. They follow engine.rs `shade_region` step by step; the CPU code is the reference.
// `Params` accessors (p_*) are generated in gpu/mod.rs and put in front of this file.

struct Step { f: vec4<f32>, g: vec4<f32>, u: vec4<u32>, t: vec4<u32> };
struct Grades { v: array<vec4<f32>, 40> };

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<uniform> G: Grades;
@group(0) @binding(2) var<storage, read> RAMP: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> SDF: array<f32>;
@group(0) @binding(4) var<storage, read> INS: array<f32>;
@group(0) @binding(5) var<storage, read> UHAZE: array<f32>;
@group(0) @binding(6) var<storage, read> UGRASS: array<f32>;

@group(1) @binding(0) var<uniform> S: Step;
@group(1) @binding(1) var<storage, read_write> f0: array<f32>;
@group(1) @binding(2) var<storage, read_write> f1: array<f32>;
@group(1) @binding(3) var<storage, read_write> f2: array<f32>;
@group(1) @binding(4) var<storage, read_write> f3: array<f32>;
@group(1) @binding(5) var<storage, read_write> f4: array<f32>;
@group(1) @binding(6) var<storage, read_write> f5: array<f32>;
@group(1) @binding(7) var<storage, read_write> f6: array<f32>;
@group(1) @binding(8) var<storage, read_write> f7: array<f32>;
@group(1) @binding(9) var<storage, read_write> f8: array<f32>;
@group(1) @binding(10) var<storage, read_write> f9: array<f32>;
@group(1) @binding(11) var<storage, read_write> u0: array<u32>;
@group(1) @binding(12) var<storage, read_write> u1: array<u32>;
@group(1) @binding(13) var<storage, read_write> u2: array<u32>;
@group(1) @binding(14) var<storage, read_write> u3: array<u32>;
@group(1) @binding(15) var<storage, read_write> u4: array<u32>;
@group(1) @binding(16) var<storage, read_write> u5: array<atomic<u32>>;
@group(1) @binding(17) var<storage, read_write> u6: array<atomic<u32>>;
@group(1) @binding(18) var out_tex: texture_storage_2d<rgba8unorm, write>;

const PI: f32 = 3.14159265;

fn flat_index(gid: vec3<u32>, nwg: vec3<u32>) -> u32 {
    return gid.x + gid.y * nwg.x * 256u;
}

// ------------------------------------------------------------------ noise (noise.rs)

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    var h: u32 = (bitcast<u32>(x) * 0x8da6b343u) ^ (bitcast<u32>(y) * 0xd8163841u) ^ (seed * 0xcb1ab31fu);
    h = h ^ (h >> 13u);
    h = h * 0x5bd1e995u;
    h = h ^ (h >> 15u);
    return f32(h & 0x00ffffffu) / 16777216.0;
}

fn fade_t(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn value(x: f32, y: f32, seed: u32) -> f32 {
    let xf = floor(x);
    let yf = floor(y);
    let xi = i32(xf);
    let yi = i32(yf);
    let tx = fade_t(x - xf);
    let ty = fade_t(y - yf);
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    let top = a + (b - a) * tx;
    let bot = c + (d - c) * tx;
    return top + (bot - top) * ty;
}

fn fbm(x: f32, y: f32, period: f32, octaves: u32, gain: f32, seed: u32) -> f32 {
    var sum = 0.0;
    var tot = 0.0;
    var amp = 1.0;
    var p = period;
    for (var o = 0u; o < octaves; o++) {
        sum += amp * value(x / p, y / p, seed + o * 1013u);
        tot += amp;
        amp *= gain;
        p = max(p * 0.5, 1.0);
    }
    return sum / tot;
}

fn ss(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// map coordinates of a region pixel
fn nat(x: u32, y: u32) -> vec2<f32> {
    return vec2<f32>((f32(x + p_ox()) + 0.5) * p_inv(), (f32(y + p_oy()) + 0.5) * p_inv());
}

// ------------------------------------------------------------------ plan samplers (engine.rs Plan)

fn sdf_map(nx: f32, ny: f32) -> f32 {
    if p_any_shape() == 0u {
        return -1e6;
    }
    let w = p_sdf_w();
    let h = p_sdf_h();
    let fx = clamp(nx * p_plan_s() - 0.5, 0.0, f32(w - 1u));
    let fy = clamp(ny * p_plan_s() - 0.5, 0.0, f32(h - 1u));
    let x0 = u32(fx);
    let y0 = u32(fy);
    let x1 = min(x0 + 1u, w - 1u);
    let y1 = min(y0 + 1u, h - 1u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let a = SDF[y0 * w + x0] * (1.0 - tx) + SDF[y0 * w + x1] * tx;
    let b = SDF[y1 * w + x0] * (1.0 - tx) + SDF[y1 * w + x1] * tx;
    return a * (1.0 - ty) + b * ty;
}

fn ins_plane(base: u32, nx: f32, ny: f32) -> f32 {
    let w = p_ins_gw();
    let h = p_ins_gh();
    let fx = clamp(nx * p_plan_s() / p_ins_cs() - 0.5, 0.0, f32(w - 1u));
    let fy = clamp(ny * p_plan_s() / p_ins_cs() - 0.5, 0.0, f32(h - 1u));
    let x0 = u32(fx);
    let y0 = u32(fy);
    let x1 = min(x0 + 1u, w - 1u);
    let y1 = min(y0 + 1u, h - 1u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let a = INS[base + y0 * w + x0] * (1.0 - tx) + INS[base + y0 * w + x1] * tx;
    let b = INS[base + y1 * w + x0] * (1.0 - tx) + INS[base + y1 * w + x1] * tx;
    return a * (1.0 - ty) + b * ty;
}

fn inset_map(nx: f32, ny: f32) -> f32 {
    return ins_plane(0u, nx, ny);
}

// how much of the edge wobble applies here: the second plane of the pull-in buffer
fn warp_map(nx: f32, ny: f32) -> f32 {
    return ins_plane(p_ins_gw() * p_ins_gh(), nx, ny);
}

// ------------------------------------------------------------------ heights and shadows

// top surface: grass height and every layer's height
@compute @workgroup_size(256)
fn k_top(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    if i >= p_w() * p_h() {
        return;
    }
    var t = f0[i] * 0.4 * p_s();
    t = max(t, f1[i]);
    t = max(t, f2[i]);
    t = max(t, f3[i]);
    t = max(t, f4[i]);
    f5[i] = t;
}

// field.rs `horizon`. Each workgroup sweeps a chunk of S.t.x columns row by row, with S.t.y columns of
// margin each side: an occluder only matters within so many rows, and each row moves sideways by at most a
// column, so the margin keeps the chunk exact. f0 top, f1 occ_h, f2 occ_k, f3/f4 two rows of running best
// and its distance per workgroup. S.u: (swap, down, minor length, major length); S.f: (ex, drop, len).
fn hz_index(major: u32, minor: u32) -> u32 {
    if S.u.x == 1u {
        return minor * p_w() + major;
    }
    return major * p_w() + minor;
}

@compute @workgroup_size(256)
fn k_horizon(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {
    let m = S.u.z;
    let l = S.u.w;
    let ex = S.f.x;
    let drop = S.f.y;
    let len = S.f.z;
    let chunk = S.t.x;
    let margin = S.t.y;
    let c0 = wid.x * chunk;
    let c1 = min(c0 + chunk, m);
    let e0 = select(0u, c0 - margin, c0 > margin);
    let e1 = min(c1 + margin, m);
    let e = e1 - e0;
    // this workgroup's two rows of scratch
    let base = wid.x * 2u * (chunk + 2u * margin);
    let stride = chunk + 2u * margin;
    for (var step = 0u; step < l; step++) {
        var y = step;
        if S.u.y == 1u {
            y = l - 1u - step;
        }
        let cur = base + (step % 2u) * stride;
        let prev = base + (1u - step % 2u) * stride;
        for (var x = e0 + lid.x; x < e1; x += 256u) {
            let fx = clamp(f32(x) + ex, f32(e0), f32(e1 - 1u));
            let x0 = u32(fx);
            let x1 = min(x0 + 1u, e1 - 1u);
            let t = fx - f32(x0);
            var g: f32;
            var k: f32;
            if step == 0u {
                g = f0[hz_index(y, x0)] * (1.0 - t) + f0[hz_index(y, x1)] * t;
                k = 0.0;
            } else {
                g = f3[prev + x0 - e0] * (1.0 - t) + f3[prev + x1 - e0] * t;
                k = f4[prev + x0 - e0] * (1.0 - t) + f4[prev + x1 - e0] * t;
            }
            g = g - drop;
            k = k + len;
            let i = hz_index(y, x);
            if x >= c0 && x < c1 {
                f1[i] = g;
                f2[i] = k;
            }
            let c = f0[i];
            if c >= g {
                f3[cur + x - e0] = c;
                f4[cur + x - e0] = 0.0;
            } else {
                f3[cur + x - e0] = g;
                f4[cur + x - e0] = k;
            }
        }
        storageBarrier();
        workgroupBarrier();
    }
}

// Unblurred shadow on a receiver. f0 occ_h, f1 occ_k, f2 receiver, f3 where it is present, f4 out.
// S.u.x: receiver (0 ground, 1 height, 2 grass cover); S.u.y: presence (0 everywhere, 1 where f3 > 0).
@compute @workgroup_size(256)
fn k_shadow_raw(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    if i >= p_w() * p_h() {
        return;
    }
    if S.u.y == 1u && f3[i] <= 0.0 {
        f4[i] = 0.0;
        return;
    }
    var recv = 0.0;
    if S.u.x == 1u {
        recv = f2[i];
    } else if S.u.x == 2u {
        recv = f2[i] * 0.4 * p_s();
    }
    let base = recv + 0.5 * p_s();
    f4[i] = clamp((f0[i] - base) / p_soft(), 0.0, 1.0) * max(1.0 - f1[i] * p_fall(), 0.0);
}

// ------------------------------------------------------------------ blur (field.rs)

// One box pass along rows (k_box_h) or columns (k_box_v), reading past the ends as the end value.
// f0 in, f1 out; S.u: (w, h, r).
@compute @workgroup_size(256)
fn k_box_h(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = S.u.x;
    let h = S.u.y;
    if i >= w * h {
        return;
    }
    let r = i32(S.u.z);
    let x = i32(i % w);
    let row = i - u32(x);
    var acc = 0.0;
    for (var k = -r; k <= r; k++) {
        acc += f0[row + u32(clamp(x + k, 0, i32(w) - 1))];
    }
    f1[i] = acc / f32(2 * r + 1);
}

@compute @workgroup_size(256)
fn k_box_v(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = S.u.x;
    let h = S.u.y;
    if i >= w * h {
        return;
    }
    let r = i32(S.u.z);
    let x = i % w;
    let y = i32(i / w);
    var acc = 0.0;
    for (var k = -r; k <= r; k++) {
        acc += f0[u32(clamp(y + k, 0, i32(h) - 1)) * w + x];
    }
    f1[i] = acc / f32(2 * r + 1);
}

// Average 2 x 2 blocks. f0 in (w, h), f1 out (hw, hh); S.u: (w, h, hw, hh).
@compute @workgroup_size(256)
fn k_down2(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = S.u.x;
    let h = S.u.y;
    let hw = S.u.z;
    if i >= hw * S.u.w {
        return;
    }
    let x0 = 2u * (i % hw);
    let y0 = 2u * (i / hw);
    let x1 = min(x0 + 1u, w - 1u);
    let y1 = min(y0 + 1u, h - 1u);
    f1[i] = 0.25 * (f0[y0 * w + x0] + f0[y0 * w + x1] + f0[y1 * w + x0] + f0[y1 * w + x1]);
}

// Fill a half-size field back out. f0 in (hw, hh), f1 out (w, h); S.u: (w, h, hw, hh).
@compute @workgroup_size(256)
fn k_up2(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = S.u.x;
    let hw = S.u.z;
    let hh = S.u.w;
    if i >= w * S.u.y {
        return;
    }
    let fx = max((f32(i % w) - 0.5) * 0.5, 0.0);
    let fy = max((f32(i / w) - 0.5) * 0.5, 0.0);
    let x0 = u32(fx);
    let y0 = u32(fy);
    let x1 = min(x0 + 1u, hw - 1u);
    let y1 = min(y0 + 1u, hh - 1u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let r0 = f0[y0 * hw + x0] * (1.0 - tx) + f0[y0 * hw + x1] * tx;
    let r1 = f0[y1 * hw + x0] * (1.0 - tx) + f0[y1 * hw + x1] * tx;
    f1[i] = r0 * (1.0 - ty) + r1 * ty;
}

// Masks from a layer: f1 = 1 where f0 (height) > 0; f2 = 1 where the pixel is a painted tree (u0 = w2).
@compute @workgroup_size(256)
fn k_mask(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    if i >= p_w() * p_h() {
        return;
    }
    f1[i] = select(0.0, 1.0, f0[i] > 0.0);
    f2[i] = select(0.0, 1.0, ((u0[i] >> 7u) & 1u) == 1u);
}

// Multiply by the drawn-area coverage when clipping. f0 in, f1 area, f2 out; S.u.x: clip or copy.
@compute @workgroup_size(256)
fn k_clip(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    if i >= p_w() * p_h() {
        return;
    }
    var v = f0[i];
    if S.u.x == 1u {
        v *= f1[i];
    }
    f2[i] = v;
}

// ------------------------------------------------------------------ snow and fire (engine.rs closures)

fn snow_where(x: u32, y: u32) -> f32 {
    let m = p_snow_mode();
    if m == 0u {
        return 1.0;
    }
    let n = nat(x, y);
    if m == 1u {
        return 1.0 - ss(0.0, max(p_snow_reach(), 10.0), sdf_map(n.x, n.y) - inset_map(n.x, n.y));
    }
    let c = p_snow_c();
    let s = p_snow_s();
    let t = 0.5 + 0.5 * ((n.x / p_map_w() * 2.0 - 1.0) * c + (n.y / p_map_h() * 2.0 - 1.0) * s) / max(abs(c) + abs(s), 0.01);
    return ss(0.3, 0.75, t);
}

fn snow_k(x: u32, y: u32, cling: f32) -> f32 {
    let n = nat(x, y);
    let nz = fbm(n.x, n.y, 30.0, 4u, 0.55, p_sd() + 70u) - 0.5;
    let a = p_snow_amount();
    let v = snow_where(x, y) * a * 1.35 * cling + nz * p_snow_patch() * 0.9 - 0.12 + a * 0.12;
    return ss(0.3, 0.62, v);
}

fn snow_color(c: vec3<f32>, k: f32) -> vec3<f32> {
    let lum = clamp(0.3 * c.x + 0.59 * c.y + 0.11 * c.z, 0.0, 1.0);
    let sh = 0.5 + 0.6 * min(lum * 2.4, 1.0);
    let t = vec3<f32>(p_snow_r(), p_snow_g(), p_snow_b());
    return c * (1.0 - k) + t * sh * k;
}

// x: how burnt (0..1), y: how much in flames
fn fire_at(x: u32, y: u32) -> vec2<f32> {
    let n = nat(x, y);
    let nz = fbm(n.x, n.y, 55.0, 4u, 0.55, p_sd() + 90u) - 0.5;
    let a = p_fire_amount();
    let m = p_fire_mode();
    var base = a;
    if m == 1u {
        base = (1.0 - ss(0.0, max(p_fire_reach(), 10.0), sdf_map(n.x, n.y) - inset_map(n.x, n.y))) * a * 1.2;
    } else if m == 2u {
        let c = p_fire_c();
        let s = p_fire_s();
        let t = 0.5 + 0.5 * ((n.x / p_map_w() * 2.0 - 1.0) * c + (n.y / p_map_h() * 2.0 - 1.0) * s) / max(abs(c) + abs(s), 0.01);
        base = ss(0.25, 0.75, t) * (0.6 + 0.6 * a);
    }
    let burn = clamp(base * 1.15 + nz * p_fire_patch() * 2.2 - 0.08, 0.0, 1.0);
    let band = 0.04 + 0.24 * p_fire_flames();
    var flame = 0.0;
    if p_fire_flames() > 0.0 {
        flame = 1.0 - ss(0.0, band, abs(burn - 0.62));
    }
    return vec2<f32>(burn, flame);
}

fn flicker(x: u32, y: u32) -> f32 {
    let n = nat(x, y);
    return fbm(n.x, n.y, 6.0, 3u, 0.6, p_sd() + 91u);
}

fn flame_color(q: f32) -> vec3<f32> {
    let red = vec3<f32>(0.72, 0.12, 0.03);
    let orange = vec3<f32>(1.0, 0.45, 0.06);
    let yellow = vec3<f32>(1.0, 0.86, 0.42);
    if q < 0.5 {
        return red + (orange - red) * (q * 2.0);
    }
    return orange + (yellow - orange) * ((q - 0.5) * 2.0);
}

// flames on a coarse grid, the source of the glow. f0 out; S.u: (gw, gh, cell).
@compute @workgroup_size(256)
fn k_glow_src(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let gw = S.u.x;
    if i >= gw * S.u.y {
        return;
    }
    let c = S.u.z;
    let x = min((i % gw) * c, p_w() - 1u);
    let y = min((i / gw) * c, p_h() - 1u);
    f0[i] = fire_at(x, y).y;
}

// glow grid sampled at a region pixel; `glow` is the bound coarse grid
fn glow_at(x: u32, y: u32) -> f32 {
    if p_glow_on() == 0u {
        return 0.0;
    }
    let gw = i32(p_glow_w());
    let gh = i32(p_glow_h());
    let fx = f32(x) / 4.0;
    let fy = f32(y) / 4.0;
    let x0 = i32(floor(fx));
    let y0 = i32(floor(fy));
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let ax = clamp(x0, 0, gw - 1);
    let bx = clamp(x0 + 1, 0, gw - 1);
    let ay = clamp(y0, 0, gh - 1);
    let by = clamp(y0 + 1, 0, gh - 1);
    let a = f7[u32(ay * gw + ax)] * (1.0 - tx) + f7[u32(ay * gw + bx)] * tx;
    let b = f7[u32(by * gw + ax)] * (1.0 - tx) + f7[u32(by * gw + bx)] * tx;
    return a * (1.0 - ty) + b * ty;
}

fn add_glow(c: vec3<f32>, x: u32, y: u32) -> vec3<f32> {
    let k = glow_at(x, y) * p_fire_glow();
    return c + vec3<f32>(k * 0.6, k * 0.22, k * 0.03);
}

fn fire_crown(c: vec3<f32>, x: u32, y: u32, top: f32) -> vec3<f32> {
    let bf = fire_at(x, y);
    let lum = clamp(0.3 * c.x + 0.59 * c.y + 0.11 * c.z, 0.0, 1.0);
    let scorched = vec3<f32>(0.05 + 0.75 * lum, 0.03 + 0.5 * lum, 0.02 + 0.28 * lum);
    let charred = vec3<f32>(0.05 + 0.34 * lum, 0.045 + 0.29 * lum, 0.04 + 0.26 * lum);
    var o = mix(c, scorched, ss(0.05, 0.4, bf.x));
    o = mix(o, charred, ss(0.35, 0.75, bf.x) * p_fire_char());
    if bf.y > 0.0 {
        let fl = flicker(x, y);
        let q = clamp(bf.y * (0.25 + 0.75 * top) * ss(0.4, 0.8, fl + 0.2 * top), 0.0, 1.0);
        o = mix(o, flame_color(q), ss(0.0, 0.45, q));
    }
    return add_glow(o, x, y);
}

fn fire_ground(c: vec3<f32>, x: u32, y: u32) -> vec3<f32> {
    let bf = fire_at(x, y);
    let lum = clamp(0.3 * c.x + 0.59 * c.y + 0.11 * c.z, 0.0, 1.0);
    let ash = vec3<f32>(0.11 + 0.25 * lum, 0.10 + 0.22 * lum, 0.09 + 0.2 * lum);
    var o = mix(c, ash, ss(0.1, 0.5, bf.x) * p_fire_char());
    if bf.y > 0.0 {
        let fl = flicker(x, y);
        o = mix(o, vec3<f32>(0.55 * (0.6 + 0.8 * fl), 0.12 * (0.6 + 0.8 * fl), 0.02), bf.y * 0.6 * p_fire_embers());
    }
    let gx = i32(x + p_ox());
    let gy = i32(y + p_oy());
    let r = hash(gx, gy, p_sd() + 92u);
    if r < p_fire_embers() * 0.05 * ss(0.3, 0.7, bf.x) {
        o = flame_color(0.4 + 0.6 * hash(gx, gy, p_sd() + 93u));
    }
    return add_glow(o, x, y);
}

// ------------------------------------------------------------------ crown colour (engine.rs sprite_color)

fn ramp_color(sp: u32, t: f32) -> vec3<f32> {
    let tt = clamp(t, 0.0, 1.0) * 3.0;
    let k = min(u32(floor(tt)), 2u);
    let f = tt - f32(k);
    let a = RAMP[sp * 5u + k].xyz;
    let b = RAMP[sp * 5u + k + 1u].xyz;
    return a * (1.0 - f) + b * f;
}

fn species_index(sp: u32) -> u32 {
    return min(sp, p_n_species() - 1u);
}

fn soft_knee(v: f32) -> f32 {
    if v > 1.0 {
        return 1.0 + (v - 1.0) * 0.5;
    }
    return v;
}

fn tone_noise(x: f32, y: f32, size0: f32, sd: u32) -> f32 {
    let size = max(size0, 4.0);
    let a = value(x / (size * 2.0), y / (size * 2.0), sd + 301u) - 0.5;
    let b = value(x / size, y / size, sd + 302u) - 0.5;
    let c = value(x / (size * 0.5), y / (size * 0.5), sd + 303u) - 0.5;
    return (0.55 * a + 0.35 * b + 0.2 * c) / 0.12;
}

fn sprite_color(gx: u32, gy: u32, nx: f32, ny: f32, depth0: f32, tone: f32, tree_tone: f32, cls: u32, hb: f32, b3: f32, b9: f32, bx: f32, by: f32, rel: f32, sp: u32, recol: vec3<f32>) -> vec3<f32> {
    let s = p_s();
    let lv = vec3<f32>(p_lvx(), p_lvy(), p_lvz());
    let n = sqrt(nx * nx + ny * ny + 1.0);
    var diff = 0.63;
    if p_flat() == 0u {
        diff = clamp((nx * lv.x + ny * lv.y + lv.z) / n, 0.0, 1.0);
    }
    let ao = clamp(1.0 - max(b3 - hb, 0.0) / (6.0 * s), 0.0, 1.0) * clamp(1.0 - max(b9 - hb, 0.0) / (18.0 * s), 0.0, 1.0);
    if RAMP[sp * 5u + 4u].w > 0.5 {
        // colours baked into the sprite: only light them
        let shade = (0.42 + 0.58 * diff) * (0.7 + 0.3 * ao);
        if cls == 2u {
            return clamp(RAMP[sp * 5u + 4u].xyz * shade * (0.75 + 0.5 * tone), vec3<f32>(0.0), vec3<f32>(1.0));
        }
        let f = clamp(tone * 0.9, 0.0, 0.9);
        let cap = mix(ramp_color(sp, clamp(depth0, 0.0, 1.0)), RAMP[sp * 5u + 4u].xyz, f);
        return clamp(cap * shade * recol, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if cls == 2u {
        let k = (0.4 + 0.9 * diff) * (0.5 + 0.5 * ao) * (0.55 + 1.1 * tone);
        return RAMP[sp * 5u + 4u].xyz * k;
    }
    let depth = clamp(depth0, 0.0, 1.0);
    let dk = clamp(0.4 * p_depth_strength(), 0.0, 0.95);
    let mx = (f32(gx) + 0.5) * p_inv();
    let my = (f32(gy) + 0.5) * p_inv();
    let sd = p_sd();
    // depth_mod
    let occl = clamp(1.0 + p_crown_shade() * 0.6 * tanh(rel), 0.3, 1.6);
    let raw = tone_noise(mx, my, p_crown_tone_size(), sd);
    let tn = 1.5 * tanh(raw / 1.5) * p_crown_tone();
    let dm = occl * exp(0.5 * tn);
    var shade = pow(max((0.4 + 0.9 * diff) * (0.5 + 0.5 * ao) * ((1.0 - dk) + dk * pow(depth, 0.8)) * dm, 0.0), p_gamma()) * p_gain();
    shade = soft_knee(shade * exp(3.0 * p_crown_var() * tree_tone));
    var t = 0.2 + 0.5 * depth + (tone - 0.5) * 0.4 * p_cluster_tone() * clamp(p_variety() / 1.5, 0.0, 2.0) + 0.3 * (diff - 0.5) + tree_tone * (1.0 + 2.5 * p_crown_var());
    if p_mottling() > 0.0 {
        let ms = max(p_mottle_size(), 1.0);
        t += (value(mx / ms, my / ms, sd + 91u) - 0.5) * 0.45 * p_mottling() * 0.5;
    }
    var c = ramp_color(sp, t) * shade * recol;
    if p_dappled() > 0.0 {
        let fc = clamp((-bx - by) * 0.35 / s, 0.0, 1.0) * p_dappled();
        c.x *= 1.0 + fc * 0.28;
        c.y *= 1.0 + fc * 0.2;
        c.z *= 1.0 - fc * 0.05;
    }
    if p_tree_lean() > 0.0 {
        let hv = (hash(i32(tree_tone * 1e5), 7, sd + 7u) - 0.5) * 2.4 * 0.14 * p_tree_lean();
        c.x *= 1.0 + hv * 0.8;
        c.y *= 1.0 + hv * 0.3;
        c.z *= 1.0 - hv;
    }
    c.x *= 1.0 + 0.04 * tn;
    c.z *= 1.0 - 0.06 * tn;
    let m = max(c.x, max(c.y, c.z));
    return clamp(c * select(1.0, 1.0 / m, m > 1.0), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn store_color(i: u32, c: vec3<f32>, a: f32) {
    u2[2u * i] = pack2x16float(c.xy);
    u2[2u * i + 1u] = pack2x16float(vec2<f32>(c.z, a));
}

// final alpha after the blurred-mode fade and the clip to the drawn area
fn finish_alpha(a0: f32, i: u32, painted: bool, painted_blur: f32) -> f32 {
    var a = a0;
    if p_realistic() == 0u {
        let fe = p_feather();
        let fade = ss(-fe * 0.5, fe * 0.5, f8[i]);
        if S.u.x == 1u {
            a *= max(fade, painted_blur);
        } else {
            a *= fade;
        }
    }
    if p_clip_on() == 1u && !painted {
        a *= f9[i];
    }
    return a;
}

// One tree layer. f0 height, f1 blurred (5s), f2 b3, f3 b9, f4 b_big, f5 alpha, f6 painted blur, f7 glow,
// f8 depth, f9 area; u0 sprite tilt/tone/depth bytes, u1 class/species/painted/tree tone, u2 out colour.
// S.f: recolour; S.u.x: layer has painted trees.
@compute @workgroup_size(256)
fn k_shade_tree(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = p_w();
    let h = p_h();
    if i >= w * h {
        return;
    }
    let x = i % w;
    let y = i / w;
    let alpha = f5[i];
    if alpha <= 0.0 {
        store_color(i, vec3<f32>(0.0), 0.0);
        return;
    }
    let w1 = u0[i];
    let w2 = u1[i];
    let cls = w2 & 0x7fu;
    let painted = ((w2 >> 7u) & 1u) == 1u;
    let tnx = f32(bitcast<i32>(w1 << 24u) >> 24u) / 80.0;
    let tny = f32(bitcast<i32>(w1 << 16u) >> 24u) / 80.0;
    let hb = f0[i];
    let rel_scale = p_rel_scale();
    let rel = (hb - f4[i]) * rel_scale;
    let recol = S.f.xyz;
    var c: vec3<f32>;
    if cls != 0u {
        let spr_tone = f32((w1 >> 16u) & 0xffu) / 255.0;
        let spr_depth = f32((w1 >> 24u) & 0xffu) / 255.0;
        let tree_tone = f32(bitcast<i32>(w2) >> 16u) / 8192.0;
        let sp = species_index((w2 >> 8u) & 0xffu);
        let xl = max(i32(x) - 1, 0);
        let xr = min(i32(x) + 1, i32(w) - 1);
        let yu = max(i32(y) - 1, 0);
        let yd = min(i32(y) + 1, i32(h) - 1);
        let bx = (f1[y * w + u32(xr)] - f1[y * w + u32(xl)]) * 0.5;
        let by = (f1[u32(yd) * w + x] - f1[u32(yu) * w + x]) * 0.5;
        c = sprite_color(x + p_ox(), y + p_oy(), tnx, tny, spr_depth, spr_tone, tree_tone, cls, hb, f2[i], f3[i], bx, by, rel, sp, recol);
    } else {
        // the thin edge just outside a tree: a dark rim in the colours of the tree beside it
        var sp = 0u;
        var found = false;
        for (var dy = -1; dy <= 1; dy++) {
            for (var dx = -1; dx <= 1; dx++) {
                let xx = i32(x) + dx;
                let yy = i32(y) + dy;
                if !found && xx >= 0 && yy >= 0 && xx < i32(w) && yy < i32(h) {
                    let j = u32(yy) * w + u32(xx);
                    if (u1[j] & 0x7fu) != 0u {
                        sp = (u1[j] >> 8u) & 0xffu;
                        found = true;
                    }
                }
            }
        }
        c = ramp_color(species_index(sp), 0.1) * 0.85 * recol;
    }
    if p_snow_amount() > 0.0 {
        let up = 1.0 - min(length(vec2<f32>(tnx, tny)) * 1.2, 1.0);
        let top = clamp(0.5 + 1.2 * rel, 0.0, 1.0) * up;
        let cling = 1.0 - p_snow_tops() * (1.0 - top);
        let k = snow_k(x, y, cling);
        if k > 0.0 {
            c = snow_color(c, k);
        }
    }
    if p_fire_amount() > 0.0 {
        let up = 1.0 - min(length(vec2<f32>(tnx, tny)) * 1.2, 1.0);
        let top = clamp(0.5 + 1.2 * rel, 0.0, 1.0) * (0.5 + 0.5 * up);
        c = fire_crown(c, x, y, top);
    }
    store_color(i, c, finish_alpha(alpha, i, painted, f6[i]));
}

// Grass (realistic) or the dark canopy floor (blurred mode). f0 grass cover, f7 glow, f8 depth, f9 area,
// u2 out colour.
@compute @workgroup_size(256)
fn k_grass(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = p_w();
    if i >= w * p_h() {
        return;
    }
    let x = i % w;
    let y = i / w;
    let n = nat(x, y);
    let sd = p_sd();
    if p_realistic() == 0u {
        let c = ramp_color(species_index(p_main_species()), 0.12 + 0.3 * fbm(n.x, n.y, 6.0, 3u, 0.6, sd + 40u)) * 0.7;
        store_color(i, c, finish_alpha(1.0, i, false, 0.0));
        return;
    }
    let ga = f0[i];
    if ga <= 0.0 {
        store_color(i, vec3<f32>(0.0), 0.0);
        return;
    }
    let gx = i32(x + p_ox());
    let gy = i32(y + p_oy());
    let grain = fbm(n.x, n.y, 2.0, 2u, 0.5, sd + 30u);
    let r = hash(gx, gy, sd + 31u);
    var dots = 0.0;
    if r < 0.04 {
        dots = hash(gx, gy, sd + 32u) - 0.5;
    }
    let m = (0.75 + 0.5 * grain + dots) * (0.8 + 0.4 * fbm(n.x, n.y, 24.0, 3u, 0.5, sd + 33u)) * 0.95;
    let patch_k = clamp((fbm(n.x, n.y, 70.0, 3u, 0.5, sd + 34u) - 0.55) * 7.0, 0.0, 1.0) * 0.55;
    var fleck = 0.0;
    if r > 0.985 {
        fleck = 0.85;
    }
    let k = min(patch_k + fleck, 1.0);
    let base = vec3<f32>(p_grass_r(), p_grass_g(), p_grass_b());
    let acc = vec3<f32>(p_acc_r(), p_acc_g(), p_acc_b());
    var c = (base * (1.0 - k) + acc * k) * m;
    if p_snow_amount() > 0.0 && p_snow_ground() > 0.0 {
        let sk = snow_k(x, y, p_snow_ground());
        if sk > 0.0 {
            c = snow_color(c, sk);
        }
    }
    if p_fire_amount() > 0.0 {
        c = fire_ground(c, x, y);
    }
    var a = ga;
    if p_clip_on() == 1u {
        a *= f9[i];
    }
    store_color(i, c, a);
}

// ------------------------------------------------------------------ compose (compose.rs)

// grade g: three vec4 starting at G.v[3 * g]: (visible, opacity, hue on, cos), (sin, saturation,
// brightness, contrast), (tint rgb, receive shadow)
fn grade(c0: vec3<f32>, g: u32) -> vec3<f32> {
    let a = G.v[3u * g];
    let b = G.v[3u * g + 1u];
    let t = G.v[3u * g + 2u];
    var c = c0 * t.xyz;
    if a.z > 0.5 {
        let cs = a.w;
        let sn = b.x;
        let yy = 0.299 * c.x + 0.587 * c.y + 0.114 * c.z;
        let ii = 0.596 * c.x - 0.274 * c.y - 0.322 * c.z;
        let qq = 0.211 * c.x - 0.523 * c.y + 0.312 * c.z;
        let i2 = ii * cs - qq * sn;
        let q2 = ii * sn + qq * cs;
        c = vec3<f32>(yy + 0.956 * i2 + 0.621 * q2, yy - 0.272 * i2 - 0.647 * q2, yy - 1.106 * i2 + 1.703 * q2);
    }
    let lum = 0.299 * c.x + 0.587 * c.y + 0.114 * c.z;
    let pivot = 0.25;
    var o = vec3<f32>(lum) + (c - vec3<f32>(lum)) * b.y;
    o = o * b.z;
    o = (o - vec3<f32>(pivot)) * b.w + vec3<f32>(pivot);
    return clamp(o, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn blend(acc: vec4<f32>, c: vec3<f32>, a: f32) -> vec4<f32> {
    if a <= 0.0 {
        return acc;
    }
    return vec4<f32>(c * a + acc.xyz * (1.0 - a), a + acc.w * (1.0 - a));
}

// a coloured layer: grade, cast shadow, blend. `l` is the layer index (2 grass .. 6 pebbles).
fn layer_px(acc: vec4<f32>, l: u32, cw0: u32, cw1: u32, shadow: f32) -> vec4<f32> {
    let opts = G.v[24];
    let rg = unpack2x16float(cw0);
    let ba = unpack2x16float(cw1);
    let a0 = G.v[3u * l];
    if a0.x < 0.5 || opts.y > 0.5 || ba.y <= 0.0 {
        return acc;
    }
    let a = ba.y * a0.y;
    if a <= 0.0 {
        return acc;
    }
    var c = grade(vec3<f32>(rg.x, rg.y, ba.x), l);
    c = grade(c, 7u);
    var has_shadow = G.v[27].x > 0.5;
    if l < 6u {
        has_shadow = G.v[26][l - 2u] > 0.5;
    }
    if opts.x > 0.5 && G.v[3u * l + 2u].w > 0.5 && has_shadow {
        c = c * (1.0 - G.v[25].w * 0.92 * shadow);
    }
    return blend(acc, c, a);
}

// f0 haze alpha, f1 ground shadow, f2..f6 shadows on grass, shrubs, canopy, lone trees, pebbles; u0..u4
// their colours. G.v[24]: (shadows, shadow only, background on, 0); G.v[25]: (shadow rgb, opacity);
// G.v[26..27]: which layers have shadows (grass, shrub, canopy, lone | pebble); G.v[28]: background;
// G.v[29]: haze colour; G.v[30].x: ground shadow present.
@compute @workgroup_size(256)
fn k_compose(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = p_w();
    if i >= w * p_h() {
        return;
    }
    let opts = G.v[24];
    var acc = vec4<f32>(0.0);
    if opts.z > 0.5 {
        acc = vec4<f32>(G.v[28].xyz, 1.0);
    }
    // haze
    let hz = G.v[0];
    if hz.x > 0.5 && opts.y < 0.5 {
        let a = f0[i] * hz.y;
        if a > 0.0 {
            var c = grade(G.v[29].xyz, 0u);
            c = grade(c, 7u);
            acc = blend(acc, c, a);
        }
    }
    // ground shadow
    let gs = G.v[3];
    if gs.x > 0.5 && opts.x > 0.5 && G.v[30].x > 0.5 {
        acc = blend(acc, G.v[25].xyz, f1[i] * G.v[25].w * gs.y);
    }
    acc = layer_px(acc, 2u, u0[2u * i], u0[2u * i + 1u], f2[i]);
    acc = layer_px(acc, 3u, u1[2u * i], u1[2u * i + 1u], f3[i]);
    acc = layer_px(acc, 4u, u2[2u * i], u2[2u * i + 1u], f4[i]);
    acc = layer_px(acc, 5u, u3[2u * i], u3[2u * i + 1u], f5[i]);
    acc = layer_px(acc, 6u, u4[2u * i], u4[2u * i + 1u], f6[i]);
    let a = clamp(acc.w, 0.0, 1.0);
    var rgb = vec3<f32>(0.0);
    if opts.w > 0.5 {
        rgb = clamp(acc.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
    } else if a > 1e-4 {
        rgb = clamp(acc.xyz / a, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    textureStore(out_tex, vec2<i32>(i32(i % w), i32(i / w)), vec4<f32>(rgb, a));
}
