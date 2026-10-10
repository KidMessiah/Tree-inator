//! Float raster helpers: parallel fill, box blur, rank equalisation.

use rayon::prelude::*;

#[derive(Clone)]
pub struct Field {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

impl Field {
    pub fn new(w: usize, h: usize) -> Self {
        Field { w, h, data: vec![0.0; w * h] }
    }

    /// Fill in parallel by rows from a per-pixel function.
    pub fn from_fn(w: usize, h: usize, f: impl Fn(usize, usize) -> f32 + Sync) -> Self {
        let mut data = vec![0.0; w * h];
        data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, v) in row.iter_mut().enumerate() {
                *v = f(x, y);
            }
        });
        Field { w, h, data }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.w + x]
    }

    /// Clamped lookup with signed coordinates.
    #[inline]
    pub fn at(&self, x: i32, y: i32) -> f32 {
        let x = x.clamp(0, self.w as i32 - 1) as usize;
        let y = y.clamp(0, self.h as i32 - 1) as usize;
        self.data[y * self.w + x]
    }

    /// Approximate gaussian blur: three box passes per axis.
    pub fn blur(&self, radius: f32) -> Field {
        if radius < 0.5 {
            return self.clone();
        }
        let (w, h) = (self.w, self.h);
        // Wide blurs are soft enough to work out at half size.
        if radius >= 8.0 && w >= 64 && h >= 64 {
            let (hw, hh) = (w.div_ceil(2), h.div_ceil(2));
            let half = Field::from_fn(hw, hh, |x, y| {
                let (x0, y0) = (2 * x, 2 * y);
                let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
                0.25 * (self.data[y0 * w + x0] + self.data[y0 * w + x1] + self.data[y1 * w + x0] + self.data[y1 * w + x1])
            });
            let b = half.box3(half_radii(((radius * 0.9).round() as usize).max(1)));
            // half-size pixel centres sit at 2x + 0.5 in full-size pixels
            return Field::from_fn(w, h, |x, y| {
                let fx = ((x as f32 - 0.5) * 0.5).max(0.0);
                let fy = ((y as f32 - 0.5) * 0.5).max(0.0);
                let (x0, y0) = (fx as usize, fy as usize);
                let (x1, y1) = ((x0 + 1).min(hw - 1), (y0 + 1).min(hh - 1));
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let r0 = b.data[y0 * hw + x0] * (1.0 - tx) + b.data[y0 * hw + x1] * tx;
                let r1 = b.data[y1 * hw + x0] * (1.0 - tx) + b.data[y1 * hw + x1] * tx;
                r0 * (1.0 - ty) + r1 * ty
            });
        }
        let r = ((radius * 0.9).round() as usize).max(1);
        self.box3([r; 3])
    }

    /// Three box passes along rows then columns, with these radii.
    fn box3(&self, rs: [usize; 3]) -> Field {
        let mut a = self.data.clone();
        box_rows3(&mut a, self.w, rs);
        box_cols3(&mut a, self.w, self.h, rs);
        Field { w: self.w, h: self.h, data: a }
    }

    /// A smooth field worked out every `step` pixels and filled in between, for a `w` x `h` map.
    /// `f` gets full resolution pixel coordinates.
    pub fn coarse(w: usize, h: usize, step: usize, f: impl Fn(usize, usize) -> f32 + Sync) -> Field {
        let (gw, gh) = ((w - 1) / step + 2, (h - 1) / step + 2);
        Field::from_fn(gw, gh, |gx, gy| f((gx * step).min(w - 1), (gy * step).min(h - 1)))
    }

    /// Fill a `coarse` field back out to `w` x `h` pixels.
    pub fn upsample(&self, w: usize, h: usize, step: usize) -> Field {
        let inv = 1.0 / step as f32;
        let mut data = vec![0f32; w * h];
        data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let fy = y as f32 * inv;
            let y0 = (fy as usize).min(self.h - 1);
            let y1 = (y0 + 1).min(self.h - 1);
            let ty = fy - y0 as f32;
            let (r0, r1) = (&self.data[y0 * self.w..(y0 + 1) * self.w], &self.data[y1 * self.w..(y1 + 1) * self.w]);
            for (x, v) in row.iter_mut().enumerate() {
                let fx = x as f32 * inv;
                let x0 = (fx as usize).min(self.w - 1);
                let x1 = (x0 + 1).min(self.w - 1);
                let tx = fx - x0 as f32;
                let a = r0[x0] + (r0[x1] - r0[x0]) * tx;
                let b = r1[x0] + (r1[x1] - r1[x0]) * tx;
                *v = a + (b - a) * ty;
            }
        });
        Field { w, h, data }
    }

    /// Replace values by their rank in [0, 1] so thresholds become exact coverage fractions.
    pub fn rank(&self) -> Field {
        let n = self.data.len();
        let step = (n / 65_536).max(1);
        let mut sample: Vec<f32> = self.data.iter().step_by(step).copied().collect();
        sample.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let m = sample.len() as f32;
        let data = self
            .data
            .par_iter()
            .map(|v| sample.partition_point(|s| s < v) as f32 / m)
            .collect();
        Field { w: self.w, h: self.h, data }
    }
}

/// One running box sum over `src` into `dst`, reading past the ends as the end value.
#[inline]
fn box_line(src: &[f32], dst: &mut [f32], r: usize) {
    let n = src.len() as isize;
    let ri = r as isize;
    let norm = 1.0 / (2 * r + 1) as f32;
    let at = |i: isize| src[i.clamp(0, n - 1) as usize];
    let mut acc = 0.0;
    for i in -ri..=ri {
        acc += at(i);
    }
    for x in 0..n {
        dst[x as usize] = acc * norm;
        acc += at(x + ri + 1) - at(x - ri);
    }
}

/// Box radii for three passes on a half-size grid that spread as much as three passes of radius `r` at
/// full size. A box of radius k spreads by k(k + 1) / 3 squared pixels, so the half-size passes should sum
/// to a quarter of the full-size spread.
pub(crate) fn half_radii(r: usize) -> [usize; 3] {
    let target = 3.0 * (r * (r + 1)) as f32 / 4.0;
    let k = ((target / 3.0).sqrt() as usize).max(1);
    let mut best = ([k; 3], f32::MAX);
    for a in k.saturating_sub(1).max(0)..=k + 1 {
        for b in a..=k + 1 {
            for c in b..=k + 1 {
                let got = (a * (a + 1) + b * (b + 1) + c * (c + 1)) as f32;
                let err = (got - target).abs();
                if err < best.1 {
                    best = ([a, b, c], err);
                }
            }
        }
    }
    best.0
}

/// Stretches of `n` items worth blurring: runs of non-zero values (as told by `busy`) widened by the blur
/// `reach`, and joined when they would touch. A box blur of zeros is zero, so everything else stays zero.
fn busy_runs(n: usize, reach: usize, busy: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if !busy(i) {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && busy(i) {
            i += 1;
        }
        let (a, b) = (start.saturating_sub(reach), (i + reach).min(n));
        match runs.last_mut() {
            Some(last) if a <= last.1 => last.1 = b,
            _ => runs.push((a, b)),
        }
    }
    runs
}

/// Three box passes along every row, skipping empty stretches.
fn box_rows3(a: &mut [f32], w: usize, rs: [usize; 3]) {
    let reach = rs.iter().sum::<usize>() + 1;
    a.par_chunks_mut(w).for_each(|row| {
        let runs = busy_runs(w, reach, |x| row[x] != 0.0);
        for (x0, x1) in runs {
            // a run that stops short of the row's end has zeros past it, so treating its ends as edges
            // changes nothing
            let seg = &mut row[x0..x1];
            let mut tmp = seg.to_vec();
            box_line(&tmp, seg, rs[0]);
            box_line(seg, &mut tmp, rs[1]);
            box_line(&tmp, seg, rs[2]);
        }
    });
}

/// Three box passes down every column, in strips of columns so memory is read in order, skipping empty
/// stretches.
fn box_cols3(a: &mut [f32], w: usize, h: usize, rs: [usize; 3]) {
    const STRIP: usize = 32;
    let reach = rs.iter().sum::<usize>() + 1;
    let strips: Vec<usize> = (0..w).step_by(STRIP).collect();
    let results: Vec<(usize, usize, Vec<(usize, Vec<f32>)>)> = strips
        .par_iter()
        .map(|&x0| {
            let bw = STRIP.min(w - x0);
            let runs = busy_runs(h, reach, |y| a[y * w + x0..y * w + x0 + bw].iter().any(|v| *v != 0.0));
            let mut done = Vec::new();
            for (y0, y1) in runs {
                let hh = y1 - y0;
                let mut cur = vec![0f32; bw * hh];
                for y in 0..hh {
                    cur[y * bw..(y + 1) * bw].copy_from_slice(&a[(y0 + y) * w + x0..(y0 + y) * w + x0 + bw]);
                }
                let mut nxt = vec![0f32; bw * hh];
                let hi = hh as isize;
                for &r in &rs {
                    let norm = 1.0 / (2 * r + 1) as f32;
                    let ri = r as isize;
                    let row = |y: isize| y.clamp(0, hi - 1) as usize * bw;
                    let mut acc = vec![0f32; bw];
                    for i in -ri..=ri {
                        let o = row(i);
                        for j in 0..bw {
                            acc[j] += cur[o + j];
                        }
                    }
                    for y in 0..hi {
                        let o = y as usize * bw;
                        let (add, sub) = (row(y + ri + 1), row(y - ri));
                        for j in 0..bw {
                            nxt[o + j] = acc[j] * norm;
                            acc[j] += cur[add + j] - cur[sub + j];
                        }
                    }
                    std::mem::swap(&mut cur, &mut nxt);
                }
                done.push((y0, cur));
            }
            (x0, bw, done)
        })
        .collect();
    for (x0, bw, done) in results {
        for (y0, cur) in done {
            for (y, chunk) in cur.chunks(bw).enumerate() {
                a[(y0 + y) * w + x0..(y0 + y) * w + x0 + bw].copy_from_slice(chunk);
            }
        }
    }
}

pub(crate) fn transpose(a: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0; w * h];
    out.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
        for y in 0..h {
            col[y] = a[y * w + x];
        }
    });
    out
}

const EDT_INF: f32 = 1e20;

/// 1D squared distance transform (Felzenszwalb and Huttenlocher).
fn edt_1d(f: &[f32], out: &mut [f32]) {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0f32; n + 1];
    let mut k = 0usize;
    z[0] = -EDT_INF;
    z[1] = EDT_INF;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if s <= z[k] {
                // k == 0: replace the only parabola
                v[0] = q;
                z[0] = -EDT_INF;
                z[1] = EDT_INF;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = EDT_INF;
            break;
        }
    }
    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let d = q as f32 - p as f32;
        out[q] = d * d + f[p];
    }
}

/// Euclidean distance from each pixel to the nearest pixel where `seed` is true.
pub fn distance_to(seed: &[bool], w: usize, h: usize) -> Vec<f32> {
    let mut a: Vec<f32> = seed.iter().map(|s| if *s { 0.0 } else { EDT_INF }).collect();
    a.par_chunks_mut(w).for_each(|row| {
        let f = row.to_vec();
        edt_1d(&f, row);
    });
    let mut t = transpose(&a, w, h);
    t.par_chunks_mut(h).for_each(|col| {
        let f = col.to_vec();
        edt_1d(&f, col);
    });
    transpose(&t, h, w).into_iter().map(|d| d.min(EDT_INF).sqrt()).collect()
}

/// Signed distance: positive inside `mask > 0.5`, negative outside, in pixels.
pub fn signed_distance(mask: &Field) -> Field {
    let inside: Vec<bool> = mask.data.iter().map(|v| *v > 0.5).collect();
    let outside: Vec<bool> = inside.iter().map(|v| !v).collect();
    let to_in = distance_to(&inside, mask.w, mask.h);
    let to_out = distance_to(&outside, mask.w, mask.h);
    let data = (0..inside.len())
        .into_par_iter()
        .map(|i| if inside[i] { to_out[i] - 0.5 } else { -(to_in[i] - 0.5) })
        .collect();
    Field { w: mask.w, h: mask.h, data }
}

/// For every pixel, how high the surface rises above a ray from it toward the light: the largest
/// `top(p + k*d) - k*slope` over distances k >= 1, and the k where that happens. One sweep along the
/// light direction does what marching every ray would, in time that does not grow with shadow length.
/// Positions off the map read the nearest edge, like `Field::at`.
pub fn horizon(top: &Field, dx: f32, dy: f32, slope: f32) -> (Vec<f32>, Vec<f32>) {
    // Work so the light's main axis runs along rows.
    let swap = dx.abs() > dy.abs();
    let (w, h, src, dx, dy) = if swap { (top.h, top.w, transpose(&top.data, top.w, top.h), dy, dx) } else { (top.w, top.h, top.data.clone(), dx, dy) };
    let len = 1.0 / dy.abs().max(1e-6);
    let ex = dx * len;
    let drop = slope * len;
    let down = dy > 0.0;
    let mut gn = vec![0f32; w * h];
    let mut kn = vec![0f32; w * h];
    // The best value along the ray including the pixel itself, for the row last done.
    let mut g_prev = vec![0f32; w];
    let mut k_prev = vec![0f32; w];
    let sample = |row: &[f32], x: f32| -> f32 {
        let x = x.clamp(0.0, (w - 1) as f32);
        let x0 = x.floor() as usize;
        let x1 = (x0 + 1).min(w - 1);
        let t = x - x0 as f32;
        row[x0] * (1.0 - t) + row[x1] * t
    };
    for step in 0..h {
        let y = if down { h - 1 - step } else { step };
        let cur = &src[y * w..(y + 1) * w];
        let mut g_row = vec![0f32; w];
        let mut k_row = vec![0f32; w];
        for x in 0..w {
            let fx = x as f32 + ex;
            // the first row looks off the map, where the edge row repeats
            let (g, k) = if step == 0 { (sample(cur, fx), 0.0) } else { (sample(&g_prev, fx), sample(&k_prev, fx)) };
            let (g, k) = (g - drop, k + len);
            gn[y * w + x] = g;
            kn[y * w + x] = k;
            if cur[x] >= g {
                g_row[x] = cur[x];
                k_row[x] = 0.0;
            } else {
                g_row[x] = g;
                k_row[x] = k;
            }
        }
        g_prev = g_row;
        k_prev = k_row;
    }
    if swap {
        (transpose(&gn, w, h), transpose(&kn, w, h))
    } else {
        (gn, kn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain three-pass box blur over every row and column, for checking the faster one.
    fn reference(f: &Field, r: usize) -> Vec<f32> {
        let (w, h) = (f.w, f.h);
        let mut a = f.data.clone();
        for y in 0..h {
            for _ in 0..3 {
                let src = a[y * w..(y + 1) * w].to_vec();
                box_line(&src, &mut a[y * w..(y + 1) * w], r);
            }
        }
        for x in 0..w {
            let mut col: Vec<f32> = (0..h).map(|y| a[y * w + x]).collect();
            for _ in 0..3 {
                let src = col.clone();
                box_line(&src, &mut col, r);
            }
            for y in 0..h {
                a[y * w + x] = col[y];
            }
        }
        a
    }

    #[test]
    fn sparse_blur_matches_plain_blur() {
        let (w, h) = (300, 200);
        let mut f = Field::new(w, h);
        // a few scattered blobs, one touching an edge
        for (cx, cy) in [(20usize, 30usize), (150, 100), (290, 5), (60, 190), (200, 60)] {
            for y in cy.saturating_sub(4)..(cy + 4).min(h) {
                for x in cx.saturating_sub(4)..(cx + 4).min(w) {
                    f.data[y * w + x] = 1.0 + (x * 7 + y * 3) as f32 % 5.0;
                }
            }
        }
        // mostly full with scattered holes, like a canopy
        for y in 0..h {
            for x in 0..w {
                if x > 100 && ((x * 31 + y * 17) % 23) > 2 {
                    f.data[y * w + x] = ((x * 13 + y * 7) % 11) as f32 * 0.3;
                }
            }
        }
        for r in [1usize, 3, 6] {
            let radius = r as f32 / 0.9;
            let got = f.blur(radius);
            let want = reference(&f, r);
            let worst = got.data.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(worst < 1e-4, "radius {r}: off by {worst}");
        }
    }
}
