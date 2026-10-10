//! Parametric tree generator after Weber and Penn, ported from friggog/tree-gen (Chris Hewitt, 2017), the
//! implementation described in his dissertation. The original is a Blender add-on in Python, GPL-3.0; this is a
//! line by line port of `parametric/gen.py`, `chturtle.py` and `leaf.py` to Rust, without Blender.
//! Because it derives from GPL code, anything built with it is GPL-3.0 if it is distributed.
//!
//! It builds the branch curves (cubic Bezier points with a radius) and the leaf placements. Drawing them
//! from above is in `treegen_sprite.rs`.

use crate::treegen_data::{Preset, BLOSSOM_SHAPES, LEAF_SHAPES};
use std::f64::consts::{PI, TAU};
use std::ops::{Add, Mul, Neg, Sub};

// ---------------------------------------------------------------------------------------------- vectors

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V(pub f64, pub f64, pub f64);

impl Add for V {
    type Output = V;
    fn add(self, o: V) -> V {
        V(self.0 + o.0, self.1 + o.1, self.2 + o.2)
    }
}
impl Sub for V {
    type Output = V;
    fn sub(self, o: V) -> V {
        V(self.0 - o.0, self.1 - o.1, self.2 - o.2)
    }
}
impl Mul<f64> for V {
    type Output = V;
    fn mul(self, s: f64) -> V {
        V(self.0 * s, self.1 * s, self.2 * s)
    }
}
impl Neg for V {
    type Output = V;
    fn neg(self) -> V {
        V(-self.0, -self.1, -self.2)
    }
}

impl V {
    pub fn dot(self, o: V) -> f64 {
        self.0 * o.0 + self.1 * o.1 + self.2 * o.2
    }
    pub fn cross(self, o: V) -> V {
        V(self.1 * o.2 - self.2 * o.1, self.2 * o.0 - self.0 * o.2, self.0 * o.1 - self.1 * o.0)
    }
    pub fn mag(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn normalized(self) -> V {
        let m = self.mag();
        if m > 0.0 {
            self * (1.0 / m)
        } else {
            self
        }
    }
    /// Angle from the vertical in degrees.
    pub fn declination(self) -> f64 {
        (self.0 * self.0 + self.1 * self.1).sqrt().atan2(self.2).to_degrees()
    }
    pub fn angle(self, o: V) -> f64 {
        let m = self.mag() * o.mag();
        if m == 0.0 {
            return 0.0;
        }
        (self.dot(o) / m).clamp(-1.0, 1.0).acos()
    }
}

/// A rotation as a 3x3 matrix.
#[derive(Clone, Copy, Debug)]
pub struct Rot([[f64; 3]; 3]);

impl Rot {
    pub fn axis_angle(axis: V, a: f64) -> Rot {
        let n = axis.mag();
        if n == 0.0 {
            return Rot([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        }
        let (x, y, z) = (axis.0 / n, axis.1 / n, axis.2 / n);
        let (c, s) = (a.cos(), a.sin());
        let t = 1.0 - c;
        Rot([
            [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
            [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
            [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
        ])
    }
    pub fn apply(&self, v: V) -> V {
        let m = &self.0;
        V(
            m[0][0] * v.0 + m[0][1] * v.1 + m[0][2] * v.2,
            m[1][0] * v.0 + m[1][1] * v.1 + m[1][2] * v.2,
            m[2][0] * v.0 + m[2][1] * v.1 + m[2][2] * v.2,
        )
    }
    pub fn inverse(&self) -> Rot {
        let m = &self.0;
        Rot([[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]])
    }
    /// Takes +Z to `dir`, with local +Y leaning toward world +Z (Blender's `to_track_quat('Z', 'Y')`).
    pub fn track_z_up_y(dir: V) -> Rot {
        let z = dir.normalized();
        let mut up = V(0.0, 0.0, 1.0) - z * z.2;
        if up.mag() < 1e-6 {
            up = V(0.0, 1.0, 0.0);
        }
        let up = up.normalized();
        let x = up.cross(z);
        Rot([[x.0, up.0, z.0], [x.1, up.1, z.1], [x.2, up.2, z.2]])
    }
}

/// Small deterministic generator with a copyable state, so pruning can replay random choices.
#[derive(Clone, Copy)]
pub struct Rand(u64);

impl Rand {
    pub fn new(seed: u64) -> Rand {
        Rand(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }
    /// Uniform in [0, 1).
    pub fn random(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn in_range(&mut self, lo: f64, hi: f64) -> f64 {
        self.random() * (hi - lo) + lo
    }
    /// Uniform in [-1, 1).
    pub fn sym(&mut self) -> f64 {
        -1.0 + 2.0 * self.random()
    }
}

// ---------------------------------------------------------------------------------------------- turtle

#[derive(Clone, Copy, Debug)]
pub struct Turtle {
    pub dir: V,
    pub pos: V,
    pub right: V,
}

impl Turtle {
    pub fn new() -> Turtle {
        Turtle { dir: V(0.0, 0.0, 1.0), pos: V(0.0, 0.0, 0.0), right: V(1.0, 0.0, 0.0) }
    }
    fn rotate_both(&mut self, axis: V, a: f64) {
        let q = Rot::axis_angle(axis, a);
        self.dir = q.apply(self.dir).normalized();
        self.right = q.apply(self.right).normalized();
    }
    pub fn turn_right(&mut self, angle: f64) {
        let axis = self.dir.cross(self.right).normalized();
        self.rotate_both(axis, angle.to_radians());
    }
    pub fn turn_left(&mut self, angle: f64) {
        let axis = self.dir.cross(self.right).normalized();
        self.rotate_both(axis, -angle.to_radians());
    }
    pub fn pitch_down(&mut self, angle: f64) {
        self.dir = Rot::axis_angle(self.right, -angle.to_radians()).apply(self.dir).normalized();
    }
    pub fn roll_right(&mut self, angle: f64) {
        self.right = Rot::axis_angle(self.dir, angle.to_radians()).apply(self.right).normalized();
    }
    pub fn mv(&mut self, d: f64) {
        self.pos = self.pos + self.dir * d;
    }
}

// ---------------------------------------------------------------------------------------------- output

#[derive(Clone, Copy, Debug)]
pub struct BP {
    pub co: V,
    pub hl: V,
    pub hr: V,
    pub radius: f64,
}

impl BP {
    fn new() -> BP {
        BP { co: V::default(), hl: V::default(), hr: V::default(), radius: 1.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Leaf {
    pub position: V,
    pub direction: V,
    pub right: V,
}

/// A leaf or blossom polygon in tree coordinates.
pub struct Poly {
    pub verts: Vec<V>,
    pub blossom: bool,
    /// A tone for the whole polygon, when the sprite builder wants to pick it.
    pub tone: Option<f32>,
}

/// How the crown is grown, on top of a preset. These change the generator itself (more twigs, more and
/// larger leaves, leaves on the branches that carry the twigs, leaves turning toward the light), so every
/// leaf still grows on a branch. `Canopy::RAW` is the original generator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Canopy {
    /// Multiplies the leaves (and blossoms) on each twig.
    pub leaves: f64,
    /// Multiplies how thick the trunk and branches are.
    pub wood: f64,
    /// Multiplies the bare stretch at the base of each limb before it branches.
    pub limb_base: f64,
    /// Degrees the main limbs curve upward along their length, for a domed crown instead of a bowl.
    pub limb_rise: f64,
    /// Multiplies how many main limbs the trunk grows.
    pub limbs: f64,
    /// Multiplies how many leaf-bearing twigs each branch grows.
    pub twigs: f64,
    /// Multiplies twig length.
    pub twig_len: f64,
    /// Multiplies leaf size.
    pub leaf_size: f64,
    /// Multiplies leaf width (capped at as wide as long).
    pub leaf_width: f64,
    /// Multiplies blossom size.
    pub blossom_size: f64,
    /// Leaves along the branches below the twigs and the trunk above its base, as a share of a twig's leaf count.
    pub inner: f64,
    /// How far each leaf turns its face up to the sky, 0 to 1.
    pub sky: f64,
    /// How strongly the late branches bend toward open sky while they grow, 0 to 1: toward nearby spots
    /// with nothing above them yet, so the crown fills its gaps.
    pub light: f64,
    /// Degrees the twigs curve upward along their length, so the foliage rides on top of the limbs.
    pub rise: f64,
}

impl Canopy {
    pub const RAW: Canopy = Canopy { leaves: 1.0, wood: 1.0, limb_base: 1.0, limb_rise: 0.0, limbs: 1.0, twigs: 1.0, twig_len: 1.0, leaf_size: 1.0, leaf_width: 1.0, blossom_size: 1.0, inner: 0.0, sky: 0.0, light: 0.0, rise: 0.0 };

    /// Blend from the raw generator (0) to this canopy (1).
    pub fn scaled(&self, k: f64) -> Canopy {
        let m = |v: f64| 1.0 + (v - 1.0) * k;
        Canopy { leaves: m(self.leaves), wood: m(self.wood), limb_base: m(self.limb_base), limb_rise: self.limb_rise * k, limbs: m(self.limbs), twigs: m(self.twigs), twig_len: m(self.twig_len), leaf_size: m(self.leaf_size), leaf_width: m(self.leaf_width), blossom_size: m(self.blossom_size), inner: self.inner * k, sky: (self.sky * k).clamp(0.0, 1.0), light: (self.light * k).clamp(0.0, 1.0), rise: self.rise * k }
    }
}

/// What a generation run produced.
pub struct TreeGeo {
    pub preset: Preset,
    pub canopy: Canopy,
    /// Branch curves with their level (0 is the trunk).
    pub splines: Vec<(usize, Vec<BP>)>,
    pub leaves: Vec<Leaf>,
    pub tree_scale: f64,
}

// ---------------------------------------------------------------------------------------------- bezier

fn point_on_bezier(offset: f64, a: &BP, b: &BP) -> V {
    let m = 1.0 - offset;
    a.co * (m * m * m) + a.hr * (3.0 * m * m * offset) + b.hl * (3.0 * m * offset * offset) + b.co * (offset * offset * offset)
}

fn tangent_to_bezier(offset: f64, a: &BP, b: &BP) -> V {
    let m = 1.0 - offset;
    (a.hr - a.co) * (3.0 * m * m) + (b.hl - a.hr) * (6.0 * m * offset) + (b.co - b.hl) * (3.0 * offset * offset)
}

fn apply_tropism(t: &mut Turtle, trop: V) {
    let mut h = t.dir.cross(trop);
    let alpha = 10.0 * h.mag();
    h = h.normalized();
    let q = Rot::axis_angle(h, alpha.to_radians());
    t.dir = q.apply(t.dir).normalized();
    t.right = q.apply(t.right).normalized();
}

fn helix_points(t: &Turtle, rad: f64, pitch: f64, rng: &mut Rand) -> (V, V, V, V) {
    let mut pts = [V(0.0, -rad, -pitch / 4.0), V((4.0 * rad) / 3.0, -rad, 0.0), V((4.0 * rad) / 3.0, rad, 0.0), V(0.0, rad, pitch / 4.0)];
    let trf = Rot::track_z_up_y(t.dir);
    let spin = rng.in_range(0.0, TAU);
    let q = Rot::axis_angle(V(0.0, 0.0, 1.0), spin);
    for p in pts.iter_mut() {
        *p = trf.apply(q.apply(*p));
    }
    (pts[1] - pts[0], pts[2] - pts[0], pts[3] - pts[0], t.dir)
}

fn branch_dir_turtle(turtle: &Turtle, helix: bool, offset: f64, a: &BP, b: &BP) -> Turtle {
    let mut t = Turtle::new();
    t.dir = tangent_to_bezier(offset, a, b).normalized();
    if helix {
        let tan_d = tangent_to_bezier(offset + 0.0001, a, b).normalized();
        t.right = t.dir.cross(tan_d);
    } else {
        t.right = turtle.dir.cross(turtle.right).cross(t.dir);
    }
    t
}

fn branch_pos_turtle(dir_turtle: &mut Turtle, offset: f64, a: &BP, b: &BP, radius_limit: f64) -> Turtle {
    dir_turtle.pos = point_on_bezier(offset, a, b);
    let mut t = *dir_turtle;
    t.pitch_down(90.0);
    t.mv(radius_limit);
    t
}

// ---------------------------------------------------------------------------------------------- stems

#[derive(Clone, Copy)]
struct Par {
    length: f64,
    lcm: f64,
    radius: f64,
}

#[derive(Clone, Copy)]
struct Stem {
    depth: usize,
    curve: usize,
    par: Option<Par>,
    offset: f64,
    radius_limit: f64,
    length: f64,
    radius: f64,
    lcm: f64,
}

impl Stem {
    fn new(depth: usize, curve: usize, par: Option<Par>, offset: f64, radius_limit: f64) -> Stem {
        Stem { depth, curve, par, offset, radius_limit, length: 0.0, radius: 0.0, lcm: 0.0 }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    AltOpp,
    Whorled,
    Fan,
}

struct Gen {
    p: Preset,
    rng: Rand,
    tree_scale: f64,
    base_length: f64,
    split_err: [f64; 7],
    splines: Vec<(usize, Vec<BP>)>,
    leaves: Vec<Leaf>,
    levels: usize,
    inner: f64,
    /// Strength of the light competition, and the light measured on the previous pass.
    light: f64,
    shade: Option<Shade>,
}

/// How much light reaches each point of a grown tree: crowns and leaves shade what lies below them. A later
/// growth pass reads it so branches lengthen where it is bright, give up where they are shaded and bend
/// away from the shade of their neighbours.
#[derive(Clone)]
struct Shade {
    n: usize,
    nz: usize,
    half: f64,
    top: f64,
    light: Vec<f32>,
}

impl Shade {
    const N: usize = 36;
    const NZ: usize = 36;

    /// Light under the leaves and twigs of `leaves` and `splines`, blended with `prev` to keep passes calm.
    fn build(leaves: &[Leaf], splines: &[(usize, Vec<BP>)], prev: Option<&Shade>) -> Shade {
        let (n, nz) = (Self::N, Self::NZ);
        let mut half = 1e-3f64;
        let mut top = 1e-3f64;
        for l in leaves {
            half = half.max(l.position.0.abs()).max(l.position.1.abs());
            top = top.max(l.position.2);
        }
        let (half, top) = (half * 1.08, top * 1.08);
        let cell = |v: f64, lo: f64, hi: f64, k: usize| (((v - lo) / (hi - lo)) * k as f64).floor();
        let mut dens = vec![0f32; n * n * nz];
        let mut add = |p: V, w: f32| {
            let (x, y, z) = (cell(p.0, -half, half, n), cell(p.1, -half, half, n), cell(p.2, 0.0, top, nz));
            if x >= 0.0 && y >= 0.0 && z >= 0.0 && (x as usize) < n && (y as usize) < n && (z as usize) < nz {
                dens[(z as usize * n + y as usize) * n + x as usize] += w;
            }
        };
        for l in leaves {
            add(l.position, 1.0);
        }
        // twigs and branches intercept light too, a little
        for (d, pts) in splines {
            if *d > 0 {
                for b in pts {
                    add(b.co, 0.15);
                }
            }
        }
        // spread sideways, since light comes from a cone and not straight down
        let mut soft = vec![0f32; dens.len()];
        for z in 0..nz {
            for y in 0..n {
                for x in 0..n {
                    let (mut sum, mut wt) = (0f32, 0f32);
                    for dy in -2i64..=2 {
                        for dx in -2i64..=2 {
                            let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                            if xx < 0 || yy < 0 || xx >= n as i64 || yy >= n as i64 {
                                continue;
                            }
                            let w = 1.0 / (1.0 + (dx * dx + dy * dy) as f32);
                            sum += w * dens[(z * n + yy as usize) * n + xx as usize];
                            wt += w;
                        }
                    }
                    soft[(z * n + y) * n + x] = sum / wt.max(1e-6);
                }
            }
        }
        // light left after everything above, per column
        let mut light = vec![1f32; dens.len()];
        let mut col_total = 0f32;
        let mut cols = 0usize;
        for i in 0..n * n {
            let t: f32 = (0..nz).map(|z| soft[z * n * n + i]).sum();
            if t > 0.05 {
                col_total += t;
                cols += 1;
            }
        }
        // a typical column leaves its bottom in deep shade
        let k = 1.8 / (col_total / cols.max(1) as f32).max(1e-3);
        for i in 0..n * n {
            let mut above = 0f32;
            for z in (0..nz).rev() {
                let at = z * n * n + i;
                light[at] = (-k * (above + 0.5 * soft[at])).exp();
                above += soft[at];
            }
        }
        if let Some(pv) = prev {
            // the old grid may cover other bounds, so blend by position
            for z in 0..nz {
                for y in 0..n {
                    for x in 0..n {
                        let p = V(
                            -half + (x as f64 + 0.5) / n as f64 * 2.0 * half,
                            -half + (y as f64 + 0.5) / n as f64 * 2.0 * half,
                            (z as f64 + 0.5) / nz as f64 * top,
                        );
                        let at = (z * n + y) * n + x;
                        light[at] = 0.5 * light[at] + 0.5 * pv.at(p) as f32;
                    }
                }
            }
        }
        Shade { n, nz, half, top, light }
    }

    /// Light from 0 (deep shade) to 1 (open sky) at a point, smoothly between cells.
    fn at(&self, p: V) -> f64 {
        if p.2 >= self.top {
            return 1.0;
        }
        let f = |v: f64, lo: f64, hi: f64, k: usize| ((v - lo) / (hi - lo) * k as f64 - 0.5).clamp(0.0, (k - 1) as f64);
        let (fx, fy, fz) = (f(p.0, -self.half, self.half, self.n), f(p.1, -self.half, self.half, self.n), f(p.2, 0.0, self.top, self.nz));
        let (x0, y0, z0) = (fx as usize, fy as usize, fz as usize);
        let (x1, y1, z1) = ((x0 + 1).min(self.n - 1), (y0 + 1).min(self.n - 1), (z0 + 1).min(self.nz - 1));
        let (tx, ty, tz) = (fx - x0 as f64, fy - y0 as f64, fz - z0 as f64);
        let g = |x: usize, y: usize, z: usize| self.light[(z * self.n + y) * self.n + x] as f64;
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        let row = |y: usize, z: usize| lerp(g(x0, y, z), g(x1, y, z), tx);
        let plane = |z: usize| lerp(row(y0, z), row(y1, z), ty);
        lerp(plane(z0), plane(z1), tz)
    }

    /// Which way the light gets better from `p`: sideways around whatever shades it, with only a little up,
    /// so branches path around their neighbours instead of all turning skyward.
    fn toward(&self, p: V) -> Option<V> {
        let h = 2.0 * self.half / self.n as f64 * 1.5;
        let dx = self.at(V(p.0 + h, p.1, p.2)) - self.at(V(p.0 - h, p.1, p.2));
        let dy = self.at(V(p.0, p.1 + h, p.2)) - self.at(V(p.0, p.1 - h, p.2));
        let dz = self.at(V(p.0, p.1, p.2 + h)) - self.at(V(p.0, p.1, p.2 - h));
        let g = V(dx, dy, dz * 0.25);
        (g.mag() > 1e-3).then(|| g.normalized())
    }
}

fn shape_ratio(p: &Preset, shape: i64, ratio: f64) -> f64 {
    match shape {
        1 => 0.2 + 0.8 * (PI * ratio).sin(),
        2 => 0.2 + 0.8 * (0.5 * PI * ratio).sin(),
        3 => 1.0,
        4 => 0.5 + 0.5 * ratio,
        5 => {
            if ratio <= 0.7 {
                ratio / 0.7
            } else {
                (1.0 - ratio) / 0.3
            }
        }
        6 => 1.0 - 0.8 * ratio,
        7 => {
            if ratio <= 0.7 {
                0.5 + 0.5 * ratio / 0.7
            } else {
                0.5 + 0.5 * (1.0 - ratio) / 0.3
            }
        }
        8 => {
            if !(0.0..=1.0).contains(&ratio) {
                0.0
            } else if ratio < 1.0 - p.prune_width_peak {
                (ratio / (1.0 - p.prune_width_peak)).powf(p.prune_power_high)
            } else {
                ((1.0 - ratio) / (1.0 - p.prune_width_peak)).powf(p.prune_power_low)
            }
        }
        _ => 0.2 + 0.8 * ratio,
    }
}

impl Gen {
    fn d1(depth: usize) -> usize {
        (depth + 1).min(3)
    }

    fn new_spline(&mut self, level: usize) -> usize {
        self.splines.push((level, vec![BP::new()]));
        self.splines.len() - 1
    }

    fn calc_stem_length(&mut self, stem: &Stem) -> f64 {
        let p = self.p;
        let r = if stem.depth == 0 {
            self.tree_scale * (p.length[0] + self.rng.sym() * p.length_v[0])
        } else if stem.depth == 1 {
            let par = stem.par.unwrap();
            par.length * par.lcm * shape_ratio(&p, p.shape as i64, (par.length - stem.offset) / (par.length - self.base_length))
        } else {
            let par = stem.par.unwrap();
            par.lcm * (par.length - 0.7 * stem.offset)
        };
        r.max(0.0)
    }

    fn calc_stem_radius(&self, stem: &Stem) -> f64 {
        let p = &self.p;
        if stem.depth == 0 {
            stem.length * p.ratio * p.radius_mod[0]
        } else {
            let par = stem.par.unwrap();
            let r = p.radius_mod[stem.depth.min(3)] * par.radius * (stem.length / par.length).powf(p.ratio_power);
            let r = r.max(0.005);
            r.min(stem.radius_limit)
        }
    }

    fn calc_curve_angle(&mut self, depth: usize, seg_ind: usize) -> f64 {
        let p = self.p;
        let curve_res = p.curve_res[depth] as usize;
        let mut a = if p.curve_back[depth] == 0.0 {
            p.curve[depth] / curve_res as f64
        } else if (seg_ind as f64) < curve_res as f64 / 2.0 {
            p.curve[depth] / (curve_res as f64 / 2.0)
        } else {
            p.curve_back[depth] / (curve_res as f64 / 2.0)
        };
        a += self.rng.sym() * (p.curve_v[depth] / curve_res as f64);
        a
    }

    fn calc_down_angle(&mut self, stem: &Stem, stem_offset: f64) -> f64 {
        let p = self.p;
        let d1 = Gen::d1(stem.depth);
        if p.down_angle_v[d1] >= 0.0 {
            p.down_angle[d1] + self.rng.sym() * p.down_angle_v[d1]
        } else {
            let sr = shape_ratio(&p, 0, (stem.length - stem_offset) / (stem.length * (1.0 - p.base_size[stem.depth.min(3)])));
            let mut d = p.down_angle[d1] + p.down_angle_v[d1] * (1.0 - 2.0 * sr);
            d += self.rng.sym() * (d * 0.1).abs();
            d
        }
    }

    fn calc_rotate_angle(&mut self, depth: usize, prev: f64) -> f64 {
        let p = self.p;
        if p.rotate[depth] >= 0.0 {
            (prev + p.rotate[depth] + self.rng.sym() * p.rotate_v[depth]).rem_euclid(360.0)
        } else {
            prev * (180.0 + p.rotate[depth] + self.rng.sym() * p.rotate_v[depth])
        }
    }

    fn calc_leaf_count(&self, stem: &Stem) -> f64 {
        let p = &self.p;
        if p.leaf_blos_num >= 0.0 {
            let par = stem.par.unwrap();
            let leaves = p.leaf_blos_num * self.tree_scale / p.g_scale;
            leaves * (stem.length / (par.lcm * par.length))
        } else {
            p.leaf_blos_num
        }
    }

    fn calc_branch_count(&mut self, stem: &Stem) -> f64 {
        let p = self.p;
        let d1 = Gen::d1(stem.depth);
        let r = if stem.depth == 0 {
            p.branches[d1] * (self.rng.random() * 0.2 + 0.9)
        } else if p.branches[d1] < 0.0 {
            p.branches[d1]
        } else if stem.depth == 1 {
            let par = stem.par.unwrap();
            p.branches[d1] * (0.2 + 0.8 * (stem.length / par.length) / par.lcm)
        } else {
            let par = stem.par.unwrap();
            p.branches[d1] * (1.0 - 0.5 * stem.offset / par.length)
        };
        r / (1.0 - p.base_size[stem.depth.min(3)])
    }

    fn radius_at_offset(&self, stem: &Stem, z1: f64) -> f64 {
        let p = &self.p;
        let n_taper = p.taper[stem.depth.min(3)];
        let unit_taper = if n_taper < 1.0 {
            n_taper
        } else if n_taper < 2.0 {
            2.0 - n_taper
        } else {
            0.0
        };
        let taper = stem.radius * (1.0 - unit_taper * z1);
        let mut radius;
        if n_taper < 1.0 {
            radius = taper;
        } else {
            let z2 = (1.0 - z1) * stem.length;
            let depth = if n_taper < 2.0 || z2 < taper { 1.0 } else { n_taper - 2.0 };
            let z3 = if n_taper < 2.0 { z2 } else { (z2 - 2.0 * taper * (z2 / (2.0 * taper).max(1e-12) + 0.5).trunc()).abs() };
            if n_taper < 2.0 && z3 >= taper {
                radius = taper;
            } else {
                radius = (1.0 - depth) * taper + depth * (taper * taper - (z3 - taper) * (z3 - taper)).max(0.0).sqrt();
            }
        }
        if stem.depth == 0 {
            let y = (1.0 - 8.0 * z1).max(0.0);
            let flare = p.flare * ((100f64.powf(y) - 1.0) / 100.0) + 1.0;
            radius *= flare;
        }
        radius
    }

    fn point_inside(&self, pt: V) -> bool {
        let p = &self.p;
        let dist = (pt.0 * pt.0 + pt.1 * pt.1).sqrt();
        let ratio = (self.tree_scale - pt.2) / (self.tree_scale * (1.0 - p.base_size[0]));
        dist / self.tree_scale < p.prune_width * shape_ratio(p, 8, ratio)
    }

    // ------------------------------------------------------------------------------------ the stem

    #[allow(clippy::too_many_arguments)]
    fn make_stem(
        &mut self,
        mut turtle: Turtle,
        mut stem: Stem,
        start: usize,
        mut split_corr_angle: f64,
        mut num_branches_factor: f64,
        mut clone_prob: f64,
        pos_corr_turtle: Option<Turtle>,
        cloned_turtle: Option<Turtle>,
    ) {
        if stem.radius_limit >= 0.0 && stem.radius_limit < 0.0001 {
            return;
        }
        let p = self.p;
        let depth = stem.depth;
        let d1 = Gen::d1(depth);

        if start == 0 {
            stem.lcm = p.length[d1] + self.rng.sym() * p.length_v[d1];
            stem.length = self.calc_stem_length(&stem);
            if depth > 0 && self.light > 0.0 {
                if let Some(sh) = &self.shade {
                    // branches in sun lengthen, shaded ones fall short and carry fewer leaves
                    let base = pos_corr_turtle.map(|t| t.pos).unwrap_or(turtle.pos);
                    let l = sh.at(base + turtle.dir * stem.length);
                    stem.length *= (1.0 + self.light * 0.9 * (l - 0.4)).clamp(0.3, 1.2);
                }
            }
            stem.radius = self.calc_stem_radius(&stem);
            if depth == 0 {
                self.base_length = stem.length * p.base_size[0];
            }
        }

        if let Some(mut pc) = pos_corr_turtle {
            pc.mv(-stem.radius.min(stem.radius_limit));
            turtle.pos = pc.pos;
        }

        // pruning, not needed for a clone since the original was already tested
        if cloned_turtle.is_none() && p.prune_ratio > 0.0 {
            let start_length = stem.length;
            let r_state = self.rng;
            let split_state = self.split_err;
            let mut inside = self.test_stem(turtle, &stem, start, split_corr_angle, clone_prob);
            while !inside {
                stem.length *= 0.9;
                if stem.length < 0.15 * start_length {
                    if p.prune_ratio < 1.0 {
                        stem.length = 0.0;
                        break;
                    } else {
                        return;
                    }
                }
                self.rng = r_state;
                self.split_err = split_state;
                inside = self.test_stem(turtle, &stem, start, split_corr_angle, clone_prob);
            }
            let fitting = stem.length;
            stem.length = start_length * (1.0 - p.prune_ratio) + fitting * p.prune_ratio;
            stem.radius = self.calc_stem_radius(&stem);
            self.rng = r_state;
            self.split_err = split_state;
        }

        let curve_res = p.curve_res[depth.min(3)] as usize;
        if curve_res == 0 {
            return;
        }
        let cr = curve_res as f64;
        let seg_splits = p.seg_splits[depth.min(3)];
        let seg_length = stem.length / cr;
        let base_seg_ind = (p.base_size[0] * p.curve_res[0]).ceil() as usize;

        let mut leaf_count = 0.0;
        let mut branch_count = 0.0;
        let mut f_leaves_on_seg = 0.0;
        let mut f_branches_on_seg = 0.0;
        if depth + 1 == self.levels && depth > 0 && p.leaf_blos_num != 0.0 {
            leaf_count = self.calc_leaf_count(&stem);
            leaf_count *= 1.0 - start as f64 / cr;
            f_leaves_on_seg = leaf_count / cr;
        } else {
            branch_count = self.calc_branch_count(&stem);
            branch_count *= 1.0 - start as f64 / cr;
            branch_count *= num_branches_factor;
            f_branches_on_seg = branch_count / cr;
        }

        // branches below the twigs can carry leaves of their own
        let f_inner_on_seg = if self.inner > 0.0 && depth + 2 <= self.levels && p.leaf_blos_num > 0.0 {
            // the trunk counts as one twig length's worth
            let count = if depth == 0 { p.leaf_blos_num * self.tree_scale / p.g_scale } else { self.calc_leaf_count(&stem) };
            self.inner * count * (1.0 - start as f64 / cr) / cr
        } else {
            0.0
        };
        let mut inner_num_error = 0.0;
        let mut inner_rotation_angle = 0.0;

        let max_points_per_seg = (100.0 / cr).max(1.0).ceil() as usize;
        let mut branch_num_error = 0.0;
        let mut leaf_num_error = 0.0;

        let mut prev_rotation_angle = if p.rotate[d1] >= 0.0 { self.rng.in_range(0.0, 360.0) } else { 1.0 };

        let helix = p.curve_v[depth.min(3)] < 0.0;
        let (mut hel_p0, mut hel_p1, mut hel_p2, mut hel_axis) = (V::default(), V::default(), V::default(), V::default());
        if helix {
            let tan_ang = (90.0 - p.curve_v[depth.min(3)].abs()).to_radians().tan();
            let hel_pitch = 2.0 * stem.length / cr * self.rng.in_range(0.8, 1.2);
            let hel_radius = 3.0 * hel_pitch / (16.0 * tan_ang) * self.rng.in_range(0.8, 1.2);
            if depth > 1 {
                apply_tropism(&mut turtle, V(p.tropism[0], p.tropism[1], p.tropism[2]));
            } else {
                apply_tropism(&mut turtle, V(p.tropism[0], p.tropism[1], 0.0));
            }
            let mut r = self.rng;
            let h = helix_points(&turtle, hel_radius, hel_pitch, &mut r);
            self.rng = r;
            hel_p0 = h.0;
            hel_p1 = h.1;
            hel_p2 = h.2;
            hel_axis = h.3;
        }

        let points_per_seg = if depth == 0 || p.taper[depth.min(3)] > 1.0 { max_points_per_seg } else { 2 };
        let ci = stem.curve;

        for seg_ind in start..=curve_res {
            let remaining_segs = (curve_res + 1 - seg_ind) as f64;

            if helix {
                let pos = turtle.pos;
                if seg_ind == 0 {
                    let np = &mut self.splines[ci].1[0];
                    np.co = pos;
                    np.hr = hel_p0 + pos;
                    np.hl = pos;
                } else {
                    self.splines[ci].1.push(BP::new());
                    let n = self.splines[ci].1.len();
                    let (co, hl, hr);
                    if seg_ind == 1 {
                        co = hel_p2 + pos;
                        hl = hel_p1 + pos;
                        hr = co * 2.0 - hl;
                    } else {
                        let prev_co = self.splines[ci].1[n - 2].co;
                        let q = Rot::axis_angle(hel_axis, (seg_ind as f64 - 1.0) * PI);
                        co = q.apply(hel_p2) + prev_co;
                        let dif = q.apply(hel_p2 - hel_p1);
                        hl = co - dif;
                        hr = co * 2.0 - hl;
                    }
                    let np = &mut self.splines[ci].1[n - 1];
                    np.co = co;
                    np.hl = hl;
                    np.hr = hr;
                }
                let last = *self.splines[ci].1.last().unwrap();
                turtle.pos = last.co;
                turtle.dir = last.hr.normalized();
            } else {
                if seg_ind != start {
                    turtle.mv(seg_length);
                    self.splines[ci].1.push(BP::new());
                }
                let idx = if seg_ind == start { 0 } else { self.splines[ci].1.len() - 1 };
                let np = &mut self.splines[ci].1[idx];
                np.co = turtle.pos;
                if let (Some(c), true) = (cloned_turtle, seg_ind == start) {
                    np.hl = turtle.pos - c.dir * (stem.length / (cr * 3.0));
                    np.hr = turtle.pos + c.dir * (stem.length / (cr * 3.0));
                } else {
                    np.hl = turtle.pos - turtle.dir * (stem.length / (cr * 3.0));
                    np.hr = turtle.pos + turtle.dir * (stem.length / (cr * 3.0));
                }
            }

            let actual_radius = self.radius_at_offset(&stem, seg_ind as f64 / cr);
            {
                let last = self.splines[ci].1.len() - 1;
                let idx = if !helix && seg_ind == start { 0 } else { last };
                self.splines[ci].1[idx].radius = actual_radius;
            }

            if seg_ind > start {
                let mut num_of_splits = 0usize;
                if !helix {
                    if p.base_splits > 0.0 && depth == 0 && seg_ind == base_seg_ind {
                        num_of_splits = p.base_splits as usize;
                    } else if p.base_splits < 0.0 && depth == 0 && seg_ind == base_seg_ind {
                        num_of_splits = (self.rng.random() * (p.base_splits.abs() + 0.5)) as usize;
                    } else if seg_splits > 0.0 && seg_ind < curve_res && (depth > 0 || seg_ind > base_seg_ind) {
                        if self.rng.random() <= clone_prob {
                            let n = (seg_splits + self.split_err[depth]) as i64;
                            num_of_splits = n.max(0) as usize;
                            self.split_err[depth] -= n as f64 - seg_splits;
                            clone_prob /= n as f64 + 1.0;
                            num_branches_factor /= n as f64 + 1.0;
                            num_branches_factor = num_branches_factor.max(0.8);
                            branch_count *= num_branches_factor;
                            f_branches_on_seg = branch_count / cr;
                        }
                    }
                }

                let r_state = self.rng;
                if branch_count.abs() > 0.0 && depth + 1 < self.levels {
                    let branches_on_seg: i64;
                    if branch_count < 0.0 {
                        branches_on_seg = if seg_ind == curve_res { branch_count as i64 } else { 0 };
                    } else {
                        let b = (f_branches_on_seg + branch_num_error) as i64;
                        branch_num_error -= b as f64 - f_branches_on_seg;
                        branches_on_seg = b;
                    }
                    if branches_on_seg.abs() > 0 {
                        self.make_branches(&turtle, &stem, seg_ind, branches_on_seg, &mut prev_rotation_angle, false);
                    }
                    if f_inner_on_seg > 0.0 {
                        let l = (f_inner_on_seg + inner_num_error) as i64;
                        inner_num_error -= l as f64 - f_inner_on_seg;
                        if l > 0 {
                            let base = if depth == 0 { stem.length * p.base_size[0] } else { 0.0 };
                            self.make_branches_from(&turtle, &stem, seg_ind, l, &mut inner_rotation_angle, true, base);
                        }
                    }
                } else if leaf_count.abs() > 0.0 && depth > 0 {
                    let leaves_on_seg: i64;
                    if leaf_count < 0.0 {
                        leaves_on_seg = if seg_ind == curve_res { leaf_count as i64 } else { 0 };
                    } else {
                        let l = (f_leaves_on_seg + leaf_num_error) as i64;
                        leaf_num_error -= l as f64 - f_leaves_on_seg;
                        leaves_on_seg = l;
                    }
                    if leaves_on_seg.abs() > 0 {
                        self.make_branches(&turtle, &stem, seg_ind, leaves_on_seg, &mut prev_rotation_angle, true);
                    }
                }
                self.rng = r_state;

                if !helix {
                    if num_of_splits > 0 {
                        let is_base_split = p.base_splits > 0.0 && depth == 0 && seg_ind == base_seg_ind;
                        let direct = p.split_angle[depth.min(3)] < 0.0;
                        let (spr_angle, spl_angle);
                        if direct {
                            spr_angle = p.split_angle[depth.min(3)].abs() + self.rng.sym() * p.split_angle_v[depth.min(3)];
                            spl_angle = 0.0;
                            split_corr_angle = 0.0;
                        } else {
                            let declination = turtle.dir.declination();
                            let sa = (p.split_angle[depth.min(3)] + self.rng.sym() * p.split_angle_v[depth.min(3)] - declination).max(0.0);
                            spl_angle = sa;
                            split_corr_angle = sa / remaining_segs;
                            let r = self.rng.random();
                            spr_angle = -(20.0 + 0.75 * (30.0 + (declination - 90.0).abs() * r * r));
                        }

                        let r_state = self.rng;
                        self.make_clones(&turtle, seg_ind, split_corr_angle, num_branches_factor, clone_prob, &stem, num_of_splits, spl_angle, spr_angle, is_base_split);
                        self.rng = r_state;

                        turtle.pitch_down(spl_angle / 2.0);
                        if !is_base_split && num_of_splits == 1 {
                            if direct {
                                turtle.turn_right(spr_angle / 2.0);
                            } else {
                                let q = Rot::axis_angle(V(0.0, 0.0, 1.0), (-spr_angle / 2.0).to_radians());
                                turtle.dir = q.apply(turtle.dir).normalized();
                                turtle.right = q.apply(turtle.right).normalized();
                            }
                        }
                    } else {
                        turtle.turn_left(self.rng.sym() * p.bend_v[depth.min(3)] / cr);
                        let curve_angle = self.calc_curve_angle(depth.min(3), seg_ind);
                        turtle.pitch_down(curve_angle - split_corr_angle);
                    }
                    if depth > 1 {
                        apply_tropism(&mut turtle, V(p.tropism[0], p.tropism[1], p.tropism[2]));
                    } else {
                        apply_tropism(&mut turtle, V(p.tropism[0], p.tropism[1], 0.0));
                    }
                    // Branches in shade bend toward the gaps their neighbours leave, the way real ones race for light.
                    if depth > 0 {
                        if let Some(sh) = &self.shade {
                            let shaded = (1.0 - sh.at(turtle.pos)).max(0.0).powf(0.7);
                            if let Some(g) = sh.toward(turtle.pos + turtle.dir * (2.0 * seg_length)) {
                                let axis = turtle.dir.cross(g);
                                let a = (self.light * 16f64.to_radians() * shaded).min(turtle.dir.angle(g));
                                if axis.mag() > 1e-6 && a > 0.0 {
                                    let q = Rot::axis_angle(axis, a);
                                    turtle.dir = q.apply(turtle.dir).normalized();
                                    turtle.right = q.apply(turtle.right).normalized();
                                }
                            }
                        }
                    }
                }

                if points_per_seg > 2 {
                    self.increase_bezier_point_res(&stem, seg_ind, points_per_seg);
                }
            }
        }

        if points_per_seg > 2 {
            for bp in self.splines[ci].1.iter_mut() {
                bp.hl = bp.co + (bp.hl - bp.co) * (1.0 / max_points_per_seg as f64);
                bp.hr = bp.co + (bp.hr - bp.co) * (1.0 / max_points_per_seg as f64);
            }
        }
    }

    /// Does the stem stay inside the pruning envelope? Mirrors `make_stem` without building anything.
    fn test_stem(&mut self, mut turtle: Turtle, stem: &Stem, start: usize, mut split_corr_angle: f64, mut clone_prob: f64) -> bool {
        let p = self.p;
        let depth = stem.depth;
        let d1 = Gen::d1(depth);
        let curve_res = p.curve_res[depth.min(3)] as usize;
        if curve_res == 0 {
            return true;
        }
        let cr = curve_res as f64;
        let seg_splits = p.seg_splits[depth.min(3)];
        let seg_length = stem.length / cr;
        let base_seg_ind = (p.base_size[0] * p.curve_res[0]).ceil() as usize;

        let _prev = if p.rotate[d1] >= 0.0 { self.rng.in_range(0.0, 360.0) } else { 1.0 };

        let helix = p.curve_v[depth.min(3)] < 0.0;
        let mut hel_p2 = V::default();
        let mut hel_axis = V::default();
        let mut previous_helix_point = V::default();
        if helix {
            let tan_ang = (90.0 - p.curve_v[depth.min(3)].abs()).to_radians().tan();
            let hel_pitch = 2.0 * stem.length / cr * self.rng.in_range(0.8, 1.2);
            let hel_radius = 3.0 * hel_pitch / (16.0 * tan_ang) * self.rng.in_range(0.8, 1.2);
            if depth > 1 {
                apply_tropism(&mut turtle, V(p.tropism[0], p.tropism[1], p.tropism[2]));
            } else {
                apply_tropism(&mut turtle, V(p.tropism[0], p.tropism[1], 0.0));
            }
            let mut r = self.rng;
            let h = helix_points(&turtle, hel_radius, hel_pitch, &mut r);
            self.rng = r;
            hel_p2 = h.2;
            hel_axis = h.3;
        }

        for seg_ind in start..=curve_res {
            let remaining_segs = (curve_res + 1 - seg_ind) as f64;
            if helix {
                let pos = turtle.pos;
                if seg_ind == 0 {
                    turtle.pos = pos;
                } else if seg_ind == 1 {
                    turtle.pos = hel_p2 + pos;
                } else {
                    hel_p2 = Rot::axis_angle(hel_axis, (seg_ind as f64 - 1.0) * PI).apply(hel_p2);
                    turtle.pos = hel_p2 + previous_helix_point;
                }
                previous_helix_point = turtle.pos;
            } else if seg_ind != start {
                turtle.mv(seg_length);
                if !(stem.depth == 0 && start < base_seg_ind) && !self.point_inside(turtle.pos) {
                    return false;
                }
            }

            if seg_ind > start && !helix {
                let mut num_of_splits = 0usize;
                if p.base_splits > 0.0 && depth == 0 && seg_ind == base_seg_ind {
                    num_of_splits = (self.rng.random() * (p.base_splits + 0.5)) as usize;
                } else if seg_splits > 0.0 && seg_ind < curve_res && (depth > 0 || seg_ind > base_seg_ind) {
                    if self.rng.random() <= clone_prob {
                        let n = (seg_splits + self.split_err[depth]) as i64;
                        num_of_splits = n.max(0) as usize;
                        self.split_err[depth] -= n as f64 - seg_splits;
                        clone_prob /= n as f64 + 1.0;
                    }
                }
                if num_of_splits > 0 {
                    let is_base_split = p.base_splits > 0.0 && depth == 0 && seg_ind == base_seg_ind;
                    let direct = p.split_angle[depth.min(3)] < 0.0;
                    let (spr_angle, spl_angle);
                    if direct {
                        spr_angle = p.split_angle[depth.min(3)].abs() + self.rng.sym() * p.split_angle_v[depth.min(3)];
                        spl_angle = 0.0;
                        split_corr_angle = 0.0;
                    } else {
                        let declination = turtle.dir.declination();
                        let sa = (p.split_angle[depth.min(3)] + self.rng.sym() * p.split_angle_v[depth.min(3)] - declination).max(0.0);
                        spl_angle = sa;
                        split_corr_angle = sa / remaining_segs;
                        let r = self.rng.random();
                        spr_angle = -(20.0 + 0.75 * (30.0 + (declination - 90.0).abs() * r * r));
                    }
                    turtle.pitch_down(spl_angle / 2.0);
                    if !is_base_split && num_of_splits == 1 {
                        if direct {
                            turtle.turn_left(spr_angle / 2.0);
                        } else {
                            let q = Rot::axis_angle(V(0.0, 0.0, 1.0), (-spr_angle / 2.0).to_radians());
                            turtle.dir = q.apply(turtle.dir).normalized();
                            turtle.right = q.apply(turtle.right).normalized();
                        }
                    }
                } else {
                    turtle.turn_left(self.rng.sym() * p.bend_v[depth.min(3)] / cr);
                    let curve_angle = self.calc_curve_angle(depth.min(3), seg_ind);
                    turtle.pitch_down(curve_angle - split_corr_angle);
                }
                if depth > 1 {
                    apply_tropism(&mut turtle, V(p.tropism[0], p.tropism[1], p.tropism[2]));
                } else {
                    apply_tropism(&mut turtle, V(p.tropism[0], p.tropism[1], 0.0));
                }
            }
        }
        self.point_inside(turtle.pos)
    }

    #[allow(clippy::too_many_arguments)]
    fn make_clones(
        &mut self,
        turtle: &Turtle,
        seg_ind: usize,
        split_corr_angle: f64,
        num_branches_factor: f64,
        clone_prob: f64,
        stem: &Stem,
        num_of_splits: usize,
        spl_angle: f64,
        spr_angle: f64,
        is_base_split: bool,
    ) {
        let p = self.p;
        let sd = stem.depth.min(3);
        let direct = p.split_angle[sd] < 0.0;
        let split_v = p.split_angle_v[sd];
        if !is_base_split && num_of_splits > 2 && direct {
            return; // the original raises an error here
        }
        for split_index in 0..num_of_splits {
            let mut nt = *turtle;
            nt.pitch_down(spl_angle / 2.0);
            let eff = if is_base_split && !direct {
                (split_index as f64 + 1.0) * (360.0 / (num_of_splits as f64 + 1.0)) + self.rng.sym() * split_v
            } else if split_index == 0 {
                spr_angle / 2.0
            } else {
                -spr_angle / 2.0
            };
            if direct {
                nt.turn_left(eff);
            } else {
                let q = Rot::axis_angle(V(0.0, 0.0, 1.0), eff.to_radians());
                nt.dir = q.apply(nt.dir).normalized();
                nt.right = q.apply(nt.right).normalized();
            }
            let spline = self.new_spline(stem.depth);
            let mut ns = *stem;
            ns.curve = spline;
            let cloned = if split_v >= 0.0 { Some(*turtle) } else { None };
            self.make_stem(nt, ns, seg_ind, split_corr_angle, num_branches_factor, clone_prob, None, cloned);
        }
    }

    fn make_branches(&mut self, turtle: &Turtle, stem: &Stem, seg_ind: usize, branches_on_seg: i64, prev_rot: &mut f64, is_leaves: bool) {
        self.make_branches_from(turtle, stem, seg_ind, branches_on_seg, prev_rot, is_leaves, stem.length * self.p.base_size[stem.depth.min(3)]);
    }

    #[allow(clippy::too_many_arguments)]
    fn make_branches_from(&mut self, turtle: &Turtle, stem: &Stem, seg_ind: usize, branches_on_seg: i64, prev_rot: &mut f64, is_leaves: bool, base_length: f64) {
        let p = self.p;
        let pts = &self.splines[stem.curve].1;
        let n = pts.len();
        if n < 2 {
            return;
        }
        let (start_point, end_point) = (pts[n - 2], pts[n - 1]);
        let d1 = Gen::d1(stem.depth);
        let mut arr: Vec<(Turtle, Turtle, f64, f64)> = Vec::new();

        if branches_on_seg < 0 {
            for branch_ind in 0..branches_on_seg.unsigned_abs() as usize {
                let b = self.set_up_branch(turtle, stem, Mode::Fan, 1.0, &start_point, &end_point, 1.0, branch_ind, prev_rot, branches_on_seg.unsigned_abs() as usize);
                arr.push(b);
            }
        } else {
            let branch_dist = p.branch_dist[d1];
            let curve_res = p.curve_res[stem.depth.min(3)];
            if branch_dist > 1.0 {
                let num_whorls = (branches_on_seg as f64 / (branch_dist + 1.0)) as usize;
                let per_whorl = branch_dist + 1.0;
                let mut whorl_err = 0.0;
                for whorl in 0..num_whorls {
                    let offset = (whorl as f64 / num_whorls as f64).clamp(0.0, 1.0);
                    let stem_offset = (((seg_ind as f64 - 1.0) + offset) / curve_res) * stem.length;
                    if stem_offset > base_length {
                        let this = (per_whorl + whorl_err) as i64;
                        whorl_err -= this as f64 - per_whorl;
                        for bi in 0..this.max(0) as usize {
                            let b = self.set_up_branch(turtle, stem, Mode::Whorled, offset, &start_point, &end_point, stem_offset, bi, prev_rot, this as usize);
                            arr.push(b);
                        }
                    }
                    *prev_rot += p.rotate[d1];
                }
            } else {
                for bi in 0..branches_on_seg as usize {
                    let offset = if bi % 2 == 0 {
                        (bi as f64 / branches_on_seg as f64).clamp(0.0, 1.0)
                    } else {
                        ((bi as f64 - branch_dist) / branches_on_seg as f64).clamp(0.0, 1.0)
                    };
                    let stem_offset = (((seg_ind as f64 - 1.0) + offset) / curve_res) * stem.length;
                    if stem_offset > base_length {
                        let b = self.set_up_branch(turtle, stem, Mode::AltOpp, offset, &start_point, &end_point, stem_offset, bi, prev_rot, 0);
                        arr.push(b);
                    }
                }
            }
        }

        if is_leaves {
            for (pos_t, dir_t, _, _) in arr {
                self.leaves.push(Leaf { position: pos_t.pos, direction: dir_t.dir, right: dir_t.right });
            }
        } else {
            for (pos_t, dir_t, rad, b_offset) in arr {
                let spline = self.new_spline(d1);
                let par = Par { length: stem.length, lcm: stem.lcm, radius: stem.radius };
                self.make_stem(dir_t, Stem::new(d1, spline, Some(par), b_offset, rad), 0, 0.0, 1.0, 1.0, Some(pos_t), None);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn set_up_branch(
        &mut self,
        turtle: &Turtle,
        stem: &Stem,
        mode: Mode,
        offset: f64,
        sp: &BP,
        ep: &BP,
        stem_offset: f64,
        branch_ind: usize,
        prev_rot: &mut f64,
        group: usize,
    ) -> (Turtle, Turtle, f64, f64) {
        let p = self.p;
        let d1 = Gen::d1(stem.depth);
        let mut dir_t = branch_dir_turtle(turtle, p.curve_v[stem.depth.min(3)] < 0.0, offset, sp, ep);
        let radius_limit;
        if mode == Mode::Fan {
            let t_angle = if group == 1 {
                0.0
            } else {
                p.rotate[d1] * (branch_ind as f64 / (group as f64 - 1.0) - 0.5) + self.rng.sym() * p.rotate_v[d1]
            };
            dir_t.turn_right(t_angle);
            radius_limit = 0.0;
        } else {
            let r_angle;
            if mode == Mode::Whorled {
                r_angle = *prev_rot + 360.0 * branch_ind as f64 / group as f64 + self.rng.sym() * p.rotate_v[d1];
            } else {
                r_angle = self.calc_rotate_angle(d1, *prev_rot);
                if p.rotate[d1] >= 0.0 {
                    *prev_rot = r_angle;
                } else {
                    *prev_rot = -*prev_rot;
                }
            }
            dir_t.roll_right(r_angle);
            radius_limit = self.radius_at_offset(stem, stem_offset / stem.length);
        }
        let pos_t = branch_pos_turtle(&mut dir_t, offset, sp, ep, radius_limit);
        let d_angle = self.calc_down_angle(stem, stem_offset);
        dir_t.pitch_down(d_angle);
        (pos_t, dir_t, radius_limit, stem_offset)
    }

    /// Add extra points along the last segment so the flared base of the trunk is smooth.
    fn increase_bezier_point_res(&mut self, stem: &Stem, seg_ind: usize, points_per_seg: usize) {
        let ci = stem.curve;
        let curve_res = self.p.curve_res[stem.depth.min(3)];
        let n = self.splines[ci].1.len();
        if n < 2 {
            return;
        }
        let end_point = self.splines[ci].1[n - 1];
        let start_point = self.splines[ci].1[n - 2];
        for k in 0..points_per_seg {
            let offset = k as f64 / (points_per_seg as f64 - 1.0);
            let idx;
            if k == 0 {
                idx = n - 2;
            } else {
                if k == 1 {
                    idx = n - 1;
                } else {
                    self.splines[ci].1.push(BP::new());
                    idx = self.splines[ci].1.len() - 1;
                }
                let cur = &mut self.splines[ci].1[idx];
                if k == points_per_seg - 1 {
                    cur.co = end_point.co;
                    cur.hl = end_point.hl;
                    cur.hr = end_point.hr;
                } else {
                    cur.co = point_on_bezier(offset, &start_point, &end_point);
                    let tangent = tangent_to_bezier(offset, &start_point, &end_point).normalized();
                    let mag = (end_point.hl - end_point.co).mag();
                    cur.hl = cur.co - tangent * mag;
                    cur.hr = cur.co + tangent * mag;
                }
            }
            let r = self.radius_at_offset(stem, (offset + seg_ind as f64 - 1.0) / curve_res);
            self.splines[ci].1[idx].radius = r;
        }
    }

    fn points_for_floor_split(&mut self) -> Vec<(V, f64)> {
        let p = self.p;
        let mut array: Vec<(V, f64)> = Vec::new();
        self.tree_scale = p.g_scale + p.g_scale_v;
        let mut stem = Stem::new(0, 0, None, 0.0, -1.0);
        stem.length = self.calc_stem_length(&stem);
        stem.radius = self.calc_stem_radius(&stem);
        let rad = 2.5 * stem.radius;
        for _ in 0..p.branches[0] as usize {
            // a crowded set of thick trunks can leave no room, so give up after a number of tries
            for attempt in 0..400 {
                let dis = (self.rng.random() * p.branches[0] / 2.5 * p.g_scale * p.ratio).sqrt();
                let theta = self.rng.in_range(0.0, TAU);
                let pos = V(dis * theta.cos(), dis * theta.sin(), 0.0);
                if array.iter().all(|q| (q.0 - pos).mag() >= rad) || attempt == 399 {
                    array.push((pos, theta));
                    break;
                }
            }
        }
        array
    }
}

// ---------------------------------------------------------------------------------------------- public

/// Build a tree from one of the presets, grown with `canopy`.
pub fn generate(preset: &Preset, seed: u64, canopy: &Canopy) -> TreeGeo {
    let mut p = *preset;
    let levels = (p.levels as usize).max(1);
    let twig = (levels - 1).min(3);
    p.leaf_blos_num *= canopy.leaves;
    p.ratio *= canopy.wood;
    p.base_size[1] *= canopy.limb_base;
    if levels > 2 {
        p.curve[1] -= canopy.limb_rise;
    }
    if levels > 2 && p.branches[1] > 0.0 {
        p.branches[1] *= canopy.limbs;
    }
    if twig > 0 && p.branches[twig] > 0.0 {
        p.branches[twig] *= canopy.twigs;
    }
    p.length[twig] *= canopy.twig_len;
    p.curve[twig] -= canopy.rise;
    p.leaf_scale *= canopy.leaf_size;
    p.leaf_scale_x = (p.leaf_scale_x * canopy.leaf_width).min(p.leaf_scale_x.max(1.0));
    p.blossom_scale *= canopy.blossom_size;
    // With light competition the tree is grown, its shade measured, and grown again from the same seed.
    let passes = if canopy.light > 0.0 && levels > 1 { 3 } else { 1 };
    let mut shade: Option<Shade> = None;
    for pass in 0..passes {
        let geo = grow(p, seed, levels, canopy, shade.clone());
        if pass + 1 == passes {
            return geo;
        }
        shade = Some(Shade::build(&geo.leaves, &geo.splines, shade.as_ref()));
    }
    unreachable!()
}

fn grow(p: Preset, seed: u64, levels: usize, canopy: &Canopy, shade: Option<Shade>) -> TreeGeo {
    let mut g = Gen { p, rng: Rand::new(seed), tree_scale: 0.0, base_length: 0.0, split_err: [0.0; 7], splines: Vec::new(), leaves: Vec::new(), levels, inner: canopy.inner, light: canopy.light, shade };
    let n_trunks = p.branches[0] as usize;
    let floor_points = if n_trunks > 0 { g.points_for_floor_split() } else { Vec::new() };
    for ind in 0..n_trunks {
        g.tree_scale = p.g_scale + g.rng.sym() * p.g_scale_v;
        let mut turtle = Turtle::new();
        if n_trunks > 1 {
            let (pos, theta) = floor_points[ind];
            turtle.roll_right((theta - 90.0).to_degrees());
            turtle.pos = pos;
        } else {
            turtle.roll_right(g.rng.in_range(0.0, 360.0));
        }
        let spline = g.new_spline(0);
        g.make_stem(turtle, Stem::new(0, spline, None, 0.0, -1.0), 0, 0.0, 1.0, 1.0, None, None);
    }
    TreeGeo { preset: p, canopy: *canopy, splines: g.splines, leaves: g.leaves, tree_scale: g.tree_scale }
}

pub fn preset_by_name(name: &str) -> Option<&'static Preset> {
    crate::treegen_data::PRESETS.iter().find(|p| p.name == name).or_else(|| crate::extra::find(name).map(|e| &e.preset))
}

impl TreeGeo {
    /// Leaf and blossom polygons, all at once. Big crowns have millions, so drawing should use
    /// `for_each_polygon`.
    pub fn polygons(&self, rng: &mut Rand) -> Vec<Poly> {
        let mut out = Vec::with_capacity(self.leaves.len() * 3);
        self.for_each_polygon(rng, |p| out.push(p));
        out
    }

    /// Every leaf and blossom polygon, one at a time and in the same order as `polygons`, so none are kept.
    pub fn for_each_polygon(&self, rng: &mut Rand, mut f: impl FnMut(Poly)) {
        let p = &self.preset;
        let ratio = self.tree_scale / p.g_scale;
        let leaf_type = p.leaf_shape as i64;
        let leaf_type = if !(1..=11).contains(&leaf_type) { 8 } else { leaf_type };
        let (lv, lf): (&[[f64; 3]], &[&[usize]]) = if leaf_type == 11 { (&crate::extra::GINKGO.0, crate::extra::GINKGO.1) } else { LEAF_SHAPES[(leaf_type - 1) as usize] };
        let scale_leaf = |v: &[f64; 3], scale: f64, sx: f64| V(v[0] * scale * ratio * sx, v[1] * scale * ratio, v[2] * scale * ratio);
        let leaf_verts: Vec<V> = lv.iter().map(|v| scale_leaf(v, p.leaf_scale, p.leaf_scale_x)).collect();
        let blossom_ok = p.blossom_scale > 0.0 && p.blossom_rate > 0.0;
        let bi = (p.blossom_shape as i64 - 1).clamp(0, 2) as usize;
        let (bv, bf) = BLOSSOM_SHAPES[bi];
        let blossom_verts: Vec<V> = bv.iter().map(|v| scale_leaf(v, p.blossom_scale, 1.0)).collect();

        for leaf in &self.leaves {
            let blossom = blossom_ok && rng.random() < p.blossom_rate;
            let (verts, faces) = if blossom { (&blossom_verts, bf) } else { (&leaf_verts, lf) };
            let moved = leaf_mesh(leaf, p.leaf_bend, self.canopy.sky, verts);
            for face in faces {
                f(Poly { verts: face.iter().map(|&i| moved[i]).collect(), blossom, tone: None });
            }
        }
    }
}

/// Place a base leaf shape at a leaf, aligned to its direction and bent out (leaf.py `get_mesh`), then turned
/// by `sky` toward facing straight up, the way leaves turn to the light.
fn leaf_mesh(leaf: &Leaf, bend: f64, sky: f64, base: &[V]) -> Vec<V> {
    let trf = Rot::track_z_up_y(leaf.direction);
    let right_t = trf.inverse().apply(leaf.right);
    let spin = PI - right_t.angle(V(1.0, 0.0, 0.0));
    let spin_q = Rot::axis_angle(V(0.0, 0.0, 1.0), spin);
    let bend_q = if bend > 0.0 {
        let normal = leaf.direction.cross(leaf.right);
        let theta_pos = leaf.position.1.atan2(leaf.position.0);
        let theta_bend = theta_pos - normal.1.atan2(normal.0);
        Some(Rot::axis_angle(V(0.0, 0.0, 1.0), theta_bend * bend))
    } else {
        None
    };
    let place = |v: V| {
        let n = trf.apply(spin_q.apply(v));
        match &bend_q {
            Some(b) => b.apply(n),
            None => n,
        }
    };
    // base shapes lie flat in the x-z plane, so their face points along y
    let sky_q = if sky > 0.0 {
        let mut normal = place(V(0.0, 1.0, 0.0)).normalized();
        if normal.2 < 0.0 {
            normal = -normal;
        }
        let axis = normal.cross(V(0.0, 0.0, 1.0));
        let a = normal.2.clamp(-1.0, 1.0).acos() * sky.min(1.0);
        Some(Rot::axis_angle(axis, a))
    } else {
        None
    };
    base.iter()
        .map(|&v| {
            let mut n = place(v);
            if let Some(q) = &sky_q {
                n = q.apply(n);
            }
            n + leaf.position
        })
        .collect()
}
