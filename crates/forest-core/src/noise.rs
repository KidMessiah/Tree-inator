//! Seeded value noise and fBm. Coordinates are in map (native) pixels.

#[inline]
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Per-integer-coordinate random number in [0, 1).
#[inline]
pub fn hash01(x: i32, y: i32, seed: u32) -> f32 {
    hash(x, y, seed)
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Smooth value noise in [0, 1], one unit per lattice cell.
#[inline]
pub fn value(x: f32, y: f32, seed: u32) -> f32 {
    let xf = x.floor();
    let yf = y.floor();
    let (xi, yi) = (xf as i32, yf as i32);
    let (tx, ty) = (fade(x - xf), fade(y - yf));
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    let top = a + (b - a) * tx;
    let bot = c + (d - c) * tx;
    top + (bot - top) * ty
}

/// fBm normalised to roughly [0, 1]. `period` is the largest feature size in pixels.
#[inline]
pub fn fbm(x: f32, y: f32, period: f32, octaves: u32, gain: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut tot = 0.0;
    let mut amp = 1.0;
    let mut p = period;
    for o in 0..octaves {
        sum += amp * value(x / p, y / p, seed.wrapping_add(o * 1013));
        tot += amp;
        amp *= gain;
        p = (p * 0.5).max(1.0);
    }
    sum / tot
}

/// fBm sampled at a domain-warped position.
#[inline]
pub fn fbm_warped(x: f32, y: f32, period: f32, octaves: u32, seed: u32, warp_amt: f32, warp_period: f32) -> f32 {
    let dx = (fbm(x, y, warp_period, 3, 0.5, seed ^ 0x51ed) - 0.5) * warp_amt;
    let dy = (fbm(x, y, warp_period, 3, 0.5, seed ^ 0x2a17) - 0.5) * warp_amt;
    fbm(x + dx, y + dy, period, octaves, 0.5, seed)
}

#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
