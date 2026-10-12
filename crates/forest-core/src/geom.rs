//! Plane geometry for the drawing tools: boolean operations on zones, a path widened into a band, a region traced from
//! a picture by colour (the wand), and simplifying and smoothing of outlines.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use std::collections::HashMap;

pub type Pt = [f32; 2];
/// One closed outline, without its first point repeated at the end.
pub type Ring = Vec<Pt>;
/// A filled shape: the outer ring first, then its holes.
pub type Shape = Vec<Ring>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

/// Combine two sets of rings. Rings in a set are filled together by the even-odd rule, so a ring inside another
/// is a hole.
pub fn boolean(a: &[Ring], b: &[Ring], op: BoolOp) -> Vec<Shape> {
    let rule = match op {
        BoolOp::Union => OverlayRule::Union,
        BoolOp::Subtract => OverlayRule::Difference,
        BoolOp::Intersect => OverlayRule::Intersect,
    };
    let (a, b) = (a.to_vec(), b.to_vec());
    let out: Vec<Vec<Vec<Pt>>> = a.overlay(&b, rule, FillRule::EvenOdd);
    out
}

/// Like `boolean`, but every ring is a solid area of its own: overlapping rings in a set merge instead of
/// cancelling out. Used for whole zones.
pub fn boolean_solid(a: &[Ring], b: &[Ring], op: BoolOp) -> Vec<Shape> {
    let rule = match op {
        BoolOp::Union => OverlayRule::Union,
        BoolOp::Subtract => OverlayRule::Difference,
        BoolOp::Intersect => OverlayRule::Intersect,
    };
    let ccw = |set: &[Ring]| -> Vec<Ring> {
        set.iter()
            .map(|r| {
                let mut r = r.clone();
                if area(&r) < 0.0 {
                    r.reverse();
                }
                r
            })
            .collect()
    };
    let (a, b) = (ccw(a), ccw(b));
    a.overlay(&b, rule, FillRule::NonZero)
}

/// Rings turned into shapes, with holes assigned to the ring that contains them.
pub fn to_shapes(rings: &[Ring]) -> Vec<Shape> {
    boolean(rings, &[], BoolOp::Union)
}

pub fn area(r: &[Pt]) -> f32 {
    let n = r.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (r[i], r[(i + 1) % n]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    s * 0.5
}

/// A path widened to a band of the given total width, round at the joints and the ends.
pub fn buffer_path(path: &[Pt], width: f32) -> Vec<Shape> {
    let r = (width * 0.5).max(0.25);
    let disc = |c: Pt| -> Ring {
        let n = 14;
        (0..n).map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [c[0] + a.cos() * r, c[1] + a.sin() * r]
        }).collect()
    };
    let mut parts: Vec<Ring> = Vec::new();
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt();
        if l < 1e-4 {
            continue;
        }
        let (nx, ny) = (-dy / l * r, dx / l * r);
        parts.push(vec![[a[0] + nx, a[1] + ny], [b[0] + nx, b[1] + ny], [b[0] - nx, b[1] - ny], [a[0] - nx, a[1] - ny]]);
    }
    for p in path {
        parts.push(disc(*p));
    }
    // every part is its own positive shape; merge them
    let mut acc: Vec<Shape> = Vec::new();
    for p in parts {
        let rings: Vec<Ring> = acc.iter().flatten().cloned().collect();
        let merged = boolean(&rings, &[p], BoolOp::Union);
        acc = merged;
    }
    acc
}

/// Ramer–Douglas–Peucker simplification of a closed ring.
pub fn simplify_ring(r: &[Pt], eps: f32) -> Ring {
    if r.len() < 4 {
        return r.to_vec();
    }
    // split at the two points farthest apart, so the first and last point are never both lost
    let (mut a, mut b, mut best) = (0, 0, 0.0f32);
    for i in 0..r.len() {
        let d = (r[i][0] - r[0][0]).powi(2) + (r[i][1] - r[0][1]).powi(2);
        if d > best {
            best = d;
            b = i;
        }
    }
    best = 0.0;
    for i in 0..r.len() {
        let d = (r[i][0] - r[b][0]).powi(2) + (r[i][1] - r[b][1]).powi(2);
        if d > best {
            best = d;
            a = i;
        }
    }
    let (lo, hi) = (a.min(b), a.max(b));
    let first: Vec<Pt> = r[lo..=hi].to_vec();
    let mut second: Vec<Pt> = r[hi..].to_vec();
    second.extend_from_slice(&r[..=lo]);
    let mut out = rdp(&first, eps);
    let tail = rdp(&second, eps);
    out.pop();
    out.extend(tail);
    out.pop();
    out
}

fn rdp(p: &[Pt], eps: f32) -> Vec<Pt> {
    if p.len() < 3 {
        return p.to_vec();
    }
    let (a, b) = (p[0], p[p.len() - 1]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let (mut idx, mut dmax) = (0, 0.0f32);
    for (i, q) in p.iter().enumerate().take(p.len() - 1).skip(1) {
        let d = if l2 < 1e-9 {
            ((q[0] - a[0]).powi(2) + (q[1] - a[1]).powi(2)).sqrt()
        } else {
            ((q[0] - a[0]) * dy - (q[1] - a[1]) * dx).abs() / l2.sqrt()
        };
        if d > dmax {
            dmax = d;
            idx = i;
        }
    }
    if dmax > eps {
        let mut left = rdp(&p[..=idx], eps);
        let right = rdp(&p[idx..], eps);
        left.pop();
        left.extend(right);
        left
    } else {
        vec![a, b]
    }
}

/// Chaikin corner cutting on a closed ring.
pub fn smooth_ring(pts: &[Pt], iterations: usize) -> Ring {
    let mut p = pts.to_vec();
    if p.len() < 3 {
        return p;
    }
    for _ in 0..iterations {
        let n = p.len();
        let mut q = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (a, b) = (p[i], p[(i + 1) % n]);
            q.push([0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]]);
            q.push([0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]);
        }
        p = q;
    }
    p
}

// ---------------------------------------------------------------------- the wand

/// Pixels of an RGBA picture whose colour is within `tol` (0 to 100) of the one at the seed. With `contiguous` only
/// those connected to the seed, like a paint bucket; otherwise every such pixel in the picture.
pub fn flood_region(rgba: &[u8], w: usize, h: usize, seed: (usize, usize), tol: f32, contiguous: bool) -> Vec<bool> {
    let mut mask = vec![false; w * h];
    if seed.0 >= w || seed.1 >= h || rgba.len() < w * h * 4 {
        return mask;
    }
    // the colour to match: the average of a small patch around the seed, so one noisy pixel does not decide
    let mut sum = [0f32; 3];
    let mut n = 0.0;
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let (x, y) = (seed.0 as i32 + dx, seed.1 as i32 + dy);
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                let i = (y as usize * w + x as usize) * 4;
                for k in 0..3 {
                    sum[k] += rgba[i + k] as f32;
                }
                n += 1.0;
            }
        }
    }
    let c = [sum[0] / n, sum[1] / n, sum[2] / n];
    let limit = (tol.clamp(0.0, 100.0) * 2.55).powi(2) * 3.0;
    let close = |i: usize| {
        let d = (0..3).map(|k| (rgba[i * 4 + k] as f32 - c[k]).powi(2)).sum::<f32>();
        d <= limit && rgba[i * 4 + 3] > 8
    };
    if !contiguous {
        for i in 0..w * h {
            mask[i] = close(i);
        }
        return mask;
    }
    let mut stack = vec![seed.1 * w + seed.0];
    while let Some(i) = stack.pop() {
        if mask[i] || !close(i) {
            continue;
        }
        mask[i] = true;
        let (x, y) = (i % w, i / w);
        if x > 0 {
            stack.push(i - 1);
        }
        if x + 1 < w {
            stack.push(i + 1);
        }
        if y > 0 {
            stack.push(i - w);
        }
        if y + 1 < h {
            stack.push(i + w);
        }
    }
    mask
}

/// The outlines of the set pixels of a mask, outer rings and holes, in pixel corner coordinates.
pub fn trace_mask(mask: &[bool], w: usize, h: usize) -> Vec<Ring> {
    let at = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && mask[y as usize * w + x as usize];
    // directed edges with the set pixel on the left, so outer rings run one way and holes the other
    let mut out_edges: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !at(x, y) {
                continue;
            }
            if !at(x, y - 1) {
                out_edges.entry((x, y)).or_default().push((x + 1, y));
            }
            if !at(x + 1, y) {
                out_edges.entry((x + 1, y)).or_default().push((x + 1, y + 1));
            }
            if !at(x, y + 1) {
                out_edges.entry((x + 1, y + 1)).or_default().push((x, y + 1));
            }
            if !at(x - 1, y) {
                out_edges.entry((x, y + 1)).or_default().push((x, y));
            }
        }
    }
    let mut rings = Vec::new();
    let keys: Vec<(i32, i32)> = out_edges.keys().cloned().collect();
    for start in keys {
        while let Some(first) = out_edges.get_mut(&start).and_then(|v| v.pop()) {
            let mut ring: Ring = vec![[start.0 as f32, start.1 as f32]];
            let (mut prev, mut cur) = (start, first);
            while cur != start {
                ring.push([cur.0 as f32, cur.1 as f32]);
                let Some(next) = out_edges.get_mut(&cur) else { break };
                if next.is_empty() {
                    break;
                }
                // at a pinch point take the sharpest left turn, so touching corners stay separate shapes
                let dir = (cur.0 - prev.0, cur.1 - prev.1);
                let mut pick = 0;
                let mut best = i32::MIN;
                for (k, cand) in next.iter().enumerate() {
                    let d2 = (cand.0 - cur.0, cand.1 - cur.1);
                    let cross = dir.0 * d2.1 - dir.1 * d2.0;
                    if cross > best {
                        best = cross;
                        pick = k;
                    }
                }
                let n = next.swap_remove(pick);
                prev = cur;
                cur = n;
            }
            if ring.len() >= 4 {
                rings.push(ring);
            }
        }
    }
    rings
}

/// The wand: the region of a picture like the one at `seed`, as shapes in picture pixels, simplified and smoothed.
pub fn wand(rgba: &[u8], w: usize, h: usize, seed: (usize, usize), tol: f32, contiguous: bool, smooth: f32) -> Vec<Shape> {
    let mask = flood_region(rgba, w, h, seed, tol, contiguous);
    let rings = trace_mask(&mask, w, h);
    let eps = 0.7 + smooth * 2.0;
    let iterations = (smooth * 3.0).round() as usize;
    let rings: Vec<Ring> = rings
        .into_iter()
        .filter(|r| area(r).abs() >= 4.0)
        .map(|r| smooth_ring(&simplify_ring(&r, eps), iterations))
        .collect();
    to_shapes(&rings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f32, y: f32, s: f32) -> Ring {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
    }

    fn total(shapes: &[Shape]) -> f32 {
        shapes.iter().map(|s| s.iter().enumerate().map(|(i, r)| if i == 0 { area(r).abs() } else { -area(r).abs() }).sum::<f32>()).sum()
    }

    #[test]
    fn solid_union_merges_overlaps() {
        let mut b = square(150.0, 150.0, 100.0);
        b.reverse();
        let r = boolean_solid(&[square(100.0, 100.0, 100.0), b], &[], BoolOp::Union);
        assert_eq!(r.len(), 1);
        assert!((total(&r) - 17500.0).abs() < 1.0);
    }

    #[test]
    fn boolean_operations_have_the_right_areas() {
        let a = vec![square(0.0, 0.0, 10.0)];
        let b = vec![square(5.0, 5.0, 10.0)];
        assert!((total(&boolean(&a, &b, BoolOp::Union)) - 175.0).abs() < 0.5);
        assert!((total(&boolean(&a, &b, BoolOp::Intersect)) - 25.0).abs() < 0.5);
        assert!((total(&boolean(&a, &b, BoolOp::Subtract)) - 75.0).abs() < 0.5);
        // a hole stays a hole
        let hole = boolean(&a, &[square(3.0, 3.0, 4.0)], BoolOp::Subtract);
        assert_eq!(hole.len(), 1);
        assert_eq!(hole[0].len(), 2);
    }

    #[test]
    fn a_path_becomes_a_band_of_that_width() {
        let band = buffer_path(&[[0.0, 0.0], [100.0, 0.0]], 10.0);
        assert_eq!(band.len(), 1);
        let a = total(&band);
        assert!((a - 1000.0 - 78.0).abs() < 12.0, "band area {a}");
        let bent = buffer_path(&[[0.0, 0.0], [50.0, 0.0], [50.0, 50.0]], 8.0);
        assert_eq!(bent.len(), 1);
    }

    #[test]
    fn the_wand_finds_the_region_it_is_clicked_on() {
        // a 40 x 30 picture: green on the left half, brown on the right, with a brown hole in the green
        let (w, h) = (40usize, 30usize);
        let mut px = vec![255u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let green = x < 20 && !((8..12).contains(&x) && (10..14).contains(&y));
                let i = (y * w + x) * 4;
                let c = if green { [60, 140, 60] } else { [170, 130, 90] };
                px[i..i + 3].copy_from_slice(&c);
            }
        }
        let shapes = wand(&px, w, h, (3, 3), 15.0, true, 0.0);
        assert_eq!(shapes.len(), 1, "one region");
        assert_eq!(shapes[0].len(), 2, "with a hole in it");
        let a = total(&shapes);
        assert!((a - (20.0 * 30.0 - 16.0)).abs() < 2.0, "area {a}");
        // not contiguous: the brown hole is brown like the right half
        let mask = flood_region(&px, w, h, (30, 5), 15.0, false);
        assert!(mask[10 * w + 9] && mask[5 * w + 30] && !mask[5 * w + 3]);
        // contiguous brown does not reach the hole
        let mask = flood_region(&px, w, h, (30, 5), 15.0, true);
        assert!(!mask[10 * w + 9]);
    }

    #[test]
    fn simplifying_keeps_the_shape() {
        let mut ring: Ring = Vec::new();
        for i in 0..100 {
            let a = i as f32 / 100.0 * std::f32::consts::TAU;
            ring.push([a.cos() * 50.0, a.sin() * 50.0]);
        }
        let s = simplify_ring(&ring, 0.5);
        assert!(s.len() < 60 && s.len() > 12, "{} points", s.len());
        assert!((area(&s).abs() - area(&ring).abs()).abs() / area(&ring).abs() < 0.05);
    }
}
