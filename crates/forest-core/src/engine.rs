//! Layered forest generation: depth field, per-layer coverage, placement, crowns, light and shadow.

use crate::field::Field;
use crate::noise::{fbm, fbm_warped, hash01, smoothstep as ss, value};
use crate::params::*;
use crate::rng::Rng;
use crate::wp::{sprite_for, SpriteSpec};
use rayon::prelude::*;

#[path = "shy.rs"]
mod shy;
pub(crate) use shy::Shy;
pub(crate) fn shy_pack(f: &Factors) -> [u32; 6] {
    shy::pack(f)
}
use shy::{factor_at, hidden, Factors};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Haze,
    GroundShadow,
    Grass,
    Shrub,
    Canopy,
    Lone,
    Pebble,
}

impl LayerKind {
    /// Bottom to top.
    pub const ALL: [LayerKind; 7] = [
        LayerKind::Haze,
        LayerKind::GroundShadow,
        LayerKind::Grass,
        LayerKind::Shrub,
        LayerKind::Canopy,
        LayerKind::Lone,
        LayerKind::Pebble,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LayerKind::Haze => "Haze",
            LayerKind::GroundShadow => "Ground shadow",
            LayerKind::Grass => "Grass",
            LayerKind::Shrub => "Shrubs",
            LayerKind::Canopy => "Canopy",
            LayerKind::Lone => "Lone trees",
            LayerKind::Pebble => "Pebbles",
        }
    }

    pub fn index(self) -> usize {
        LayerKind::ALL.iter().position(|k| *k == self).unwrap()
    }
}

/// One finished layer, kept small for big maps: 16 bit numbers in 64 x 64 tiles, where a tile with nothing
/// showing takes no memory. Read it through `get`.
pub struct LayerBuf {
    pub kind: LayerKind,
    w: usize,
    tw: usize,
    /// Where each tile's pixels start in the arrays below, or `EMPTY`.
    tiles: Vec<u32>,
    /// Colour in steps of 1/16384, so values up to 4 fit. Empty when the whole layer is `flat`.
    rgb: Vec<[u16; 3]>,
    flat: [f32; 3],
    alpha: Vec<u16>,
    /// Shadow falling onto this layer, 0..1. Empty when there is none.
    shadow: Vec<u16>,
}

/// One pixel of a layer.
pub struct LayerPx {
    pub rgb: [f32; 3],
    pub alpha: f32,
    pub shadow: f32,
}

const TILE: usize = 64;
const EMPTY: u32 = u32::MAX;
const RGB_STEP: f32 = 16384.0;
const UNIT_STEP: f32 = 65535.0;

type PackedTile = (Vec<[u16; 3]>, Vec<u16>, Vec<u16>);

impl LayerBuf {
    fn new(kind: LayerKind, w: usize, h: usize, rgb: Vec<[f32; 3]>, alpha: Vec<f32>, shadow: Vec<f32>) -> LayerBuf {
        Self::build(kind, w, h, rgb, None, alpha, shadow)
    }

    /// A layer of one colour.
    fn flat(kind: LayerKind, w: usize, h: usize, color: [f32; 3], alpha: Vec<f32>) -> LayerBuf {
        Self::build(kind, w, h, Vec::new(), Some(color), alpha, Vec::new())
    }

    fn build(kind: LayerKind, w: usize, h: usize, rgb: Vec<[f32; 3]>, flat: Option<[f32; 3]>, alpha: Vec<f32>, shadow: Vec<f32>) -> LayerBuf {
        let (tw, th) = (w.div_ceil(TILE), h.div_ceil(TILE));
        let unit = |v: f32| (v.clamp(0.0, 1.0) * UNIT_STEP + 0.5) as u16;
        let col = |c: [f32; 3]| c.map(|v| (v.clamp(0.0, 3.99) * RGB_STEP + 0.5) as u16);
        let packed: Vec<Option<PackedTile>> = if alpha.is_empty() {
            Vec::new()
        } else {
            (0..tw * th)
                .into_par_iter()
                .map(|t| {
                    let (x0, y0) = ((t % tw) * TILE, (t / tw) * TILE);
                    let (x1, y1) = ((x0 + TILE).min(w), (y0 + TILE).min(h));
                    if !(y0..y1).any(|y| alpha[y * w + x0..y * w + x1].iter().any(|a| unit(*a) > 0)) {
                        return None;
                    }
                    let n = TILE * TILE;
                    let mut c = if rgb.is_empty() { Vec::new() } else { vec![[0u16; 3]; n] };
                    let mut a = vec![0u16; n];
                    let mut sh = if shadow.is_empty() { Vec::new() } else { vec![0u16; n] };
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let (i, j) = (y * w + x, (y - y0) * TILE + (x - x0));
                            a[j] = unit(alpha[i]);
                            if !c.is_empty() {
                                c[j] = col(rgb[i]);
                            }
                            if !sh.is_empty() {
                                sh[j] = unit(shadow[i]);
                            }
                        }
                    }
                    Some((c, a, sh))
                })
                .collect()
        };
        let mut buf = LayerBuf { kind, w, tw, tiles: Vec::new(), rgb: Vec::new(), flat: flat.unwrap_or([0.0; 3]), alpha: Vec::new(), shadow: Vec::new() };
        if packed.is_empty() {
            return buf;
        }
        buf.tiles = vec![EMPTY; tw * th];
        let mut next = 0u32;
        for (t, p) in packed.into_iter().enumerate() {
            if let Some((c, a, sh)) = p {
                buf.tiles[t] = next;
                next += (TILE * TILE) as u32;
                buf.rgb.extend(c);
                buf.alpha.extend(a);
                buf.shadow.extend(sh);
            }
        }
        buf
    }

    /// The pixel at `x`, `y`, or None where nothing shows.
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Option<LayerPx> {
        let at = *self.tiles.get((y / TILE) * self.tw + x / TILE)?;
        if at == EMPTY {
            return None;
        }
        let j = at as usize + (y % TILE) * TILE + x % TILE;
        let alpha = self.alpha[j] as f32 / UNIT_STEP;
        if alpha <= 0.0 {
            return None;
        }
        let rgb = match self.rgb.get(j) {
            Some(c) => c.map(|v| v as f32 / RGB_STEP),
            None => self.flat,
        };
        let shadow = self.shadow.get(j).map_or(0.0, |s| *s as f32 / UNIT_STEP);
        Some(LayerPx { rgb, alpha, shadow })
    }
    pub fn has_alpha(&self) -> bool {
        !self.tiles.is_empty()
    }
    pub fn has_shadow(&self) -> bool {
        !self.shadow.is_empty()
    }
    /// Every alpha value, 0..1, row by row, for a layer `h` pixels tall.
    pub fn alphas(&self, h: usize) -> impl Iterator<Item = f32> + '_ {
        (0..h).flat_map(move |y| (0..self.w).map(move |x| self.get(x, y).map_or(0.0, |p| p.alpha)))
    }
}

pub struct RenderLayers {
    pub w: usize,
    pub h: usize,
    pub scale: f32,
    pub layers: Vec<LayerBuf>,
    pub tree_counts: [usize; 4],
    /// How far, in map pixels, the generation was pulled in to fit the drawn area.
    pub fit_inset: f32,
    /// Every tree that was stamped, in map coordinates.
    pub trees: Vec<TreeRec>,
}

#[derive(Clone, Copy)]
struct Tree {
    x: f32,
    y: f32,
    r: f32,
    h: f32,
    tone: f32,
    species: u8,
    painted: bool,
    seed: u64,
    /// A mushroom grown to the size of a tree.
    giant: bool,
}

impl Tree {
    fn gen(x: f32, y: f32, r: f32, h: f32, tone: f32) -> Tree {
        Tree { x, y, r, h, tone, species: 0, painted: false, seed: 0, giant: false }
    }
}

/// Give generated trees the per-tree seeds they had before records existed,
/// so filtering or mixing never changes the look of the remaining trees.
fn assign_seeds(trees: &mut [Tree], base: u64) {
    for (n, t) in trees.iter_mut().enumerate() {
        t.seed = base ^ (n as u64).wrapping_mul(0x2545_F491_4F6C_DD1D);
    }
}

/// Every tree of one layer drawn into map-sized buffers. Only height is kept at full precision (shadows
/// and shading compare heights); the rest is packed small, since a big map holds four of these at once.
pub(crate) struct Stamp {
    pub(crate) height: Vec<f32>,
    /// Tree tone in steps of 1/8192.
    pub(crate) tone: Vec<i16>,
    pub(crate) species: Vec<u8>,
    pub(crate) painted: Vec<u8>,
    /// Per pixel surface tilt (steps of 1/80), tone and depth (steps of 1/255) from the tree sprite, and its
    /// class (0 = empty).
    pub(crate) spr_nx: Vec<i8>,
    pub(crate) spr_ny: Vec<i8>,
    pub(crate) spr_tone: Vec<u8>,
    pub(crate) spr_depth: Vec<u8>,
    pub(crate) spr_cls: Vec<u8>,
}

const TILT_STEP: f32 = 80.0;
const TONE_STEP: f32 = 8192.0;

impl Stamp {
    fn new(w: usize, h: usize) -> Stamp {
        let n = w * h;
        Stamp {
            height: vec![0.0; n],
            tone: vec![0; n],
            species: vec![0; n],
            painted: vec![0; n],
            spr_nx: vec![0; n],
            spr_ny: vec![0; n],
            spr_tone: vec![0; n],
            spr_depth: vec![0; n],
            spr_cls: vec![0; n],
        }
    }
    #[inline]
    fn tone(&self, i: usize) -> f32 {
        self.tone[i] as f32 / TONE_STEP
    }
    #[inline]
    fn tilt(&self, i: usize) -> (f32, f32) {
        (self.spr_nx[i] as f32 / TILT_STEP, self.spr_ny[i] as f32 / TILT_STEP)
    }
    #[inline]
    fn spr_tone(&self, i: usize) -> f32 {
        self.spr_tone[i] as f32 / 255.0
    }
    #[inline]
    fn spr_depth(&self, i: usize) -> f32 {
        self.spr_depth[i] as f32 / 255.0
    }
    fn painted_f32(&self) -> Vec<f32> {
        self.painted.par_iter().map(|p| *p as f32).collect()
    }
}

/// One tree ready to draw: its sprite, where the sprite's top left corner lands in render pixels, how it is
/// turned, and how much its height is scaled so its top matches the tree's height.
pub(crate) struct SpriteInstance {
    pub(crate) spr: std::sync::Arc<crate::wp::Sprite>,
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    /// Quarter turns, then a mirror.
    pub(crate) k: u32,
    pub(crate) flip: bool,
    pub(crate) zs: f32,
    pub(crate) tone: i16,
    pub(crate) species: u8,
    pub(crate) painted: bool,
    /// Crown shyness: how far each direction of the crown is pulled in, when the tree has neighbours to avoid.
    pub(crate) fac: Option<Factors>,
}

/// Crown radii are rounded to a bucket, so trees of about the same size share a sprite.
fn size_bucket(r_px: f32) -> f32 {
    (if r_px < 8.0 { (r_px * 2.0).round() / 2.0 } else { r_px.round() }).max(1.0)
}

/// The sprite and placement for a tree, or None when it is under about a pixel across.
fn sprite_instance(t: &Tree, sp: &Species, s: f32) -> Option<SpriteInstance> {
    let r_px = t.r * s;
    if r_px < 1.0 {
        return None;
    }
    let bucket = size_bucket(r_px);
    let spec = SpriteSpec {
        form: sp.form,
        branchiness: sp.branchiness,
        clumpiness: sp.clumpiness,
        leaf_cover: sp.leaf_cover,
        // Leaves never outgrow the crown, so tiny trees still read as a small bush.
        leaf_px: (sp.leaf_size * s).min(r_px * 0.5),
        leaf_elong: sp.leaf_shape,
        along: sp.needles,
        bare: sp.bare,
    };
    let preset = sp.tree_type(false).and_then(|ty| crate::treegen_sprite::sprite_for_radius(ty, (t.seed % crate::treegen_sprite::VARIANTS as u64) as u32, bucket, sp.canopy_fill));
    let spr = match preset {
        Some(b) => b,
        None => sprite_for(&spec, bucket, (t.seed % 10) as u32),
    };
    let orient = ((t.seed >> 8) % 8) as u32;
    let n = spr.size as i32;
    let zs = t.h * s * sp.height / spr.zmax.max(1e-3);
    Some(SpriteInstance {
        x0: (t.x * s - n as f32 / 2.0).round() as i32,
        y0: (t.y * s - n as f32 / 2.0).round() as i32,
        k: orient % 4,
        flip: orient >= 4,
        zs,
        tone: (t.tone * TONE_STEP).round().clamp(-32767.0, 32767.0) as i16,
        species: t.species,
        painted: t.painted,
        fac: None,
        spr,
    })
}

/// Stamp one generated sprite tree.
fn stamp_sprite(st: &mut Stamp, t: &Tree, sp: &Species, region: Region, s: f32, fac: Option<&Factors>) {
    let r_px = t.r * s;
    // Skip trees that cannot reach this region, before fetching a sprite.
    let reach = 1.6 * r_px + 6.0;
    let (cx, cy) = (t.x * s - region.x0 as f32, t.y * s - region.y0 as f32);
    if cx + reach < 0.0 || cy + reach < 0.0 || cx - reach > region.w as f32 || cy - reach > region.h as f32 {
        return;
    }
    let Some(inst) = sprite_instance(t, sp, s) else { return };
    let (w, h) = (region.w, region.h);
    let spr = &inst.spr;
    let (k, flip, zs) = (inst.k, inst.flip, inst.zs);
    let n = spr.size as i32;
    let x0 = inst.x0 - region.x0 as i32;
    let y0 = inst.y0 - region.y0 as i32;
    for dy in 0..n {
        let py = y0 + dy;
        if py < 0 || py >= h as i32 {
            continue;
        }
        for dx in 0..n {
            let px = x0 + dx;
            if px < 0 || px >= w as i32 {
                continue;
            }
            // A shy crown is pulled in toward its centre, so look further out in the sprite.
            let (dx, dy) = match fac {
                Some(f) => {
                    let (ox, oy) = (dx as f32 + 0.5 - n as f32 / 2.0, dy as f32 + 0.5 - n as f32 / 2.0);
                    let k = factor_at(f, ox, oy);
                    let (sx, sy) = ((n as f32 / 2.0 + ox / k).floor() as i32, (n as f32 / 2.0 + oy / k).floor() as i32);
                    if sx < 0 || sy < 0 || sx >= n || sy >= n {
                        continue;
                    }
                    (sx, sy)
                }
                None => (dx, dy),
            };
            // Undo the flip, then the quarter turns, to find the source pixel.
            let (mut ux, mut uy) = if flip { (n - 1 - dx, dy) } else { (dx, dy) };
            for _ in 0..k {
                let (a, b) = (uy, n - 1 - ux);
                ux = a;
                uy = b;
            }
            let si = (uy * n + ux) as usize;
            if spr.cls[si] == 0 {
                continue;
            }
            let i = py as usize * w + px as usize;
            let z = spr.z[si] * zs;
            if z > st.height[i] {
                st.height[i] = z;
                st.tone[i] = inst.tone;
                st.species[i] = inst.species;
                st.painted[i] = inst.painted as u8;
                let (mut nx, mut ny) = (spr.nx[si], spr.ny[si]);
                for _ in 0..k {
                    let (a, b) = (-ny, nx);
                    nx = a;
                    ny = b;
                }
                if flip {
                    nx = -nx;
                }
                st.spr_nx[i] = (nx * TILT_STEP).round().clamp(-127.0, 127.0) as i8;
                st.spr_ny[i] = (ny * TILT_STEP).round().clamp(-127.0, 127.0) as i8;
                st.spr_tone[i] = (spr.tone[si].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                st.spr_depth[i] = (spr.depth[si].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                st.spr_cls[i] = spr.cls[si];
            }
        }
    }
}

/// Colour along a four-stop ramp, t from 0 (shade) to 1 (light).
fn ramp_color(ramp: &[[f32; 3]; 4], t: f32) -> [f32; 3] {
    let tt = t.clamp(0.0, 1.0) * 3.0;
    let k = (tt.floor() as usize).min(2);
    let f = tt - k as f32;
    [
        ramp[k][0] * (1.0 - f) + ramp[k + 1][0] * f,
        ramp[k][1] * (1.0 - f) + ramp[k + 1][1] * f,
        ramp[k][2] * (1.0 - f) + ramp[k + 1][2] * f,
    ]
}

/// Even-odd scanline fill of a polygon given in render pixels.
pub(crate) fn fill_polygon(mask: &mut Field, pts: &[[f32; 2]], value: f32) {
    if pts.len() < 3 {
        return;
    }
    let ymin = pts.iter().map(|p| p[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let ymax = (pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max).ceil() as usize).min(mask.h);
    let mut xs = Vec::new();
    for y in ymin..ymax {
        let yc = y as f32 + 0.5;
        xs.clear();
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            if (a[1] <= yc) != (b[1] <= yc) {
                xs.push(a[0] + (yc - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let x0 = (pair[0] - 0.5).ceil().max(0.0) as usize;
            let x1 = ((pair[1] - 0.5).floor() as isize).min(mask.w as isize - 1);
            if x1 < 0 {
                continue;
            }
            for x in x0..=(x1 as usize) {
                mask.data[y * mask.w + x] = value;
            }
        }
    }
}

/// Dart throwing with minimum spacing, in map coordinates.
fn scatter(
    rng: &mut Rng,
    map_w: f32,
    map_h: f32,
    darts: usize,
    rmin: f32,
    rmax: f32,
    mind: f32,
    mut accept: impl FnMut(f32, f32, &mut Rng) -> f32,
) -> Vec<(f32, f32, f32)> {
    let cell = (mind * 2.0 * rmax).max(1.0);
    let gw = (map_w / cell).ceil() as usize + 1;
    let gh = (map_h / cell).ceil() as usize + 1;
    let mut grid: Vec<Vec<u32>> = vec![Vec::new(); gw * gh];
    let mut pts: Vec<(f32, f32, f32)> = Vec::new();
    for _ in 0..darts {
        let x = rng.f32() * map_w;
        let y = rng.f32() * map_h;
        let p = accept(x, y, rng);
        if p <= 0.0 || rng.f32() > p {
            continue;
        }
        let r = rng.range(rmin, rmax);
        let (gx, gy) = ((x / cell) as usize, (y / cell) as usize);
        let mut ok = true;
        'outer: for yy in gy.saturating_sub(1)..(gy + 2).min(gh) {
            for xx in gx.saturating_sub(1)..(gx + 2).min(gw) {
                for &i in &grid[yy * gw + xx] {
                    let (px, py, pr) = pts[i as usize];
                    let d = mind * (r + pr);
                    if (x - px) * (x - px) + (y - py) * (y - py) < d * d {
                        ok = false;
                        break 'outer;
                    }
                }
            }
        }
        if ok {
            grid[gy * gw + gx].push(pts.len() as u32);
            pts.push((x, y, r));
        }
    }
    pts
}

/// Stamp every tree of a layer as a generated sprite (render pixels).
fn stamp_trees(trees: &[Tree], species: &[Species], region: Region, s: f32, depth: &DepthStyle, fac: Option<&[Factors]>) -> Stamp {
    let mut st = Stamp::new(region.w, region.h);
    let fallback = Species::default();
    for (n, t) in trees.iter().enumerate() {
        let sp = species.get(t.species as usize).unwrap_or(&fallback);
        let mut tv = *t;
        tv.h *= height_factor(t, depth);
        let f = fac.map(|f| &f[n]);
        if f.map_or(false, hidden) {
            continue;
        }
        stamp_sprite(&mut st, &tv, sp, region, s, f.filter(|f| f.iter().any(|v| *v < 255)));
    }
    st
}

/// fBm rescaled to roughly zero mean, unit spread.
#[inline]
fn unit(v: f32) -> f32 {
    (v - 0.5) / 0.12
}

/// Reshape the outline with a macro pattern. `dist` is signed distance in map pixels (positive inside).
fn apply_pattern(g: &GenParams, nx: f32, ny: f32, dist: f32, sd: u32) -> f32 {
    let k = g.pattern_strength.clamp(0.0, 1.0);
    if k <= 0.0 {
        return dist;
    }
    let sc = g.pattern_scale.max(20.0);
    let target = match g.pattern {
        Pattern::Plain => return dist,
        Pattern::Fragmented => {
            // Noise pushes the outline in and out, strongest near the edge, so the core stays solid
            // and the margin breaks into islands of decreasing size.
            let n = unit(fbm_warped(nx, ny, sc, 6, sd + 50, sc * 0.5, sc));
            let fine = unit(fbm(nx, ny, sc * 0.3, 4, 0.55, sd + 51));
            let band = 1.0 - ss(0.4 * sc, 1.6 * sc, dist);
            let push = (n * 0.8 + fine * 0.35) * sc * 0.7;
            // Outward growth is capped, otherwise the outline traces offset copies of the drawn edges.
            let push = if push > 0.0 { push * 0.3 } else { push };
            // Detached islands from their own noise, fading with distance outside.
            let isl = unit(fbm(nx, ny, sc * 0.35, 4, 0.5, sd + 56));
            let islands = (isl - 1.3) * sc * 0.25 - (-dist).max(0.0) * 0.6;
            (dist + push * band).max(islands)
        }
        Pattern::Branching => {
            // Ridged noise gives connected line networks; forest grows along them. The noise is normalised, so
            // branch width is the share of the area that is forest and strength widens it toward solid.
            let z = unit(fbm_warped(nx, ny, sc, 5, sd + 52, sc * 0.6, sc * 0.8)).abs() + 0.2 * (fbm(nx, ny, sc * 0.2, 3, 0.5, sd + 53) - 0.5);
            let wdt = g.branch_width.clamp(0.05, 1.0);
            let wdt = wdt + (1.0 - wdt) * (1.0 - k) * 0.8;
            let a = 0.85 * wdt;
            // Deepest along the ridge centre, dropping away quickly past its edge.
            let along = if z < a { (1.0 - z / a) * 90.0 } else { (a - z) * sc * 0.5 };
            return dist.min(along.max(dist - sc * 1.5));
        }
        Pattern::Clumps => {
            let n = unit(fbm_warped(nx, ny, sc * 0.6, 5, sd + 54, sc * 0.3, sc * 0.6)) + 0.2;
            return dist.min(n * sc * 0.4 + (1.0 - k) * sc);
        }
        Pattern::Lobed => {
            // Large smooth bays and peninsulas.
            let n = unit(fbm_warped(nx, ny, sc * 1.5, 3, sd + 55, sc, sc * 1.5));
            let push = n * sc * 0.9;
            let push = if push > 0.0 { push * 0.3 } else { push };
            dist + push * (1.0 - ss(0.6 * sc, 2.4 * sc, dist))
        }
    };
    dist + (target - dist) * k
}

/// Treefall gaps: one random ellipse per grid cell, kept with probability from `gaps`.
fn in_gap(g: &GenParams, nx: f32, ny: f32, sd: u32) -> bool {
    if g.gaps <= 0.0 {
        return false;
    }
    let cell = g.gap_size.max(5.0) * 6.0 / g.gaps.sqrt().max(0.2);
    let (cx, cy) = ((nx / cell).floor() as i32, (ny / cell).floor() as i32);
    for oy in -1..=1 {
        for ox in -1..=1 {
            let (ix, iy) = (cx + ox, cy + oy);
            if hash01(ix, iy, sd + 60) > g.gaps.min(1.0) * 0.8 {
                continue;
            }
            let px = (ix as f32 + hash01(ix, iy, sd + 61)) * cell;
            let py = (iy as f32 + hash01(ix, iy, sd + 62)) * cell;
            let r = g.gap_size * (0.5 + hash01(ix, iy, sd + 63));
            let a = hash01(ix, iy, sd + 64) * std::f32::consts::PI;
            let (dx, dy) = (nx - px, ny - py);
            let (u, v) = (dx * a.cos() + dy * a.sin(), -dx * a.sin() + dy * a.cos());
            // Ragged outline so gaps are not perfect ellipses.
            let wob = 1.0 + 0.35 * (fbm(nx, ny, g.gap_size * 0.4, 2, 0.5, sd + 65) - 0.5);
            if (u / (r * 1.6)).powi(2) + (v / r).powi(2) < wob {
                return true;
            }
        }
    }
    false
}

struct SpritePixel<'a> {
    x: usize,
    y: usize,
    nx: f32,
    ny: f32,
    depth: f32,
    tone: f32,
    tree_tone: f32,
    cls: u8,
    hb: f32,
    b3: f32,
    b9: f32,
    bx: f32,
    by: f32,
    rel: f32,
    ramp: &'a [[f32; 3]; 4],
    bark: [f32; 3],
    recol: [f32; 3],
    /// Colours baked into the sprite (mushrooms): plain light, none of the leaf styling.
    plain: bool,
}

/// Compress only the brightest light, so lit patches stay green instead of washing to yellow.
#[inline]
fn soft_knee(v: f32) -> f32 {
    if v > 1.0 {
        1.0 + (v - 1.0) * 0.5
    } else {
        v
    }
}

/// Zero-mean broad tone noise, about one unit of spread.
fn tone_noise(x: f32, y: f32, size: f32, sd: u32) -> f32 {
    let size = size.max(4.0);
    let a = value(x / (size * 2.0), y / (size * 2.0), sd + 301) - 0.5;
    let b = value(x / size, y / size, sd + 302) - 0.5;
    let c = value(x / (size * 0.5), y / (size * 0.5), sd + 303) - 0.5;
    (0.55 * a + 0.35 * b + 0.2 * c) / 0.12
}

/// Large-scale brightness multiplier and the tone value that nudges hue. `rel` is height against the surroundings.
fn depth_mod(ds: &DepthStyle, rel: f32, nx: f32, ny: f32, sd: u32) -> (f32, f32) {
    let occl = (1.0 + ds.crown_shade * 0.6 * rel.tanh()).clamp(0.3, 1.6);
    // Soft limit, so the brightest patches stay believable instead of turning neon.
    let raw = tone_noise(nx, ny, ds.crown_tone_size, sd);
    let n = 1.5 * (raw / 1.5).tanh() * ds.crown_tone;
    (occl * (0.5 * n).exp(), n)
}

/// Colour of one generated-sprite pixel, lit from the sprite's own surface direction.
fn sprite_color(p: SpritePixel, ls: &LeafStyle, ds: &DepthStyle, lv: [f32; 3], flat: bool, s: f32, inv: f32, sd: u32) -> [f32; 3] {
    let n = (p.nx * p.nx + p.ny * p.ny + 1.0).sqrt();
    let diff = if flat { 0.63 } else { ((p.nx * lv[0] + p.ny * lv[1] + lv[2]) / n).clamp(0.0, 1.0) };
    let ao = (1.0 - (p.b3 - p.hb).max(0.0) / (6.0 * s)).clamp(0.0, 1.0) * (1.0 - (p.b9 - p.hb).max(0.0) / (18.0 * s)).clamp(0.0, 1.0);
    if p.plain {
        // the sprite already holds the colour as a ramp position, so only light it
        let shade = (0.42 + 0.58 * diff) * (0.7 + 0.3 * ao);
        let c = if p.cls == 2 {
            let k = shade * (0.75 + 0.5 * p.tone);
            [p.bark[0] * k, p.bark[1] * k, p.bark[2] * k]
        } else {
            // the tone is frost dusting the cap, in the bark colour
            let c = ramp_color(p.ramp, p.depth.clamp(0.0, 1.0));
            let f = (p.tone * 0.9).clamp(0.0, 0.9);
            let c = [0, 1, 2].map(|j| c[j] * (1.0 - f) + p.bark[j] * f);
            [c[0] * shade * p.recol[0], c[1] * shade * p.recol[1], c[2] * shade * p.recol[2]]
        };
        return c.map(|v| v.clamp(0.0, 1.0));
    }
    if p.cls == 2 {
        let k = (0.4 + 0.9 * diff) * (0.5 + 0.5 * ao) * (0.55 + 1.1 * p.tone);
        return [p.bark[0] * k, p.bark[1] * k, p.bark[2] * k];
    }
    let depth = p.depth.clamp(0.0, 1.0);
    let dk = (0.4 * ds.depth_strength).clamp(0.0, 0.95);
    let (nx, ny) = ((p.x as f32 + 0.5) * inv, (p.y as f32 + 0.5) * inv);
    let (dm, tn) = depth_mod(ds, p.rel, nx, ny, sd);
    let shade = ((0.4 + 0.9 * diff) * (0.5 + 0.5 * ao) * ((1.0 - dk) + dk * depth.powf(0.8)) * dm).max(0.0).powf(ds.gamma) * ds.gain;
    // Each crown carries its own brightness, so lit and dark clusters follow crown outlines.
    let shade = soft_knee(shade * (3.0 * ds.crown_var * p.tree_tone).exp());
    let mut t = 0.2 + 0.5 * depth + (p.tone - 0.5) * 0.4 * ds.cluster_tone * (ls.variety / 1.5).clamp(0.0, 2.0) + 0.3 * (diff - 0.5) + p.tree_tone * (1.0 + 2.5 * ds.crown_var);
    if ls.mottling > 0.0 {
        let ms = ls.mottle_size.max(1.0);
        t += (value(nx / ms, ny / ms, sd + 91) - 0.5) * 0.45 * ls.mottling * 0.5;
    }
    let tt = t.clamp(0.0, 1.0) * 3.0;
    let k = (tt.floor() as usize).min(2);
    let f = tt - k as f32;
    let mut c = [0f32; 3];
    for j in 0..3 {
        c[j] = (p.ramp[k][j] * (1.0 - f) + p.ramp[k + 1][j] * f) * shade * p.recol[j];
    }
    if ls.dappled > 0.0 {
        let fc = ((-p.bx - p.by) * 0.35 / s).clamp(0.0, 1.0) * ls.dappled;
        c[0] *= 1.0 + fc * 0.28;
        c[1] *= 1.0 + fc * 0.2;
        c[2] *= 1.0 - fc * 0.05;
    }
    if ls.tree_lean > 0.0 {
        let hv = (hash01((p.tree_tone * 1e5) as i32, 7, sd + 7) - 0.5) * 2.4 * 0.14 * ls.tree_lean;
        c[0] *= 1.0 + hv * 0.8;
        c[1] *= 1.0 + hv * 0.3;
        c[2] *= 1.0 - hv;
    }
    // Brighter patches lean lime, darker ones lean blue-green.
    c[0] *= 1.0 + 0.04 * tn;
    c[2] *= 1.0 - 0.06 * tn;
    // Past full brightness the colour is scaled down, not clipped per channel, so bright leaves stay
    // their own colour instead of washing out to white.
    let m = c[0].max(c[1]).max(c[2]);
    let k = if m > 1.0 { 1.0 / m } else { 1.0 };
    c.map(|v| (v * k).clamp(0.0, 1.0))
}

/// Generate all layers at `scale` times map resolution. Returns None if cancelled.
pub fn generate(project: &Project, scale: f32, cancelled: &(dyn Fn() -> bool + Sync)) -> Option<RenderLayers> {
    let plan = plan(project, scale, cancelled)?;
    let (w, h) = render_size(project, scale);
    let mut stamped = stamp_region(project, &plan, scale, Region { x0: 0, y0: 0, w, h }, cancelled)?;
    shade_region(project, &plan, &mut stamped, cancelled, true, false)
}

/// Everything decided for the whole map before any pixel is drawn: the outline, the pull-in and every tree.
/// Worked out once, then drawn in one go or region by region.
pub struct Plan {
    /// Plan pixels per generated-map pixel.
    pub(crate) s: f32,
    pub(crate) sdf: Field,
    pub(crate) any_shape: bool,
    /// Pull-in grid: cell size in plan pixels, values in map pixels.
    pub(crate) ins: Vec<f32>,
    pub(crate) cs: usize,
    pub(crate) gw: usize,
    pub(crate) gh: usize,
    /// Ranked haze and grass noise, every `fine` plan pixels.
    pub(crate) u_haze: Field,
    pub(crate) u_grass: Field,
    pub(crate) fine: usize,
    groups: [Vec<Tree>; 4],
    /// Crown shyness factors of the canopy layer for the settings they were made with.
    shy_cache: Mutex<Option<(Shy, f32, Arc<Vec<Factors>>)>>,
    /// Generated trees before erasing and painting, so brush edits only redo the last step.
    base_groups: [Vec<Tree>; 4],
    pub(crate) species: Vec<Species>,
    /// The tallest tree or grass, in map pixels.
    pub(crate) max_height: f32,
    pub tree_counts: [usize; 4],
    pub trees: Vec<TreeRec>,
    pub fit_inset: f32,
}

fn bilerp(f: &Field, fx: f32, fy: f32) -> f32 {
    let fx = fx.clamp(0.0, (f.w - 1) as f32);
    let fy = fy.clamp(0.0, (f.h - 1) as f32);
    let (x0, y0) = (fx as usize, fy as usize);
    let (x1, y1) = ((x0 + 1).min(f.w - 1), (y0 + 1).min(f.h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let a = f.data[y0 * f.w + x0] * (1.0 - tx) + f.data[y0 * f.w + x1] * tx;
    let b = f.data[y1 * f.w + x0] * (1.0 - tx) + f.data[y1 * f.w + x1] * tx;
    a * (1.0 - ty) + b * ty
}

impl Plan {
    /// Redo the erased and painted trees after a brush edit, keeping the outline, noise and generated trees.
    pub fn reapply_edits(&mut self, project: &Project) {
        let f = project.gen.scale.clamp(0.1, 4.0);
        let (groups, counts, trees, max_height) = finish_trees(project, &self.base_groups, f);
        self.groups = groups;
        *self.shy_cache.lock().unwrap() = None;
        self.tree_counts = counts;
        self.trees = trees;
        self.max_height = max_height;
    }

    /// Every tree of one layer (0 shrubs, 1 canopy, 2 lone, 3 pebbles) ready to draw at `s` render pixels
    /// per map pixel, in drawing order.
    /// With `rect` (x0, y0, x1, y1 in render pixels) only the trees that can reach it.
    pub(crate) fn instances(&self, layer: usize, depth: &DepthStyle, s: f32, rect: Option<[f32; 4]>, fac: Option<&[Factors]>) -> Vec<SpriteInstance> {
        let fallback = Species::default();
        let in_rect = |t: &&Tree| match rect {
            Some([x0, y0, x1, y1]) => {
                let reach = 1.6 * t.r * s + 6.0;
                let (cx, cy) = (t.x * s, t.y * s);
                cx + reach >= x0 && cx - reach <= x1 && cy + reach >= y0 && cy - reach <= y1
            }
            None => true,
        };
        // Make the sprites this layer needs, all sizes at once, before they are looked up one by one.
        {
            let mut need: std::collections::HashSet<(String, u32, u32, u32)> = std::collections::HashSet::new();
            for t in self.groups[layer].iter().filter(in_rect) {
                let sp = self.species.get(t.species as usize).unwrap_or(&fallback);
                if let (Some(ty), true) = (sp.tree_type(false), t.r * s >= 1.0) {
                    need.insert((ty.to_string(), (t.seed % crate::treegen_sprite::VARIANTS as u64) as u32, size_bucket(t.r * s).to_bits(), sp.canopy_fill.to_bits()));
                }
            }
            let need: Vec<_> = need.into_iter().collect();
            // A few at a time: building a big sprite takes a lot of memory, so the whole pool at once is too many.
            static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
            let pool = POOL.get_or_init(|| rayon::ThreadPoolBuilder::new().num_threads(3).build().ok());
            let build = || {
                need.par_iter().for_each(|(ty, v, b, fill)| {
                    let _ = crate::treegen_sprite::sprite_for_radius(ty, *v, f32::from_bits(*b), f32::from_bits(*fill));
                })
            };
            match pool {
                Some(p) => p.install(build),
                None => build(),
            }
        }
        self.groups[layer]
            .iter()
            .enumerate()
            .filter(|(_, t)| match rect {
                Some([x0, y0, x1, y1]) => {
                    let reach = 1.6 * t.r * s + 6.0;
                    let (cx, cy) = (t.x * s, t.y * s);
                    cx + reach >= x0 && cx - reach <= x1 && cy + reach >= y0 && cy - reach <= y1
                }
                None => true,
            })
            .filter_map(|(n, t)| {
                if layer == 1 && fac.map_or(false, |f| hidden(&f[n])) {
                    return None;
                }
                let sp = self.species.get(t.species as usize).unwrap_or(&fallback);
                let mut tv = *t;
                tv.h *= height_factor(t, depth);
                sprite_instance(&tv, sp, s).map(|mut q| {
                    // crowns only meet in the canopy layer
                    if layer == 1 {
                        q.fac = fac.map(|f| f[n]).filter(|f| f.iter().any(|v| *v < 255));
                    }
                    q
                })
            })
            .collect()
    }

    /// Crown shyness factors for the canopy layer, or None when shyness is off. `gap_scale` turns the
    /// setting (map pixels) into tree coordinates. Kept until the settings or the trees change.
    pub(crate) fn shy_factors(&self, shy: &Shy, gap_scale: f32) -> Option<Arc<Vec<Factors>>> {
        if !shy.on {
            return None;
        }
        let mut cache = self.shy_cache.lock().unwrap();
        if let Some((k, sc, v)) = cache.as_ref() {
            if k == shy && *sc == gap_scale {
                return Some(v.clone());
            }
        }
        let v = Arc::new(shy::compute(&self.groups[1], &self.species, shy, shy.gap * gap_scale));
        *cache = Some((*shy, gap_scale, v.clone()));
        Some(v)
    }

    /// How far the generation is pulled in from the drawn line, in map pixels, on a grid of `cell` plan pixels:
    /// the values, the grid width and height, and the cell size. For looking at what the fit decided.
    pub fn pull_in(&self) -> (&[f32], usize, usize, usize) {
        (&self.ins[..self.gw * self.gh], self.gw, self.gh, self.cs)
    }

    /// The tallest tree or grass, in map pixels.
    pub fn max_height(&self) -> f32 {
        self.max_height
    }
    /// Signed distance to the drawn line at a map point, in map pixels.
    fn sdf_map(&self, nx: f32, ny: f32) -> f32 {
        if !self.any_shape {
            return -1e6;
        }
        bilerp(&self.sdf, nx * self.s - 0.5, ny * self.s - 0.5)
    }
    /// Pull-in at a map point.
    fn inset_map(&self, nx: f32, ny: f32) -> f32 {
        self.grid_map(0, nx, ny)
    }
    /// How much of the edge wobble applies at a map point (1 in wide parts, less in thin ones).
    fn warp_map(&self, nx: f32, ny: f32) -> f32 {
        self.grid_map(self.gw * self.gh, nx, ny)
    }
    fn grid_map(&self, base: usize, nx: f32, ny: f32) -> f32 {
        let fx = (nx * self.s / self.cs as f32 - 0.5).clamp(0.0, (self.gw - 1) as f32);
        let fy = (ny * self.s / self.cs as f32 - 0.5).clamp(0.0, (self.gh - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(self.gw - 1), (y0 + 1).min(self.gh - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let v = &self.ins[base..];
        let top = v[y0 * self.gw + x0] * (1.0 - tx) + v[y0 * self.gw + x1] * tx;
        let bot = v[y1 * self.gw + x0] * (1.0 - tx) + v[y1 * self.gw + x1] * tx;
        top * (1.0 - ty) + bot * ty
    }
    fn noise_map(&self, f: &Field, nx: f32, ny: f32) -> f32 {
        bilerp(f, (nx * self.s - 0.5) / self.fine as f32, (ny * self.s - 0.5) / self.fine as f32)
    }
}

/// Random crown height, so taller crowns shade the ones beside them.
fn height_factor(t: &Tree, depth: &DepthStyle) -> f32 {
    let u = ((t.seed >> 20) & 0xffff) as f32 / 65536.0;
    (1.0 + depth.height_var * (u - 0.5) * 1.2).max(0.3)
}

/// Depth at a map point with the whole edge moved `inset` map pixels inward: 1 well inside, 0 on the
/// edge, -1 well outside.
fn depth_value(g: &GenParams, sd: u32, nx: f32, ny: f32, inset: f32, wk: f32, sdf_at: &dyn Fn(f32, f32) -> f32) -> f32 {
    let band = g.shape_softness.max(5.0);
    // wk above 1 marks the envelope mode: the outline may wobble and branch, but never leaves the drawn zone
    let env = wk > 1.5;
    let wk = wk.min(1.0);
    let dx = (fbm(nx, ny, 160.0, 3, 0.5, sd + 10) - 0.5) * g.edge_warp * wk;
    let dy = (fbm(nx, ny, 160.0, 3, 0.5, sd + 9) - 0.5) * g.edge_warp * wk;
    let mut dist = sdf_at(nx + dx, ny + dy) - inset;
    if dist < -4.0 * band - g.pattern_scale * 2.0 {
        return -1.0;
    }
    dist += g.edge_noise * band * 2.0 * (fbm(nx, ny, 64.0, 4, 0.5, sd + 11) - 0.5);
    let mut dist = apply_pattern(g, nx, ny, dist, sd);
    if env {
        dist = dist.min(ENVELOPE_SLOPE * (sdf_at(nx, ny) - inset));
    }
    // Depth: 1 at `band` px inside, -1 at 2 x `band` px outside.
    if dist >= 0.0 { (dist / band).min(1.0) } else { (dist / (2.0 * band)).max(-1.0) }
}

/// Render size in pixels of the map at `scale`.
pub fn render_size(project: &Project, scale: f32) -> (usize, usize) {
    (((project.map_w as f32 * scale).round() as usize).max(8), ((project.map_h as f32 * scale).round() as usize).max(8))
}

/// Remove trees from `small` whose centre lies within `reach` of a crown radius of a tree in `big` that is at
/// least `ratio` times their size.
fn drop_covered(small: &mut Vec<Tree>, big: &[Tree], reach: f32, ratio: f32) {
    if big.is_empty() || small.is_empty() {
        return;
    }
    const CELL: f32 = 32.0;
    let key = |x: f32, y: f32| ((x / CELL).floor() as i32, (y / CELL).floor() as i32);
    let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();
    for (i, b) in big.iter().enumerate() {
        grid.entry(key(b.x, b.y)).or_default().push(i);
    }
    let reach_max = big.iter().map(|b| b.r).fold(0.0f32, f32::max) * reach;
    let span = (reach_max / CELL).ceil() as i32 + 1;
    small.retain(|t| {
        let (cx, cy) = key(t.x, t.y);
        for gy in cy - span..=cy + span {
            for gx in cx - span..=cx + span {
                if let Some(v) = grid.get(&(gx, gy)) {
                    for &i in v {
                        let b = &big[i];
                        if b.r >= t.r * ratio && (b.x - t.x).powi(2) + (b.y - t.y).powi(2) < (b.r * reach).powi(2) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    });
}

/// Apply the project's erased areas and painted trees to the generated trees, then work out what the plan
/// reports about them. Cheap next to a full plan, so brush edits redo only this.
/// How steeply the forest outline falls away outside the envelope, in outline distance per map pixel.
const ENVELOPE_SLOPE: f32 = 3.0;

fn finish_trees(project: &Project, base: &[Vec<Tree>; 4], f: f32) -> ([Vec<Tree>; 4], [usize; 4], Vec<TreeRec>, f32) {
    let g = &project.gen;
    let mut groups = base.clone();
    if !project.erased.is_empty() {
        for v in groups.iter_mut() {
            v.retain(|t| !project.erased.iter().any(|e| (t.x * f - e[0]).powi(2) + (t.y * f - e[1]).powi(2) < e[2] * e[2]));
        }
    }
    if g.painted_trees {
        for p in &project.painted {
            groups[p.layer.index()].push(Tree {
                x: p.x / f,
                y: p.y / f,
                r: p.r / f,
                h: p.h / f,
                tone: p.tone,
                species: p.species,
                painted: true,
                giant: false,
                seed: (p.seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xB5,
            });
        }
    }
    let tree_counts = [groups[1].len(), groups[0].len(), groups[2].len(), groups[3].len()];
    let trees: Vec<TreeRec> = groups
        .iter()
        .zip(TreeLayer::ALL)
        .flat_map(|(v, layer)| {
            v.iter().map(move |t| TreeRec { x: t.x * f, y: t.y * f, r: t.r * f, h: t.h * f, tone: t.tone, layer, species: t.species, seed: t.seed as u32 })
        })
        .collect();
    // Preset trees take a moment to make the first time, so make them all up front.
    {
        let mut used = vec![false; project.species.len()];
        for grp in &groups {
            for t in grp {
                if let Some(u) = used.get_mut(t.species as usize) {
                    *u = true;
                }
            }
        }
        for (i, u) in used.iter().enumerate() {
            let sp = &project.species[i];
            if let (true, Some(ty)) = (*u, sp.tree_type(g.procedural_forms)) {
                crate::treegen_sprite::prewarm(ty, sp.canopy_fill);
            }
        }
    }
    // The tallest anything stands, in map pixels, which sets how far shadows can reach.
    let mut max_height = 0.4f32;
    for t in groups.iter().flatten() {
        let sp_h = project.species.get(t.species as usize).map_or(1.0, |sp| sp.height);
        max_height = max_height.max(t.h * height_factor(t, &g.depth) * sp_h);
    }
    (groups, tree_counts, trees, max_height)
}

/// Work out the outline, pull-in and every tree, on a grid of `scale` times map resolution.
pub fn plan(project: &Project, scale: f32, cancelled: &(dyn Fn() -> bool + Sync)) -> Option<Plan> {
    let g = &project.gen;
    // Size scale: the forest is generated on a map `f` times smaller and drawn `f` times larger, so every
    // crown, leaf and shadow grows together. Inside, `s` is render pixels per generated-map pixel.
    let f = g.scale.clamp(0.1, 4.0);
    let s = scale * f;
    let inv = 1.0 / s;
    let w = ((project.map_w as f32 * scale).round() as usize).max(8);
    let h = ((project.map_h as f32 * scale).round() as usize).max(8);
    let (map_w, map_h) = (project.map_w as f32 / f, project.map_h as f32 / f);
    let sd = (g.seed as u32).wrapping_mul(7919);
    let realistic = g.edge_mode == EdgeMode::Realistic;
    let nat = |x: usize, y: usize| ((x as f32 + 0.5) * inv, (y as f32 + 0.5) * inv);
    // FOREST_TIMING=1 prints how long each stage takes.
    let timing = std::env::var_os("FOREST_TIMING").is_some();
    let clock = std::time::Instant::now();
    let mark = |label: &str| {
        if timing {
            eprintln!("  {:>7.2}s  {label}", clock.elapsed().as_secs_f32());
        }
    };

    // 1. Shape mask and depth field D (0 at the drawn edge, 1 deep inside, negative outside).
    let mut mask = Field::new(w, h);
    for shape in project.shapes.iter().filter(|s| s.visible) {
        let pts: Vec<[f32; 2]> = shape.points.iter().map(|p| [p[0] * scale, p[1] * scale]).collect();
        fill_polygon(&mut mask, &pts, if shape.op == ShapeOp::Add { 1.0 } else { 0.0 });
    }
    // Signed distance in map pixels, so patterns can reshape the outline at any scale.
    let any_shape = mask.data.iter().any(|v| *v > 0.5);
    let contain = g.contain && any_shape;
    let clip_on = contain && g.contain_clip;
    let sdf = if any_shape {
        let mut f = crate::field::signed_distance(&mask);
        f.data.par_iter_mut().for_each(|v| *v *= inv);
        f
    } else {
        Field { w, h, data: vec![-1e6; w * h] }
    };
    // Depth at a pixel with the whole edge moved `inset` map pixels inward.
    let sdf_at = |x: f32, y: f32| sdf.at((x * s) as i32, (y * s) as i32);
    let depth_at = |x: usize, y: usize, inset: f32, wk: f32| -> f32 {
        let (nx, ny) = nat(x, y);
        depth_value(g, sd, nx, ny, inset, wk, &sdf_at)
    };

    // Fit: the outline is clamped inside the drawn zone by a steep ramp (see `depth_value`), so everything
    // ends before the line. The grid holds the ramp's offset and the envelope flag.
    let cs = ((5.0 * s) as usize).max(1);
    let (gw, gh) = ((w + cs - 1) / cs, (h + cs - 1) / cs);
    let mut ins = vec![0f32; gw * gh];
    // 2 where the envelope applies, otherwise 1
    let mut wk = vec![1f32; gw * gh];
    if contain && g.contain_fit > 0.0 {
        // One envelope for everything: the forest outline is clamped inside the zone shrunk by the farthest
        // outward reach of any ground feature, so nothing needs a per-spot estimate.
        let band = g.shape_softness.max(5.0);
        let mut r = 0.05f32;
        if realistic {
            if g.haze_opacity > 0.005 {
                r = r.max(g.haze_reach);
            }
            if g.grass_density > 0.0 {
                r = r.max(g.grass_reach);
            }
            if g.pebbles > 0.0 {
                r = r.max(0.7);
            }
        } else {
            r = r.max(0.5 * g.feather.max(0.02) + 0.15);
        }
        let m = g.contain_fit * 2.0 * band * r / ENVELOPE_SLOPE;
        ins.iter_mut().for_each(|v| *v = m);
        wk.iter_mut().for_each(|v| *v = 2.0);
    }
    let fit_max = ins.iter().cloned().fold(0.0f32, f32::max);
    // Pull-in at a render pixel, smoothly interpolated between cells.
    let grid_at = |ins: &[f32], x: usize, y: usize| -> f32 {
        let fx = ((x as f32 + 0.5) / cs as f32 - 0.5).clamp(0.0, (gw - 1) as f32);
        let fy = ((y as f32 + 0.5) / cs as f32 - 0.5).clamp(0.0, (gh - 1) as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(gw - 1), (y0 + 1).min(gh - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let top = ins[y0 * gw + x0] * (1.0 - tx) + ins[y0 * gw + x1] * tx;
        let bot = ins[y1 * gw + x0] * (1.0 - tx) + ins[y1 * gw + x1] * tx;
        top * (1.0 - ty) + bot * ty
    };
    // Smooth at pixel scale, so worked out every other pixel and filled in.
    let fine = 2;
    let depth = Field::coarse(w, h, fine, |x, y| depth_at(x, y, grid_at(&ins, x, y), grid_at(&wk, x, y))).upsample(w, h, fine);
    if cancelled() {
        return None;
    }

    mark("1 shape and depth");
    // 2. Per-layer noise, ranked so thresholds are exact coverage fractions.
    let u_can = Field::coarse(w, h, fine, |x, y| {
        let (nx, ny) = nat(x, y);
        fbm_warped(nx, ny, g.canopy_patch_size, 5, sd + 21, 70.0, 128.0) + 0.25 * fbm(nx, ny, 32.0, 3, 0.5, sd + 26)
    })
    .rank()
    .upsample(w, h, fine);
    let u_grass_c = Field::coarse(w, h, fine, |x, y| {
        let (nx, ny) = nat(x, y);
        fbm(nx, ny, 40.0, 4, 0.5, sd + 25) + 0.4 * (fbm(nx, ny, 8.0, 3, 0.5, sd + 20) - 0.5)
    })
    .rank();
    let u_grass = u_grass_c.upsample(w, h, fine);
    // Haze is drawn later, region by region, so only its coarse noise is kept.
    let u_haze_c = Field::coarse(w, h, fine, |x, y| {
        let (nx, ny) = nat(x, y);
        fbm(nx, ny, 64.0, 4, 0.5, sd + 23)
    })
    .rank();
    if cancelled() {
        return None;
    }

    let feather = g.feather.max(0.02);
    let cover_can = |d: f32| {
        if realistic {
            g.canopy_cover * ss(-0.05, g.canopy_edge_depth.max(0.0), d)
        } else {
            0.98 * ss(-feather * 0.5 - 0.15, -feather * 0.5, d)
        }
    };
    let can_mask = Field::from_fn(w, h, |x, y| {
        let i = y * w + x;
        let (nx, ny) = nat(x, y);
        (u_can.data[i] < cover_can(depth.data[i]) && !in_gap(g, nx, ny, sd)) as u8 as f32
    });
    let can_fit = can_mask.blur(8.0 * s);

    mark("2 layer noise");
    // 3. Haze and grass.
    // Grass at plan resolution, for where shrubs may grow.
    let grass_imp = Field::from_fn(w, h, |x, y| {
        if !realistic {
            return 0.0;
        }
        let i = y * w + x;
        let cover = ss(-g.grass_reach, 0.0, depth.data[i]);
        if cover <= 0.0 {
            return 0.0;
        }
        let (nx, ny) = nat(x, y);
        let soft = ss(-0.02, 0.18, cover - u_grass.data[i]);
        let dens = soft * (0.5 + fbm(nx, ny, 16.0, 3, 0.5, sd + 28)) * g.grass_density;
        (hash01(x as i32, y as i32, sd + 29) < dens * 0.35) as u8 as f32
    });
    let grass_cov = grass_imp.blur(1.2 * s);
    let grass_a: Vec<f32> = grass_cov.data.par_iter().map(|c| 1.0 - (-c * 6.0 * 1.6).exp()).collect();
    if cancelled() {
        return None;
    }

    mark("3 haze and grass");
    // 4. Placement in map coordinates, so previews and full renders agree.
    let at = |f: &Field, x: f32, y: f32| f.at((x * s) as i32, (y * s) as i32);
    let at_v = |v: &Vec<f32>, x: f32, y: f32| {
        let xi = ((x * s) as usize).min(w - 1);
        let yi = ((y * s) as usize).min(h - 1);
        v[yi * w + xi]
    };
    let area = map_w * map_h;
    let mut rng = Rng::new(g.seed);
    let tone = |rng: &mut Rng| rng.normal(0.0, 0.1);

    let crown_min = g.crown_min.max(1.0);
    let crown_max = g.crown_max.max(crown_min + 0.1);
    let mut canopy: Vec<Tree> = scatter(&mut rng, map_w, map_h, (0.078 * area) as usize, crown_min, crown_max, 0.55, |x, y, _| {
        if at(&can_fit, x, y) > 0.8 {
            1.0
        } else if at(&can_mask, x, y) > 0.5 {
            0.15
        } else {
            0.0
        }
    })
    .into_iter()
    .map(|(x, y, r)| Tree::gen(x, y, r, r * 1.3 + 18.0, tone(&mut rng)))
    .collect();

    let mut shrubs: Vec<Tree> = if realistic {
        scatter(&mut rng, map_w, map_h, (0.017 * area) as usize, 4.0, 9.0, 1.0, |x, y, _| {
            if at_v(&grass_a, x, y) > 0.5 && at(&can_mask, x, y) < 0.5 {
                (0.6 * g.shrub_density).min(1.0)
            } else {
                0.0
            }
        })
        .into_iter()
        .map(|(x, y, r)| Tree::gen(x, y, r, r * 0.8 + 4.0, tone(&mut rng)))
        .collect()
    } else {
        Vec::new()
    };

    // Lone trees: Matern cluster process in the fringe band.
    let mut lone = Vec::new();
    if realistic {
        let parents = (0.0035 * area * g.lone_groups) as usize;
        for _ in 0..parents {
            let x = rng.f32() * map_w;
            let y = rng.f32() * map_h;
            let d = at(&depth, x, y);
            let fringe = d > -g.lone_reach && d < 0.3;
            let inside = d >= 0.3 && rng.f32() < g.interior_lone;
            if (fringe || inside) && at(&can_mask, x, y) < 0.5 && rng.f32() < 0.10 {
                let n = 1 + rng.below(3);
                for _ in 0..n {
                    let r = if rng.f32() < 0.5 { rng.range(8.0, 15.0) } else { rng.range(3.0, 6.0) };
                    let (cx, cy) = (x + rng.normal(0.0, 14.0), y + rng.normal(0.0, 14.0));
                    lone.push(Tree::gen(cx, cy, r, r * 1.4 + 12.0, tone(&mut rng)));
                }
            }
        }
    }

    let mut pebbles: Vec<Tree> = if realistic {
        scatter(&mut rng, map_w, map_h, (0.003 * area * g.pebbles) as usize, 2.0, 4.5, 1.2, |x, y, _| {
            let d = at(&depth, x, y);
            if d > -0.7 && d < 0.15 && at(&can_mask, x, y) < 0.5 {
                0.35 * ss(-0.75, -0.1, d)
            } else {
                0.0
            }
        })
        .into_iter()
        .map(|(x, y, r)| Tree::gen(x, y, r, r + 3.0, tone(&mut rng)))
        .collect()
    } else {
        Vec::new()
    };
    if cancelled() {
        return None;
    }

    // Mushrooms: clusters on open ground near the edge and in clearings, small caps of mixed ages.
    let mut mushrooms: Vec<Tree> = Vec::new();
    let mix_total: f32 = g.mushroom_mix.iter().sum();
    let mush_species: Vec<Option<u8>> = crate::mushrooms::KINDS
        .iter()
        .map(|(name, _)| project.species.iter().position(|s| s.preset == *name).map(|i| i as u8))
        .collect();
    if realistic && g.mushrooms > 0.0 && mix_total > 0.0 {
        let kinds = &mush_species;
        let parents = scatter(&mut rng, map_w, map_h, (0.0006 * area * g.mushrooms) as usize, 14.0, 14.5, 1.0, |x, y, _| {
            let d = at(&depth, x, y);
            if d > -0.6 && d < 0.9 && at(&can_mask, x, y) < 0.5 {
                // more where the ground is open, none under a crown
                0.35 + 0.65 * ss(-0.5, 0.2, d)
            } else {
                0.0
            }
        });
        for (px, py, _) in parents {
            // pick a kind by the mix
            let mut pick = rng.f32() * mix_total;
            let mut kind = 0;
            for (k, w) in g.mushroom_mix.iter().enumerate() {
                if pick < *w {
                    kind = k;
                    break;
                }
                pick -= *w;
            }
            let Some(sp) = kinds[kind] else { continue };
            let n = 1 + rng.below(5);
            let mut placed: Vec<(f32, f32, f32)> = Vec::new();
            for _ in 0..n * 4 {
                if placed.len() >= n {
                    break;
                }
                let r = rng.range(3.2, 7.5);
                let (x, y) = (px + rng.normal(0.0, 8.0), py + rng.normal(0.0, 8.0));
                if placed.iter().all(|q| ((q.0 - x).powi(2) + (q.1 - y).powi(2)).sqrt() > (q.2 + r) * 0.8) {
                    placed.push((x, y, r));
                    let mut t = Tree::gen(x, y, r, r * 0.32 + 0.6, tone(&mut rng));
                    t.species = sp;
                    mushrooms.push(t);
                }
            }
        }
        assign_seeds(&mut mushrooms, g.seed.wrapping_add(5 * 1_000_003));
    }

    mark("4 placement");
    // 5. Seeds, species mix, erasing, painted trees, then crowns per layer in parallel.
    for (v, k) in [(&mut shrubs, 1u64), (&mut canopy, 2), (&mut lone, 3), (&mut pebbles, 4)] {
        assign_seeds(v, g.seed.wrapping_add(k * 1_000_003));
    }
    // Forest type: every generated tree takes the chosen species before any mixing.
    let n_species = project.species.len();
    if (g.main_species as usize) < n_species && g.main_species != 0 {
        for t in canopy.iter_mut().chain(lone.iter_mut()) {
            t.species = g.main_species;
        }
    }
    if (g.shrub_species as usize) < n_species && g.shrub_species != 0 {
        for t in shrubs.iter_mut().chain(pebbles.iter_mut()) {
            t.species = g.shrub_species;
        }
    }
    if g.mix_amount > 0.0 && (g.mix_species as usize) < project.species.len() {
        // Second species in clumps, chosen by noise so it never disturbs the RNG sequence.
        let q = 0.5 + 0.12 * 2.2 * (1.0 - 2.0 * g.mix_amount.min(1.0));
        let sc = g.mix_scale.max(20.0);
        for t in canopy.iter_mut().chain(lone.iter_mut()) {
            if fbm_warped(t.x, t.y, sc, 4, sd + 70, sc * 0.5, sc) > q {
                t.species = g.mix_species;
            }
        }
    }
    // Giants: some of the canopy trees are mushrooms as big as the trees, picked by their own seed.
    if g.giant_mushrooms > 0.0 && mix_total > 0.0 {
        for t in canopy.iter_mut() {
            let u = ((t.seed >> 11) % 10_000) as f32 / 10_000.0;
            if u >= g.giant_mushrooms {
                continue;
            }
            let mut pick = ((t.seed >> 29) % 1_000) as f32 / 1_000.0 * mix_total;
            let mut kind = 0;
            for (k, w) in g.mushroom_mix.iter().enumerate() {
                if pick < *w {
                    kind = k;
                    break;
                }
                pick -= *w;
            }
            if let Some(sp) = mush_species[kind] {
                t.species = sp;
                t.giant = true;
                t.r *= 1.1;
                t.h = t.r * 1.15 + 12.0;
            }
        }
    }
    // A smaller tree never sits inside a bigger crown, where it would show as a dark spot on top of it.
    let big_lone = lone.clone();
    drop_covered(&mut lone, &big_lone, 0.8, 1.3);
    drop_covered(&mut shrubs, &lone, 0.9, 1.0);
    drop_covered(&mut pebbles, &lone, 0.9, 1.0);
    drop_covered(&mut pebbles, &canopy, 0.9, 1.0);
    drop_covered(&mut shrubs, &canopy, 0.9, 1.0);
    drop_covered(&mut mushrooms, &lone, 0.9, 1.0);
    drop_covered(&mut mushrooms, &canopy, 0.9, 1.0);
    // the canopy layer takes no colour cast, so the caps keep their own colours
    canopy.extend(mushrooms);
    let mut groups = [shrubs, canopy, lone, pebbles];
    if !g.procedural_trees {
        groups.iter_mut().for_each(|v| v.clear());
    }
    if clip_on {
        // A tree whose crown would cross the line is dropped, so the forest ends short of it and the clip has
        // nothing to cut. With the fit turned down, crowns may reach out past the line again.
        let k = g.contain_fit.clamp(0.0, 1.0);
        for v in groups.iter_mut() {
            v.retain(|t| sdf.at((t.x * s) as i32, (t.y * s) as i32) > (-0.35 + 1.6 * k) * t.r);
        }
    }
    let base_groups = groups.clone();
    let (groups, tree_counts, trees, max_height) = finish_trees(project, &base_groups, f);
    // With procedural trees on for the forest, every species grows as its tree type.
    let species: Vec<Species> = project
        .species
        .iter()
        .map(|sp| Species { procedural: sp.procedural || g.procedural_forms, ..sp.clone() })
        .collect();
    mark("5 plan");
    Some(Plan {
        s,
        sdf,
        any_shape,
        ins: [ins, wk].concat(),
        cs,
        gw,
        gh,
        u_haze: u_haze_c,
        u_grass: u_grass_c,
        fine,
        groups,
        shy_cache: Mutex::new(None),
        base_groups,
        species,
        max_height,
        tree_counts,
        trees,
        fit_inset: fit_max * f,
    })
}

/// A rectangle of the render, in pixels at the render scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub x0: usize,
    pub y0: usize,
    pub w: usize,
    pub h: usize,
}

/// The trees and ground cover of one region, drawn and ready to shade.
pub struct Stamped {
    pub(crate) region: Region,
    pub(crate) scale: f32,
    pub(crate) stamps: Vec<Stamp>,
    pub(crate) depth: Vec<f32>,
    pub(crate) haze_a: Vec<f32>,
    pub(crate) grass_a: Vec<f32>,
    pub(crate) area_a: Vec<f32>,
}

/// Draw the planned trees, haze and grass cover into one region at `scale`.
pub fn stamp_region(project: &Project, plan: &Plan, scale: f32, region: Region, cancelled: &(dyn Fn() -> bool + Sync)) -> Option<Stamped> {
    let g = &project.gen;
    let f = g.scale.clamp(0.1, 4.0);
    let s = scale * f;
    let inv = 1.0 / s;
    let (w, h, ox, oy) = (region.w, region.h, region.x0, region.y0);
    let sd = (g.seed as u32).wrapping_mul(7919);
    let realistic = g.edge_mode == EdgeMode::Realistic;
    let nat = |x: usize, y: usize| (((x + ox) as f32 + 0.5) * inv, ((y + oy) as f32 + 0.5) * inv);
    let timing = std::env::var_os("FOREST_TIMING").is_some();
    let clock = std::time::Instant::now();
    let mark = |label: &str| {
        if timing {
            eprintln!("  {:>7.2}s  {label}", clock.elapsed().as_secs_f32());
        }
    };

    // Depth from the plan's outline and pull-in; smooth, so worked out every other pixel.
    let fine = plan.fine;
    let sdf_at = |x: f32, y: f32| plan.sdf_map(x, y);
    let depth = Field::coarse(w, h, fine, |x, y| {
        let (nx, ny) = nat(x, y);
        depth_value(g, sd, nx, ny, plan.inset_map(nx, ny), plan.warp_map(nx, ny), &sdf_at)
    })
    .upsample(w, h, fine);
    mark("region depth");
    // How much of each pixel lies inside the drawn area, with a one pixel soft edge.
    let clip_on = g.contain && plan.any_shape && g.contain_clip;
    let area_a: Vec<f32> = if clip_on {
        let mut mask = Field::new(w, h);
        for shape in project.shapes.iter().filter(|s| s.visible) {
            let pts: Vec<[f32; 2]> = shape.points.iter().map(|p| [p[0] * scale - ox as f32, p[1] * scale - oy as f32]).collect();
            fill_polygon(&mut mask, &pts, if shape.op == ShapeOp::Add { 1.0 } else { 0.0 });
        }
        mask.blur(0.8 * s).data.par_iter().map(|v| ss(0.25, 0.75, *v)).collect()
    } else {
        Vec::new()
    };
    let haze_a = Field::from_fn(w, h, |x, y| {
        if !realistic {
            return 0.0;
        }
        let i = y * w + x;
        let cover = ss(-g.haze_reach, -0.1, depth.data[i]);
        if cover <= 0.0 {
            return 0.0;
        }
        let (nx, ny) = nat(x, y);
        ss(0.0, 0.15, cover - plan.noise_map(&plan.u_haze, nx, ny)) * g.haze_opacity * (0.6 + 0.8 * fbm(nx, ny, 2.0, 2, 0.5, sd + 24))
    });
    let grass_imp = Field::from_fn(w, h, |x, y| {
        if !realistic {
            return 0.0;
        }
        let i = y * w + x;
        let cover = ss(-g.grass_reach, 0.0, depth.data[i]);
        if cover <= 0.0 {
            return 0.0;
        }
        let (nx, ny) = nat(x, y);
        let soft = ss(-0.02, 0.18, cover - plan.noise_map(&plan.u_grass, nx, ny));
        let dens = soft * (0.5 + fbm(nx, ny, 16.0, 3, 0.5, sd + 28)) * g.grass_density;
        (hash01((x + ox) as i32, (y + oy) as i32, sd + 29) < dens * 0.35) as u8 as f32
    });
    let grass_a: Vec<f32> = grass_imp.blur(1.2 * s).data.par_iter().map(|c| 1.0 - (-c * 6.0 * 1.6).exp()).collect();
    drop(grass_imp);
    mark("region haze and grass");
    if cancelled() {
        return None;
    }
    let fac = plan.shy_factors(&Shy::of(g), 1.0 / f);
    let stamps: Vec<Stamp> = plan
        .groups
        .par_iter()
        .enumerate()
        // crowns only meet in the canopy layer
        .map(|(l, trees)| stamp_trees(trees, &plan.species, region, s, &g.depth, if l == 1 { fac.as_deref().map(|v| v.as_slice()) } else { None }))
        .collect();
    if cancelled() {
        return None;
    }
    mark("region stamps");
    Some(Stamped { region, scale, stamps, depth: depth.data, haze_a: haze_a.data, grass_a, area_a })
}

/// Shadows, colour and the finished layers for a stamped region. `with_trees` copies the planned tree list
/// into the result. Unless `keep` is set, each layer's stamp is freed once it is shaded.
pub fn shade_region(
    project: &Project,
    plan: &Plan,
    stamped: &mut Stamped,
    cancelled: &(dyn Fn() -> bool + Sync),
    with_trees: bool,
    keep: bool,
) -> Option<RenderLayers> {
    let g = &project.gen;
    let light = &project.light;
    let f = g.scale.clamp(0.1, 4.0);
    let scale = stamped.scale;
    let s = scale * f;
    let inv = 1.0 / s;
    let region = stamped.region;
    let (w, h, ox, oy) = (region.w, region.h, region.x0, region.y0);
    let (map_w, map_h) = (project.map_w as f32 / f, project.map_h as f32 / f);
    let sd = (g.seed as u32).wrapping_mul(7919);
    let realistic = g.edge_mode == EdgeMode::Realistic;
    let nat = |x: usize, y: usize| (((x + ox) as f32 + 0.5) * inv, ((y + oy) as f32 + 0.5) * inv);
    let timing = std::env::var_os("FOREST_TIMING").is_some();
    let clock = std::time::Instant::now();
    let mark = |label: &str| {
        if timing {
            eprintln!("  {:>7.2}s  {label}", clock.elapsed().as_secs_f32());
        }
    };
    let feather = g.feather.max(0.02);
    let clip_on = g.contain && plan.any_shape && g.contain_clip;
    let grass_a = &stamped.grass_a;
    let area_a = &stamped.area_a;
    // 6. Heights of every layer and the combined top surface.
    let grass_h: Vec<f32> = grass_a.iter().map(|a| a * 0.4 * s).collect();
    let mut top = grass_h.clone();
    for st in &stamped.stamps {
        top.par_iter_mut().zip(st.height.par_iter()).for_each(|(t, v)| *t = t.max(*v));
    }
    let top = Field { w, h, data: top };
    // From the plan, so every region of one render agrees on how far shadows reach.
    let max_top = plan.max_height * s;

    mark("6 heights");
    // 7. Cascade shadows: march toward the sun over the top surface, against each receiver's height.
    let ang = light.shadow_angle.to_radians();
    let (sx, sy) = (-ang.cos(), -ang.sin());
    let slope = 0.6 / light.shadow_length.max(0.05);
    let steps = ((max_top / slope).ceil() as usize).clamp(1, 600);
    // The highest occluder above every pixel's ray toward the sun, found once and shared by every layer.
    let (occ_h, occ_k) = if light.shadows { crate::field::horizon(&top, sx, sy, slope) } else { (Vec::new(), Vec::new()) };
    let shadow_for = |recv: Option<&Vec<f32>>, present: &(dyn Fn(usize) -> bool + Sync)| -> Vec<f32> {
        if !light.shadows {
            return Vec::new();
        }
        let soft = 3.0 * s;
        let fall = 1.0 / (steps as f32 + 6.0);
        let raw = Field::from_fn(w, h, |x, y| {
            let i = y * w + x;
            if !present(i) {
                return 0.0;
            }
            let base = recv.map_or(0.0, |r| r[i]) + 0.5 * s;
            // Partial occlusion near the grazing ray gives a soft penumbra instead of streaks; far
            // occluders fade.
            ((occ_h[i] - base) / soft).clamp(0.0, 1.0) * (1.0 - occ_k[i] * fall).max(0.0)
        });
        raw.blur(2.5 * s * light.shadow_softness * (0.7 + 0.3 * light.shadow_length)).data
    };

    mark("7 setup");
    // 8. Shading.
    let sun = ang + std::f32::consts::PI;
    let el = light.elevation.clamp(5.0, 90.0).to_radians();
    let lv = [sun.cos() * el.cos(), sun.sin() * el.cos(), el.sin()];
    let ls = &g.leaf;
    let ramps: Vec<[[f32; 3]; 4]> = project.species.iter().map(|sp| sp.ramp).collect();
    let barks: Vec<[f32; 3]> = project.species.iter().map(|sp| sp.bark).collect();
    let plains: Vec<bool> = project.species.iter().map(|sp| sp.plain_colours()).collect();
    // Snow: where it lies (0..1) at a pixel, before crown shape and patchiness are applied.
    let snow = &g.snow;
    let snow_on = snow.amount > 0.0;
    let snow_cell = snow.reach.max(10.0);
    let (sa_c, sa_s) = (snow.angle.to_radians().cos(), snow.angle.to_radians().sin());
    let snow_where = |x: usize, y: usize| -> f32 {
        match snow.mode {
            SnowMode::Everywhere => 1.0,
            SnowMode::Edge => 1.0 - ss(0.0, snow_cell, {
                let (nx, ny) = nat(x, y);
                plan.sdf_map(nx, ny) - plan.inset_map(nx, ny)
            }),
            SnowMode::Gradient => {
                let (nx, ny) = nat(x, y);
                let t = 0.5 + 0.5 * ((nx / map_w * 2.0 - 1.0) * sa_c + (ny / map_h * 2.0 - 1.0) * sa_s) / (sa_c.abs() + sa_s.abs()).max(0.01);
                ss(0.3, 0.75, t)
            }
        }
    };
    // Snow covers a pixel by this much (0..1). `cling` is how well the surface holds snow.
    let snow_k = |x: usize, y: usize, cling: f32| -> f32 {
        let (nx, ny) = nat(x, y);
        let n = fbm(nx, ny, 30.0, 4, 0.55, sd + 70) - 0.5;
        let v = snow_where(x, y) * snow.amount * 1.35 * cling + n * snow.patchiness * 0.9 - 0.12 + snow.amount * 0.12;
        ss(0.3, 0.62, v)
    };
    let snow_color = |c: [f32; 3], k: f32| -> [f32; 3] {
        let lum = (0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]).clamp(0.0, 1.0);
        // Snow keeps some of the shading underneath so crowns stay readable.
        let sh = 0.5 + 0.6 * (lum * 2.4).min(1.0);
        let t = snow.tint;
        [c[0] * (1.0 - k) + t[0] * sh * k, c[1] * (1.0 - k) + t[1] * sh * k, c[2] * (1.0 - k) + t[2] * sh * k]
    };
    // Fire: how burnt a spot is (0 untouched, then scorched, then charred) and how much it is in flames.
    let fire = &g.fire;
    let fire_on = fire.amount > 0.0;
    let (fa_c, fa_s) = (fire.angle.to_radians().cos(), fire.angle.to_radians().sin());
    let fire_at = |x: usize, y: usize| -> (f32, f32) {
        let (nx, ny) = nat(x, y);
        let n = fbm(nx, ny, 55.0, 4, 0.55, sd + 90) - 0.5;
        let base = match fire.mode {
            FireMode::Everywhere => fire.amount,
            FireMode::Edge => (1.0 - ss(0.0, fire.reach.max(10.0), {
                let (nx, ny) = nat(x, y);
                plan.sdf_map(nx, ny) - plan.inset_map(nx, ny)
            })) * fire.amount * 1.2,
            FireMode::Front => {
                let t = 0.5 + 0.5 * ((nx / map_w * 2.0 - 1.0) * fa_c + (ny / map_h * 2.0 - 1.0) * fa_s) / (fa_c.abs() + fa_s.abs()).max(0.01);
                ss(0.25, 0.75, t) * (0.6 + 0.6 * fire.amount)
            }
        };
        let burn = (base * 1.15 + n * fire.patchiness * 2.2 - 0.08).clamp(0.0, 1.0);
        // flames run along the margin between burnt out and not yet burnt
        let band = 0.04 + 0.24 * fire.flames;
        let flame = if fire.flames > 0.0 { 1.0 - ss(0.0, band, (burn - 0.62).abs()) } else { 0.0 };
        (burn, flame)
    };
    let flicker = |x: usize, y: usize| {
        let (nx, ny) = nat(x, y);
        fbm(nx, ny, 6.0, 3, 0.6, sd + 91)
    };
    let flame_color = |q: f32| -> [f32; 3] {
        let (red, orange, yellow) = ([0.72, 0.12, 0.03], [1.0, 0.45, 0.06], [1.0, 0.86, 0.42]);
        let mix = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        if q < 0.5 {
            mix(red, orange, q * 2.0)
        } else {
            mix(orange, yellow, (q - 0.5) * 2.0)
        }
    };
    let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    // Warm light around the flames, worked out on a coarse grid since it is soft anyway.
    let glow_cell = 4usize;
    let glow = (fire_on && fire.glow > 0.0 && fire.flames > 0.0).then(|| {
        let (gw, gh) = (w / glow_cell + 1, h / glow_cell + 1);
        Field::from_fn(gw, gh, |x, y| fire_at((x * glow_cell).min(w - 1), (y * glow_cell).min(h - 1)).1).blur(18.0 * s / glow_cell as f32)
    });
    let glow_at = |x: usize, y: usize| -> f32 {
        let Some(gf) = &glow else { return 0.0 };
        let (fx, fy) = (x as f32 / glow_cell as f32, y as f32 / glow_cell as f32);
        let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let a = gf.at(x0, y0) * (1.0 - tx) + gf.at(x0 + 1, y0) * tx;
        let b = gf.at(x0, y0 + 1) * (1.0 - tx) + gf.at(x0 + 1, y0 + 1) * tx;
        a * (1.0 - ty) + b * ty
    };
    let add_glow = |c: [f32; 3], x: usize, y: usize| -> [f32; 3] {
        let k = glow_at(x, y) * fire.glow;
        [c[0] + k * 0.6, c[1] + k * 0.22, c[2] + k * 0.03]
    };
    // A crown pixel: scorched, then charred, with flames licking up from the tops.
    let fire_crown = |c: [f32; 3], x: usize, y: usize, top: f32| -> [f32; 3] {
        let (burn, flame) = fire_at(x, y);
        let lum = (0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]).clamp(0.0, 1.0);
        let scorched = [0.05 + 0.75 * lum, 0.03 + 0.5 * lum, 0.02 + 0.28 * lum];
        let charred = [0.05 + 0.34 * lum, 0.045 + 0.29 * lum, 0.04 + 0.26 * lum];
        let mut o = lerp3(c, scorched, ss(0.05, 0.4, burn));
        o = lerp3(o, charred, ss(0.35, 0.75, burn) * fire.char);
        if flame > 0.0 {
            let fl = flicker(x, y);
            let q = (flame * (0.25 + 0.75 * top) * ss(0.4, 0.8, fl + 0.2 * top)).clamp(0.0, 1.0);
            o = lerp3(o, flame_color(q), ss(0.0, 0.45, q));
        }
        add_glow(o, x, y)
    };
    // The ground: ash, embers and glowing beds under the flames.
    let fire_ground = |c: [f32; 3], x: usize, y: usize| -> [f32; 3] {
        let (burn, flame) = fire_at(x, y);
        let lum = (0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]).clamp(0.0, 1.0);
        let ash = [0.11 + 0.25 * lum, 0.10 + 0.22 * lum, 0.09 + 0.2 * lum];
        let mut o = lerp3(c, ash, ss(0.1, 0.5, burn) * fire.char);
        if flame > 0.0 {
            let fl = flicker(x, y);
            o = lerp3(o, [0.55 * (0.6 + 0.8 * fl), 0.12 * (0.6 + 0.8 * fl), 0.02], flame * 0.6 * fire.embers);
        }
        let r = hash01((x + ox) as i32, (y + oy) as i32, sd + 92);
        if r < fire.embers * 0.05 * ss(0.3, 0.7, burn) {
            o = flame_color(0.4 + 0.6 * hash01((x + ox) as i32, (y + oy) as i32, sd + 93));
        }
        add_glow(o, x, y)
    };
    let shade_layer = |st: &Stamp, recol: [f32; 3]| -> (Vec<[f32; 3]>, Vec<f32>) {
        let hb = &st.height;
        let hf = Field { w, h, data: hb.clone() };
        let blurred = hf.blur(5.0 * s);
        let b3 = hf.blur(3.0 * s).data;
        let b9 = hf.blur(9.0 * s).data;
        // Height relative to the surroundings at crown scale, for large-scale light and shade.
        let b_big = hf.blur(g.depth.shade_radius.max(2.0) * s).data;
        let rel_scale = 1.0 / (g.depth.shade_radius.max(2.0) * s * 0.8);
        let alpha = Field::from_fn(w, h, |x, y| (hb[y * w + x] > 0.0) as u8 as f32).blur(0.6 * s).data;
        let mut rgb = vec![[0f32; 3]; w * h];
        rgb.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, px) in row.iter_mut().enumerate() {
                let i = y * w + x;
                if alpha[i] <= 0.0 || st.spr_cls[i] == 0 {
                    continue;
                }
                px.copy_from_slice(&sprite_color(
                    SpritePixel {
                        x: x + ox,
                        y: y + oy,
                        nx: st.tilt(i).0,
                        ny: st.tilt(i).1,
                        depth: st.spr_depth(i),
                        tone: st.spr_tone(i),
                        tree_tone: st.tone(i),
                        cls: st.spr_cls[i],
                        hb: hb[i],
                        b3: b3[i],
                        b9: b9[i],
                        bx: (blurred.at(x as i32 + 1, y as i32) - blurred.at(x as i32 - 1, y as i32)) * 0.5,
                        by: (blurred.at(x as i32, y as i32 + 1) - blurred.at(x as i32, y as i32 - 1)) * 0.5,
                        rel: (hb[i] - b_big[i]) * rel_scale,
                        ramp: &ramps[(st.species[i] as usize).min(ramps.len() - 1)],
                        bark: barks[(st.species[i] as usize).min(barks.len() - 1)],
                        plain: plains[(st.species[i] as usize).min(plains.len() - 1)],
                        recol,
                    },
                    ls,
                    &g.depth,
                    lv,
                    light.flat,
                    s,
                    inv,
                    sd,
                ));
            }
        });
        // The thin edge just outside a tree gets a dark rim in the colours of the tree beside it,
        // which keeps leaves and crowns crisp against whatever lies behind.
        rgb.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, px) in row.iter_mut().enumerate() {
                let i = y * w + x;
                if alpha[i] <= 0.0 || st.spr_cls[i] != 0 {
                    continue;
                }
                let mut sp = 0usize;
                'find: for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                        if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                            continue;
                        }
                        let j = yy as usize * w + xx as usize;
                        if st.spr_cls[j] != 0 {
                            sp = st.species[j] as usize;
                            break 'find;
                        }
                    }
                }
                let c = ramp_color(&ramps[sp.min(ramps.len() - 1)], 0.1);
                *px = [c[0] * 0.85 * recol[0], c[1] * 0.85 * recol[1], c[2] * 0.85 * recol[2]];
            }
        });
        if snow_on {
            rgb.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, px) in row.iter_mut().enumerate() {
                    let i = y * w + x;
                    if alpha[i] <= 0.0 {
                        continue;
                    }
                    // Upward facing and high relative to its neighbours holds snow best.
                    let (tx, ty) = st.tilt(i);
                    let up = 1.0 - (tx.hypot(ty) * 1.2).min(1.0);
                    let rel = (hb[i] - b_big[i]) * rel_scale;
                    let top = ((0.5 + 1.2 * rel).clamp(0.0, 1.0)) * up;
                    let cling = 1.0 - snow.tops * (1.0 - top);
                    let k = snow_k(x, y, cling);
                    if k > 0.0 {
                        *px = snow_color(*px, k);
                    }
                }
            });
        }
        if fire_on {
            rgb.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, px) in row.iter_mut().enumerate() {
                    let i = y * w + x;
                    if alpha[i] <= 0.0 {
                        continue;
                    }
                    let (tx, ty) = st.tilt(i);
                    let up = 1.0 - (tx.hypot(ty) * 1.2).min(1.0);
                    let rel = (hb[i] - b_big[i]) * rel_scale;
                    let top = (0.5 + 1.2 * rel).clamp(0.0, 1.0) * (0.5 + 0.5 * up);
                    *px = fire_crown(*px, x, y, top);
                }
            });
        }
        (rgb, alpha)
    };

    let fade: Vec<f32> = if realistic {
        Vec::new()
    } else {
        stamped.depth.par_iter().map(|d| ss(-feather * 0.5, feather * 0.5, *d)).collect()
    };
    let apply_fade = |a: &mut Vec<f32>, keep: Option<&Vec<f32>>| {
        if !fade.is_empty() {
            match keep {
                // Painted trees stay solid wherever the user put them.
                Some(k) => a.par_iter_mut().zip(fade.par_iter()).zip(k.par_iter()).for_each(|((a, f), k)| *a *= f.max(*k)),
                None => a.par_iter_mut().zip(fade.par_iter()).for_each(|(a, f)| *a *= f),
            }
        }
    };

    // Clip a layer to the drawn area. Painted trees are exempt, so a tree placed by hand always shows.
    let clip = |a: &mut Vec<f32>, keep: Option<&Vec<f32>>| {
        if !clip_on || a.is_empty() {
            return;
        }
        match keep {
            Some(k) => a.par_iter_mut().zip(area_a.par_iter()).zip(k.par_iter()).for_each(|((a, c), k)| *a *= if *k > 0.0 { 1.0 } else { *c }),
            None => a.par_iter_mut().zip(area_a.par_iter()).for_each(|(a, c)| *a *= *c),
        }
    };
    let clip_shadow = |a: &mut Vec<f32>| {
        if g.contain_shadows {
            clip(a, None);
        }
    };
    let recols = [[0.85, 0.95, 0.8], [1.0, 1.0, 1.0], [1.05, 1.1, 0.95], [0.8, 0.9, 0.75]];
    let kinds = [LayerKind::Shrub, LayerKind::Canopy, LayerKind::Lone, LayerKind::Pebble];
    let mut layers = Vec::new();

    // Haze
    let mut haze_alpha = stamped.haze_a.clone();
    clip(&mut haze_alpha, None);
    layers.push(LayerBuf::flat(LayerKind::Haze, w, h, g.haze_color, haze_alpha));
    // Ground shadow (not in blurred mode, where the patch must blend without a footprint)
    let mut ground = if realistic { shadow_for(None, &|_| true) } else { Vec::new() };
    clip_shadow(&mut ground);
    layers.push(LayerBuf::new(LayerKind::GroundShadow, w, h, Vec::new(), ground, Vec::new()));
    if cancelled() {
        return None;
    }
    mark("haze and ground shadow");
    // Grass
    if realistic {
        let rgb: Vec<[f32; 3]> = (0..w * h)
            .into_par_iter()
            .map(|i| {
                if grass_a[i] <= 0.0 {
                    return [0.0; 3];
                }
                let (x, y) = (i % w, i / w);
                let (nx, ny) = nat(x, y);
                let grain = fbm(nx, ny, 2.0, 2, 0.5, sd + 30);
                let r = hash01((x + ox) as i32, (y + oy) as i32, sd + 31);
                let dots = if r < 0.04 { (hash01((x + ox) as i32, (y + oy) as i32, sd + 32) - 0.5) * 1.0 } else { 0.0 };
                let m = (0.75 + 0.5 * grain + dots) * (0.8 + 0.4 * fbm(nx, ny, 24.0, 3, 0.5, sd + 33)) * 0.95;
                // Accent patches with soft edges, plus a sprinkling of single bright flecks.
                let patch = ((fbm(nx, ny, 70.0, 3, 0.5, sd + 34) - 0.55) * 7.0).clamp(0.0, 1.0) * 0.55;
                let fleck = if r > 0.985 { 0.85 } else { 0.0 };
                let k = (patch + fleck).min(1.0);
                let c = [
                    g.grass_color[0] * (1.0 - k) + g.grass_accent[0] * k,
                    g.grass_color[1] * (1.0 - k) + g.grass_accent[1] * k,
                    g.grass_color[2] * (1.0 - k) + g.grass_accent[2] * k,
                ];
                [c[0] * m, c[1] * m, c[2] * m]
            })
            .collect();
        let mut rgb = rgb;
        if snow_on && snow.ground > 0.0 {
            rgb.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, px) in row.iter_mut().enumerate() {
                    if grass_a[y * w + x] > 0.0 {
                        let k = snow_k(x, y, snow.ground);
                        if k > 0.0 {
                            *px = snow_color(*px, k);
                        }
                    }
                }
            });
        }
        if fire_on {
            rgb.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, px) in row.iter_mut().enumerate() {
                    if grass_a[y * w + x] > 0.0 {
                        *px = fire_ground(*px, x, y);
                    }
                }
            });
        }
        mark("grass colour");
        let mut shadow = shadow_for(Some(&grass_h), &|i| grass_a[i] > 0.0);
        mark("grass shadow");
        clip_shadow(&mut shadow);
        let mut grass_alpha = grass_a.clone();
        clip(&mut grass_alpha, None);
        layers.push(LayerBuf::new(LayerKind::Grass, w, h, rgb, grass_alpha, shadow));
    } else {
        // Blurred mode: a dark continuous canopy floor so no background shows between crowns.
        let floor_ramp = ramps[(g.main_species as usize).min(ramps.len() - 1)];
        let rgb: Vec<[f32; 3]> = (0..w * h)
            .into_par_iter()
            .map(|i| {
                let (nx, ny) = nat(i % w, i / w);
                let c = ramp_color(&floor_ramp, 0.12 + 0.3 * fbm(nx, ny, 6.0, 3, 0.6, sd + 40));
                [c[0] * 0.7, c[1] * 0.7, c[2] * 0.7]
            })
            .collect();
        let mut alpha = vec![1.0; w * h];
        apply_fade(&mut alpha, None);
        clip(&mut alpha, None);
        layers.push(LayerBuf::new(LayerKind::Grass, w, h, rgb, alpha, Vec::new()));
    }
    for j in 0..stamped.stamps.len() {
        if cancelled() {
            return None;
        }
        let owned;
        let st: &Stamp = if keep {
            &stamped.stamps[j]
        } else {
            owned = std::mem::replace(&mut stamped.stamps[j], Stamp::new(0, 0));
            &owned
        };
        let (rgb, mut alpha) = shade_layer(st, recols[j]);
        mark("layer shaded");
        let mut shadow = shadow_for(Some(&st.height), &|i| alpha[i] > 0.0);
        mark("layer shadow");
        clip_shadow(&mut shadow);
        // Painted trees stay solid; most layers have none, so skip the buffers then.
        let painted_px = st.painted.iter().any(|p| *p > 0).then(|| st.painted_f32());
        let painted = painted_px.as_ref().map(|p| Field { w, h, data: p.clone() }.blur(1.0 * s).data);
        apply_fade(&mut alpha, painted.as_ref());
        clip(&mut alpha, painted_px.as_ref());
        layers.push(LayerBuf::new(kinds[j], w, h, rgb, alpha, shadow));
    }
    mark("layers done");
    // Keep LayerKind::ALL order.
    layers.sort_by_key(|l| l.kind.index());

    Some(RenderLayers {
        w,
        h,
        scale,
        layers,
        tree_counts: plan.tree_counts,
        fit_inset: plan.fit_inset,
        trees: if with_trees { plan.trees.clone() } else { Vec::new() },
    })
}
