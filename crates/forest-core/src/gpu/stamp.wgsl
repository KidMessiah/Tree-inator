// Stamping and ground cover kernels, appended to kernels.wgsl. They follow engine.rs `stamp_sprite` and
// `stamp_region`.

// ------------------------------------------------------------------ stamping

// Instances: 16 words each in u2: x0, y0 (render pixels, whole map), atlas offset, size, turns | mirror << 2,
// height scale, class byte template (painted, species, tree tone), 1 when the crown is pulled in by crown
// shyness, then the 24 pull-in factors, four bytes to a word (words 8 to 13). Atlas: f0 heights, u0 packed tilt, tone
// and depth bytes, u1 classes. u5 tallest height so far per pixel, u6 which tree won it.

struct Hit {
    i: u32,
    si: u32,
    ok: bool,
}

// An angle that grows with the true angle, 0 to 4 round the circle (shy.rs `diamond`).
fn diamond(ox: f32, oy: f32) -> f32 {
    let p = oy / max(abs(ox) + abs(oy), 1e-6);
    if ox >= 0.0 {
        if oy >= 0.0 {
            return p;
        }
        return 4.0 + p;
    }
    return 2.0 - p;
}

fn shy_byte(base: u32, k: i32) -> f32 {
    let s = u32(k);
    return f32((u2[base + 8u + s / 4u] >> (8u * (s % 4u))) & 255u);
}

// how far the crown is pulled in at an offset from its centre (shy.rs `factor_at`)
fn shy_factor(base: u32, ox: f32, oy: f32) -> f32 {
    let u = diamond(ox, oy) * 6.0 - 0.5;
    let k = floor(u);
    let t = u - k;
    let ka = ((i32(k) % 24) + 24) % 24;
    let kb = (ka + 1) % 24;
    let a = shy_byte(base, ka);
    let b = shy_byte(base, kb);
    return (a + (b - a) * t) / 255.0;
}

// pixel p of instance `inst`: where it lands and which atlas pixel it shows
fn stamp_hit(inst: u32, p: u32) -> Hit {
    let base = inst * 16u;
    let n = u2[base + 3u];
    var h: Hit;
    h.ok = false;
    h.i = 0u;
    h.si = 0u;
    let dx = p % n;
    let dy = p / n;
    let px = bitcast<i32>(u2[base]) - i32(p_ox()) + i32(dx);
    let py = bitcast<i32>(u2[base + 1u]) - i32(p_oy()) + i32(dy);
    if px < 0 || py < 0 || px >= i32(p_w()) || py >= i32(p_h()) {
        return h;
    }
    // a shy crown is pulled in toward its centre, so look further out in the sprite
    var sx = i32(dx);
    var sy = i32(dy);
    if u2[base + 7u] != 0u {
        let nf = f32(n);
        let ox = f32(dx) + 0.5 - nf * 0.5;
        let oy = f32(dy) + 0.5 - nf * 0.5;
        let kf = shy_factor(base, ox, oy);
        sx = i32(floor(nf * 0.5 + ox / kf));
        sy = i32(floor(nf * 0.5 + oy / kf));
        if sx < 0 || sy < 0 || sx >= i32(n) || sy >= i32(n) {
            return h;
        }
    }
    let orient = u2[base + 4u];
    let k = orient & 3u;
    var ux = sx;
    if (orient >> 2u) == 1u {
        ux = i32(n) - 1 - sx;
    }
    var uy = sy;
    for (var q = 0u; q < k; q++) {
        let a = uy;
        let b = i32(n) - 1 - ux;
        ux = a;
        uy = b;
    }
    h.si = u2[base + 2u] + u32(uy) * n + u32(ux);
    h.i = u32(py) * p_w() + u32(px);
    h.ok = u1[h.si] != 0u;
    return h;
}

// i is the destination pixel; the weave jitter hashes its map-wide coordinates, as the CPU does
fn stamp_z(inst: u32, si: u32, i: u32) -> f32 {
    let base = inst * 16u;
    var z = f0[si] * bitcast<f32>(u2[base + 5u]);
    let wv = bitcast<f32>(u2[base + 15u]);
    if wv > 0.0 {
        let top = bitcast<f32>(u2[base + 14u]);
        let b = 0.8 * wv;
        let j = 0.35 * wv;
        let gx = i32(i % p_w()) + i32(p_ox());
        let gy = i32(i / p_w()) + i32(p_oy());
        let species = (u2[base + 6u] >> 8u) & 0xffu;
        z = top * (b + (1.0 - b) * (z / max(top, 1e-3))) + top * j * (hash(gx, gy, species + 1717u) - 0.5);
    }
    return z;
}

@compute @workgroup_size(256)
fn k_stamp_z(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let inst = wid.x + wid.y * nwg.x;
    if inst >= S.u.x {
        return;
    }
    let n = u2[inst * 16u + 3u];
    for (var p = li; p < n * n; p += 256u) {
        let h = stamp_hit(inst, p);
        if h.ok {
            let z = stamp_z(inst, h.si, h.i);
            if z > 0.0 {
                atomicMax(&u5[h.i], bitcast<u32>(z));
            }
        }
    }
}

// among trees reaching the same height, the first one drawn wins, as on the CPU
@compute @workgroup_size(256)
fn k_stamp_win(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let inst = wid.x + wid.y * nwg.x;
    if inst >= S.u.x {
        return;
    }
    let n = u2[inst * 16u + 3u];
    for (var p = li; p < n * n; p += 256u) {
        let h = stamp_hit(inst, p);
        if h.ok {
            let z = stamp_z(inst, h.si, h.i);
            if z > 0.0 && bitcast<u32>(z) == atomicLoad(&u5[h.i]) {
                atomicMax(&u6[h.i], 0xffffffffu - inst);
            }
        }
    }
}

// the winner writes its tilt, tone, depth, class, species, painted flag and tree tone; u3, u4 out
@compute @workgroup_size(256)
fn k_stamp_attr(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let inst = wid.x + wid.y * nwg.x;
    if inst >= S.u.x {
        return;
    }
    let base = inst * 16u;
    let n = u2[base + 3u];
    let orient = u2[base + 4u];
    for (var p = li; p < n * n; p += 256u) {
        let h = stamp_hit(inst, p);
        if h.ok {
            let z = stamp_z(inst, h.si, h.i);
            if z > 0.0 && atomicLoad(&u6[h.i]) == 0xffffffffu - inst {
                let a = u0[h.si];
                var nx = bitcast<i32>(a << 24u) >> 24u;
                var ny = bitcast<i32>(a << 16u) >> 24u;
                for (var q = 0u; q < (orient & 3u); q++) {
                    let t = -ny;
                    ny = nx;
                    nx = t;
                }
                if (orient >> 2u) == 1u {
                    nx = -nx;
                }
                u3[h.i] = (bitcast<u32>(nx) & 0xffu) | ((bitcast<u32>(ny) & 0xffu) << 8u) | (a & 0xffff0000u);
                u4[h.i] = (u1[h.si] & 0x7fu) | u2[base + 6u];
            }
        }
    }
}

// ------------------------------------------------------------------ ground

fn unitn(v: f32) -> f32 {
    return (v - 0.5) / 0.12;
}

fn fbm_warped(x: f32, y: f32, period: f32, octaves: u32, seed: u32, warp_amt: f32, warp_period: f32) -> f32 {
    let dx = (fbm(x, y, warp_period, 3u, 0.5, seed ^ 0x51edu) - 0.5) * warp_amt;
    let dy = (fbm(x, y, warp_period, 3u, 0.5, seed ^ 0x2a17u) - 0.5) * warp_amt;
    return fbm(x + dx, y + dy, period, octaves, 0.5, seed);
}

fn apply_pattern(nx: f32, ny: f32, dist: f32, sd: u32) -> f32 {
    let k = p_pattern_k();
    let pat = p_pattern();
    if k <= 0.0 || pat == 0u {
        return dist;
    }
    let sc = p_pattern_sc();
    var goal = dist;
    if pat == 1u {
        let n = unitn(fbm_warped(nx, ny, sc, 6u, sd + 50u, sc * 0.5, sc));
        let fine = unitn(fbm(nx, ny, sc * 0.3, 4u, 0.55, sd + 51u));
        let band = 1.0 - ss(0.4 * sc, 1.6 * sc, dist);
        var push = (n * 0.8 + fine * 0.35) * sc * 0.7;
        if push > 0.0 {
            push = push * 0.3;
        }
        let isl = unitn(fbm(nx, ny, sc * 0.35, 4u, 0.5, sd + 56u));
        let islands = (isl - 1.3) * sc * 0.25 - max(-dist, 0.0) * 0.6;
        goal = max(dist + push * band, islands);
    } else if pat == 2u {
        let z = abs(unitn(fbm_warped(nx, ny, sc, 5u, sd + 52u, sc * 0.6, sc * 0.8))) + 0.2 * (fbm(nx, ny, sc * 0.2, 3u, 0.5, sd + 53u) - 0.5);
        var wdt = clamp(p_branch_width(), 0.05, 1.0);
        wdt = wdt + (1.0 - wdt) * (1.0 - k) * 0.8;
        let a = 0.85 * wdt;
        var along: f32;
        if z < a {
            along = (1.0 - z / a) * 90.0;
        } else {
            along = (a - z) * sc * 0.5;
        }
        return min(dist, max(along, dist - sc * 1.5));
    } else if pat == 3u {
        let n = unitn(fbm_warped(nx, ny, sc * 0.6, 5u, sd + 54u, sc * 0.3, sc * 0.6)) + 0.2;
        return min(dist, n * sc * 0.4 + (1.0 - k) * sc);
    } else {
        let n = unitn(fbm_warped(nx, ny, sc * 1.5, 3u, sd + 55u, sc, sc * 1.5));
        var push = n * sc * 0.9;
        if push > 0.0 {
            push = push * 0.3;
        }
        goal = dist + push * (1.0 - ss(0.6 * sc, 2.4 * sc, dist));
    }
    return dist + (goal - dist) * k;
}

fn depth_value(nx: f32, ny: f32, inset: f32, wk_in: f32) -> f32 {
    let env = wk_in > 1.5;
    let wk = min(wk_in, 1.0);
    let sd = p_sd();
    let band = p_band();
    let dx = (fbm(nx, ny, 160.0, 3u, 0.5, sd + 10u) - 0.5) * p_edge_warp() * wk;
    let dy = (fbm(nx, ny, 160.0, 3u, 0.5, sd + 9u) - 0.5) * p_edge_warp() * wk;
    var dist = sdf_map(nx + dx, ny + dy) - inset;
    if dist < -4.0 * band - p_pattern_scale_raw() * 2.0 {
        return -1.0;
    }
    dist += p_edge_noise() * band * 2.0 * (fbm(nx, ny, 64.0, 4u, 0.5, sd + 11u) - 0.5);
    dist = apply_pattern(nx, ny, dist, sd);
    if env {
        dist = min(dist, 3.0 * (sdf_map(nx, ny) - inset));
    }
    if dist >= 0.0 {
        return min(dist / band, 1.0);
    }
    return max(dist / (2.0 * band), -1.0);
}

// depth every `fine` pixels. f0 out; S.u: (coarse w, coarse h, fine)
@compute @workgroup_size(256)
fn k_depth_coarse(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let cw = S.u.x;
    if i >= cw * S.u.y {
        return;
    }
    let x = min((i % cw) * S.u.z, p_w() - 1u);
    let y = min((i / cw) * S.u.z, p_h() - 1u);
    let n = nat(x, y);
    f0[i] = depth_value(n.x, n.y, inset_map(n.x, n.y), warp_map(n.x, n.y));
}

// field.rs `upsample`: f0 coarse (cw, ch) every `step` pixels into f1 (w, h). S.u: (cw, ch, step)
@compute @workgroup_size(256)
fn k_upsample(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = p_w();
    if i >= w * p_h() {
        return;
    }
    let cw = S.u.x;
    let ch = S.u.y;
    let inv = 1.0 / f32(S.u.z);
    let fx = f32(i % w) * inv;
    let fy = f32(i / w) * inv;
    let x0 = min(u32(fx), cw - 1u);
    let y0 = min(u32(fy), ch - 1u);
    let x1 = min(x0 + 1u, cw - 1u);
    let y1 = min(y0 + 1u, ch - 1u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let a = f0[y0 * cw + x0] + (f0[y0 * cw + x1] - f0[y0 * cw + x0]) * tx;
    let b = f0[y1 * cw + x0] + (f0[y1 * cw + x1] - f0[y1 * cw + x0]) * tx;
    f1[i] = a + (b - a) * ty;
}

// the plan's ranked noise at a map point, `which` 0 haze, 1 grass
fn noise_map(which: u32, nx: f32, ny: f32) -> f32 {
    var w = p_uh_w();
    var h = p_uh_h();
    if which == 1u {
        w = p_ug_w();
        h = p_ug_h();
    }
    let fine = f32(p_plan_fine());
    let fx = clamp((nx * p_plan_s() - 0.5) / fine, 0.0, f32(w - 1u));
    let fy = clamp((ny * p_plan_s() - 0.5) / fine, 0.0, f32(h - 1u));
    let x0 = u32(fx);
    let y0 = u32(fy);
    let x1 = min(x0 + 1u, w - 1u);
    let y1 = min(y0 + 1u, h - 1u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    if which == 1u {
        let a = UGRASS[y0 * w + x0] * (1.0 - tx) + UGRASS[y0 * w + x1] * tx;
        let b = UGRASS[y1 * w + x0] * (1.0 - tx) + UGRASS[y1 * w + x1] * tx;
        return a * (1.0 - ty) + b * ty;
    }
    let a = UHAZE[y0 * w + x0] * (1.0 - tx) + UHAZE[y0 * w + x1] * tx;
    let b = UHAZE[y1 * w + x0] * (1.0 - tx) + UHAZE[y1 * w + x1] * tx;
    return a * (1.0 - ty) + b * ty;
}

// haze cover from depth. f0 depth, f1 out
@compute @workgroup_size(256)
fn k_haze(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = p_w();
    if i >= w * p_h() {
        return;
    }
    if p_realistic() == 0u {
        f1[i] = 0.0;
        return;
    }
    let cover = ss(-p_haze_reach(), -0.1, f0[i]);
    if cover <= 0.0 {
        f1[i] = 0.0;
        return;
    }
    let n = nat(i % w, i / w);
    f1[i] = ss(0.0, 0.15, cover - noise_map(0u, n.x, n.y)) * p_haze_opacity() * (0.6 + 0.8 * fbm(n.x, n.y, 2.0, 2u, 0.5, p_sd() + 24u));
}

// single grass blades from depth. f0 depth, f1 out
@compute @workgroup_size(256)
fn k_grass_imp(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    let w = p_w();
    if i >= w * p_h() {
        return;
    }
    if p_realistic() == 0u {
        f1[i] = 0.0;
        return;
    }
    let cover = ss(-p_grass_reach(), 0.0, f0[i]);
    if cover <= 0.0 {
        f1[i] = 0.0;
        return;
    }
    let x = i % w;
    let y = i / w;
    let n = nat(x, y);
    let soft = ss(-0.02, 0.18, cover - noise_map(1u, n.x, n.y));
    let dens = soft * (0.5 + fbm(n.x, n.y, 16.0, 3u, 0.5, p_sd() + 28u)) * p_grass_density();
    f1[i] = select(0.0, 1.0, hash(i32(x + p_ox()), i32(y + p_oy()), p_sd() + 29u) < dens * 0.35);
}

// blurred blades into cover, f0 in, f1 out
@compute @workgroup_size(256)
fn k_grass_a(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    if i >= p_w() * p_h() {
        return;
    }
    f1[i] = 1.0 - exp(-f0[i] * 6.0 * 1.6);
}

// soft edge of the drawn area, f0 blurred mask, f1 out
@compute @workgroup_size(256)
fn k_area(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let i = flat_index(gid, nwg);
    if i >= p_w() * p_h() {
        return;
    }
    f1[i] = ss(0.25, 0.75, f0[i]);
}
