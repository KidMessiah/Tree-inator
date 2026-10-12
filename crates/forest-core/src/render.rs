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
    // only the crown height spread and the weave change how trees are drawn
    gq.depth = DepthStyle { height_var: p.gen.depth.height_var, weave: p.gen.depth.weave, ..d.depth.clone() };
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
    // smoothing reads pixels around each one, which must be shaded exactly too
    let smooth = crate::smooth::radius(g.depth.smooth, 0.5 * (g.crown_min + g.crown_max) * s);
    let reach = (3.0 * blur + 12.0).ceil() as usize + smooth;
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

/// How an export is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ExportFormat {
    /// PNG, quick to write.
    #[default]
    Png,
    /// PNG with the best compression: a smaller file, slower to write.
    PngSmall,
    /// A folder of PNG tiles, 1024 px square, with a small index file, for very large maps and for web or table
    /// top programs that load maps in tiles. Written strip by strip, so any size fits in memory.
    Tiles,
    /// Lossless WebP: usually well under half the size of the PNG. The whole picture is held in memory.
    Webp,
    /// JPEG at high quality, without transparency. The whole picture is held in memory.
    Jpeg,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 5] = [ExportFormat::Png, ExportFormat::PngSmall, ExportFormat::Tiles, ExportFormat::Webp, ExportFormat::Jpeg];

    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::PngSmall => "PNG, smallest file",
            ExportFormat::Tiles => "PNG tiles (folder)",
            ExportFormat::Webp => "WebP, lossless",
            ExportFormat::Jpeg => "JPEG",
        }
    }

    /// File extension, or None for the tile folder.
    pub fn extension(&self) -> Option<&'static str> {
        match self {
            ExportFormat::Png | ExportFormat::PngSmall => Some("png"),
            ExportFormat::Tiles => None,
            ExportFormat::Webp => Some("webp"),
            ExportFormat::Jpeg => Some("jpg"),
        }
    }
}

/// Where the rows of an export go.
trait Sink {
    fn rows(&mut self, rgba: &[u8], n: usize) -> Result<(), String>;
    fn finish(self: Box<Self>) -> Result<(), String>;
}

struct PngSink(png::StreamWriter<'static, std::io::BufWriter<std::fs::File>>);

impl Sink for PngSink {
    fn rows(&mut self, rgba: &[u8], _n: usize) -> Result<(), String> {
        self.0.write_all(rgba).map_err(|e| e.to_string())
    }
    fn finish(self: Box<Self>) -> Result<(), String> {
        self.0.finish().map_err(|e| e.to_string())
    }
}

/// The picture is gathered whole, then encoded in one go.
struct WholeSink {
    path: PathBuf,
    w: usize,
    h: usize,
    format: ExportFormat,
    data: Vec<u8>,
}

/// More than this many pixels will not be gathered whole.
const WHOLE_MAX_PX: usize = 120_000_000;

impl Sink for WholeSink {
    fn rows(&mut self, rgba: &[u8], _n: usize) -> Result<(), String> {
        self.data.extend_from_slice(rgba);
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<(), String> {
        let file = std::io::BufWriter::new(std::fs::File::create(&self.path).map_err(|e| format!("{}: {e}", self.path.display()))?);
        match self.format {
            ExportFormat::Webp => image::codecs::webp::WebPEncoder::new_lossless(file)
                .encode(&self.data, self.w as u32, self.h as u32, image::ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string()),
            _ => {
                // JPEG has no transparency: blend over white
                let rgb: Vec<u8> = self
                    .data
                    .chunks_exact(4)
                    .flat_map(|p| {
                        let a = p[3] as u32;
                        [0, 1, 2].map(|k| ((p[k] as u32 * a + 255 * (255 - a) + 127) / 255) as u8)
                    })
                    .collect();
                image::codecs::jpeg::JpegEncoder::new_with_quality(file, 94)
                    .encode(&rgb, self.w as u32, self.h as u32, image::ExtendedColorType::Rgb8)
                    .map_err(|e| e.to_string())
            }
        }
    }
}

/// A folder of 1024 px PNG tiles, written a band of rows at a time.
struct TileSink {
    dir: PathBuf,
    stem: String,
    w: usize,
    h: usize,
    band: Vec<u8>,
    rows_in_band: usize,
    band_index: usize,
    written: Vec<(usize, usize, usize, usize)>,
}

const TILE: usize = 1024;

impl TileSink {
    fn flush_band(&mut self) -> Result<(), String> {
        if self.rows_in_band == 0 {
            return Ok(());
        }
        let cols = self.w.div_ceil(TILE);
        for c in 0..cols {
            let x0 = c * TILE;
            let tw = TILE.min(self.w - x0);
            let mut tile = Vec::with_capacity(tw * self.rows_in_band * 4);
            for r in 0..self.rows_in_band {
                let from = (r * self.w + x0) * 4;
                tile.extend_from_slice(&self.band[from..from + tw * 4]);
            }
            let path = self.dir.join(format!("{}_x{}_y{}.png", self.stem, c, self.band_index));
            let file = std::io::BufWriter::new(std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?);
            let mut enc = png::Encoder::new(file, tw as u32, self.rows_in_band as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Fast);
            let mut wr = enc.write_header().map_err(|e| e.to_string())?;
            wr.write_image_data(&tile).map_err(|e| e.to_string())?;
            self.written.push((c, self.band_index, tw, self.rows_in_band));
        }
        self.band.clear();
        self.rows_in_band = 0;
        self.band_index += 1;
        Ok(())
    }
}

impl Sink for TileSink {
    fn rows(&mut self, rgba: &[u8], n: usize) -> Result<(), String> {
        let mut done = 0;
        while done < n {
            let take = (TILE - self.rows_in_band).min(n - done);
            self.band.extend_from_slice(&rgba[done * self.w * 4..(done + take) * self.w * 4]);
            self.rows_in_band += take;
            done += take;
            if self.rows_in_band == TILE {
                self.flush_band()?;
            }
        }
        Ok(())
    }
    fn finish(mut self: Box<Self>) -> Result<(), String> {
        self.flush_band()?;
        let index = serde_json::json!({
            "width": self.w,
            "height": self.h,
            "tile": TILE,
            "columns": self.w.div_ceil(TILE),
            "rows": self.h.div_ceil(TILE),
            "tiles": self.written.iter().map(|(c, r, tw, th)| serde_json::json!({"file": format!("{}_x{}_y{}.png", self.stem, c, r), "x": c * TILE, "y": r * TILE, "w": tw, "h": th})).collect::<Vec<_>>(),
        });
        std::fs::write(self.dir.join(format!("{}_tiles.json", self.stem)), serde_json::to_string_pretty(&index).unwrap_or_default()).map_err(|e| e.to_string())
    }
}

fn make_sink(path: &PathBuf, w: usize, h: usize, format: ExportFormat) -> Result<Box<dyn Sink>, String> {
    match format {
        ExportFormat::Png | ExportFormat::PngSmall => {
            let file = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?);
            let mut enc = png::Encoder::new(file, w as u32, h as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(if format == ExportFormat::PngSmall { png::Compression::High } else { png::Compression::Fast });
            let wr = enc.write_header().map_err(|e| e.to_string())?;
            Ok(Box::new(PngSink(wr.into_stream_writer().map_err(|e| e.to_string())?)))
        }
        ExportFormat::Tiles => {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "map".into());
            let dir = path.with_extension("");
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            Ok(Box::new(TileSink { dir, stem, w, h, band: Vec::new(), rows_in_band: 0, band_index: 0, written: Vec::new() }))
        }
        ExportFormat::Webp | ExportFormat::Jpeg => {
            if w * h > WHOLE_MAX_PX {
                return Err(format!("{} px is too large for {}: use PNG or tiles", w * h, format.label()));
            }
            Ok(Box::new(WholeSink { path: path.clone(), w, h, format, data: Vec::with_capacity(w * h * 4) }))
        }
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
    export_strips_as(project, scale, outputs, ExportFormat::Png, cancelled, progress)
}

/// `export_strips` in a chosen file format.
pub fn export_strips_as(
    project: &Project,
    scale: f32,
    outputs: &[(PathBuf, ComposeOptions)],
    format: ExportFormat,
    cancelled: &(dyn Fn() -> bool + Sync),
    progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let passes = zone_passes(project);
    let many = passes.len() > 1;
    export_with(&passes, scale, outputs, format, cancelled, progress, &mut |pi, plan, region| {
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
    export_strips_gpu_as(gpu, project, scale, outputs, ExportFormat::Png, cancelled, progress)
}

/// `export_strips_gpu` in a chosen file format.
#[cfg(feature = "gpu")]
pub fn export_strips_gpu_as(
    gpu: &crate::gpu::Gpu,
    project: &Project,
    scale: f32,
    outputs: &[(PathBuf, ComposeOptions)],
    format: ExportFormat,
    cancelled: &(dyn Fn() -> bool + Sync),
    progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let passes = zone_passes(project);
    let many = passes.len() > 1;
    export_with(&passes, scale, outputs, format, cancelled, progress, &mut |pi, plan, region| {
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
    format: ExportFormat,
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

    let mut writers: Vec<Box<dyn Sink>> = Vec::new();
    for (path, _) in outputs {
        writers.push(make_sink(path, w, h, format)?);
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
            wr.rows(&rgba[skip..skip + core * w * 4], core)?;
        }
        cy += core;
        progress(cy as f32 / h as f32);
    }
    for wr in writers {
        wr.finish()?;
    }
    Ok(())
}
