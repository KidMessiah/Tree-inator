//! Dead and dying trees. A dead tree is a normal tree whose geometry has been damaged in stages, so the wood
//! on the sprite is real branches: nothing is painted on. The stages follow how standing dead trees decay
//! (the five snag decay classes used by foresters): fine twigs go first, then the small branches, the
//! larger limbs snap and leave stubs, and last the top breaks off. A stag-headed tree is the dieback pattern
//! that works from the outside in: the outer and upper crown is bare wood while the inner crown stays green.
//! Fire only changes how the wood looks (charred, with a few paler cracks) and how much of it is left.

use crate::treegen::{Canopy, Rand, TreeGeo, BP, V};

/// How a dead tree is made from a living one.
#[derive(Clone, Copy)]
pub struct DeadStyle {
    /// The living tree it starts as.
    pub base: &'static str,
    /// How the living crown is grown before it is damaged.
    pub canopy: Canopy,
    /// 0 for a living skeleton, 1 for nearly gone. Drives how many twigs and branches are lost and how
    /// many of the rest are snapped.
    pub decay: f32,
    /// Share of the trunk kept; below 1 the top has broken off.
    pub top_break: f32,
    /// Share of the crown radius that stays alive (leafy), 0 for none.
    pub stag: f32,
    /// Bark tone (0 dark, 1 light) and how much it varies from branch to branch.
    pub tone: f32,
    pub tone_var: f32,
    /// Charring: darkens the tone and adds paler cracks.
    pub char: f32,
    /// Branches lying on the ground around the base.
    pub fallen: usize,
    /// Share of the wood that stays after a limb snaps, at the shortest and longest.
    pub snap: (f32, f32),
}

/// A crown grown with few leaves and plenty of twigs, since only the wood will show.
const WOOD: Canopy = Canopy { leaves: 1.0, wood: 1.0, limb_base: 1.0, limb_rise: 0.0, limbs: 1.0, twigs: 1.6, twig_len: 1.2, leaf_size: 1.0, leaf_width: 1.0, blossom_size: 1.0, inner: 0.0, sky: 0.0, light: 0.0, rise: 0.0 };

const BASE: DeadStyle = DeadStyle { base: "black_oak", canopy: WOOD, decay: 0.0, top_break: 1.0, stag: 0.0, tone: 0.55, tone_var: 0.12, char: 0.0, fallen: 0, snap: (0.3, 0.85) };

/// The dead tree types. Names are tree types like the living ones.
pub const DEAD_TYPES: &[(&str, &str)] = &[
    ("bare_oak", "Bare oak"),
    ("dead_oak", "Dead oak"),
    ("stag_oak", "Stag-headed oak"),
    ("dead_birch", "Dead birch"),
    ("dead_willow", "Dead willow"),
    ("dead_pine", "Dead pine"),
    ("snag", "Snag"),
    ("burnt_oak", "Burnt oak"),
    ("burnt_pine", "Burnt pine"),
];

pub fn style(name: &str) -> Option<DeadStyle> {
    Some(match name {
        // a living tree in winter: every twig is still there
        "bare_oak" => DeadStyle { decay: 0.0, tone: 0.42, ..BASE },
        // recently dead hardwood: the fine twigs are gone and some limbs have snapped
        "dead_oak" => DeadStyle { decay: 0.5, tone: 0.62, fallen: 2, ..BASE },
        // dying from the outside in: a living inner crown under bare outer limbs
        "stag_oak" => DeadStyle { canopy: Canopy { leaves: 1.2, twigs: 1.6, twig_len: 1.2, leaf_size: 1.2, inner: 0.2, sky: 0.7, ..WOOD }, decay: 0.45, stag: 0.8, tone: 0.5, fallen: 1, ..BASE },
        "dead_birch" => DeadStyle { base: "silver_birch", decay: 0.55, tone: 0.8, tone_var: 0.1, fallen: 2, ..BASE },
        // long drooping whips with the leaves gone
        "dead_willow" => DeadStyle { base: "weeping_willow", canopy: Canopy { wood: 0.5, limb_base: 0.0, limb_rise: 120.0, limbs: 1.4, twigs: 0.7, ..WOOD }, decay: 0.5, tone: 0.5, fallen: 1, ..BASE },
        // a dead conifer: the needles are gone, the whorls are bare and snap short
        "dead_pine" => DeadStyle { base: "balsam_fir", canopy: Canopy { twigs: 1.3, ..WOOD }, decay: 0.38, tone: 0.5, fallen: 2, snap: (0.4, 0.95), ..BASE },
        // a long dead trunk, broken off, with a few stubs
        "snag" => DeadStyle { base: "douglas_fir", canopy: Canopy { wood: 0.55, ..WOOD }, decay: 0.8, top_break: 0.6, tone: 0.56, tone_var: 0.08, fallen: 3, snap: (0.2, 0.55), ..BASE },
        "burnt_oak" => DeadStyle { decay: 0.55, top_break: 0.85, tone: 0.3, tone_var: 0.08, char: 0.8, fallen: 2, snap: (0.25, 0.7), ..BASE },
        "burnt_pine" => DeadStyle { base: "balsam_fir", canopy: Canopy { twigs: 1.3, ..WOOD }, decay: 0.5, top_break: 0.8, tone: 0.3, tone_var: 0.08, char: 0.9, fallen: 2, snap: (0.25, 0.75), ..BASE },
        _ => return None,
    })
}

pub fn is_dead(name: &str) -> bool {
    style(name).is_some()
}

/// Where the live crown is when only part of it is alive: narrower toward the top.
fn live_zone(p: V, cx: f64, cy: f64, r: f64, zmax: f64, stag: f64, seed: u64) -> bool {
    let d = ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt() / r.max(1e-6);
    let up = (p.2 / zmax.max(1e-6)).clamp(0.0, 1.0);
    // patches of leaf that thin out toward the outside and the top, so the live crown has a ragged edge
    let k = 1.0 / (r.max(1e-6) * 0.5);
    let n = crate::noise::value((p.0 * k) as f32, (p.1 * k) as f32, (seed & 0xFFFF) as u32 + 17) as f64;
    let x = d / (stag * 1.5).max(1e-3) + 0.2 * up;
    n > 0.95 * x.powi(2)
}

fn nearest_point(pts: &[BP], v: V) -> usize {
    let mut best = (f64::MAX, 0);
    for (i, b) in pts.iter().enumerate() {
        let d = (b.co - v).mag();
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

/// Damage a grown tree in place and return a bark tone for each of its branches. `cx`, `cy`, `r` and `zmax`
/// describe the living crown in tree units, so the damage is measured against the tree's own size.
pub fn apply(geo: &mut TreeGeo, st: &DeadStyle, seed: u64, cx: f64, cy: f64, r: f64, zmax: f64) -> Vec<f32> {
    let mut rng = Rand::new(seed ^ 0xDEAD_BEEF);
    let n = geo.splines.len();
    let top = geo.splines.iter().map(|s| s.0).max().unwrap_or(0);
    // a branch belongs to the latest branch one level up
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut last = [usize::MAX; 8];
    for i in 0..n {
        let l = geo.splines[i].0.min(7);
        parent[i] = if l > 0 && last[l - 1] != usize::MAX { Some(last[l - 1]) } else { None };
        last[l] = i;
    }
    let mut alive = vec![true; n];
    let mut cut: Vec<Option<usize>> = vec![None; n];
    let decay = st.decay as f64;
    for i in 0..n {
        let (l, ref pts) = geo.splines[i];
        if let Some(p) = parent[i] {
            if !alive[p] {
                alive[i] = false;
                continue;
            }
            if let Some(c) = cut[p] {
                // grew from the part that snapped off
                if nearest_point(&geo.splines[p].1, pts[0].co) > c {
                    alive[i] = false;
                    continue;
                }
            }
        }
        if l == 0 {
            if st.top_break < 1.0 && pts.len() > 2 {
                cut[i] = Some((((pts.len() - 1) as f64 * st.top_break as f64) as usize).clamp(1, pts.len() - 1));
            }
            continue;
        }
        let q = l as f64 / top.max(1) as f64;
        let mut p_rm = (decay * 2.2 * q.powf(1.2) - 0.15).clamp(0.0, 1.0);
        if st.stag > 0.0 {
            let inside = live_zone(pts[0].co, cx, cy, r, zmax, st.stag as f64, seed);
            p_rm = if inside { p_rm * 0.15 } else { (p_rm * 1.5 + 0.35).min(1.0) };
        }
        if rng.random() < p_rm {
            alive[i] = false;
            continue;
        }
        let p_br = (decay * 1.3).clamp(0.0, 1.0) * (1.0 - 0.3 * q);
        if pts.len() > 2 && rng.random() < p_br {
            let f = rng.in_range(st.snap.0 as f64, st.snap.1 as f64);
            cut[i] = Some((((pts.len() - 1) as f64 * f).round() as usize).clamp(1, pts.len() - 1));
        }
    }
    if st.stag > 0.0 {
        geo.leaves.retain(|l| live_zone(l.position, cx, cy, r, zmax, st.stag as f64, seed));
    } else {
        geo.leaves.clear();
    }
    // keep the surviving branches, cut where they snapped
    let old = std::mem::take(&mut geo.splines);
    let mut tones = Vec::new();
    for (i, (l, mut pts)) in old.into_iter().enumerate() {
        if !alive[i] {
            continue;
        }
        if let Some(c) = cut[i] {
            pts.truncate(c + 1);
            if let Some(b) = pts.last_mut() {
                b.hr = b.co;
            }
        }
        let jitter = (rng.random() as f32 - 0.5) * 2.0 * st.tone_var;
        // fine twigs weather paler, the thick wood stays darker
        let level = 0.05 * (l as f32 - 1.0).max(0.0);
        let mut tone = st.tone + jitter + level * (1.0 - st.char);
        if st.char > 0.0 {
            // a few limbs burnt less, most burnt black
            tone += if rng.random() < 0.15 { 0.2 * st.char } else { 0.0 };
        }
        tones.push(tone.clamp(0.04, 0.96));
        geo.splines.push((l, pts));
    }
    // branches that have come down around the base
    // fallen limbs are as thick as the limbs left on the tree, not as the trunk
    let mut limb_r: Vec<f64> = geo.splines.iter().filter(|s| s.0 == 1).map(|s| s.1[0].radius).collect();
    limb_r.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let trunk_r = limb_r.get(limb_r.len() / 2).copied().or_else(|| geo.splines.first().map(|s| s.1[0].radius)).unwrap_or(0.02);
    for _ in 0..st.fallen {
        let a = rng.in_range(0.0, std::f64::consts::TAU);
        let d0 = r * rng.in_range(0.08, 0.4);
        let len = r * rng.in_range(0.25, 0.55);
        let dir = V(a.cos(), a.sin(), 0.0);
        // limbs lie roughly across the way they fell, with a slight bend
        let bend = rng.sym() * 0.25;
        let base = V(cx, cy, 0.0) + dir * d0;
        let rad = trunk_r * rng.in_range(0.6, 1.0);
        let mut pts = Vec::new();
        for k in 0..4 {
            let u = k as f64 / 3.0;
            let side = V(-dir.1, dir.0, 0.0) * (bend * len * u * (1.0 - u) * 2.0);
            let co = base + dir * (len * u) + side + V(0.0, 0.0, zmax * 0.012);
            pts.push(BP { co, hl: co, hr: co, radius: rad * (1.0 - 0.45 * u) });
        }
        geo.splines.push((1, pts));
        let t = if st.char > 0.0 { (st.tone * 0.8).clamp(0.04, 0.9) } else { (st.tone - 0.12).clamp(0.04, 0.9) };
        tones.push(t);
    }
    tones
}

/// Tone of one stretch of a branch: the branch's own tone, with pale cracks across charred wood.
pub fn segment_tone(base: f32, char: f32, amp: f32, spline: usize, seg: usize) -> f32 {
    let h = (spline as u32).wrapping_mul(0x9E37_79B1) ^ (seg as u32).wrapping_mul(0x85EB_CA6B);
    let h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    let u = ((h ^ (h >> 12)) & 0xFFFF) as f32 / 65535.0;
    let t = base + (u - 0.5) * amp;
    if char > 0.0 && u > 0.88 {
        // a crack showing grey wood
        (t + 0.28 * char).min(0.95)
    } else {
        t.clamp(0.03, 0.97)
    }
}
