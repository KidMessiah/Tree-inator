//! Draws trees from `treegen` from above into the engine's sprite format (height, surface direction, tone,
//! depth, class), caches them, and resamples them to any crown size.
//!
//! Each preset is grown with its own `Canopy` (see `canopy_for`), so sparse crowns become solid through the
//! generator itself: more twigs, more and larger leaves, leaves on inner branches and leaves turned up to the
//! sky. Nothing is painted in afterwards.

use crate::treegen::{generate, preset_by_name, Canopy, Poly, Rand, TreeGeo, V};
use crate::sprite::Sprite;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const SS: usize = 2;
/// Crown radius, in pixels, of the stored reference sprites.
pub const R0: f32 = 96.0;
/// How many variants (seeds) of each tree type are made.
pub const VARIANTS: u32 = 5;

/// Procedural tree types: preset name and the name shown in the app.
pub const TREE_TYPES: &[(&str, &str)] = &[
    ("quaking_aspen", "Quaking aspen"),
    ("black_tupelo", "Black tupelo"),
    ("black_oak", "Black oak"),
    ("cambridge_oak", "Cambridge oak"),
    ("sassafras", "Sassafras"),
    ("silver_birch", "Silver birch"),
    ("acer", "Japanese maple"),
    ("sphere_tree", "Round tree"),
    ("lombardy_poplar", "Lombardy poplar"),
    ("weeping_willow", "Weeping willow"),
    ("weeping_willow_o", "Weeping willow, wide"),
    ("apple", "Apple in blossom"),
    ("hill_cherry", "Cherry in blossom"),
    ("balsam_fir", "Balsam fir"),
    ("douglas_fir", "Douglas fir"),
    ("small_pine", "Pine"),
    ("european_larch", "European larch"),
    ("bamboo", "Bamboo"),
    ("palm", "Palm"),
    ("fan_palm", "Fan palm"),
    ("bare_oak", "Bare oak"),
    ("dead_oak", "Dead oak"),
    ("stag_oak", "Stag-headed oak"),
    ("dead_birch", "Dead birch"),
    ("dead_willow", "Dead willow"),
    ("dead_pine", "Dead pine"),
    ("snag", "Snag"),
    ("burnt_oak", "Burnt oak"),
    ("burnt_pine", "Burnt pine"),
    ("baobab", "Baobab"),
    ("banyan", "Banyan"),
    ("mangrove", "Mangrove, on stilt roots"),
    ("dragon_blood", "Dragon blood tree"),
    ("joshua_tree", "Joshua tree"),
    ("krummholz", "Krummholz, wind-bent pine"),
    ("eucalyptus", "Eucalyptus"),
    ("juniper", "Juniper"),
    ("spruce", "Spruce, spired"),
    ("bald_cypress", "Bald cypress"),
    ("live_oak", "Live oak, spreading"),
    ("umbrella_acacia", "Umbrella acacia"),
    ("ginkgo", "Ginkgo"),
    ("fallen_log", "Fallen log"),
    ("stump", "Stump"),
    ("mushroom_redcap", "Mushroom, redcap"),
    ("mushroom_brown", "Mushroom, brown cap"),
    ("mushroom_violet", "Mushroom, violet cap"),
    ("mushroom_funnel", "Mushroom, funnel cap"),
    ("mushroom_puffball", "Puffball"),
];

/// The name shown in the app for a tree type.
pub fn tree_type_label(name: &str) -> &str {
    TREE_TYPES.iter().find(|t| t.0 == name).map(|t| t.1).unwrap_or(name)
}

/// How much of each leaf's own tilt is replaced by the tilt of the crown around it, so a clump is lit as one
/// dome (lit on the sun side, dark on the far side) instead of leaf by leaf.
const CROWN_NORMALS: f32 = 0.6;

/// How each preset's crown is grown for a view from above.
pub fn canopy_for(name: &str) -> Canopy {
    let mut c = canopy_base(name);
    // How hard branches compete for light. Types that go lopsided or gappy with it get less.
    c.light = match name {
        "sassafras" => 0.25,
        "european_larch" => 0.2,
        "black_tupelo" | "acer" | "cambridge_oak" => 0.3,
        _ => c.light,
    };
    c
}

fn canopy_base(name: &str) -> Canopy {
    if let Some(d) = crate::dead::style(name) {
        return d.canopy;
    }
    if let Some(e) = crate::extra::find(name) {
        return e.canopy;
    }
    let normal = Canopy { leaves: 2.0, twigs: 1.5, twig_len: 1.1, leaf_size: 1.3, inner: 0.4, sky: 0.7, light: 0.7, ..Canopy::RAW };
    match name {
        // palms keep their open fronds
        "palm" => Canopy::RAW,
        // more, shorter fronds with fewer and wider blades: an even spread over the whole top instead of a ring
        // around the trunk, without the dense fur
        "fan_palm" => Canopy { leaves: 0.35, twigs: 3.0, twig_len: 0.6, leaf_size: 0.8, wood: 0.15, ..Canopy::RAW },
        // already full, so only a little
        "lombardy_poplar" | "sphere_tree" => Canopy { leaves: 1.4, twigs: 1.2, twig_len: 1.0, leaf_size: 1.15, inner: 0.2, sky: 0.5, ..normal },
        // few limbs leave empty sides, so it grows more of them
        "acer" => Canopy { leaves: 3.0, twigs: 2.0, twig_len: 1.5, leaf_size: 1.6, inner: 1.5, rise: 30.0, limbs: 2.0, ..normal },
        "apple" => Canopy { blossom_size: 2.0, ..normal },
        "hill_cherry" => Canopy { blossom_size: 2.4, ..normal },
        "silver_birch" | "douglas_fir" | "european_larch" => Canopy { leaf_size: 1.4, inner: 1.0, ..normal },
        // needles are tiny next to the branches, so they grow larger and along the branches too
        "balsam_fir" => Canopy { twig_len: 1.2, leaf_size: 2.0, inner: 2.0, sky: 0.6, ..normal },
        "bamboo" => Canopy { leaves: 3.0, twigs: 2.0, twig_len: 1.3, leaf_size: 1.8, inner: 0.0, sky: 0.8, ..normal },
        // twisting limbs rise over the leaves, so the twigs turn up to sit on top of them
        "cambridge_oak" => Canopy { leaves: 3.0, twigs: 2.0, twig_len: 1.2, leaf_size: 1.8, inner: 2.0, sky: 0.8, rise: 60.0, ..normal },
        // narrow leaves on long hanging twigs: wider leaves, slimmer limbs that branch from the start and
        // rise before they arch, so the crown is a dome rather than a bowl
        "weeping_willow" => Canopy { leaves: 3.0, leaf_size: 2.0, leaf_width: 3.0, inner: 4.0, sky: 0.8, wood: 0.4, limbs: 1.6, limb_base: 0.0, limb_rise: 120.0, ..normal },
        "weeping_willow_o" => Canopy { leaves: 3.0, leaf_size: 2.0, leaf_width: 3.0, inner: 4.0, sky: 0.8, wood: 0.4, limb_base: 0.0, limb_rise: 120.0, ..normal },
        _ => normal,
    }
}

/// The canopy at strength `fill`: below zero is the preset's own, 0 the raw generator.
fn canopy_at(name: &str, fill: f32) -> Canopy {
    let k = if fill < 0.0 { 1.0 } else { fill as f64 };
    canopy_for(name).scaled(k)
}

struct Raster {
    size: usize,
    z: Vec<f32>,
    nx: Vec<f32>,
    ny: Vec<f32>,
    tone: Vec<f32>,
    cls: Vec<u8>,
}

impl Raster {
    fn new(size: usize) -> Raster {
        let n = size * size;
        Raster { size, z: vec![f32::MIN; n], nx: vec![0.0; n], ny: vec![0.0; n], tone: vec![0.0; n], cls: vec![0; n] }
    }

    #[allow(clippy::too_many_arguments)]
    fn tri(&mut self, p: [[f32; 2]; 3], z: [f32; 3], nx: f32, ny: f32, tone: f32, cls: u8) {
        let min_x = p.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let max_x = (p.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil() as i32).min(self.size as i32 - 1);
        let min_y = p.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let max_y = (p.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil() as i32).min(self.size as i32 - 1);
        if max_x < min_x || max_y < min_y {
            return;
        }
        let d = (p[1][1] - p[2][1]) * (p[0][0] - p[2][0]) + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
        if d.abs() < 1e-9 {
            return;
        }
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let a = ((p[1][1] - p[2][1]) * (fx - p[2][0]) + (p[2][0] - p[1][0]) * (fy - p[2][1])) / d;
                let b = ((p[2][1] - p[0][1]) * (fx - p[2][0]) + (p[0][0] - p[2][0]) * (fy - p[2][1])) / d;
                let c = 1.0 - a - b;
                if a < -0.001 || b < -0.001 || c < -0.001 {
                    continue;
                }
                let zz = a * z[0] + b * z[1] + c * z[2];
                let i = y as usize * self.size + x as usize;
                if zz > self.z[i] {
                    self.z[i] = zz;
                    self.nx[i] = nx;
                    self.ny[i] = ny;
                    self.tone[i] = tone;
                    self.cls[i] = cls;
                }
            }
        }
    }

    /// A tapered capsule between two points, drawn per pixel.
    #[allow(clippy::too_many_arguments)]
    fn capsule(&mut self, a: [f32; 2], b: [f32; 2], za: f32, zb: f32, ra: f32, rb: f32, dome: f32, tone: f32, round: f32, cls: u8) {
        let rmax = ra.max(rb);
        let min_x = (a[0].min(b[0]) - rmax).floor().max(0.0) as i32;
        let max_x = ((a[0].max(b[0]) + rmax).ceil() as i32).min(self.size as i32 - 1);
        let min_y = (a[1].min(b[1]) - rmax).floor().max(0.0) as i32;
        let max_y = ((a[1].max(b[1]) + rmax).ceil() as i32).min(self.size as i32 - 1);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = (dx * dx + dy * dy).max(1e-9);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((fx - a[0]) * dx + (fy - a[1]) * dy) / len2).clamp(0.0, 1.0);
                let (cx, cy) = (a[0] + dx * t, a[1] + dy * t);
                let r = ra + (rb - ra) * t;
                let (ox, oy) = (fx - cx, fy - cy);
                let dd = (ox * ox + oy * oy).sqrt();
                if dd > r {
                    continue;
                }
                let u = dd / r.max(1e-6);
                let zz = za + (zb - za) * t + (1.0 - u * u).max(0.0).sqrt() * dome;
                let i = y as usize * self.size + x as usize;
                if zz > self.z[i] {
                    self.z[i] = zz;
                    self.nx[i] = ox / r.max(1e-6) * round;
                    self.ny[i] = oy / r.max(1e-6) * round;
                    self.tone[i] = tone;
                    self.cls[i] = cls;
                }
            }
        }
    }
}

struct View {
    cx: f64,
    cy: f64,
    k: f64,
    zmax: f64,
}

fn view_for(geo: &TreeGeo) -> View {
    let mut pts: Vec<(f64, f64)> = geo.leaves.iter().map(|l| (l.position.0, l.position.1)).collect();
    if pts.is_empty() {
        for (_, s) in &geo.splines {
            for b in s {
                pts.push((b.co.0, b.co.1));
            }
        }
    }
    let n = pts.len().max(1) as f64;
    let cx = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let cy = pts.iter().map(|p| p.1).sum::<f64>() / n;
    let mut d: Vec<f64> = pts.iter().map(|p| ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()).collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| d.get(((d.len().max(1) - 1) as f64 * q) as usize).copied().unwrap_or(1.0);
    let r_fit = at(0.92).max(at(0.99) / 1.35).max(1e-4);
    let mut zmax = 1e-6f64;
    for (_, s) in &geo.splines {
        for b in s {
            zmax = zmax.max(b.co.2);
        }
    }
    for l in &geo.leaves {
        zmax = zmax.max(l.position.2);
    }
    View { cx, cy, k: 1.0 / r_fit, zmax: zmax * 1.02 }
}

/// A palmate fan palm seen from above: a ring of big fans, a smaller ring above them and a starburst of young
/// leaves in the middle. Each fan is a solid hub with long thin leaflets spreading from it, so it is closed
/// near the middle and splits into separate blades toward the rim, with the fans overlapping their neighbours.
fn fan_palm_polys(seed: u64) -> Vec<Poly> {
    let mut rng = Rand::new(seed ^ 0x9A1F);
    let mut out = Vec::new();
    // (fans, hub distance from the trunk, leaflet length, half spread in degrees, height, leaflets per fan)
    let rings: [(usize, f64, f64, f64, f64, usize); 3] = [(7, 0.36, 0.76, 70.0, 0.50, 34), (4, 0.16, 0.56, 78.0, 0.50, 28), (2, 0.02, 0.34, 180.0, 0.50, 30)];
    let mut order = 0.0;
    for (ri, &(n, hub_r, len, spread, height, m)) in rings.iter().enumerate() {
        let phase = rng.in_range(0.0, 6.283);
        for i in 0..n {
            let dir = phase + (i as f64 + rng.sym() * 0.15) / n as f64 * 6.283 + ri as f64 * 0.6;
            let (bx, by) = (dir.cos() * hub_r, dir.sin() * hub_r);
            let scale = 1.0 + rng.sym() * 0.1;
            // each fan sits a little above the one before it, and has its own brightness
            order += 1.0;
            let z0 = height + order * 0.006;
            let fan_tone = (0.12 + 0.76 * rng.random()) as f32;
            let hub_len = len * scale * if ri == 2 { 0.5 } else { 0.26 };
            let mut hub = vec![V(bx, by, z0)];
            for j in 0..=10 {
                let a = dir + (j as f64 / 10.0 - 0.5) * 2.0 * spread.to_radians();
                hub.push(V(bx + a.cos() * hub_len, by + a.sin() * hub_len, z0 - 0.01));
            }
            out.push(Poly { verts: hub, blossom: false, tone: Some(fan_tone) });
            for j in 0..m {
                let t = j as f64 / (m - 1) as f64 - 0.5;
                let a = dir + t * 2.0 * spread.to_radians() + rng.sym() * 0.02;
                let l = len * scale * (0.84 + 0.16 * (1.0 - (2.0 * t).abs().powi(2))) * (0.92 + 0.08 * rng.random());
                let (ca, sa) = (a.cos(), a.sin());
                let w = 0.022 * (0.8 + 0.4 * rng.random());
                let (nx, ny) = (-sa, ca);
                // each blade is tilted a little its own way, so neighbours catch the light differently
                let tip_z = z0 - 0.06 + rng.sym() * 0.05;
                let start = hub_len * 0.8;
                let (sx, sy) = (bx + ca * start, by + sa * start);
                let tone = (fan_tone + rng.sym() as f32 * 0.08).clamp(0.05, 0.95);
                out.push(Poly { verts: vec![V(sx + nx * w, sy + ny * w, z0), V(bx + ca * l, by + sa * l, tip_z), V(sx - nx * w, sy - ny * w, z0)], blossom: false, tone: Some(tone) });
            }
        }
    }
    out
}

fn project(v: V, vw: &View, r0: f32, mid: f32) -> ([f32; 2], f32) {
    let s = r0 as f64 * vw.k;
    ([(mid as f64 + (v.0 - vw.cx) * s) as f32, (mid as f64 + (v.1 - vw.cy) * s) as f32], (v.2 / vw.zmax) as f32)
}

fn plane_normal(f: &[V]) -> (f32, f32) {
    if f.len() < 3 {
        return (0.0, 0.0);
    }
    let mut n = (f[1] - f[0]).cross(f[2] - f[0]);
    if n.2 < 0.0 {
        n = -n;
    }
    let nz = n.2.max(1e-4 * n.mag()).max(1e-12);
    ((n.0 / nz).clamp(-1.5, 1.5) as f32, (n.1 / nz).clamp(-1.5, 1.5) as f32)
}

/// Draw one leaf or blossom polygon. `rng` is the tone generator, used once per polygon in drawing order.
fn draw_poly(r: &mut Raster, poly: &Poly, vw: &View, r0: f32, mid: f32, tone_seed: u32, rng: &mut Rand) {
    {
        if poly.verts.len() < 3 {
            return;
        }
        let (nx, ny) = plane_normal(&poly.verts);
        let c = poly.verts[0];
        let (px, _) = project(c, vw, r0, mid);
        let lf = crate::noise::fbm(px[0] / SS as f32, px[1] / SS as f32, 0.55 * r0 / SS as f32, 3, 0.5, tone_seed);
        let jitter = (rng.random() as f32 - 0.5) * 0.16;
        let tone = if let Some(t) = poly.tone { t } else if poly.blossom { (0.86 + 0.12 * rng.random() as f32).min(0.98) } else { (0.5 + (lf - 0.5) * 1.1 + jitter).clamp(0.06, 0.94) };
        let pr: Vec<([f32; 2], f32)> = poly.verts.iter().map(|&v| project(v, vw, r0, mid)).collect();
        for i in 1..pr.len() - 1 {
            r.tri([pr[0].0, pr[i].0, pr[i + 1].0], [pr[0].1, pr[i].1, pr[i + 1].1], nx, ny, tone, 1);
        }
    }
}

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

/// Build one sprite at crown radius `r0` pixels. `fill` is the canopy strength: below zero the preset's own,
/// 0 the raw generator.
pub fn build_sprite(preset_name: &str, variant: u32, fill: f32, r0: f32) -> Option<Sprite> {
    if crate::mushrooms::is_mushroom(preset_name) {
        return crate::mushrooms::sprite(preset_name, variant, r0 > R0 * 1.5);
    }
    build_sprite_with(preset_name, variant, &canopy_at(preset_name, fill), r0)
}

/// A tree's geometry once it is grown and damaged, which does not depend on the sprite size.
struct Built {
    geo: TreeGeo,
    /// The fan palm is made of ready polygons; every other tree makes its polygons from its leaves as it is drawn.
    fan_polys: Vec<Poly>,
    vw: View,
    tones: Vec<f32>,
    charred: f32,
    amp: f32,
    seed: u64,
    dead: bool,
    fan: bool,
}

/// A cache that keeps at most `limit` bytes, dropping the least recently used entries first, so a long
/// session of zooming and switching themes does not hold on to every sprite it ever made.
struct ByteCache<K, V> {
    map: HashMap<K, (V, usize, u64)>,
    bytes: usize,
    tick: u64,
    limit: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> ByteCache<K, V> {
    fn new(limit: usize) -> Self {
        ByteCache { map: HashMap::new(), bytes: 0, tick: 0, limit }
    }

    fn get(&mut self, k: &K) -> Option<V> {
        self.tick += 1;
        let t = self.tick;
        self.map.get_mut(k).map(|e| {
            e.2 = t;
            e.0.clone()
        })
    }

    fn insert(&mut self, k: K, v: V, bytes: usize) {
        self.tick += 1;
        if let Some(old) = self.map.insert(k.clone(), (v, bytes, self.tick)) {
            self.bytes -= old.1;
        }
        self.bytes += bytes;
        while self.bytes > self.limit && self.map.len() > 1 {
            let Some(oldest) = self.map.iter().filter(|(key, _)| **key != k).min_by_key(|(_, e)| e.2).map(|(key, _)| key.clone()) else { break };
            if let Some(e) = self.map.remove(&oldest) {
                self.bytes -= e.1;
            }
        }
    }
}

fn sprite_bytes(s: &Sprite) -> usize {
    s.size * s.size * 21
}

type BuiltKey = (String, u32, String);
static BUILT: OnceLock<Mutex<ByteCache<BuiltKey, Option<Arc<Built>>>>> = OnceLock::new();

/// The geometry of a dead tree, kept so each new sprite size only has to draw it again.
fn built_geometry(preset_name: &str, variant: u32, canopy: &Canopy) -> Option<Arc<Built>> {
    let key = (preset_name.to_string(), variant, format!("{canopy:?}"));
    let cache = BUILT.get_or_init(|| Mutex::new(ByteCache::new(256 << 20)));
    if let Some(b) = cache.lock().unwrap().get(&key) {
        return b;
    }
    let b = make_geometry(preset_name, variant, canopy).map(Arc::new);
    // leaves are 72 bytes each, branch points about 80
    let bytes = b.as_ref().map_or(0, |b| b.geo.leaves.len() * 72 + b.geo.splines.iter().map(|s| s.1.len() * 80).sum::<usize>() + b.fan_polys.len() * 120);
    cache.lock().unwrap().insert(key, b.clone(), bytes);
    b
}

/// Conifers and other dense needled crowns draw their branches as the darkest foliage, so no bare wood shows
/// between the needle masses.
fn hides_wood(name: &str) -> bool {
    matches!(name, "balsam_fir" | "douglas_fir" | "small_pine" | "european_larch") || crate::extra::find(name).is_some_and(|e| e.hide_wood)
}

fn make_geometry(preset_name: &str, variant: u32, canopy: &Canopy) -> Option<Built> {
    let dead = crate::dead::style(preset_name);
    let extra = crate::extra::find(preset_name);
    let preset = preset_by_name(dead.as_ref().map_or(preset_name, |d| d.base))?;
    let name_hash = preset_name.bytes().fold(7u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64));
    let seed = name_hash.wrapping_mul(31).wrapping_add(variant as u64 * 7919 + 11);
    // logs and stumps make their own branches
    let direct = extra.and_then(|e| crate::extra::direct(e, seed));
    let mut geo = match &direct {
        Some(d) => TreeGeo { preset: d.geo.preset, canopy: d.geo.canopy, splines: d.geo.splines.clone(), leaves: Vec::new(), tree_scale: 1.0 },
        None => generate(preset, seed, canopy),
    };
    // The fan palm is built directly as overlapping fans, not by the branching generator.
    let fan = preset_name == "fan_palm";
    // the view is fitted to the living crown, so a damaged tree keeps the size it had
    let mut vw = match &direct {
        Some(d) => View { cx: d.view.0, cy: d.view.1, k: 1.0 / d.view.2, zmax: d.view.3 },
        None if fan => View { cx: 0.0, cy: 0.0, k: 1.0, zmax: 1.2 },
        None => view_for(&geo),
    };
    let crown_scale = extra.map_or(1.0, |e| e.crown_scale);
    vw.k /= crown_scale;
    // a tone for each branch, and how much it varies along the branch, for the types that set their own
    let (tones, charred, amp): (Vec<f32>, f32, f32) = match (&dead, &direct, extra.and_then(|e| e.roots.as_ref())) {
        (Some(d), _, _) => (crate::dead::apply(&mut geo, d, seed, vw.cx, vw.cy, 1.0 / vw.k / crown_scale, vw.zmax), d.char, 0.16),
        (_, Some(d), _) => (d.tones.clone(), 0.0, d.noise),
        (_, _, Some(roots)) => (crate::extra::add_roots(&mut geo, roots, seed, vw.cx, vw.cy, 1.0 / vw.k / crown_scale, vw.zmax), 0.0, 0.1),
        _ => (Vec::new(), 0.0, 0.0),
    };
    let fan_polys = if fan { fan_palm_polys(seed) } else { Vec::new() };

    Some(Built { geo, fan_polys, vw, tones, charred, amp, seed, dead: dead.is_some(), fan })
}

/// Build one sprite with an explicit canopy.
pub fn build_sprite_with(preset_name: &str, variant: u32, canopy: &Canopy, r0: f32) -> Option<Sprite> {
    let built = if crate::dead::is_dead(preset_name) { built_geometry(preset_name, variant, canopy)? } else { Arc::new(make_geometry(preset_name, variant, canopy)?) };
    let Built { geo, fan_polys, vw, tones, charred, amp, seed, dead, fan } = &*built;
    let (charred, amp, seed, dead, fan) = (*charred, *amp, *seed, *dead, *fan);

    let sz = (2.0 * r0 * 1.45) as usize + 4;
    let sss = sz * SS;
    let mid = sss as f32 / 2.0;
    let r0s = r0 * SS as f32;
    let mut r = Raster::new(sss);

    // branches first, then leaves over them (the z buffer sorts the rest out)
    let kpx = r0s as f64 * vw.k;
    let min_r = if dead { 0.8 } else { 0.55 } * SS as f32;
    let hidden = hides_wood(preset_name) && !dead;
    for (si, (level, pts)) in geo.splines.iter().enumerate().filter(|_| !fan) {
        let n = if *level <= 1 { 3 } else { 1 };
        for (w_index, w) in pts.windows(2).enumerate() {
            let (a, b) = (&w[0], &w[1]);
            let mut prev = a.co;
            let mut prev_r = a.radius;
            for j in 1..=n {
                let u = j as f64 / n as f64;
                let m = 1.0 - u;
                let q = a.co * (m * m * m) + a.hr * (3.0 * m * m * u) + b.hl * (3.0 * m * u * u) + b.co * (u * u * u);
                let rr = a.radius + (b.radius - a.radius) * u;
                let (pa, za) = project(prev, &vw, r0s, mid);
                let (pb, zb) = project(q, &vw, r0s, mid);
                let (ra, rb) = (((prev_r * kpx) as f32).max(min_r), ((rr * kpx) as f32).max(min_r));
                let tone = match tones.get(si) {
                    Some(t) if !t.is_nan() => crate::dead::segment_tone(*t, charred, amp, si, w_index * 8 + j),
                    _ => 0.4,
                };
                // a stem seen end on, such as a broken trunk, shows a flat top rather than a rounded one
                let flat = dead && ((pb[0] - pa[0]).powi(2) + (pb[1] - pa[1]).powi(2)).sqrt() < 0.5 * ra.max(rb);
                r.capsule(pa, pb, za, zb, ra, rb, (ra.max(rb) / r0s) * 0.5, if hidden { 0.08 } else { tone }, if flat { 0.25 } else { 0.8 }, if hidden { 1 } else { 2 });
                prev = q;
                prev_r = rr;
            }
        }
    }
    let tone_seed = (seed & 0xFFFF) as u32;
    let mut tone_rng = Rand::new(tone_seed as u64 + 5);
    if fan {
        for poly in fan_polys {
            draw_poly(&mut r, poly, vw, r0s, mid, tone_seed, &mut tone_rng);
        }
    } else {
        // the polygons are made one by one from the leaves and drawn at once, so millions of them never sit in memory
        geo.for_each_polygon(&mut Rand::new(seed ^ 0xABCDEF), |poly| draw_poly(&mut r, &poly, vw, r0s, mid, tone_seed, &mut tone_rng));
    }

    // downsample
    let n = sz * sz;
    let mut sp = Sprite { size: sz, z: vec![0.0; n], nx: vec![0.0; n], ny: vec![0.0; n], tone: vec![0.0; n], depth: vec![0.0; n], cls: vec![0; n], zmax: 0.0 };
    for y in 0..sz {
        for x in 0..sz {
            let (mut cnt, mut bark) = (0usize, 0usize);
            let (mut z, mut nx, mut ny, mut t) = (0f32, 0f32, 0f32, 0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let i = (y * SS + sy) * sss + x * SS + sx;
                    if r.cls[i] != 0 {
                        cnt += 1;
                        bark += (r.cls[i] == 2) as usize;
                        z += r.z[i];
                        nx += r.nx[i];
                        ny += r.ny[i];
                        t += r.tone[i];
                    }
                }
            }
            if cnt * 2 >= SS * SS {
                let c = cnt as f32;
                let i = y * sz + x;
                sp.z[i] = (z / c).max(0.01);
                sp.nx[i] = nx / c;
                sp.ny[i] = ny / c;
                sp.tone[i] = t / c;
                sp.cls[i] = if bark * 2 > cnt { 2 } else { 1 };
                sp.zmax = sp.zmax.max(sp.z[i]);
            }
        }
    }
    let zm = sp.zmax.max(1e-3);
    let mask: Vec<f32> = sp.cls.iter().map(|c| (*c != 0) as u8 as f32).collect();
    let zmasked: Vec<f32> = sp.z.iter().zip(&mask).map(|(z, m)| z * m).collect();
    let rad = ((0.3 * r0) as usize).max(2);
    let (bz, bm) = (box_blur(&zmasked, sz, rad), box_blur(&mask, sz, rad));
    // Crown scale form: the tilt of the smoothed height, so each clump and crown is lit on the side toward
    // the sun and dark on the far side. Heights are 0..1 of the tree, so the slope is scaled back to pixels.
    {
        let rr = ((0.2 * r0) as usize).max(2);
        let kd = 1.3 * r0;
        let (b, m) = (box_blur(&zmasked, sz, rr), box_blur(&mask, sz, rr));
        let zs: Vec<f32> = (0..n).map(|i| b[i] / m[i].max(1e-3)).collect();
        let at = |x: i64, y: i64| zs[(y.clamp(0, sz as i64 - 1) as usize) * sz + x.clamp(0, sz as i64 - 1) as usize];
        for y in 0..sz {
            for x in 0..sz {
                let i = y * sz + x;
                if sp.cls[i] == 1 {
                    let (gx, gy) = ((at(x as i64 + 2, y as i64) - at(x as i64 - 2, y as i64)) / 4.0, (at(x as i64, y as i64 + 2) - at(x as i64, y as i64 - 2)) / 4.0);
                    sp.nx[i] = (1.0 - CROWN_NORMALS) * sp.nx[i] + CROWN_NORMALS * (-gx * kd).clamp(-1.5, 1.5);
                    sp.ny[i] = (1.0 - CROWN_NORMALS) * sp.ny[i] + CROWN_NORMALS * (-gy * kd).clamp(-1.5, 1.5);
                }
            }
        }
    }
    for i in 0..n {
        if sp.cls[i] != 0 {
            let local = ((sp.z[i] - bz[i] / bm[i].max(1e-3)) / (0.2 * zm)).clamp(-1.0, 1.0);
            sp.depth[i] = (0.45 * (sp.z[i] / zm) + 0.55 * (0.5 + 0.5 * local)).clamp(0.0, 1.0);
        }
    }
    Some(sp)
}

// ------------------------------------------------------------------------------------------ cache

type RefKey = (String, u32, u32, u32);
static REFS: OnceLock<Mutex<ByteCache<RefKey, Option<Arc<Sprite>>>>> = OnceLock::new();
type SizedKey = (String, u32, u32, usize);
static SIZED: OnceLock<Mutex<ByteCache<SizedKey, Arc<Sprite>>>> = OnceLock::new();

pub(crate) fn reference(name: &str, variant: u32, fill: f32) -> Option<Arc<Sprite>> {
    reference_at(name, variant, fill, R0)
}

/// The sprite built at crown radius `r0`, kept for reuse.
fn reference_at(name: &str, variant: u32, fill: f32, r0: f32) -> Option<Arc<Sprite>> {
    let variant = variant % VARIANTS;
    let key = (name.to_string(), variant, fill.to_bits(), r0.to_bits());
    let cache = REFS.get_or_init(|| Mutex::new(ByteCache::new(512 << 20)));
    if let Some(s) = cache.lock().unwrap().get(&key) {
        return s;
    }
    let disk = if crate::mushrooms::is_mushroom(name) { None } else { disk_path(name, variant, fill, r0) };
    let sp = match disk.as_deref().and_then(disk_load) {
        Some(s) => Some(Arc::new(s)),
        None => {
            let built = build_sprite(name, variant, fill, r0).map(Arc::new);
            if let (Some(p), Some(s)) = (&disk, &built) {
                disk_store(p, s);
            }
            built
        }
    };
    let bytes = sp.as_ref().map_or(0, |s| sprite_bytes(s));
    cache.lock().unwrap().insert(key, sp.clone(), bytes);
    sp
}

/// Make every variant of a preset ahead of time, in parallel, so stamping never waits on one.
pub fn prewarm(name: &str, fill: f32) {
    (0..VARIANTS).into_par_iter().for_each(|v| {
        let _ = reference(name, v, fill);
    });
}

/// A sprite of this preset for a crown radius in render pixels. `fill` is the canopy strength, below zero
/// the preset's own.
pub fn sprite_for_radius(name: &str, variant: u32, r_px: f32, fill: f32) -> Option<Arc<Sprite>> {
    let size_t = (2.0 * r_px * 1.45) as usize + 4;
    let key = (name.to_string(), variant % VARIANTS, fill.to_bits(), size_t);
    let cache = SIZED.get_or_init(|| Mutex::new(ByteCache::new(384 << 20)));
    if let Some(s) = cache.lock().unwrap().get(&key) {
        return Some(s);
    }
    // Crowns bigger than the base sprite are built at a bigger size, not stretched, so their edges stay sharp.
    // Thin wood does not survive being shrunk from a big sprite, so dead trees are built at their own size.
    let direct = crate::dead::is_dead(name);
    let r0 = if direct { r_px.max(3.0) } else if r_px > R0 * 1.15 { R0 * 2.0 } else { R0 };
    let src = reference_at(name, variant, fill, r0)?;
    let out = if direct { src } else { Arc::new(resample(&src, r0, r_px)) };
    cache.lock().unwrap().insert(key, out.clone(), sprite_bytes(&out));
    Some(out)
}

/// Resample a sprite made for crown radius `r0` to crown radius `r_px`.
pub fn resample(src: &Sprite, r0: f32, r_px: f32) -> Sprite {
    let size_t = (2.0 * r_px * 1.45) as usize + 4;
    let f = r_px / r0;
    let n = size_t * size_t;
    let mut sp = Sprite { size: size_t, z: vec![0.0; n], nx: vec![0.0; n], ny: vec![0.0; n], tone: vec![0.0; n], depth: vec![0.0; n], cls: vec![0; n], zmax: 0.0 };
    let half = (0.5 / f).max(0.5);
    let ss = src.size;
    for ty in 0..size_t {
        for tx in 0..size_t {
            let cx = (tx as f32 + 0.5 - size_t as f32 / 2.0) / f + ss as f32 / 2.0;
            let cy = (ty as f32 + 0.5 - size_t as f32 / 2.0) / f + ss as f32 / 2.0;
            let (x0, x1) = ((cx - half).floor().max(0.0) as usize, ((cx + half).ceil() as usize).min(ss));
            let (y0, y1) = ((cy - half).floor().max(0.0) as usize, ((cy + half).ceil() as usize).min(ss));
            let (mut cnt, mut tot, mut bark) = (0u32, 0u32, 0u32);
            let (mut z, mut nx, mut ny, mut tn, mut dp) = (0f32, 0f32, 0f32, 0f32, 0f32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    tot += 1;
                    let i = sy * ss + sx;
                    if src.cls[i] != 0 {
                        cnt += 1;
                        bark += (src.cls[i] == 2) as u32;
                        z += src.z[i];
                        nx += src.nx[i];
                        ny += src.ny[i];
                        tn += src.tone[i];
                        dp += src.depth[i];
                    }
                }
            }
            if tot > 0 && cnt * 2 >= tot {
                let c = cnt as f32;
                let i = ty * size_t + tx;
                sp.z[i] = (z / c).max(0.01);
                sp.nx[i] = nx / c;
                sp.ny[i] = ny / c;
                sp.tone[i] = tn / c;
                sp.depth[i] = dp / c;
                sp.cls[i] = if bark * 2 > cnt { 2 } else { 1 };
                sp.zmax = sp.zmax.max(sp.z[i]);
            }
        }
    }
    sp
}

// ------------------------------------------------------------------------------------------ disk cache

/// Folder for baked sprites. `TREE_INATOR_CACHE` overrides it; `off` disables the disk cache.
fn disk_dir() -> Option<std::path::PathBuf> {
    static DIR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        // tuning tree types from a file changes sprites without changing the build, so nothing is kept
        if std::env::var_os("TREE_INATOR_EXTRAS").is_some() {
            return None;
        }
        let base = match std::env::var("TREE_INATOR_CACHE") {
            Ok(v) if v == "off" => return None,
            Ok(v) => std::path::PathBuf::from(v),
            Err(_) => std::path::PathBuf::from(std::env::var("LOCALAPPDATA").ok().or_else(|| std::env::var("XDG_CACHE_HOME").ok())?).join("Tree-inator"),
        };
        let dir = base.join("sprites");
        std::fs::create_dir_all(&dir).ok()?;
        // files from older generator versions can never match again, so clear them out
        let keep = format!("_{}.spr", env!("SPRITE_GEN_HASH"));
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if !e.file_name().to_string_lossy().ends_with(&keep) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        Some(dir)
    })
    .clone()
}

fn disk_path(name: &str, variant: u32, fill: f32, r0: f32) -> Option<std::path::PathBuf> {
    let gen = env!("SPRITE_GEN_HASH");
    Some(disk_dir()?.join(format!("{name}_v{variant}_f{:08x}_r{:08x}_{gen}.spr", fill.to_bits(), r0.to_bits())))
}

fn disk_load(path: &std::path::Path) -> Option<Sprite> {
    let raw = miniz_oxide::inflate::decompress_to_vec(&std::fs::read(path).ok()?).ok()?;
    let size = u32::from_le_bytes(raw.get(0..4)?.try_into().ok()?) as usize;
    let zmax = f32::from_le_bytes(raw.get(4..8)?.try_into().ok()?);
    let n = size * size;
    if raw.len() != 8 + n * 21 {
        return None;
    }
    let floats = |k: usize| -> Vec<f32> { raw[8 + k * n * 4..8 + (k + 1) * n * 4].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect() };
    Some(Sprite { size, z: floats(0), nx: floats(1), ny: floats(2), tone: floats(3), depth: floats(4), cls: raw[8 + 5 * n * 4..].to_vec(), zmax })
}

fn disk_store(path: &std::path::Path, sp: &Sprite) {
    let n = sp.size * sp.size;
    let mut raw = Vec::with_capacity(8 + n * 21);
    raw.extend_from_slice(&(sp.size as u32).to_le_bytes());
    raw.extend_from_slice(&sp.zmax.to_le_bytes());
    for a in [&sp.z, &sp.nx, &sp.ny, &sp.tone, &sp.depth] {
        for v in a.iter() {
            raw.extend_from_slice(&v.to_le_bytes());
        }
    }
    raw.extend_from_slice(&sp.cls);
    // Write to a temp name first so a crash never leaves a half file behind.
    let tmp = path.with_extension(format!("{}-{:?}.tmp", std::process::id(), std::thread::current().id()));
    if std::fs::write(&tmp, miniz_oxide::deflate::compress_to_vec(&raw, 1)).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Make any base sprites missing from the disk cache, one at a time on a background thread, so the first
/// render of a theme does not wait on them. Sprites already on disk are left alone.
pub fn prebake_in_background() {
    if disk_dir().is_none() {
        return;
    }
    std::thread::spawn(|| {
        for (name, _) in TREE_TYPES {
            let name = *name;
            // dead trees are built at each size when needed, so there is no base sprite to keep
            if crate::mushrooms::is_mushroom(name) || crate::dead::is_dead(name) || preset_by_name(name).is_none() {
                continue;
            }
            for v in 0..VARIANTS {
                let Some(path) = disk_path(name, v, -1.0, R0) else { return };
                if !path.exists() {
                    if let Some(sp) = build_sprite(name, v, -1.0, R0) {
                        disk_store(&path, &sp);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod cache_tests {
    use super::ByteCache;

    #[test]
    fn the_cache_drops_the_least_recently_used_and_stays_under_its_limit() {
        let mut c: ByteCache<u32, u32> = ByteCache::new(100);
        for k in 0..5 {
            c.insert(k, k, 40);
        }
        assert!(c.bytes <= 100);
        assert_eq!(c.get(&4), Some(4), "the newest stays");
        assert_eq!(c.get(&0), None, "the oldest is gone");
        // reading an entry keeps it: 3 is read, so 4 goes when 10 comes in
        let _ = c.get(&3);
        c.insert(10, 10, 40);
        assert_eq!(c.get(&3), Some(3));
        assert_eq!(c.get(&4), None);
        // one entry bigger than the whole limit still stays, alone
        c.insert(99, 99, 500);
        assert_eq!(c.get(&99), Some(99));
        assert!(c.map.len() >= 1);
    }
}
