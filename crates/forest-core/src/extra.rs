//! More tree types, made from the tree-gen presets with a few parameters changed (`assets/extra_presets.json`),
//! plus the ones the branching generator cannot make by itself: stilt roots, fallen logs and stumps.
//!
//! Each entry names a base preset and lists the fields to change. For trying out values,
//! `TREE_INATOR_EXTRAS=<file>` reads the table from a file instead of the built-in one.

use crate::treegen::{Canopy, Rand, TreeGeo, BP, V};
use crate::treegen_data::Preset;
use serde_json::Value;
use std::sync::OnceLock;

static BUILT_IN: &str = include_str!("../assets/extra_presets.json");

pub struct Extra {
    pub name: String,
    pub preset: Preset,
    pub canopy: Canopy,
    pub roots: Option<Roots>,
    pub kind: Kind,
    /// Shrinks the crown inside its sprite, to leave room for roots or other parts around it.
    pub crown_scale: f64,
    /// Draws the branches as the darkest foliage instead of wood, for dense needled crowns.
    pub hide_wood: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// An ordinary tree from the generator.
    Tree,
    /// A tree lying on the ground.
    Log,
    /// A cut stump seen from above.
    Stump,
}

/// Roots that arch out from the trunk to the ground, beyond the crown.
#[derive(Clone, Copy)]
pub struct Roots {
    pub count: usize,
    /// How far they reach, in crown radii.
    pub reach: f64,
    /// Height on the trunk where they leave, as a share of the trunk's height.
    pub from: f64,
    /// Thickness as a share of the trunk's radius.
    pub thick: f64,
}

/// A glossy fan leaf, for the ginkgo.
pub const GINKGO: ([[f64; 3]; 11], &[&[usize]]) = (
    [[0.005, 0.0, 0.0], [0.005, 0.0, 0.2], [0.45, 0.0, 0.55], [0.62, 0.0, 0.8], [0.35, 0.0, 0.98], [0.0, 0.0, 0.88], [-0.35, 0.0, 0.98], [-0.62, 0.0, 0.8], [-0.45, 0.0, 0.55], [-0.005, 0.0, 0.2], [-0.005, 0.0, 0.0]],
    &[&[0, 1, 9, 10], &[1, 2, 3, 4, 5, 6, 7, 8, 9]],
);

fn table() -> &'static Vec<Extra> {
    static T: OnceLock<Vec<Extra>> = OnceLock::new();
    T.get_or_init(|| {
        let text = std::env::var("TREE_INATOR_EXTRAS").ok().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_else(|| BUILT_IN.to_string());
        load(&text)
    })
}

pub fn find(name: &str) -> Option<&'static Extra> {
    table().iter().find(|e| e.name == name)
}

/// The names of the extra tree types, in table order.
pub fn names() -> Vec<&'static str> {
    table().iter().map(|e| e.name.as_str()).collect()
}

fn load(text: &str) -> Vec<Extra> {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let mut out = Vec::new();
    for (name, v) in map {
        let base = v.get("base").and_then(|b| b.as_str()).unwrap_or("black_oak");
        let Some(base) = crate::treegen_data::PRESETS.iter().find(|p| p.name == base) else { continue };
        let mut preset = *base;
        if let Some(Value::Object(o)) = v.get("preset") {
            patch_preset(&mut preset, o);
        }
        let mut canopy = Canopy { leaves: 2.0, twigs: 1.5, twig_len: 1.1, leaf_size: 1.3, inner: 0.4, sky: 0.7, light: 0.5, ..Canopy::RAW };
        if let Some(Value::Object(o)) = v.get("canopy") {
            patch_canopy(&mut canopy, o);
        }
        let roots = v.get("roots").and_then(|r| {
            let g = |k: &str, d: f64| r.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
            Some(Roots { count: g("count", 8.0) as usize, reach: g("reach", 1.2), from: g("from", 0.3), thick: g("thick", 0.35) })
        });
        let kind = match v.get("kind").and_then(|k| k.as_str()) {
            Some("log") => Kind::Log,
            Some("stump") => Kind::Stump,
            _ => Kind::Tree,
        };
        let crown_scale = v.get("crown_scale").and_then(|x| x.as_f64()).unwrap_or(1.0).max(0.2);
        let hide_wood = v.get("hide_wood").and_then(|x| x.as_bool()).unwrap_or(false);
        out.push(Extra { name, preset, canopy, roots, kind, crown_scale, hide_wood });
    }
    out
}

fn patch_preset(p: &mut Preset, o: &serde_json::Map<String, Value>) {
    macro_rules! scalars {
        ($($f:ident),*) => { $( if let Some(v) = o.get(stringify!($f)).and_then(|v| v.as_f64()) { p.$f = v; } )* };
    }
    macro_rules! arrays {
        ($($f:ident),*) => { $( if let Some(a) = o.get(stringify!($f)).and_then(|v| v.as_array()) {
            for (i, x) in a.iter().enumerate().take(p.$f.len()) { if let Some(f) = x.as_f64() { p.$f[i] = f; } }
        } )* };
    }
    scalars!(shape, g_scale, g_scale_v, levels, ratio, ratio_power, flare, base_splits, leaf_blos_num, leaf_shape, leaf_scale, leaf_scale_x, leaf_bend, blossom_shape, blossom_scale, blossom_rate, prune_ratio, prune_width, prune_width_peak, prune_power_low, prune_power_high);
    arrays!(base_size, down_angle, down_angle_v, rotate, rotate_v, branches, length, length_v, taper, seg_splits, split_angle, split_angle_v, curve_res, curve, curve_back, curve_v, bend_v, branch_dist, radius_mod, tropism);
}

fn patch_canopy(c: &mut Canopy, o: &serde_json::Map<String, Value>) {
    macro_rules! fields {
        ($($f:ident),*) => { $( if let Some(v) = o.get(stringify!($f)).and_then(|v| v.as_f64()) { c.$f = v; } )* };
    }
    fields!(leaves, wood, limb_base, limb_rise, limbs, twigs, twig_len, leaf_size, leaf_width, blossom_size, inner, sky, light, rise);
}

/// What a direct builder (one that makes its own branches) returns: the branches, a tone for each, how much
/// the tone varies along a branch, and the view as centre, radius and height in tree units.
pub struct Direct {
    pub geo: TreeGeo,
    pub tones: Vec<f32>,
    pub noise: f32,
    pub view: (f64, f64, f64, f64),
}

fn bp(co: V, radius: f64) -> BP {
    BP { co, hl: co, hr: co, radius }
}

fn empty_geo(preset: &Preset, canopy: &Canopy) -> TreeGeo {
    TreeGeo { preset: *preset, canopy: *canopy, splines: Vec::new(), leaves: Vec::new(), tree_scale: 1.0 }
}

/// Trees that are built without the generator: logs and stumps.
pub fn direct(e: &Extra, seed: u64) -> Option<Direct> {
    let mut rng = Rand::new(seed ^ 0x106_106);
    match e.kind {
        Kind::Tree => None,
        Kind::Stump => {
            let mut geo = empty_geo(&e.preset, &e.canopy);
            let mut tones = Vec::new();
            // a cut face with a few faint rings, a lumpy bark rim, and short stubs of root at the foot
            let r_out = 0.5 + rng.sym() * 0.04;
            let (ox, oy) = (rng.sym() * 0.03, rng.sym() * 0.03);
            let lump = rng.in_range(0.0, std::f64::consts::TAU);
            let mut rho = 0.03;
            let mut k = 0;
            while rho < r_out - 0.07 {
                let w = 0.03 + 0.03 * rng.random();
                let n = ((rho * 120.0) as usize).max(12);
                let wob = rng.in_range(0.0, std::f64::consts::TAU);
                let pts = (0..=n).map(|j| {
                    let a = j as f64 / n as f64 * std::f64::consts::TAU;
                    let rr = rho * (1.0 + 0.04 * (2.0 * a + wob).sin() + 0.03 * (5.0 * a + lump).sin());
                    bp(V(ox + a.cos() * rr, oy + a.sin() * rr, 0.06), w)
                });
                geo.splines.push((1, pts.collect()));
                tones.push(if k % 3 == 0 { 0.6 } else { 0.68 });
                rho += w * 0.95;
                k += 1;
            }
            let rim = |r: f64, w: f64, z: f64| -> Vec<BP> {
                (0..=64).map(|j| {
                    let a = j as f64 / 64.0 * std::f64::consts::TAU;
                    let rr = r * (1.0 + 0.05 * (3.0 * a + lump).sin() + 0.03 * (8.0 * a + 1.0).sin());
                    bp(V(ox + a.cos() * rr, oy + a.sin() * rr, z), w)
                }).collect()
            };
            geo.splines.push((1, rim(r_out - 0.03, 0.07, 0.05)));
            tones.push(0.22);
            // one crack running in from the rim
            let ca = rng.in_range(0.0, std::f64::consts::TAU);
            let crack: Vec<BP> = (0..5).map(|j| {
                let u = j as f64 / 4.0;
                bp(V(ox + ca.cos() * r_out * (0.95 - 0.7 * u), oy + ca.sin() * r_out * (0.95 - 0.7 * u), 0.065), 0.012)
            }).collect();
            geo.splines.push((1, crack));
            tones.push(0.3);
            for _ in 0..rng.in_range(3.0, 5.0) as usize {
                let a = rng.in_range(0.0, std::f64::consts::TAU);
                let l = rng.in_range(0.08, 0.16);
                let pts = (0..4).map(|j| {
                    let u = j as f64 / 3.0;
                    bp(V(a.cos() * (r_out * 0.92 + l * u), a.sin() * (r_out * 0.92 + l * u), 0.03), 0.09 * (1.0 - 0.6 * u))
                });
                geo.splines.push((1, pts.collect()));
                tones.push(0.24);
            }
            Some(Direct { geo, tones, noise: 0.03, view: (0.0, 0.0, 1.0, 0.1) })
        }
        Kind::Log => {
            let mut geo = empty_geo(&e.preset, &e.canopy);
            let mut tones = Vec::new();
            let ang = rng.in_range(0.0, std::f64::consts::TAU);
            let (dx, dy) = (ang.cos(), ang.sin());
            let (nx, ny) = (-dy, dx);
            // long and thin, nearly straight, barely tapered
            let len = 1.85 + rng.sym() * 0.1;
            let bend = rng.sym() * 0.06;
            let r_root = 0.04 + rng.random() * 0.02;
            let at = |u: f64| -> V {
                let s = (u - 0.5) * len;
                let side = bend * 4.0 * u * (1.0 - u);
                V(dx * s + nx * side, dy * s + ny * side, 0.07)
            };
            let pts: Vec<BP> = (0..=24)
                .map(|j| {
                    let u = j as f64 / 24.0;
                    let flare = if u < 0.05 { 1.0 + (0.05 - u) * 6.0 } else { 1.0 };
                    bp(at(u), r_root * (1.0 - 0.18 * u) * flare)
                })
                .collect();
            geo.splines.push((0, pts));
            tones.push(0.72);
            // a few short broken branch stubs, thin and pale
            let n = rng.in_range(2.0, 5.0) as usize;
            for _ in 0..n {
                let u = rng.in_range(0.1, 0.9);
                let side = if rng.random() < 0.5 { 1.0 } else { -1.0 };
                let base = at(u);
                let a = ang + side * rng.in_range(0.7, 1.2);
                let l = rng.in_range(0.06, 0.22);
                let rad = r_root * rng.in_range(0.25, 0.4);
                let pts: Vec<BP> = (0..3).map(|j| {
                    let t = j as f64 / 2.0;
                    bp(base + V(a.cos() * l * t, a.sin() * l * t, 0.03), rad * (1.0 - 0.4 * t))
                }).collect();
                geo.splines.push((1, pts));
                tones.push(0.7);
            }
            Some(Direct { geo, tones, noise: 0.05, view: (0.0, 0.0, 1.0, 0.1) })
        }
    }
}

/// Stilt roots: arches from the trunk out to the ground, past the edge of the crown. `cx`, `cy` is the
/// trunk, `r` the crown radius and `zmax` the tree's height, all in tree units.
pub fn add_roots(geo: &mut TreeGeo, roots: &Roots, seed: u64, cx: f64, cy: f64, r: f64, zmax: f64) -> Vec<f32> {
    let mut rng = Rand::new(seed ^ 0x7007);
    let trunk_r = geo.splines.first().map_or(0.02, |s| s.1[0].radius);
    let mut tones = vec![f32::NAN; geo.splines.len()];
    for k in 0..roots.count {
        let a = (k as f64 + rng.sym() * 0.3) / roots.count as f64 * std::f64::consts::TAU;
        let reach = r * roots.reach * rng.in_range(0.75, 1.1);
        let z0 = zmax * roots.from * rng.in_range(0.5, 1.2);
        let mut pts = Vec::new();
        let wob = rng.sym() * 0.25;
        for j in 0..=6 {
            let u = j as f64 / 6.0;
            // out from the trunk in an arch that comes down to the ground
            let d = reach * (0.04 + 0.96 * (1.0 - (1.0 - u).powf(1.6)));
            let aa = a + wob * u * (1.0 - u);
            let z = z0 * (1.0 - u * u) + 0.02;
            pts.push(bp(V(cx + aa.cos() * d, cy + aa.sin() * d, z), trunk_r * roots.thick * (1.0 - 0.45 * u)));
        }
        geo.splines.push((1, pts));
        tones.push(0.3 + 0.1 * rng.random() as f32);
    }
    tones
}
