//! Parametric trees after Weber and Penn, as described in Hewitt's 2017 dissertation
//! "Procedural generation of tree models for use in computer graphics".
//!
//! A tree is grown as a trunk with levels of branches (length, down angle, phyllotaxis, curve),
//! leaves are gathered into rounded clumps on the last level, and the result is rendered top-down into
//! a small G-buffer sprite: height, surface direction, tone, depth and class per pixel. The engine
//! relights those sprites, so shadow direction and flat light keep working.

use crate::params::Form;
use crate::rng::Rng;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 1e-9 {
        mul(a, 1.0 / l)
    } else {
        [0.0, 0.0, 1.0]
    }
}
/// Rodrigues rotation of `v` about the unit `axis`.
fn rot_about(v: V3, axis: V3, ang: f32) -> V3 {
    let (s, c) = ang.sin_cos();
    add(add(mul(v, c), mul(cross(axis, v), s)), mul(axis, dot(axis, v) * (1.0 - c)))
}
fn side_axis(d: V3) -> V3 {
    let r = cross(d, [0.0, 0.0, 1.0]);
    if dot(r, r) < 1e-6 {
        [1.0, 0.0, 0.0]
    } else {
        norm(r)
    }
}

#[derive(Clone, Copy)]
enum Shape {
    Hemispherical,
    Conical,
    TendFlame,
}

/// Weber and Penn shape envelope: relative branch length at `r` (1 at the crown base, 0 at the top).
fn shape_ratio(s: Shape, r: f32) -> f32 {
    match s {
        Shape::Hemispherical => 0.2 + 0.8 * (0.5 * PI * r).sin(),
        Shape::Conical => 0.2 + 0.8 * r,
        Shape::TendFlame => {
            if r < 0.7 {
                0.5 + 0.5 * r / 0.7
            } else {
                0.5 + 0.5 * (1.0 - r) / 0.3
            }
        }
    }
}

#[derive(Clone)]
struct Recipe {
    levels: usize,
    shape: Shape,
    base_size: f32,
    trunk_radius: f32,
    branches: [f32; 4],
    length: [f32; 4],
    down: [f32; 4],
    down_v: [f32; 4],
    rotate: [f32; 4],
    rotate_v: [f32; 4],
    curve: [f32; 4],
    curve_v: [f32; 4],
    curve_res: [usize; 4],
    droop: [f32; 4],
    child_radius: [f32; 4],
    anchors: usize,
    cover: f32,
    spread: f32,
    bend: f32,
    clump: f32,
    dome: f32,
    tilt: f32,
    bare: bool,
    twig_px: [f32; 4],
    /// The trunk lies on its side, as a fallen log.
    lie: bool,
    /// Lifts bare stems off the ground by this many stem widths.
    lift: f32,
}

impl Form {
    fn recipe(self) -> Recipe {
        let oak = Recipe {
            levels: 3,
            shape: Shape::Hemispherical,
            base_size: 0.2,
            trunk_radius: 0.03,
            branches: [1.0, 24.0, 7.0, 5.0],
            length: [1.0, 0.62, 0.5, 0.45],
            down: [0.0, 62.0, 48.0, 40.0],
            down_v: [0.0, 12.0, 12.0, 10.0],
            rotate: [0.0, 137.5, 137.5, 137.5],
            rotate_v: [0.0, 15.0, 15.0, 15.0],
            curve: [0.0, -12.0, 8.0, 5.0],
            curve_v: [0.0, 25.0, 25.0, 25.0],
            curve_res: [3, 5, 4, 3],
            droop: [0.0, 0.05, 0.08, 0.05],
            child_radius: [1.0, 0.5, 0.5, 0.5],
            anchors: 3,
            cover: 3.6,
            spread: 0.55,
            bend: 0.5,
            clump: 3.4,
            dome: 0.9,
            tilt: 0.7,
            bare: false,
            twig_px: [3.0, 1.8, 1.0, 0.6],
            lie: false,
            lift: 0.0,
        };
        match self {
            Form::Oak => oak,
            Form::Spruce => Recipe {
                shape: Shape::Conical,
                base_size: 0.06,
                branches: [1.0, 38.0, 10.0, 6.0],
                length: [1.0, 0.42, 0.36, 0.3],
                down: [0.0, 82.0, 66.0, 52.0],
                down_v: [0.0, 6.0, 8.0, 8.0],
                curve: [0.0, 22.0, 16.0, 10.0],
                droop: [0.0, 0.22, 0.2, 0.15],
                cover: 4.2,
                spread: 0.4,
                bend: 0.1,
                clump: 1.2,
                dome: 0.2,
                tilt: 0.15,
                ..oak
            },
            Form::Sage => Recipe {
                shape: Shape::TendFlame,
                branches: [1.0, 26.0, 8.0, 5.0],
                length: [1.0, 0.5, 0.45, 0.4],
                ..oak
            },
            Form::Bush => Recipe {
                base_size: 0.0,
                branches: [1.0, 18.0, 7.0, 5.0],
                length: [1.0, 0.9, 0.55, 0.45],
                down: [0.0, 70.0, 50.0, 40.0],
                cover: 3.8,
                ..oak
            },
            Form::Birch => Recipe {
                base_size: 0.3,
                branches: [1.0, 20.0, 7.0, 5.0],
                length: [1.0, 0.5, 0.45, 0.4],
                down: [0.0, 46.0, 40.0, 35.0],
                droop: [0.0, 0.12, 0.25, 0.2],
                cover: 3.0,
                clump: 2.4,
                bend: 0.4,
                ..oak
            },
            Form::Pine => Recipe {
                shape: Shape::Conical,
                base_size: 0.45,
                branches: [1.0, 14.0, 8.0, 5.0],
                length: [1.0, 0.6, 0.4, 0.35],
                down: [0.0, 78.0, 60.0, 45.0],
                curve: [0.0, 14.0, 10.0, 8.0],
                droop: [0.0, 0.1, 0.12, 0.1],
                cover: 3.4,
                clump: 2.2,
                dome: 0.5,
                ..oak
            },
            Form::Willow => Recipe {
                base_size: 0.25,
                branches: [1.0, 18.0, 8.0, 6.0],
                down: [0.0, 72.0, 70.0, 60.0],
                curve: [0.0, -20.0, 8.0, 5.0],
                droop: [0.0, 0.25, 0.45, 0.35],
                cover: 4.0,
                clump: 2.6,
                bend: 0.8,
                ..oak
            },
            Form::Cypress => Recipe {
                shape: Shape::Conical,
                base_size: 0.02,
                branches: [1.0, 30.0, 8.0, 5.0],
                length: [1.0, 0.25, 0.3, 0.3],
                down: [0.0, 40.0, 40.0, 35.0],
                cover: 4.4,
                spread: 0.28,
                clump: 1.0,
                bend: 0.1,
                dome: 0.3,
                ..oak
            },
            Form::Acacia => Recipe {
                base_size: 0.5,
                branches: [1.0, 10.0, 8.0, 6.0],
                length: [1.0, 0.9, 0.5, 0.4],
                down: [0.0, 80.0, 60.0, 45.0],
                cover: 3.0,
                clump: 3.0,
                dome: 0.3,
                ..oak
            },
            Form::Palm => Recipe {
                levels: 1,
                base_size: 0.9,
                branches: [1.0, 16.0, 1.0, 1.0],
                length: [1.0, 0.55, 0.4, 0.4],
                down: [0.0, 72.0, 40.0, 40.0],
                down_v: [0.0, 16.0, 10.0, 10.0],
                curve: [0.0, 0.0, 0.0, 0.0],
                curve_v: [0.0, 8.0, 8.0, 8.0],
                curve_res: [3, 6, 3, 3],
                droop: [0.0, 0.55, 0.1, 0.1],
                anchors: 6,
                cover: 2.6,
                spread: 0.35,
                bend: 0.4,
                clump: 1.3,
                dome: 0.4,
                tilt: 0.3,
                ..oak
            },
            Form::TreeFern => Recipe {
                levels: 1,
                base_size: 0.85,
                branches: [1.0, 24.0, 1.0, 1.0],
                length: [1.0, 0.6, 0.4, 0.4],
                down: [0.0, 58.0, 40.0, 40.0],
                down_v: [0.0, 14.0, 10.0, 10.0],
                curve_v: [0.0, 6.0, 8.0, 8.0],
                curve_res: [3, 6, 3, 3],
                droop: [0.0, 0.3, 0.1, 0.1],
                anchors: 8,
                cover: 3.4,
                spread: 0.3,
                bend: 0.3,
                clump: 1.1,
                dome: 0.25,
                tilt: 0.2,
                ..oak
            },
            Form::Baobab => Recipe {
                levels: 2,
                base_size: 0.8,
                trunk_radius: 0.12,
                branches: [1.0, 7.0, 5.0, 1.0],
                length: [1.0, 0.7, 0.35, 0.3],
                down: [0.0, 50.0, 40.0, 40.0],
                down_v: [0.0, 10.0, 10.0, 10.0],
                curve: [0.0, 6.0, 4.0, 4.0],
                curve_v: [0.0, 10.0, 12.0, 12.0],
                curve_res: [3, 4, 3, 3],
                droop: [0.0, 0.05, 0.1, 0.1],
                child_radius: [1.0, 0.5, 0.4, 0.4],
                anchors: 3,
                cover: 1.7,
                clump: 3.8,
                dome: 1.0,
                ..oak
            },
            Form::Cedar => Recipe {
                shape: Shape::Conical,
                base_size: 0.05,
                branches: [1.0, 34.0, 10.0, 6.0],
                length: [1.0, 0.5, 0.36, 0.3],
                down: [0.0, 78.0, 66.0, 52.0],
                down_v: [0.0, 5.0, 8.0, 8.0],
                curve: [0.0, 14.0, 10.0, 8.0],
                droop: [0.0, 0.5, 0.35, 0.25],
                cover: 4.4,
                spread: 0.35,
                bend: 0.1,
                clump: 0.9,
                dome: 0.15,
                tilt: 0.1,
                ..oak
            },
            Form::Gnarled => Recipe {
                base_size: 0.3,
                branches: [1.0, 12.0, 6.0, 4.0],
                length: [1.0, 0.75, 0.5, 0.4],
                down: [0.0, 62.0, 50.0, 40.0],
                down_v: [0.0, 25.0, 20.0, 15.0],
                rotate_v: [0.0, 70.0, 60.0, 50.0],
                curve: [0.0, -25.0, 15.0, 10.0],
                curve_v: [0.0, 60.0, 55.0, 45.0],
                droop: [0.0, 0.12, 0.12, 0.08],
                cover: 2.4,
                spread: 0.9,
                bend: 0.9,
                clump: 2.8,
                ..oak
            },
            Form::Fern => Recipe {
                levels: 1,
                base_size: 0.0,
                branches: [1.0, 14.0, 1.0, 1.0],
                length: [1.0, 0.9, 0.4, 0.4],
                down: [0.0, 76.0, 40.0, 40.0],
                down_v: [0.0, 18.0, 10.0, 10.0],
                curve_v: [0.0, 8.0, 8.0, 8.0],
                curve_res: [3, 6, 3, 3],
                droop: [0.0, 0.32, 0.1, 0.1],
                anchors: 6,
                cover: 3.4,
                spread: 0.3,
                bend: 0.3,
                clump: 1.0,
                dome: 0.2,
                tilt: 0.2,
                ..oak
            },
            Form::Wildflower => Recipe {
                levels: 2,
                base_size: 0.0,
                branches: [1.0, 16.0, 5.0, 1.0],
                length: [1.0, 0.9, 0.4, 0.3],
                down: [0.0, 76.0, 45.0, 40.0],
                curve: [0.0, 0.0, 0.0, 0.0],
                anchors: 2,
                cover: 2.2,
                clump: 1.7,
                dome: 0.9,
                ..oak
            },
            Form::Log => Recipe {
                levels: 1,
                lie: true,
                bare: true,
                base_size: 0.3,
                trunk_radius: 0.03,
                branches: [1.0, 3.0, 1.0, 1.0],
                length: [1.0, 0.16, 0.1, 0.1],
                down: [0.0, 55.0, 40.0, 40.0],
                down_v: [0.0, 10.0, 10.0, 10.0],
                curve: [0.0; 4],
                curve_v: [0.0, 5.0, 5.0, 5.0],
                droop: [0.0; 4],
                twig_px: [7.0, 2.4, 1.0, 0.6],
                lift: 0.9,
                ..oak
            },
            Form::Stump | Form::MushroomRing | Form::Boulder => oak,
            Form::Dead => Recipe { bare: true, base_size: 0.3, branches: [1.0, 14.0, 7.0, 5.0], ..oak },
        }
    }
}

struct Skeleton {
    /// (start, end, radius, level)
    segs: Vec<(V3, V3, f32, usize)>,
    /// Leaf anchor points with the tone they inherited down the branch hierarchy.
    anchors: Vec<(V3, f32)>,
}

#[allow(clippy::too_many_arguments)]
fn stem(r: &Recipe, rng: &mut Rng, sk: &mut Skeleton, origin: V3, mut d: V3, length: f32, radius: f32, level: usize, phase: f32, tone: f32) {
    let n = r.curve_res[level];
    let nf = n as f32;
    let mut pts = vec![origin];
    let mut dirs = Vec::with_capacity(n);
    let mut pos = origin;
    for _ in 0..n {
        let bend = (r.curve[level] + rng.normal(0.0, r.curve_v[level])) / nf;
        d = rot_about(d, side_axis(d), bend.to_radians());
        d = norm([d[0], d[1], d[2] - r.droop[level] * length / nf]);
        dirs.push(d);
        pos = add(pos, mul(d, length / nf));
        pts.push(pos);
    }
    for i in 0..n {
        sk.segs.push((pts[i], pts[i + 1], radius * (1.0 - 0.5 * i as f32 / nf), level));
    }
    let at = |t: f32| -> (V3, V3) {
        let f = t * nf;
        let i = (f as usize).min(n - 1);
        (add(pts[i], mul(sub(pts[i + 1], pts[i]), f - i as f32)), dirs[i])
    };
    if level == r.levels {
        // Deepest stems carry the leaves, so record anchor points along them.
        let k = r.anchors.max(1);
        for j in 0..k {
            let t = if k == 1 { 1.0 } else { 0.2 + 0.8 * j as f32 / (k - 1) as f32 };
            sk.anchors.push((at(t).0, (tone + rng.normal(0.0, 0.07)).clamp(0.0, 1.0)));
        }
        return;
    }
    let nc = r.branches[level + 1].max(1.0) as usize;
    let start = if level == 0 { r.base_size } else { 0.12 };
    let mut rot = phase;
    for i in 0..nc {
        let t = start + (1.0 - start) * (i as f32 + 0.5) / nc as f32;
        let (p, td) = at(t);
        let clen = if level == 0 {
            length * r.length[1] * shape_ratio(r.shape, (1.0 - t) / (1.0 - start))
        } else {
            length * r.length[level + 1] * (1.0 - 0.6 * t)
        };
        rot += (r.rotate[level + 1] + rng.normal(0.0, r.rotate_v[level + 1])).to_radians();
        let down = (r.down[level + 1] + rng.normal(0.0, r.down_v[level + 1])).to_radians();
        let cd = rot_about(rot_about(td, side_axis(td), down), norm(td), rot);
        let ph = rng.f32() * TAU;
        // Tone drifts down the hierarchy, so whole branches and clumps share a colour.
        let ct = (0.5 + (tone - 0.5) * 0.75 + rng.normal(0.0, 0.17)).clamp(0.05, 0.95);
        stem(r, rng, sk, p, norm(cd), clen, radius * r.child_radius[level + 1], level + 1, ph, ct);
    }
}

fn grow(r: &Recipe, rng: &mut Rng) -> Skeleton {
    let mut sk = Skeleton { segs: Vec::new(), anchors: Vec::new() };
    let ph = rng.f32() * TAU;
    let d0 = if r.lie { norm([1.0, 0.04, 0.0]) } else { [0.0, 0.0, 1.0] };
    stem(r, rng, &mut sk, [0.0; 3], d0, 1.0, r.trunk_radius, 0, ph, 0.5);
    sk
}

/// A top-down tree sprite. `cls` is 0 where empty, 1 for leaf and 2 for bark.
pub struct Sprite {
    pub size: usize,
    /// Height in render pixels.
    pub z: Vec<f32>,
    /// Surface tilt direction (the normal's x and y, with z fixed at 1 before normalising).
    pub nx: Vec<f32>,
    pub ny: Vec<f32>,
    pub tone: Vec<f32>,
    /// Height relative to the tallest point of this tree.
    pub depth: Vec<f32>,
    pub cls: Vec<u8>,
    pub zmax: f32,
}

#[derive(Clone, Copy)]
pub struct SpriteSpec {
    pub form: Form,
    pub branchiness: f32,
    pub clumpiness: f32,
    pub leaf_cover: f32,
    /// Leaf length in render pixels.
    pub leaf_px: f32,
    pub leaf_elong: f32,
    /// Leaves lie along their branch, like needles.
    pub along: bool,
    pub bare: bool,
}

struct Prim {
    x: f32,
    y: f32,
    z: f32,
    rx: f32,
    ry: f32,
    ang: f32,
    nx: f32,
    ny: f32,
    tone: f32,
    cls: u8,
}

const SS: usize = 3;

/// Render one tree. `r_px` is the crown radius in render pixels.
pub fn make_sprite(spec: &SpriteSpec, r_px: f32, seed: u64) -> Sprite {
    let mut rc = spec.form.recipe();
    rc.bare |= spec.bare;
    for l in 1..4 {
        rc.branches[l] = (rc.branches[l] * spec.branchiness).round().max(1.0);
    }
    rc.clump *= spec.clumpiness;
    rc.cover *= spec.leaf_cover;
    let mut rng = Rng::new(seed);
    let sk = grow(&rc, &mut rng);

    let refs: Vec<V3> = if sk.anchors.is_empty() { sk.segs.iter().map(|s| s.1).collect() } else { sk.anchors.iter().map(|a| a.0).collect() };
    let (cx, cy) = (
        refs.iter().map(|p| p[0]).sum::<f32>() / refs.len() as f32,
        refs.iter().map(|p| p[1]).sum::<f32>() / refs.len() as f32,
    );
    let mut hd: Vec<f32> = refs.iter().map(|p| (p[0] - cx).hypot(p[1] - cy)).collect();
    hd.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p92 = hd[((hd.len() - 1) as f32 * 0.92) as usize].max(1e-3);

    let ss = SS as f32;
    let r_ss = r_px * ss;
    let scale = r_ss / p92;
    let size = (2.0 * r_px * 1.45) as usize + 4;
    let size_ss = size * SS;
    let mid = size_ss as f32 / 2.0;
    let mut prims: Vec<Prim> = Vec::new();

    let special = matches!(spec.form, Form::Stump | Form::MushroomRing | Form::Boulder);
    if special {
        special_prims(spec.form, &mut prims, mid, r_ss, &mut rng);
    }

    // Bark: a chain of discs along each stem. Leafy trees only show the inner structure.
    for (p0, p1, rad, lvl) in sk.segs.iter().filter(|_| !special) {
        if !rc.bare {
            if *lvl >= 2 {
                continue;
            }
            if *lvl == 1 && ((p1[0] - cx) * scale).hypot((p1[1] - cy) * scale) > 0.55 * r_ss {
                continue;
            }
        }
        let l = len3(sub(*p1, *p0)) * scale;
        let steps = ((l / (0.8 * ss)) as usize).max(1);
        let w = if rc.bare {
            (rc.twig_px[*lvl] * (r_px / 14.0).sqrt() * (0.5 + 0.5 * rad / rc.trunk_radius)).max(0.5) * ss
        } else {
            (rad * scale).max(0.5 * ss)
        };
        for i in 0..steps {
            let q = add(*p0, mul(sub(*p1, *p0), i as f32 / steps as f32));
            prims.push(Prim {
                x: mid + (q[0] - cx) * scale,
                y: mid + (q[1] - cy) * scale,
                z: q[2] * scale * if rc.bare { 1.1 } else { 0.5 } + rc.lift * w,
                rx: w,
                ry: w,
                ang: 0.0,
                nx: 0.0,
                ny: 0.0,
                tone: 0.4 + rng.normal(0.0, 0.06),
                cls: 2,
            });
        }
    }

    // Leaves, gathered into rounded clumps around the twig anchors.
    if !special && !rc.bare && !sk.anchors.is_empty() {
        let leaf_px = spec.leaf_px.max(0.4) * ss;
        let area = PI * r_ss * r_ss;
        let leaf_area = PI * leaf_px * leaf_px * 0.8;
        let n = ((rc.cover * area / leaf_area) as usize).min(6000);
        let clump_r = (rc.clump * leaf_px).max(0.5);
        let el = spec.leaf_elong.max(0.3);
        for _ in 0..n {
            let (a, atone) = sk.anchors[rng.below(sk.anchors.len())];
            let off = [rng.normal(0.0, 0.5) * clump_r, rng.normal(0.0, 0.5) * clump_r];
            let offn = (off[0].hypot(off[1]) / clump_r).min(1.0);
            let dome = (1.0 - offn * offn).max(0.0).sqrt() * clump_r * rc.dome;
            let x = mid + (a[0] - cx) * scale + off[0];
            let y = mid + (a[1] - cy) * scale + off[1];
            if (x - mid).hypot(y - mid) > 1.25 * r_ss {
                continue;
            }
            let (ox, oy) = (a[0] - cx, a[1] - cy);
            let ro = ox.hypot(oy);
            let (ux, uy) = if ro > 1e-6 { (ox / ro, oy / ro) } else { (0.0, 0.0) };
            let reach = (ro * scale / r_ss).min(1.2);
            // Leaves lean away from their clump centre, so each clump reads as a dome.
            let nx = rng.normal(0.0, rc.spread) * 0.6 + rc.bend * 0.6 * ux * reach + off[0] / clump_r * rc.tilt;
            let ny = rng.normal(0.0, rc.spread) * 0.6 + rc.bend * 0.6 * uy * reach + off[1] / clump_r * rc.tilt;
            let size_px = leaf_px * rng.range(0.75, 1.35);
            let ang = if spec.along { uy.atan2(ux) + rng.normal(0.0, 0.3) } else { rng.f32() * PI };
            prims.push(Prim {
                x,
                y,
                z: a[2] * scale * 1.1 + dome,
                rx: size_px * el.sqrt(),
                ry: size_px / el.sqrt(),
                ang,
                nx,
                ny,
                tone: (atone + rng.normal(0.0, 0.07)).clamp(0.0, 1.0),
                cls: 1,
            });
        }
    }

    // Splat every primitive into a supersampled buffer, highest z wins.
    let n_ss = size_ss * size_ss;
    let mut zb = vec![f32::MIN; n_ss];
    let (mut nxb, mut nyb, mut tb) = (vec![0f32; n_ss], vec![0f32; n_ss], vec![0f32; n_ss]);
    let mut cb = vec![0u8; n_ss];
    for p in &prims {
        let area = PI * p.rx * p.ry;
        let k = ((area * 2.5) as usize).clamp(6, 90_000);
        let (ca, sa) = (p.ang.cos(), p.ang.sin());
        let bump = p.rx.min(p.ry) * 0.9;
        let rmax = p.rx.max(p.ry);
        for i in 0..k {
            let f = (i as f32 + 0.5) / k as f32;
            let rr = f.sqrt();
            let th = (i as f32 + 0.5) * 2.399_963;
            let (ox, oy) = (rr * th.cos() * p.rx, rr * th.sin() * p.ry);
            let wx = p.x + ox * ca - oy * sa;
            let wy = p.y + ox * sa + oy * ca;
            let (ix, iy) = (wx.floor() as i32, wy.floor() as i32);
            if ix < 0 || iy < 0 || ix >= size_ss as i32 || iy >= size_ss as i32 {
                continue;
            }
            let idx = iy as usize * size_ss + ix as usize;
            let z = p.z + (1.0 - rr * rr).max(0.0).sqrt() * bump;
            if z > zb[idx] {
                zb[idx] = z;
                nxb[idx] = p.nx + 0.35 * (wx - p.x) / rmax;
                nyb[idx] = p.ny + 0.35 * (wy - p.y) / rmax;
                tb[idx] = p.tone;
                cb[idx] = p.cls;
            }
        }
    }

    // Box downsample. A pixel counts as covered when at least half its sub-samples are.
    let mut sp = Sprite {
        size,
        z: vec![0.0; size * size],
        nx: vec![0.0; size * size],
        ny: vec![0.0; size * size],
        tone: vec![0.0; size * size],
        depth: vec![0.0; size * size],
        cls: vec![0; size * size],
        zmax: 0.0,
    };
    for y in 0..size {
        for x in 0..size {
            let (mut cnt, mut bark) = (0usize, 0usize);
            let (mut z, mut nx, mut ny, mut t) = (0f32, 0f32, 0f32, 0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let i = (y * SS + sy) * size_ss + x * SS + sx;
                    if cb[i] != 0 {
                        cnt += 1;
                        bark += (cb[i] == 2) as usize;
                        z += zb[i];
                        nx += nxb[i];
                        ny += nyb[i];
                        t += tb[i];
                    }
                }
            }
            if cnt * 2 >= SS * SS {
                let c = cnt as f32;
                let i = y * size + x;
                sp.z[i] = (z / c / ss).max(0.01);
                sp.nx[i] = nx / c;
                sp.ny[i] = ny / c;
                sp.tone[i] = t / c;
                sp.cls[i] = if bark * 2 > cnt { 2 } else { 1 };
                sp.zmax = sp.zmax.max(sp.z[i]);
            }
        }
    }
    // Depth mixes height within the whole tree with height relative to the nearby clumps,
    // so clump tops read as lit and the gaps between clumps read as deep.
    let zm = sp.zmax.max(1e-3);
    let mask: Vec<f32> = sp.cls.iter().map(|c| (*c != 0) as u8 as f32).collect();
    let zmasked: Vec<f32> = sp.z.iter().zip(&mask).map(|(z, m)| z * m).collect();
    let rad = ((0.3 * r_px) as usize).max(2);
    let (bz, bm) = (box_blur(&zmasked, size, rad), box_blur(&mask, size, rad));
    for i in 0..size * size {
        if sp.cls[i] != 0 {
            let local = ((sp.z[i] - bz[i] / bm[i].max(1e-3)) / (0.2 * zm)).clamp(-1.0, 1.0);
            sp.depth[i] = (0.45 * (sp.z[i] / zm) + 0.55 * (0.5 + 0.5 * local)).clamp(0.0, 1.0);
        }
    }
    sp
}

/// Primitives for forms that are not branching trees: stumps, mushroom rings and boulders.
fn special_prims(form: Form, prims: &mut Vec<Prim>, mid: f32, r: f32, rng: &mut Rng) {
    let mut put = |x: f32, y: f32, z: f32, rx: f32, ry: f32, ang: f32, tone: f32, cls: u8| {
        prims.push(Prim { x, y, z, rx, ry, ang, nx: 0.0, ny: 0.0, tone, cls });
    };
    match form {
        Form::Stump => {
            // Root flares, then a cut top with growth rings.
            let n = 5 + rng.below(3);
            let a0 = rng.f32() * TAU;
            for i in 0..n {
                let a = a0 + TAU * i as f32 / n as f32 + rng.normal(0.0, 0.2);
                let d = 0.5 * r;
                put(mid + a.cos() * d, mid + a.sin() * d, 0.1 * r, 0.32 * r, 0.12 * r, a, 0.35, 2);
            }
            put(mid, mid, 0.25 * r, 0.62 * r, 0.62 * r, 0.0, 0.35, 2);
            for (k, (rad, tone)) in [(0.52, 0.62), (0.42, 0.4), (0.32, 0.64), (0.22, 0.42), (0.12, 0.66)].iter().enumerate() {
                put(mid, mid, 0.4 * r + 0.02 * k as f32 * r, rad * r, rad * r, 0.0, *tone, 2);
            }
        }
        Form::MushroomRing => {
            let n = 9 + rng.below(5);
            let a0 = rng.f32() * TAU;
            for i in 0..n {
                let a = a0 + TAU * i as f32 / n as f32 + rng.normal(0.0, 0.18);
                let d = r * rng.range(0.5, 0.68);
                let cap = r * rng.range(0.13, 0.24);
                let tone = if rng.f32() < 0.3 { rng.range(0.8, 0.95) } else { rng.range(0.3, 0.6) };
                put(mid + a.cos() * d, mid + a.sin() * d, 0.35 * r + cap, cap, cap, 0.0, tone, 1);
            }
        }
        Form::Boulder => {
            let n = 6 + rng.below(4);
            put(mid, mid, 0.3 * r, 0.62 * r, 0.52 * r, rng.f32() * PI, 0.4, 2);
            for _ in 0..n {
                let a = rng.f32() * TAU;
                let d = r * rng.range(0.1, 0.5);
                let s = r * rng.range(0.22, 0.42);
                put(mid + a.cos() * d, mid + a.sin() * d, 0.2 * r + s * 0.8, s, s * rng.range(0.7, 1.0), rng.f32() * PI, rng.range(0.25, 0.65), 2);
            }
        }
        _ => {}
    }
}

/// Separable box blur with clamped edges.
fn box_blur(a: &[f32], size: usize, r: usize) -> Vec<f32> {
    let norm = 1.0 / (2 * r + 1) as f32;
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut dst = vec![0.0; size * size];
        for l in 0..size {
            let at = |i: i32| {
                let c = i.clamp(0, size as i32 - 1) as usize;
                if horizontal { src[l * size + c] } else { src[c * size + l] }
            };
            let mut acc = 0.0;
            for i in -(r as i32)..=(r as i32) {
                acc += at(i);
            }
            for i in 0..size as i32 {
                let v = acc * norm;
                if horizontal { dst[l * size + i as usize] = v } else { dst[i as usize * size + l] = v }
                acc += at(i + r as i32 + 1) - at(i - r as i32);
            }
        }
        dst
    };
    pass(&pass(a, true), false)
}

fn len3(a: V3) -> f32 {
    dot(a, a).sqrt()
}

static CACHE: OnceLock<Mutex<HashMap<u64, Arc<Sprite>>>> = OnceLock::new();

/// A cached sprite for this species look, crown radius (render pixels) and variant number.
pub fn sprite_for(spec: &SpriteSpec, r_px: f32, variant: u32) -> Arc<Sprite> {
    let mut h = DefaultHasher::new();
    (spec.form as u8).hash(&mut h);
    for v in [spec.branchiness, spec.clumpiness, spec.leaf_cover, spec.leaf_px, spec.leaf_elong, r_px] {
        v.to_bits().hash(&mut h);
    }
    (spec.along, spec.bare, variant).hash(&mut h);
    let key = h.finish();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(s) = cache.lock().unwrap().get(&key) {
        return s.clone();
    }
    let sp = Arc::new(make_sprite(spec, r_px, key));
    let mut m = cache.lock().unwrap();
    if m.len() > 2500 {
        m.clear();
    }
    m.insert(key, sp.clone());
    sp
}
