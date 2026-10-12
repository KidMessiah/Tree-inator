//! Canopy smoothing: an edge-preserving filter (Kuwahara) over a layer's colour and alpha. Every pixel takes the
//! average of whichever of the four quadrants around it varies least, so crowns flatten into lobes of one tone
//! while their outlines and the gaps between them stay crisp, as in aerial photographs. The GPU kernel
//! `k_smooth` does the same arithmetic.

use rayon::prelude::*;

/// The window never grows past this many pixels, which also bounds the cost.
pub const MAX_R: usize = 6;

/// Window size for a smoothing amount (0 = off) and the mean crown radius in render pixels. At 1 the window
/// reaches about 60 percent of a crown radius.
pub fn radius(amount: f32, crown_px: f32) -> usize {
    if amount <= 0.0 || crown_px <= 0.0 {
        return 0;
    }
    ((amount * 0.6 * crown_px).round() as usize).min(MAX_R)
}

const TILE: usize = 192;
/// Alpha counts for a quarter as much as brightness when judging how flat a quadrant is.
pub const ALPHA_WEIGHT: f32 = 0.25;

/// Smooth `rgb` (straight colour) and `alpha`, both `w` by `h`, with windows of `r` pixels.
pub fn smooth_layer(rgb: &mut [[f32; 3]], alpha: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 || w == 0 || h == 0 || rgb.len() != w * h || alpha.len() != w * h {
        return;
    }
    let (src_rgb, src_a) = (rgb.to_vec(), alpha.to_vec());
    let tiles: Vec<(usize, usize)> = (0..h).step_by(TILE).flat_map(|y| (0..w).step_by(TILE).map(move |x| (x, y))).collect();
    let out: Vec<(usize, usize, usize, usize, Vec<[f32; 4]>)> = tiles
        .into_par_iter()
        .map(|(tx, ty)| {
            let (x1, y1) = ((tx + TILE).min(w), (ty + TILE).min(h));
            // the source window, with the apron the quadrants reach into
            let (sx0, sy0) = (tx.saturating_sub(r), ty.saturating_sub(r));
            let (sx1, sy1) = ((x1 + r).min(w), (y1 + r).min(h));
            let (sw, sh) = (sx1 - sx0, sy1 - sy0);
            // integral images of premultiplied colour, alpha, brightness and their squares
            let stride = sw + 1;
            let mut ii = vec![[0f64; 7]; stride * (sh + 1)];
            for y in 0..sh {
                let mut row = [0f64; 7];
                for x in 0..sw {
                    let i = (sy0 + y) * w + sx0 + x;
                    let a = src_a[i] as f64;
                    let c = src_rgb[i];
                    let p = [c[0] as f64 * a, c[1] as f64 * a, c[2] as f64 * a];
                    let lp = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
                    let v = [p[0], p[1], p[2], a, lp, lp * lp, a * a];
                    for k in 0..7 {
                        row[k] += v[k];
                        ii[(y + 1) * stride + x + 1][k] = ii[y * stride + x + 1][k] + row[k];
                    }
                }
            }
            let rect = |x0: i64, y0: i64, x1: i64, y1: i64| -> ([f64; 7], f64) {
                let cx0 = (x0.max(sx0 as i64) - sx0 as i64) as usize;
                let cy0 = (y0.max(sy0 as i64) - sy0 as i64) as usize;
                let cx1 = ((x1 + 1).min(sx1 as i64) - sx0 as i64) as usize;
                let cy1 = ((y1 + 1).min(sy1 as i64) - sy0 as i64) as usize;
                let mut s = [0f64; 7];
                for k in 0..7 {
                    s[k] = ii[cy1 * stride + cx1][k] - ii[cy0 * stride + cx1][k] - ii[cy1 * stride + cx0][k] + ii[cy0 * stride + cx0][k];
                }
                (s, ((cx1 - cx0) * (cy1 - cy0)) as f64)
            };
            let ri = r as i64;
            let mut res = Vec::with_capacity((x1 - tx) * (y1 - ty));
            for y in ty..y1 {
                for x in tx..x1 {
                    let (xi, yi) = (x as i64, y as i64);
                    let quads = [(xi - ri, yi - ri, xi, yi), (xi, yi - ri, xi + ri, yi), (xi - ri, yi, xi, yi + ri), (xi, yi, xi + ri, yi + ri)];
                    let mut best = (f64::MAX, [0f64; 4]);
                    for (qx0, qy0, qx1, qy1) in quads {
                        let (s, n) = rect(qx0, qy0, qx1, qy1);
                        let (mlp, ma) = (s[4] / n, s[3] / n);
                        let var = (s[5] / n - mlp * mlp) + ALPHA_WEIGHT as f64 * (s[6] / n - ma * ma);
                        if var < best.0 {
                            best = (var, [s[0] / n, s[1] / n, s[2] / n, ma]);
                        }
                    }
                    let m = best.1;
                    res.push([m[0] as f32, m[1] as f32, m[2] as f32, m[3] as f32]);
                }
            }
            (tx, ty, x1, y1, res)
        })
        .collect();
    for (tx, ty, x1, y1, res) in out {
        let mut it = res.into_iter();
        for y in ty..y1 {
            for x in tx..x1 {
                let m = it.next().unwrap();
                let i = y * w + x;
                let a = m[3];
                alpha[i] = a;
                if a > 1e-4 {
                    rgb[i] = [m[0] / a, m[1] / a, m[2] / a];
                }
            }
        }
    }
}
