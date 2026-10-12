//! Groups of zones, see `groups.rs`. Each group is drawn with the settings it ends up with after the groups above
//! it have changed the project's, and the groups are stacked bottom to top, like groups in a paint program.
//! Zones that are in no group share the project's settings and are drawn together
//! in the bottom pass, exactly as before. Painted trees are a pass on top of all of them.
//!
//! A clearing cuts the group it is in and the groups inside that group; a clearing in no group cuts everything.
//! Light and grading are the project's, so shadows and colour grading stay consistent across passes.

use crate::compose::{compose, ComposeOptions};
use crate::engine::{generate, RenderLayers};
use crate::params::*;

/// A project the renderer can always draw: it needs at least one species.
fn drawable(mut p: Project) -> Project {
    if p.species.is_empty() {
        p.species = default_species();
    }
    p
}

/// The projects to render, bottom first. Without groups this is the project itself, so nothing changes for
/// existing projects.
///
/// Zones in no group are drawn together with the project settings. Each group that is showing and holds a
/// forest zone is drawn with the settings it ends up with after the groups above it and itself have changed
/// the project's, and the groups are stacked in drawing order. Neighbouring groups that end up with the same
/// settings and the same clearings are drawn together, so a group that changes nothing costs nothing extra.
pub fn zone_passes(p: &Project) -> Vec<Project> {
    // every pass carries its own stand age, which may be a group's, worked into its settings
    zone_passes_raw(p).into_iter().map(|q| q.aged()).collect()
}

fn zone_passes_raw(p: &Project) -> Vec<Project> {
    // projects saved in an older way are brought up to date first
    if p.shapes.iter().any(|s| s.settings.is_some()) || p.groups.iter().any(|n| n.settings.is_some() || n.id == 0) {
        let mut q = p.clone();
        q.migrate_groups();
        return zone_passes_raw(&q);
    }
    if !p.shapes.iter().any(|s| p.zone_group(s).is_some()) {
        return vec![drawable(p.clone())];
    }
    // a zone counts when it is on and every group above it is showing
    let live = |s: &Shape| s.visible && p.zone_group(s).map_or(true, |g| p.shown(g));
    // the clearings that apply to a group: those in no group, and those in the group or any group above it
    let cuts_for = |group: u32| -> Vec<usize> {
        let chain: Vec<u32> = p.chain(group).iter().map(|n| n.id).collect();
        (0..p.shapes.len()).filter(|&i| p.shapes[i].op == ShapeOp::Subtract && live(&p.shapes[i]) && p.zone_group(&p.shapes[i]).map_or(true, |g| chain.contains(&g))).collect()
    };
    let mut out = Vec::new();
    // the shared layer: every zone that follows the project settings, with the clearings in no group
    let mut shared = p.clone();
    shared.shapes.retain(|s| p.zone_group(s).is_none());
    // painted trees go on top of every group, so they are never hidden under a zone canopy
    shared.painted.clear();
    shared.groups.clear();
    if shared.shapes.iter().any(|s| s.visible && s.op == ShapeOp::Add) {
        out.push(drawable(shared));
    }
    // the groups, in drawing order, each with the settings it ends up with
    let mut stack: Vec<(String, Project, Vec<usize>)> = Vec::new();
    for id in p.draw_order() {
        if !p.shown(id) {
            continue;
        }
        let zones: Vec<Shape> = p.shapes.iter().filter(|s| s.op == ShapeOp::Add && live(s) && p.zone_group(s) == Some(id)).map(|s| Shape { group: None, ..s.clone() }).collect();
        if zones.is_empty() {
            continue;
        }
        let (gen, species) = p.effective(id);
        let cuts = cuts_for(id);
        let key = format!("{}{}{:?}", serde_json::to_string(&gen).unwrap_or_default(), serde_json::to_string(&species).unwrap_or_default(), cuts);
        match stack.last_mut() {
            // the same settings and clearings as the group below it: draw them together
            Some((k, q, _)) if *k == key => q.shapes.extend(zones),
            _ => {
                let mut q = p.clone();
                q.groups.clear();
                q.shapes = zones;
                q.gen = gen;
                q.species = species;
                // painted trees point into the project species list, so they have a layer of their own
                q.painted.clear();
                stack.push((key, q, cuts));
            }
        }
    }
    for (_, mut q, cuts) in stack {
        // the clearings come after the zones they cut
        q.shapes.extend(cuts.into_iter().map(|i| Shape { group: None, ..p.shapes[i].clone() }));
        out.push(drawable(q));
    }
    if !p.painted.is_empty() && p.gen.painted_trees {
        // the painted trees alone, with the project species they point into
        let mut top = p.clone();
        top.groups.clear();
        top.shapes.clear();
        top.gen.procedural_trees = false;
        out.push(drawable(top));
    }
    out
}

/// Put `src` over `dst`, both straight alpha RGBA8.
pub fn over(dst: &mut [u8], src: &[u8]) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let sa = s[3] as f32 / 255.0;
        if sa <= 0.0 {
            continue;
        }
        let da = d[3] as f32 / 255.0;
        let oa = sa + da * (1.0 - sa);
        for k in 0..3 {
            let c = (s[k] as f32 * sa + d[k] as f32 * da * (1.0 - sa)) / oa.max(1e-6);
            d[k] = (c + 0.5).clamp(0.0, 255.0) as u8;
        }
        d[3] = (oa * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }
}

/// Fill with an opaque colour, for the map colour under a stack of layers.
pub fn fill_background(rgba: &mut [u8], bg: [f32; 3]) {
    for px in rgba.chunks_exact_mut(4) {
        for k in 0..3 {
            px[k] = (bg[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        px[3] = 255;
    }
}

/// Every layer of the stack, generated on the CPU.
pub fn generate_stack(project: &Project, scale: f32, cancelled: &(dyn Fn() -> bool + Sync)) -> Option<Vec<RenderLayers>> {
    zone_passes(project).iter().map(|p| generate(p, scale, cancelled)).collect()
}

/// The stack as one image. With one layer this is `compose`.
pub fn compose_stack(layers: &[RenderLayers], grading: &Grading, opts: ComposeOptions) -> Vec<u8> {
    if layers.len() == 1 {
        return compose(&layers[0], grading, opts);
    }
    let plain = ComposeOptions { background: None, ..opts };
    let mut out = vec![0u8; layers[0].w * layers[0].h * 4];
    if let Some(bg) = opts.background {
        fill_background(&mut out, bg);
    }
    for l in layers {
        over(&mut out, &compose(l, grading, plain));
    }
    out
}
