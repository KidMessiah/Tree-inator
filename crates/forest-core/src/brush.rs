//! Tree brush: paint, stamp and erase trees as records. Rendering is done by the engine.

use crate::params::{Project, TreeLayer, TreeRec};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrushMode {
    /// Scatter trees under the brush while dragging.
    Paint,
    /// One tree per click.
    Single,
    /// Remove trees under the brush.
    Erase,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushSettings {
    pub mode: BrushMode,
    pub layer: TreeLayer,
    pub species: u8,
    /// Brush radius in map pixels.
    pub radius: f32,
    /// Crown radius range in map pixels.
    pub size_min: f32,
    pub size_max: f32,
    /// 0 = sparse, 1 = packed like generated canopy.
    pub density: f32,
    /// Random colour variation per tree.
    pub tone_jitter: f32,
    /// Erase also removes generated trees, not just painted ones.
    pub erase_generated: bool,
    /// Erase only touches the selected layer.
    pub erase_layer_only: bool,
    /// Painting clears generated trees under the brush, so painted trees are not hidden by them.
    pub replace_generated: bool,
}

impl Default for BrushSettings {
    fn default() -> Self {
        BrushSettings {
            mode: BrushMode::Paint,
            layer: TreeLayer::Canopy,
            species: 0,
            radius: 60.0,
            size_min: 10.0,
            size_max: 17.0,
            density: 0.7,
            tone_jitter: 0.1,
            erase_generated: true,
            erase_layer_only: false,
            replace_generated: true,
        }
    }
}

impl BrushSettings {
    /// Spacing factor between crowns: generated canopy uses 0.55.
    fn spacing(&self) -> f32 {
        0.55 + (1.0 - self.density.clamp(0.0, 1.0)) * 1.2
    }

    /// Distance the cursor travels between dabs, in map pixels.
    pub fn dab_step(&self) -> f32 {
        (self.radius * 0.35).max(2.0)
    }

    fn make(&self, x: f32, y: f32, r: f32, rng: &mut Rng) -> TreeRec {
        TreeRec {
            x,
            y,
            r,
            h: self.layer.height(r),
            tone: rng.normal(0.0, self.tone_jitter),
            layer: self.layer,
            species: self.species,
            seed: rng.next_u64() as u32,
        }
    }
}

/// Scatter trees in a circle, keeping spacing from trees already painted on the same layer.
/// Returns how many trees were added.
pub fn paint_dab(project: &mut Project, b: &BrushSettings, x: f32, y: f32, rng: &mut Rng) -> usize {
    let (rmin, rmax) = (b.size_min.max(1.0), b.size_max.max(b.size_min.max(1.0) + 0.1));
    let mind = b.spacing();
    let reach = b.radius + 2.0 * rmax;
    // Nearby trees only, so cost does not grow with the whole painting.
    let mut near: Vec<(f32, f32, f32)> = project
        .painted
        .iter()
        .filter(|t| t.layer == b.layer && (t.x - x).abs() < reach && (t.y - y).abs() < reach)
        .map(|t| (t.x, t.y, t.r))
        .collect();
    let ravg = 0.5 * (rmin + rmax);
    let tries = ((b.radius * b.radius) / (ravg * ravg) * 3.0).ceil().clamp(4.0, 400.0) as usize;
    let mut added = 0;
    for _ in 0..tries {
        let a = rng.f32() * std::f32::consts::TAU;
        let d = rng.f32().sqrt() * b.radius;
        let (px, py) = (x + a.cos() * d, y + a.sin() * d);
        let r = rng.range(rmin, rmax);
        let ok = near.iter().all(|&(nx, ny, nr)| {
            let m = mind * (r + nr);
            (px - nx).powi(2) + (py - ny).powi(2) >= m * m
        });
        if ok {
            project.painted.push(b.make(px, py, r, rng));
            near.push((px, py, r));
            added += 1;
        }
    }
    if b.replace_generated {
        add_erase_circle(project, x, y, b.radius);
    }
    added
}

/// Record an erase circle unless an existing one already covers it, so long strokes stay small.
fn add_erase_circle(project: &mut Project, x: f32, y: f32, r: f32) {
    let covered = project.erased.iter().any(|e| ((e[0] - x).powi(2) + (e[1] - y).powi(2)).sqrt() + r <= e[2]);
    if !covered {
        project.erased.push([x, y, r]);
    }
}

/// Place exactly one tree.
pub fn stamp_one(project: &mut Project, b: &BrushSettings, x: f32, y: f32, rng: &mut Rng) {
    let r = rng.range(b.size_min.max(1.0), b.size_max.max(b.size_min.max(1.0) + 0.1));
    project.painted.push(b.make(x, y, r, rng));
}

/// Remove painted trees under the brush, and mark generated ones as erased if enabled.
pub fn erase_dab(project: &mut Project, b: &BrushSettings, x: f32, y: f32) -> usize {
    let before = project.painted.len();
    let r2 = b.radius * b.radius;
    project
        .painted
        .retain(|t| (b.erase_layer_only && t.layer != b.layer) || (t.x - x).powi(2) + (t.y - y).powi(2) > r2);
    if b.erase_generated {
        add_erase_circle(project, x, y, b.radius);
    }
    before - project.painted.len()
}
