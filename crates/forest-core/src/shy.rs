//! Crown shyness. Neighbouring crowns of similar height stop short of each other and leave a thin channel,
//! and crowns reshape to fit one another. Each canopy tree gets a squash factor for 24 directions around it,
//! worked out from its neighbours; the sprite is pulled in by that factor when it is stamped, so the leaves
//! keep their texture and the gap follows the outlines of both crowns. Mushrooms stay rigid: trees keep
//! clear of them, and they are never cut.
//!
//! The rule follows the growth models behind crown shyness (crowns freeze where they meet a neighbour and
//! the contact side dies back a little): the taller crown claims the contested space first, so the line
//! between two crowns settles nearer the shorter one.

use super::Tree;
use crate::params::{GenParams, Species};
use crate::wp::Sprite;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub(crate) const SECTORS: usize = 24;
pub(crate) type Factors = [u8; SECTORS];

/// Crowns whose heights differ by more than this share of the taller one are left to overlap.
const HEIGHT_TOL: f32 = 0.3;
/// A crown is never squashed below this share of its size, however crowded.
const MIN_FACTOR: f32 = 0.55;
/// A tree whose centre sits this close to a mushroom cap (in its own radii) is not drawn: it would be mostly
/// hidden and the rest would be crushed against the cap.
const HIDE_NEAR: f32 = 0.3;
/// Share of a crown's pixels inside the radius taken as its edge, so a few stray leaves can reach into the gap.
const EDGE_PCT: f32 = 0.88;

/// Crown shyness settings.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Shy {
    pub(crate) on: bool,
    pub(crate) gap: f32,
    pub(crate) giants_only: bool,
}

impl Shy {
    pub(crate) fn of(g: &GenParams) -> Shy {
        Shy { on: g.crown_shyness && g.shyness_gap > 0.0, gap: g.shyness_gap.max(0.0), giants_only: g.shyness_giants_only }
    }
}

/// An angle that grows with the true angle but needs no trigonometry, from 0 to 4 round the circle. The GPU
/// kernel uses the same formula, so both sides agree on which sector a pixel is in.
#[inline]
pub(crate) fn diamond(ox: f32, oy: f32) -> f32 {
    let p = oy / (ox.abs() + oy.abs()).max(1e-6);
    if ox >= 0.0 {
        if oy >= 0.0 {
            p
        } else {
            4.0 + p
        }
    } else {
        2.0 - p
    }
}

/// The unit direction for a `diamond` angle.
fn direction(pa: f32) -> (f32, f32) {
    let (x, y) = if pa < 1.0 {
        (1.0 - pa, pa)
    } else if pa < 2.0 {
        (1.0 - pa, 2.0 - pa)
    } else if pa < 3.0 {
        (pa - 3.0, 2.0 - pa)
    } else {
        (pa - 3.0, pa - 4.0)
    };
    let n = (x * x + y * y).sqrt().max(1e-6);
    (x / n, y / n)
}

/// How much a crown is pulled in toward its centre at an offset from that centre.
#[inline]
pub(crate) fn factor_at(f: &Factors, ox: f32, oy: f32) -> f32 {
    let u = diamond(ox, oy) * (SECTORS as f32 / 4.0) - 0.5;
    let k = u.floor();
    let t = u - k;
    let a = f[(k as i32).rem_euclid(SECTORS as i32) as usize] as f32;
    let b = f[((k as i32) + 1).rem_euclid(SECTORS as i32) as usize] as f32;
    (a + (b - a) * t) / 255.0
}

/// True when the factors mean the tree is not drawn at all.
pub(crate) fn hidden(f: &Factors) -> bool {
    f.iter().all(|v| *v == 0)
}

/// Six words holding the factors four to a word, as the GPU reads them.
pub(crate) fn pack(f: &Factors) -> [u32; 6] {
    let mut w = [0u32; 6];
    for (i, v) in f.iter().enumerate() {
        w[i / 4] |= (*v as u32) << (8 * (i % 4));
    }
    w
}

type ProfileKey = (String, u32, u32, u32);
static PROFILES: OnceLock<Mutex<HashMap<ProfileKey, [f32; SECTORS]>>> = OnceLock::new();

/// How far a sprite's solid pixels reach in each of the 24 directions, in crown radii, for one of the eight
/// orientations a tree can be stamped in.
fn profile(sprite: &Sprite, r0: f32, orient: u32) -> [f32; SECTORS] {
    let n = sprite.size as i32;
    let (k, flip) = (orient % 4, orient >= 4);
    let mut bins: Vec<Vec<f32>> = vec![Vec::new(); SECTORS];
    for dy in 0..n {
        for dx in 0..n {
            let (mut ux, mut uy) = if flip { (n - 1 - dx, dy) } else { (dx, dy) };
            for _ in 0..k {
                let (a, b) = (uy, n - 1 - ux);
                ux = a;
                uy = b;
            }
            if sprite.cls[(uy * n + ux) as usize] == 0 {
                continue;
            }
            let (ox, oy) = (dx as f32 + 0.5 - n as f32 / 2.0, dy as f32 + 0.5 - n as f32 / 2.0);
            let s = ((diamond(ox, oy) * (SECTORS as f32 / 4.0)) as usize).min(SECTORS - 1);
            bins[s].push((ox * ox + oy * oy).sqrt() / r0);
        }
    }
    let mut out = [1.0; SECTORS];
    for (s, b) in bins.iter_mut().enumerate() {
        if b.is_empty() {
            continue;
        }
        b.sort_by(|a, c| a.partial_cmp(c).unwrap());
        // ignore a few stray leaves at the very edge
        out[s] = b[((b.len() as f32 * EDGE_PCT) as usize).min(b.len() - 1)];
    }
    out
}

/// The orientation profile of a tree, from the cache, or a plain disc when it has no generated sprite.
fn tree_profile(t: &Tree, sp: &Species) -> [f32; SECTORS] {
    let orient = ((t.seed >> 8) % 8) as u32;
    let variant = (t.seed % crate::treegen_sprite::VARIANTS as u64) as u32;
    let Some(ty) = sp.tree_type(false) else { return [1.2; SECTORS] };
    let key = (ty.to_string(), variant, orient, sp.canopy_fill.to_bits());
    let cache = PROFILES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(p) = cache.lock().unwrap().get(&key) {
        return *p;
    }
    let p = match crate::treegen_sprite::reference(ty, variant, sp.canopy_fill) {
        Some(s) => profile(&s, crate::treegen_sprite::R0, orient),
        None => [1.2; SECTORS],
    };
    cache.lock().unwrap().insert(key, p);
    p
}

/// Reach of a profile in the direction of a unit vector.
fn reach_toward(p: &[f32; SECTORS], ux: f32, uy: f32) -> f32 {
    p[((diamond(ux, uy) * (SECTORS as f32 / 4.0)) as usize).min(SECTORS - 1)]
}

/// The factors for every tree of the canopy layer. `gap` is in the units of the tree coordinates.
pub(crate) fn compute(trees: &[Tree], species: &[Species], shy: &Shy, gap: f32) -> Vec<Factors> {
    let fallback = Species::default();
    let is_mush: Vec<bool> = trees.iter().map(|t| species.get(t.species as usize).map_or(false, |s| s.preset.starts_with("mushroom_"))).collect();
    let prof: Vec<[f32; SECTORS]> = trees.par_iter().map(|t| tree_profile(t, species.get(t.species as usize).unwrap_or(&fallback))).collect();
    let pmax: Vec<f32> = prof.iter().zip(trees).map(|(p, t)| p.iter().cloned().fold(0.0, f32::max) * t.r).collect();
    let cell = (2.0 * pmax.iter().cloned().fold(1.0, f32::max) + gap).max(8.0);
    let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, t) in trees.iter().enumerate() {
        grid.entry(((t.x / cell).floor() as i32, (t.y / cell).floor() as i32)).or_default().push(i as u32);
    }
    (0..trees.len())
        .into_par_iter()
        .map(|i| {
            let mut out = [255u8; SECTORS];
            let t = &trees[i];
            if is_mush[i] {
                return out;
            }
            let (cx, cy) = ((t.x / cell).floor() as i32, (t.y / cell).floor() as i32);
            let mut lim = [f32::INFINITY; SECTORS];
            let mut any = false;
            let mut hidden = false;
            for gy in cy - 1..=cy + 1 {
                for gx in cx - 1..=cx + 1 {
                    let Some(v) = grid.get(&(gx, gy)) else { continue };
                    for &j in v {
                        let j = j as usize;
                        if j == i {
                            continue;
                        }
                        let o = &trees[j];
                        let (dx, dy) = (o.x - t.x, o.y - t.y);
                        let d = (dx * dx + dy * dy).sqrt();
                        if d < 1e-3 || d > pmax[i] + pmax[j] + gap {
                            continue;
                        }
                        let (ux, uy) = (dx / d, dy / d);
                        let gap_ij = gap.min(0.2 * t.r.min(o.r));
                        if is_mush[j] {
                            // a mushroom is rigid: keep clear of its cap, and only giants when asked
                            if shy.giants_only && !o.giant {
                                continue;
                            }
                            let cap = prof[j].iter().sum::<f32>() / SECTORS as f32 * o.r;
                            let rr = cap + gap_ij;
                            if d < cap + HIDE_NEAR * t.r {
                                hidden = true;
                            }
                            for s in 0..SECTORS {
                                let (px, py) = direction((s as f32 + 0.5) / (SECTORS as f32 / 4.0));
                                let c = px * ux + py * uy;
                                let sin2 = 1.0 - c * c;
                                if d * d * sin2 < rr * rr {
                                    let tt = (d * c - (rr * rr - d * d * sin2).sqrt()).max(0.0);
                                    lim[s] = lim[s].min(tt);
                                    any = true;
                                }
                            }
                        } else {
                            if shy.giants_only {
                                continue;
                            }
                            if (t.h - o.h).abs() > HEIGHT_TOL * t.h.max(o.h) || d < 0.3 * t.r.max(o.r) {
                                continue;
                            }
                            let ei = reach_toward(&prof[i], ux, uy) * t.r;
                            let ej = reach_toward(&prof[j], -ux, -uy) * o.r;
                            // the taller crown claims more of the space between the two centres
                            let (wi, wj) = (ei * t.h, ej * o.h);
                            let b = d * wi / (wi + wj).max(1e-6) - gap_ij * 0.5;
                            for s in 0..SECTORS {
                                let (px, py) = direction((s as f32 + 0.5) / (SECTORS as f32 / 4.0));
                                let c = px * ux + py * uy;
                                if c > 0.1 {
                                    lim[s] = lim[s].min(b.max(0.0) / c);
                                    any = true;
                                }
                            }
                        }
                    }
                }
            }
            if hidden {
                return [0u8; SECTORS];
            }
            if any {
                let mut f = [1.0f32; SECTORS];
                for s in 0..SECTORS {
                    let e = (prof[i][s] * t.r).max(1e-3);
                    f[s] = (lim[s] / e).clamp(MIN_FACTOR, 1.0);
                }
                // soften the steps between neighbouring directions, so crowns are not torn into streaks
                for s in 0..SECTORS {
                    let g = 0.25 * f[(s + SECTORS - 1) % SECTORS] + 0.5 * f[s] + 0.25 * f[(s + 1) % SECTORS];
                    out[s] = (g * 255.0).round() as u8;
                }
            }
            out
        })
        .collect()
}
