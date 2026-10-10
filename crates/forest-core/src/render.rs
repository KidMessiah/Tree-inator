//! Previews that reuse earlier work, and exports drawn strip by strip so memory stays bounded.

use crate::compose::{compose, ComposeOptions};
use crate::engine::{plan, render_size, shade_region, stamp_region, Plan, Region, RenderLayers, Stamped};
use crate::params::*;
use crate::stack::zone_passes;
use std::io::Write;
use std::path::PathBuf;

/// Stamps are kept between previews when they take at most this many bytes.
pub const KEEP_STAMPS_BYTES: usize = 3_000_000_000;
/// Rough bytes per pixel of a stamped region: four packed tree layers plus ground cover.
const STAMPED_BYTES_PER_PX: usize = 68;
/// Exports plan their trees on a grid of at most this many pixels.
const PLAN_MAX_PX: f32 = 24_000_000.0;
/// Output pixels per export strip.
const STRIP_PX: usize = 12_000_000;

/// Everything that decides where trees go and how they are drawn, but not their colour or light. Two
/// projects with the same key share a plan and stamps.
fn structure_key(p: &Project, scale: f32) -> String {
    let mut q = p.clone();
    q.light = Light::default();
    q.grading = Grading::default();
    let d = GenParams::default();
    let gq = &mut q.gen;
    gq.leaf = d.leaf.clone();
    gq.snow = d.snow.clone();
    gq.fire = d.fire.clone();
    gq.grass_color = d.grass_color;
    gq.grass_accent = d.grass_accent;
    gq.haze_color = d.haze_color;
    gq.contain_shadows = d.contain_shadows;
    // brush edits only change the last planning step, so they are handled apart from the structure
    gq.painted_trees = d.painted_trees;
    // shyness is applied when stamping, so it never needs a new plan
    gq.crown_shyness = d.crown_shyness;
    gq.shyness_gap = d.shyness_gap;
    gq.shyness_giants_only = d.shyness_giants_only;
    q.painted.clear();
    q.erased.clear();
    // the picture under the forest is only for the editor
    q.background_image = None;
    // only the crown height spread changes how trees are drawn
    gq.depth = DepthStyle { height_var: p.gen.depth.height_var, ..d.depth.clone() };
    for sp in q.species.iter_mut() {
        sp.ramp = [[0.0; 3]; 4];
        sp.bark = [0.0; 3];
        sp.name.clear();
    }
    serde_json::to_string(&(q, scale)).unwrap_or_default()
}

/// What the brush has done: painted trees and erased areas, and whether painted trees show.
fn edit_key(p: &Project) -> String {
    serde_json::to_string(&(&p.painted, &p.erased, p.gen.painted_trees)).unwrap_or_default()
}

/// A plan and its stamps, either kept in the cache or made just for this render.
pub struct Prepared<'a> {
    pub plan: &'a Plan,
    kept: Option<&'a mut Stamped>,
    owned: Option<Stamped>,
}

impl Prepared<'_> {
    pub fn stamped(&mut self) -> &mut Stamped {
        match (&mut self.kept, &mut self.owned) {
            (Some(k), _) => k,
            (None, Some(o)) => o,
            _ => unreachable!("prepared without stamps"),
        }
    }
}

/// Keeps the plan, and the stamps when they fit, so changing light, colour, snow or fire only reshades.
pub struct RenderCache {
    /// Stamps are kept only when they take at most this many bytes.
    keep_bytes: usize,
    key: String,
    edits: String,
    /// The shyness settings the kept stamps were drawn with.
    shy: Option<crate::engine::Shy>,
    plan: Option<Plan>,
    stamped: Option<Stamped>,
}

impl Default for RenderCache {
    fn default() -> Self {
        RenderCache { keep_bytes: KEEP_STAMPS_BYTES, key: String::new(), edits: String::new(), shy: None, plan: None, stamped: None }
    }
}

impl RenderCache {
    /// How many bytes of stamps this cache may keep. With several layers each gets a share of the total.
    pub fn set_stamp_budget(&mut self, bytes: usize) {
        self.keep_bytes = bytes;
    }

    pub fn render(&mut self, project: &Project, scale: f32, cancelled: &(dyn Fn() -> bool + Sync)) -> Option<RenderLayers> {
        let mut p = self.prepare(project, scale, cancelled)?;
        let keep = p.kept.is_some();
        let plan = p.plan;
        shade_region(project, plan, p.stamped(), cancelled, true, keep)
    }

    /// Only the plan, for renderers that stamp on their own (the GPU). Frees any kept stamps.
    pub fn plan_only(&mut self, project: &Project, scale: f32, cancelled: &(dyn Fn() -> bool + Sync)) -> Option<&Plan> {
        let key = structure_key(project, scale);
        if key != self.key {
            self.plan = None;
            self.key.clear();
        }
        self.stamped = None;
        if self.plan.is_none() {
            self.plan = Some(plan(project, scale, cancelled)?);
            self.key = key;
            self.edits = edit_key(project);
        } else {
            self.sync_edits(project);
        }
        self.plan.as_ref()
    }

    /// The plan and stamps for a render, from the cache when they still fit, ready to shade.
    pub fn prepare(&mut self, project: &Project, scale: f32, cancelled: &(dyn Fn() -> bool + Sync)) -> Option<Prepared<'_>> {
        let key = structure_key(project, scale);
        if key != self.key {
            // free the old work before making new
            self.plan = None;
            self.stamped = None;
            self.key.clear();
        }
        if self.plan.is_none() {
            self.plan = Some(plan(project, scale, cancelled)?);
            self.key = key;
            self.edits = edit_key(project);
        } else if self.sync_edits(project) {
            self.stamped = None;
        }
        let shy = crate::engine::Shy::of(&project.gen);
        if self.shy != Some(shy) {
            self.stamped = None;
            self.shy = Some(shy);
        }
        let RenderCache { plan, stamped, .. } = self;
        let plan = plan.as_ref()?;
        let (w, h) = render_size(project, scale);
        let region = Region { x0: 0, y0: 0, w, h };
        if w * h * STAMPED_BYTES_PER_PX <= self.keep_bytes {
            if stamped.is_none() {
                *stamped = Some(stamp_region(project, plan, scale, region, cancelled)?);
            }
            Some(Prepared { plan, kept: stamped.as_mut(), owned: None })
        } else {
            *stamped = None;
            Some(Prepared { plan, kept: None, owned: Some(stamp_region(project, plan, scale, region, cancelled)?) })
        }
    }

    /// Bring the kept plan up to date with brush edits without planning again. True when it changed.
    fn sync_edits(&mut self, project: &Project) -> bool {
        let now = edit_key(project);
        if now == self.edits {
            return false;
        }
        if let Some(p) = self.plan.as_mut() {
            p.reapply_edits(project);
        }
        self.edits = now;
        true
    }

    /// The plan kept from the last `plan_only` or render, if any.
    pub fn current_plan(&self) -> Option<&Plan> {
        self.plan.as_ref()
    }

    /// Drop everything kept, to give the memory back.
    pub fn clear(&mut self) {
        *self = RenderCache::default();
    }
}

/// Pixels a viewport render needs on every side beyond what is shown, so shadows and blurs at its edge
/// match a full render.
pub fn view_pad(project: &Project, plan: &Plan, scale: f32) -> usize {
    let (up, down) = strip_halo(project, plan, scale);
    let s = scale * project.gen.scale.clamp(0.1, 4.0);
    let slope = 0.6 / project.light.shadow_length.max(0.05);
    let steps = if project.light.shadows { ((plan.max_height() * s / slope).ceil()).clamp(1.0, 600.0) as usize } else { 0 };
    (up.max(down) + steps).min(1500)
}

/// Rows of overlap a strip needs above and below so its own rows come out exactly as in a full render:
/// shadows reach toward the light, and blurs reach both ways.
pub(crate) fn strip_halo(project: &Project, plan: &Plan, scale: f32) -> (usize, usize) {
    let g = &project.gen;
    let light = &project.light;
    let s = scale * g.scale.clamp(0.1, 4.0);
    let blur = [
        2.5 * s * light.shadow_softness * (0.7 + 0.3 * light.shadow_length),
        g.depth.shade_radius.max(2.0) * s,
        9.0 * s,
        18.0 * s,
    ]
    .into_iter()
    .fold(0.0f32, f32::max);
    let reach = (3.0 * blur + 12.0).ceil() as usize;
    if !light.shadows {
        return (reach, reach);
    }
    let slope = 0.6 / light.shadow_length.max(0.05);
    let steps = ((plan.max_height() * s / slope).ceil()).clamp(1.0, 600.0);
    let sy = -light.shadow_angle.to_radians().sin();
    let shadow = (steps * sy.abs()).ceil() as usize + 2;
    // occluders lie toward the sun, which is up the map when sy is negative
    if sy < 0.0 {
        (reach + shadow, reach)
    } else {
        (reach, reach + shadow)
    }
}

/// Render the map at `scale` strip by strip and write one PNG per output. `progress` gets 0..1.
pub fn export_strips(
    project: &Project,
    scale: f32,
    outputs: &[(PathBuf, ComposeOptions)],
    cancelled: &(dyn Fn() -> bool + Sync),
    progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let passes = zone_passes(project);
    let many = passes.len() > 1;
    export_with(&passes, scale, outputs, cancelled, progress, &mut |pi, plan, region| {
        let project = &passes[pi];
        let mut st = stamp_region(project, plan, scale, region, cancelled).ok_or("cancelled")?;
        let layers = shade_region(project, plan, &mut st, cancelled, false, false).ok_or("cancelled")?;
        Ok(outputs.iter().map(|(_, opts)| compose(&layers, &project.grading, layer_opts(*opts, many))).collect())
    })
}

/// `export_strips`, with every strip stamped, shaded and composited on the GPU.
#[cfg(feature = "gpu")]
pub fn export_strips_gpu(
    gpu: &crate::gpu::Gpu,
    project: &Project,
    scale: f32,
    outputs: &[(PathBuf, ComposeOptions)],
    cancelled: &(dyn Fn() -> bool + Sync),
    progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let passes = zone_passes(project);
    let many = passes.len() > 1;
    export_with(&passes, scale, outputs, cancelled, progress, &mut |pi, plan, region| {
        let project = &passes[pi];
        let layers = gpu.render(project, plan, scale, region);
        Ok(outputs.iter().map(|(_, opts)| gpu.compose_rgba(&layers, &project.grading, layer_opts(*opts, many))).collect())
    })
}

/// The strip loop shared by both exports. `strip` renders one region and composites every output.
type StripFn<'a> = dyn FnMut(usize, &Plan, Region) -> Result<Vec<Vec<u8>>, String> + 'a;

/// With several layers the map colour goes under all of them once, so each layer is composed without it.
fn layer_opts(o: ComposeOptions, many: bool) -> ComposeOptions {
    if many {
        ComposeOptions { background: None, ..o }
    } else {
        o
    }
}

fn export_with(
    passes: &[Project],
    scale: f32,
    outputs: &[(PathBuf, ComposeOptions)],
    cancelled: &(dyn Fn() -> bool + Sync),
    progress: &mut dyn FnMut(f32),
    strip: &mut StripFn,
) -> Result<(), String> {
    let (w, h) = render_size(&passes[0], scale);
    let px = (w * h) as f32;
    let plan_scale = if px <= PLAN_MAX_PX { scale } else { scale * (PLAN_MAX_PX / px).sqrt() };
    let mut plans = Vec::new();
    for p in passes {
        plans.push(plan(p, plan_scale, cancelled).ok_or("cancelled")?);
    }
    let (mut up, mut down) = (0, 0);
    for (p, pl) in passes.iter().zip(&plans) {
        let (u, d) = strip_halo(p, pl, scale);
        up = up.max(u);
        down = down.max(d);
    }
    let rows = ((STRIP_PX / w.max(1)).max(64)).div_ceil(4) * 4;

    let mut writers = Vec::new();
    for (path, _) in outputs {
        let file = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?);
        let mut enc = png::Encoder::new(file, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let wr = enc.write_header().map_err(|e| e.to_string())?;
        writers.push(wr.into_stream_writer().map_err(|e| e.to_string())?);
    }
    let mut cy = 0;
    while cy < h {
        let core = rows.min(h - cy);
        // strips start on a multiple of 4 rows, so coarse grids line up between strips
        let y0 = cy.saturating_sub(up) / 4 * 4;
        let y1 = (cy + core + down).min(h);
        let region = Region { x0: 0, y0, w, h: y1 - y0 };
        let mut images = strip(0, &plans[0], region)?;
        if passes.len() > 1 {
            // lay the other layers over the first, and the map colour under all of them
            for (pi, pl) in plans.iter().enumerate().skip(1) {
                let more = strip(pi, pl, region)?;
                for (img, m) in images.iter_mut().zip(&more) {
                    crate::stack::over(img, m);
                }
            }
            for (img, (_, opts)) in images.iter_mut().zip(outputs) {
                if let Some(bg) = opts.background {
                    let mut base = vec![0u8; img.len()];
                    crate::stack::fill_background(&mut base, bg);
                    crate::stack::over(&mut base, img);
                    *img = base;
                }
            }
        }
        let skip = (cy - y0) * w * 4;
        for (rgba, wr) in images.iter().zip(writers.iter_mut()) {
            wr.write_all(&rgba[skip..skip + core * w * 4]).map_err(|e| e.to_string())?;
        }
        cy += core;
        progress(cy as f32 / h as f32);
    }
    for wr in writers {
        wr.finish().map_err(|e| e.to_string())?;
    }
    Ok(())
}
