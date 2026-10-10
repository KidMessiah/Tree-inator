mod background;
mod panel;
mod simple;
mod theme_cards;
mod zones;

use crate::worker::{self, Job, Msg, Variants};
use eframe::egui::{self, Color32, Key, PointerButton, Pos2, Rect, Sense, Stroke, Vec2};
use forest_core::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Lasso,
    LassoCut,
    Polygon,
    PolygonCut,
    Pan,
    /// Select and edit the drawn zones.
    Zones,
    TreeBrush,
    TreeStamp,
    TreeErase,
}

impl Tool {
    fn is_brush(self) -> bool {
        matches!(self, Tool::TreeBrush | Tool::TreeStamp | Tool::TreeErase)
    }

    fn op(self) -> ShapeOp {
        match self {
            Tool::LassoCut | Tool::PolygonCut => ShapeOp::Subtract,
            _ => ShapeOp::Add,
        }
    }
}

/// Everything the user edits by hand, for undo.
#[derive(Clone)]
struct Snap {
    shapes: Vec<Shape>,
    groups: Vec<Group>,
    painted: Vec<TreeRec>,
    erased: Vec<[f32; 3]>,
}

/// Percent slider for a 0..1 value.
fn pct_slider(ui: &mut egui::Ui, v: &mut f32, max: f32, name: &str, tip: &str) {
    let mut p = *v * 100.0;
    if ui.add(egui::Slider::new(&mut p, 0.0..=max).suffix("%").text(name)).on_hover_text(tip).changed() {
        *v = p / 100.0;
    }
}

/// A self test run from the environment: TREE_INATOR_OPEN project.json, TREE_INATOR_ZOOM 1.0 and
/// TREE_INATOR_SHOT out.png. The frame is read back from the app's own renderer, not from the screen.
struct Shot {
    path: PathBuf,
    zoom: Option<f32>,
    center: Option<[f32; 2]>,
    stable: u32,
    asked: bool,
    placed: bool,
}

impl Shot {
    fn from_env() -> Option<Shot> {
        let path = std::env::var_os("TREE_INATOR_SHOT")?;
        let num = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
        let center = match (num("TREE_INATOR_CX"), num("TREE_INATOR_CY")) {
            (Some(x), Some(y)) => Some([x, y]),
            _ => None,
        };
        Some(Shot { path: PathBuf::from(path), zoom: num("TREE_INATOR_ZOOM"), center, stable: 0, asked: false, placed: false })
    }
}

/// Where a GPU render sits on the map.
/// One extra layer on the GPU: its layers, the composited picture and the texture showing it.
struct ExtraGpu {
    layers: Arc<forest_core::gpu::GpuLayers>,
    image: Option<forest_core::gpu::GpuImage>,
    texture: Option<egui::TextureId>,
}

struct GpuView {
    scale: f32,
    origin: [usize; 2],
    size: [usize; 2],
    core: [f32; 4],
}

pub struct TreeApp {
    project: Project,
    project_path: Option<PathBuf>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    brush: BrushSettings,
    paint_rng: forest_core::rng::Rng,
    last_dab: Option<[f32; 2]>,
    paint_rev: u64,
    last_job: Instant,
    /// Painted trees already in the preview; newer ones are drawn as outlines until it refreshes.
    painted_rendered: usize,
    selected_species: usize,
    tool: Tool,
    stroke: Vec<[f32; 2]>,

    zoom: f32,
    pan: Vec2,
    fit_pending: bool,

    preview_scale: f32,
    layers: Option<Arc<RenderLayers>>,
    texture: Option<egui::TextureHandle>,
    gen_key: String,
    grade_key: String,
    gen_dirty: bool,
    compose_dirty: bool,
    last_edit: Instant,

    job_id: u64,
    latest: Arc<AtomicU64>,
    tx: Sender<Job>,
    rx: Receiver<Msg>,
    rendering: bool,
    render_ms: u128,
    status: String,

    background: [f32; 3],
    show_outlines: bool,
    selected_layer: usize,
    map_size_edit: [u32; 2],

    export_scale: f32,
    variants: Variants,
    /// The window's GPU renderer, when the graphics backend allows it.
    gpu: Option<Arc<forest_core::gpu::Gpu>>,
    render_state: Option<eframe::egui_wgpu::RenderState>,
    gpu_layers: Option<Arc<forest_core::gpu::GpuLayers>>,
    gpu_image: Option<forest_core::gpu::GpuImage>,
    gpu_texture: Option<egui::TextureId>,
    /// What the GPU image covers: its scale, top left in render pixels and the map rectangle it is exact for.
    gpu_view: Option<GpuView>,
    /// The picture under the forest, as tiles, and the path it was loaded from.
    bg_tiles: Vec<(egui::TextureHandle, [f32; 4])>,
    bg_loaded: Option<String>,
    /// Layers of zones with their own settings, drawn over the main picture, bottom first.
    extra_gpu: Vec<ExtraGpu>,
    extra_cpu: Vec<(Arc<RenderLayers>, Option<egui::TextureHandle>)>,
    /// The layer whose settings are swapped into the project while the settings panel is drawn.
    edit_node: Option<u32>,
    /// The project settings and species, put aside while the panel shows a layer or group.
    global_stash: Option<(GenParams, Vec<Species>)>,
    /// What the panel started the frame with, to find what the user changed.
    edit_before: Option<(GenParams, Vec<Species>)>,
    /// What the node inherits, so the panel marks and resets against it instead of the defaults.
    pub(super) gen_base: Option<GenParams>,
    /// Settings the user reset this frame; they go back to inherited.
    pub(super) pending_reset: Vec<String>,
    /// The layer picked in the layers list, whose settings the panel edits.
    group_sel: Option<u32>,
    prev_zone_sel: Option<usize>,
    bg_rx: Option<Receiver<Result<background::Tiles, String>>>,
    zone_sel: Option<usize>,
    search: String,
    theme_tex: std::collections::HashMap<&'static str, egui::TextureHandle>,
    zone_drag: Option<zones::ZoneDrag>,
    /// Self test: open a project, set a zoom, save the app's own frame to a PNG and quit.
    shot: Option<Shot>,
    /// The part of the map on screen (x0, y0, x1, y1), set while drawing the canvas.
    visible: Option<[f32; 4]>,
    canvas_rect: Option<Rect>,
    /// Render pixels per screen pixel for the GPU preview.
    view_detail: f32,
    pixels_per_point: f32,
    /// Pull-in and tree counts of the last render.
    render_info: Option<(f32, [usize; 4])>,
    /// Simple mode: a short list of choices instead of the tabs of sliders.
    /// The theme last chosen, for its label.
    theme: Option<&'static str>,
}

impl TreeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (job_tx, job_rx) = channel();
        let (msg_tx, msg_rx) = channel();
        let latest = Arc::new(AtomicU64::new(0));
        let render_state = cc.wgpu_render_state.clone();
        let gpu = render_state.as_ref().map(|rs| Arc::new(forest_core::gpu::Gpu::new(rs.device.clone(), rs.queue.clone())));
        worker::spawn(job_rx, msg_tx, latest.clone(), cc.egui_ctx.clone(), gpu.clone());
        let mut project = Project::default();
        apply_user_defaults(&mut project);
        project.ensure_mushroom_species();
        forest_core::treegen_sprite::prebake_in_background();
        if let Some(p) = std::env::var_os("TREE_INATOR_OPEN") {
            if let Ok(loaded) = std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Project>(&t).map_err(|e| e.to_string())) {
                project = loaded;
                project.migrate_groups();
            }
        }
        TreeApp {
            map_size_edit: [project.map_w, project.map_h],
            project,
            project_path: None,
            undo: Vec::new(),
            redo: Vec::new(),
            tool: Tool::Lasso,
            brush: BrushSettings::default(),
            paint_rng: forest_core::rng::Rng::new(0x7EE5),
            last_dab: None,
            paint_rev: 0,
            last_job: Instant::now(),
            painted_rendered: 0,
            selected_species: 0,
            stroke: Vec::new(),
            zoom: 0.5,
            pan: Vec2::ZERO,
            fit_pending: true,
            preview_scale: 0.5,
            layers: None,
            texture: None,
            gen_key: String::new(),
            grade_key: String::new(),
            gen_dirty: false,
            compose_dirty: false,
            last_edit: Instant::now(),
            job_id: 0,
            latest,
            tx: job_tx,
            rx: msg_rx,
            rendering: false,
            render_ms: 0,
            status: "Draw a shape with the lasso to grow a forest.".into(),
            background: [214.0 / 255.0, 205.0 / 255.0, 180.0 / 255.0],
            show_outlines: true,
            selected_layer: LayerKind::Canopy.index(),
            export_scale: 1.0,
            variants: Variants { main: true, no_shadow: false, shadow_only: false },
            theme: None,
            gpu,
            render_state,
            gpu_layers: None,
            gpu_image: None,
            gpu_texture: None,
            gpu_view: None,
            bg_tiles: Vec::new(),
            bg_loaded: None,
            extra_gpu: Vec::new(),
            extra_cpu: Vec::new(),
            edit_node: None,
            global_stash: None,
            edit_before: None,
            gen_base: None,
            pending_reset: Vec::new(),
            group_sel: None,
            prev_zone_sel: None,
            bg_rx: None,
            zone_sel: None,
            search: String::new(),
            theme_tex: Default::default(),
            zone_drag: None,
            shot: Shot::from_env(),
            visible: None,
            canvas_rect: None,
            view_detail: 1.0,
            pixels_per_point: 1.0,
            render_info: None,
        }
    }

    fn snap(&self) -> Snap {
        Snap { shapes: self.project.shapes.clone(), groups: self.project.groups.clone(), painted: self.project.painted.clone(), erased: self.project.erased.clone() }
    }

    fn restore(&mut self, s: Snap) -> Snap {
        let old = self.snap();
        self.project.shapes = s.shapes;
        self.project.groups = s.groups;
        self.project.painted = s.painted;
        self.project.erased = s.erased;
        self.paint_rev += 1;
        old
    }

    /// The project as it is saved and rendered: while the panel shows a layer or group, the project settings
    /// are the ones put aside.
    fn real_project(&self) -> Project {
        let mut p = self.project.clone();
        if self.edit_node.is_some() {
            if let Some((g, sp)) = &self.global_stash {
                p.gen = g.clone();
                p.species = sp.clone();
            }
        }
        p
    }

    /// The layer or group whose settings the panel edits: the one picked in the list, or the layer of the
    /// selected zone.
    fn editing_node(&self) -> Option<u32> {
        let id = self.group_sel.or_else(|| self.zone_sel.and_then(|i| self.project.shapes.get(i)).and_then(|s| s.group))?;
        self.project.group(id).map(|n| n.id)
    }

    /// Draw the settings panel for a layer or group, or for the project when none is picked.
    ///
    /// The node's settings are put in the project for the panel to edit, so every control works unchanged, and
    /// the project's own are put aside. Afterwards only what the user changed is kept on the node, and the
    /// rest keeps following whatever the node inherits.
    fn with_zone_settings(&mut self, ui: &mut egui::Ui) {
        // picking a zone hands the panel to that zone's layer
        if self.zone_sel != self.prev_zone_sel {
            self.group_sel = None;
            self.prev_zone_sel = self.zone_sel;
        }
        let Some(id) = self.editing_node() else {
            self.gen_base = None;
            self.settings_panel(ui);
            return;
        };
        let (gen, species) = self.project.effective(id);
        let (parent_gen, _) = self.project.inherited(id);
        let before = (gen.clone(), species.clone());
        self.global_stash = Some((std::mem::replace(&mut self.project.gen, gen), std::mem::replace(&mut self.project.species, species)));
        self.edit_before = Some(before);
        self.gen_base = Some(parent_gen);
        self.pending_reset.clear();
        self.edit_node = Some(id);

        let node = self.project.group(id).cloned().unwrap_or_default();
        let inner = self.project.descendants(id).len();
        ui.label(egui::RichText::new(format!("Editing the group \"{}\"", node.name)).strong().color(Color32::from_rgb(120, 190, 255)));
        let who = if inner > 0 { format!("Changes here reach its zones and the {inner} groups inside it that have not set the same setting themselves.") } else { "Changes here affect the zones in this group.".to_string() };
        ui.small(format!("{who} A dot marks a setting changed here. Light, grading and map size are shared. Pick a zone in no group to edit the project."));
        ui.separator();

        self.settings_panel(ui);

        // put the project back, and keep on the node what the user changed
        self.edit_node = None;
        self.gen_base = None;
        let (after_gen, after_species) = (std::mem::take(&mut self.project.gen), std::mem::take(&mut self.project.species));
        if let Some((g, sp)) = self.global_stash.take() {
            self.project.gen = g;
            self.project.species = sp;
        }
        let resets = std::mem::take(&mut self.pending_reset);
        if let Some((before_gen, before_species)) = self.edit_before.take() {
            self.project.record_edit(id, (&before_gen, &before_species), (&after_gen, &after_species), &resets);
        }
    }

    /// Keep the textures of the layers that stay, free those that go.
    fn set_extra_gpu(&mut self, layers: Vec<Arc<forest_core::gpu::GpuLayers>>) {
        while self.extra_gpu.len() > layers.len() {
            if let (Some(e), Some(rs)) = (self.extra_gpu.pop(), &self.render_state) {
                if let Some(id) = e.texture {
                    rs.renderer.write().free_texture(&id);
                }
            }
        }
        for (k, l) in layers.into_iter().enumerate() {
            match self.extra_gpu.get_mut(k) {
                Some(e) => e.layers = l,
                None => self.extra_gpu.push(ExtraGpu { layers: l, image: None, texture: None }),
            }
        }
    }

    fn set_extra_cpu(&mut self, layers: Vec<Arc<RenderLayers>>) {
        self.extra_cpu.truncate(layers.len());
        for (k, l) in layers.into_iter().enumerate() {
            match self.extra_cpu.get_mut(k) {
                Some(e) => e.0 = l,
                None => self.extra_cpu.push((l, None)),
            }
        }
    }

    fn push_undo(&mut self) {
        let s = self.snap();
        self.undo.push(s);
        self.redo.clear();
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
    }

    fn add_shape(&mut self, points: Vec<[f32; 2]>, op: ShapeOp) {
        if points.len() < 3 {
            return;
        }
        self.push_undo();
        let group = self.placement_group();
        self.project.add_zone(op, points, group);
    }

    /// Random elongated blob with lobes, centred on the map.
    fn generate_patch(&mut self) {
        let seed = self.project.gen.seed.wrapping_mul(0x9E37_79B9).wrapping_add(self.project.shapes.len() as u64);
        let mut rng = forest_core::rng::Rng::new(seed);
        let (mw, mh) = (self.project.map_w as f32, self.project.map_h as f32);
        let (cx, cy) = (mw * rng.range(0.42, 0.58), mh * rng.range(0.42, 0.58));
        let r = mw.min(mh) * rng.range(0.25, 0.36);
        let stretch = rng.range(1.1, 2.0);
        let rot = rng.range(0.0, std::f32::consts::PI);
        let harmonics: Vec<(f32, f32)> = (2..7).map(|k| (rng.range(0.1, 0.32) / (k as f32 * 0.6), rng.range(0.0, std::f32::consts::TAU))).collect();
        let n = 128;
        let pts = (0..n)
            .map(|i| {
                let t = i as f32 / n as f32 * std::f32::consts::TAU;
                let mut rr = 1.0;
                for (k, (a, ph)) in harmonics.iter().enumerate() {
                    rr += a * ((k as f32 + 2.0) * t + ph).cos();
                }
                let (x, y) = (t.cos() * r * rr * stretch, t.sin() * r * rr);
                let (x, y) = (x * rot.cos() - y * rot.sin(), x * rot.sin() + y * rot.cos());
                [cx + x, cy + y]
            })
            .collect::<Vec<[f32; 2]>>();
        // Shrink to fit inside the map with a margin, so the outline is never cut by the map edge.
        let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
        let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
        let k = (0.8 * mw / (x1 - x0)).min(0.8 * mh / (y1 - y0)).min(1.0);
        let (bx, by) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
        let pts = pts
            .iter()
            .map(|p| [mw * 0.5 + (p[0] - bx) * k, mh * 0.5 + (p[1] - by) * k])
            .collect();
        self.add_shape(pts, ShapeOp::Add);
    }

    fn fill_map(&mut self) {
        let (mw, mh) = (self.project.map_w as f32, self.project.map_h as f32);
        let m = mw.min(mh) * 0.06;
        self.add_shape(vec![[m, m], [mw - m, m], [mw - m, mh - m], [m, mh - m]], ShapeOp::Add);
    }

    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            let old = self.restore(s);
            self.redo.push(old);
        }
    }

    fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            let old = self.restore(s);
            self.undo.push(old);
        }
    }

    fn map_to_screen(&self, origin: Pos2, p: [f32; 2]) -> Pos2 {
        origin + self.pan + Vec2::new(p[0], p[1]) * self.zoom
    }

    fn screen_to_map(&self, origin: Pos2, p: Pos2) -> [f32; 2] {
        let v = (p - origin - self.pan) / self.zoom;
        [v.x, v.y]
    }

    // ---------------------------------------------------------------- state sync

    fn poll_worker(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Preview { layers, extra, id, ms } => {
                    if id == self.job_id {
                        self.rendering = false;
                    }
                    self.render_info = Some((layers.fit_inset, sum_counts(layers.tree_counts, extra.iter().map(|e| e.tree_counts))));
                    self.layers = Some(layers);
                    self.set_extra_cpu(extra);
                    self.gpu_layers = None;
                    self.render_ms = ms;
                    self.compose_dirty = true;
                }
                Msg::GpuPreview { layers, extra, id, ms } => {
                    if id == self.job_id {
                        self.rendering = false;
                    }
                    self.render_info = Some((layers.fit_inset, sum_counts(layers.tree_counts, extra.iter().map(|e| e.tree_counts))));
                    self.set_extra_gpu(extra);
                    self.gpu_layers = Some(layers);
                    self.layers = None;
                    self.render_ms = ms;
                    self.compose_dirty = true;
                }
                Msg::GpuView { layers, extra, id, ms, scale, origin, core } => {
                    if id == self.job_id {
                        self.rendering = false;
                    }
                    self.render_info = Some((layers.fit_inset, sum_counts(layers.tree_counts, extra.iter().map(|e| e.tree_counts))));
                    self.set_extra_gpu(extra);
                    self.gpu_view = Some(GpuView { scale, origin, size: [layers.w, layers.h], core });
                    self.gpu_layers = Some(layers);
                    self.layers = None;
                    self.render_ms = ms;
                    self.compose_dirty = true;
                }
                Msg::Status(s) => self.status = s,
                Msg::Exported(Ok(paths)) => {
                    let names: Vec<_> = paths.iter().map(|p| p.display().to_string()).collect();
                    self.status = format!("Exported {}", names.join(", "));
                }
                Msg::Exported(Err(e)) => self.status = format!("Export failed: {e}"),
            }
        }

        // Detect edits by comparing serialised state. Cheap at these sizes and never misses a widget.
        let gen_key = format!("{}{}", self.project.render_key(), self.paint_rev);
        if gen_key != self.gen_key {
            self.gen_key = gen_key;
            self.gen_dirty = true;
            self.last_edit = Instant::now();
        }
        let grade_key = serde_json::to_string(&self.project.grading).unwrap_or_default() + &format!("{:?}{}", self.background, self.background_showing());
        if grade_key != self.grade_key {
            self.grade_key = grade_key;
            self.compose_dirty = true;
        }

        // Screen space rendering: redraw when the zoom needs more detail or the view leaves what was drawn.
        if self.gpu.is_some() && !self.rendering && !self.gen_dirty && self.view_stale() {
            self.gen_dirty = true;
            self.last_edit = Instant::now();
        }

        let painting = self.last_dab.is_some() && self.last_job.elapsed() > Duration::from_millis(350);
        if self.gen_dirty && (self.last_edit.elapsed() > Duration::from_millis(120) || painting) {
            self.gen_dirty = false;
            self.last_job = Instant::now();
            self.painted_rendered = self.project.painted.len();
            self.job_id += 1;
            self.latest.store(self.job_id, Ordering::Relaxed);
            self.rendering = true;
            if self.gpu.is_some() {
                let (scale, view) = self.wanted_view();
                let _ = self.tx.send(Job::View { project: self.real_project(), scale, view, id: self.job_id });
            } else {
                let _ = self.tx.send(Job::Preview { project: self.real_project(), scale: self.preview_scale, id: self.job_id });
            }
        } else if self.gen_dirty {
            ctx.request_repaint_after(Duration::from_millis(130));
        }

        if self.compose_dirty {
            if let (Some(g), Some(l), Some(rs)) = (&self.gpu, &self.gpu_layers, &self.render_state) {
                // composite straight into the shown texture, no copy back to the CPU
                let opts = self.preview_options();
                let before = self.gpu_image.as_ref().map(|i| (i.w, i.h));
                g.compose_image(l, &self.project.grading, opts, &mut self.gpu_image);
                let img = self.gpu_image.as_ref().unwrap();
                // the lock is let go before the layers below ask for it again
                {
                    let mut renderer = rs.renderer.write();
                    match self.gpu_texture {
                        Some(id) if before == Some((img.w, img.h)) => {
                            let _ = id;
                        }
                        Some(id) => renderer.update_egui_texture_from_wgpu_texture(&rs.device, &img.view, eframe::wgpu::FilterMode::Linear, id),
                        None => self.gpu_texture = Some(renderer.register_native_texture(&rs.device, &img.view, eframe::wgpu::FilterMode::Linear)),
                    }
                }
                // zones with their own settings sit over it, with transparency
                let over = ComposeOptions { shadows: true, shadow_only: false, background: None, premultiplied: true };
                for e in self.extra_gpu.iter_mut() {
                    let before = e.image.as_ref().map(|i| (i.w, i.h));
                    g.compose_image(&e.layers, &self.project.grading, over, &mut e.image);
                    let img = e.image.as_ref().unwrap();
                    let mut renderer = rs.renderer.write();
                    match e.texture {
                        Some(_) if before == Some((img.w, img.h)) => {}
                        Some(id) => renderer.update_egui_texture_from_wgpu_texture(&rs.device, &img.view, eframe::wgpu::FilterMode::Linear, id),
                        None => e.texture = Some(renderer.register_native_texture(&rs.device, &img.view, eframe::wgpu::FilterMode::Linear)),
                    }
                }
                ctx.request_repaint();
            } else if let Some(l) = &self.layers {
                let opts = self.preview_options();
                let rgba = compose(l, &self.project.grading, opts);
                let img = egui::ColorImage::from_rgba_premultiplied([l.w, l.h], &rgba);
                match &mut self.texture {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => self.texture = Some(ctx.load_texture("preview", img, egui::TextureOptions::LINEAR)),
                }
                let over = ComposeOptions { shadows: true, shadow_only: false, background: None, premultiplied: true };
                for (l, tex) in self.extra_cpu.iter_mut() {
                    let rgba = compose(l, &self.project.grading, over);
                    let img = egui::ColorImage::from_rgba_premultiplied([l.w, l.h], &rgba);
                    match tex {
                        Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                        None => *tex = Some(ctx.load_texture("zone layer", img, egui::TextureOptions::LINEAR)),
                    }
                }
            }
            self.compose_dirty = false;
        }
    }

    /// Render pixels per map pixel for the current zoom: exactly one render pixel per screen pixel, so the
    /// picture is not resampled when shown. At least a quarter, so a zoomed out forest still has crowns.
    fn wanted_scale(&self) -> f32 {
        (self.zoom * self.pixels_per_point * self.view_detail).clamp(0.25, 4.0)
    }

    /// The scale and map rectangle to render: what is on screen plus a margin to pan into.
    fn wanted_view(&self) -> (f32, [f32; 4]) {
        let (mw, mh) = (self.project.map_w as f32, self.project.map_h as f32);
        let v = self.visible.unwrap_or([0.0, 0.0, mw, mh]);
        let (dx, dy) = ((v[2] - v[0]) * 0.25, (v[3] - v[1]) * 0.25);
        (self.wanted_scale(), [(v[0] - dx).max(0.0), (v[1] - dy).max(0.0), (v[2] + dx).min(mw), (v[3] + dy).min(mh)])
    }

    fn view_stale(&self) -> bool {
        let Some(v) = &self.gpu_view else { return true };
        let Some(vis) = self.visible else { return false };
        if (v.scale / self.wanted_scale() - 1.0).abs() > 0.004 {
            return true;
        }
        let (mw, mh) = (self.project.map_w as f32, self.project.map_h as f32);
        let vis = [vis[0].max(0.0), vis[1].max(0.0), vis[2].min(mw), vis[3].min(mh)];
        if vis[0] >= vis[2] || vis[1] >= vis[3] {
            return false;
        }
        vis[0] < v.core[0] - 0.5 || vis[1] < v.core[1] - 0.5 || vis[2] > v.core[2] + 0.5 || vis[3] > v.core[3] + 0.5
    }

    /// Drive the self test: wait for the preview to settle, ask for a frame, save it and close.
    fn run_shot(&mut self, ctx: &egui::Context) {
        if self.shot.is_none() {
            return;
        }
        let ready = !self.rendering && !self.gen_dirty && self.gpu_view.is_some() && !self.view_stale() && self.canvas_rect.is_some();
        let (rect, size) = (self.canvas_rect, Vec2::new(self.project.map_w as f32, self.project.map_h as f32));
        let Some(shot) = &mut self.shot else { return };
        ctx.request_repaint();
        if let Some(img) = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        }) {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(out) = image::RgbaImage::from_raw(img.width() as u32, img.height() as u32, rgba) {
                let _ = out.save(&shot.path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if shot.asked {
            return;
        }
        // first frames: take the zoom and centre from the environment
        if let (Some(z), Some(rect)) = (shot.zoom, rect) {
            if !shot.placed {
                shot.placed = true;
                let c = shot.center.unwrap_or([size.x / 2.0, size.y / 2.0]);
                shot.stable = 0;
                self.fit_pending = false;
                self.zoom = z;
                self.pan = rect.size() * 0.5 - Vec2::new(c[0], c[1]) * z;
                return;
            }
        }
        shot.stable = if ready { shot.stable + 1 } else { 0 };
        if shot.stable > 12 {
            shot.asked = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (undo, redo, reroll, fit, esc, enter) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(Key::Z) && !i.modifiers.shift,
                i.modifiers.command && (i.key_pressed(Key::Y) || (i.modifiers.shift && i.key_pressed(Key::Z))),
                !i.modifiers.command && i.key_pressed(Key::R),
                !i.modifiers.command && i.key_pressed(Key::F),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Enter),
            )
        });
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if reroll {
            self.project.gen.seed = self.project.gen.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) % 1_000_000;
        }
        if fit {
            self.fit_pending = true;
        }
        if esc {
            self.stroke.clear();
        }
        if enter && matches!(self.tool, Tool::Polygon | Tool::PolygonCut) {
            let pts = std::mem::take(&mut self.stroke);
            self.add_shape(pts, self.tool.op());
        }
    }

    // ---------------------------------------------------------------- file actions

    fn new_project(&mut self) {
        self.push_undo();
        self.project.shapes.clear();
        self.project.groups.clear();
        self.group_sel = None;
        self.project.painted.clear();
        self.project.erased.clear();
        self.paint_rev += 1;
        self.project_path = None;
    }

    fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Tree-inator project", &["treeproj", "json"]).pick_file() else {
            return;
        };
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Project>(&s).map_err(|e| e.to_string())) {
            Ok(p) => {
                self.project = p;
                self.project.migrate_groups();
                self.group_sel = None;
                self.map_size_edit = [self.project.map_w, self.project.map_h];
                self.undo.clear();
                self.redo.clear();
                self.fit_pending = true;
                self.status = format!("Opened {}", path.display());
                self.project_path = Some(path);
            }
            Err(e) => self.status = format!("Open failed: {e}"),
        }
    }

    fn save_project(&mut self, save_as: bool) {
        let path = match (&self.project_path, save_as) {
            (Some(p), false) => p.clone(),
            _ => match rfd::FileDialog::new().add_filter("Tree-inator project", &["treeproj"]).set_file_name("forest.treeproj").save_file() {
                Some(p) => p,
                None => return,
            },
        };
        match serde_json::to_string_pretty(&self.real_project()).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                self.project_path = Some(path);
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    fn export(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("forest.png").save_file() else {
            return;
        };
        let _ = self.tx.send(Job::Export { project: self.real_project(), scale: self.export_scale, path, variants: self.variants });
        self.status = "Exporting...".into();
    }

    // ---------------------------------------------------------------- panels

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("New").clicked() {
                self.new_project();
            }
            if ui.button("Open").clicked() {
                self.open_project();
            }
            if ui.button("Save").clicked() {
                self.save_project(false);
            }
            if ui.button("Save as").clicked() {
                self.save_project(true);
            }
            ui.separator();
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).clicked() {
                self.undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).clicked() {
                self.redo();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Export PNG").clicked() {
                    self.export();
                }
            });
        });
    }

    /// One full-width tool button.
    fn tool_button(&mut self, ui: &mut egui::Ui, tool: Tool, label: &str) {
        let on = self.tool == tool;
        if ui.add_sized([ui.available_width(), 26.0], egui::Button::new(label).selected(on)).clicked() {
            self.tool = tool;
            self.stroke.clear();
        }
    }

    fn tool_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("DRAW FOREST").small().weak());
            self.tool_button(ui, Tool::Lasso, "Lasso: add");
            self.tool_button(ui, Tool::LassoCut, "Lasso: cut clearing");
            self.tool_button(ui, Tool::Polygon, "Polygon: add");
            self.tool_button(ui, Tool::PolygonCut, "Polygon: cut");
            self.tool_button(ui, Tool::Zones, "Edit zones");
            ui.add_space(6.0);
            ui.label(egui::RichText::new("PAINT TREES").small().weak());
            self.tool_button(ui, Tool::TreeBrush, "Tree brush");
            self.tool_button(ui, Tool::TreeStamp, "Single tree");
            self.tool_button(ui, Tool::TreeErase, "Tree eraser");
            ui.add_space(6.0);
            ui.label(egui::RichText::new("VIEW").small().weak());
            self.tool_button(ui, Tool::Pan, "Pan");
            ui.add_space(4.0);
            let hint = match self.tool {
                Tool::Lasso => "Drag a loop around the area. The forest grows when you let go.",
                Tool::LassoCut => "Drag a loop to cut an opening in the forest.",
                Tool::Polygon => "Click corners. Enter or right click closes the shape.",
                Tool::PolygonCut => "Click corners of a clearing. Enter or right click closes it.",
                Tool::Pan => "Drag to move the map.",
                Tool::Zones => "Click a zone to select it. Drag its corners or the zone itself. Double click an edge to add a corner, right click a corner to remove it. Delete removes the zone.",
                Tool::TreeBrush => "Drag to scatter trees. [ and ] change brush size.",
                Tool::TreeStamp => "Click to place one tree.",
                Tool::TreeErase => "Drag over trees to remove them.",
            };
            ui.small(hint);
            ui.small("Middle drag or Space+drag pans. Wheel zooms.");
            if self.tool.is_brush() {
                ui.separator();
                self.sec_brush(ui);
            }
            ui.separator();

            egui::CollapsingHeader::new("Map and view").default_open(true).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Map size");
                    ui.add(egui::DragValue::new(&mut self.map_size_edit[0]).range(64..=16384));
                    ui.label("x");
                    ui.add(egui::DragValue::new(&mut self.map_size_edit[1]).range(64..=16384));
                });
                if ui.button("Apply size").on_hover_text("Use the pixel size of the map you will place the forest on").clicked() {
                    self.project.map_w = self.map_size_edit[0];
                    self.project.map_h = self.map_size_edit[1];
                    self.fit_pending = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Fit (F)").clicked() {
                        self.fit_pending = true;
                    }
                    let mut z = self.zoom * 100.0;
                    if ui.add(egui::DragValue::new(&mut z).range(1.0..=800.0).speed(0.5).suffix("%")).changed() {
                        self.zoom = z / 100.0;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Background");
                    ui.color_edit_button_rgb(&mut self.background);
                });
                ui.checkbox(&mut self.show_outlines, "Show outlines");
                self.background_ui(ui);
            });
            ui.separator();
            self.zones_ui(ui);
            ui.separator();
            if ui.button("Clear painted trees").clicked() && !(self.project.painted.is_empty() && self.project.erased.is_empty()) {
                self.push_undo();
                self.project.painted.clear();
                self.project.erased.clear();
                self.paint_rev += 1;
            }
        });
    }

    // ---------------------------------------------------------------- tabs

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_size(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Size").default_open(true).show(ui, |ui| {
            let g = &mut self.project.gen;
            let mut pct = g.scale * 100.0;
            if ui.add(egui::Slider::new(&mut pct, 10.0..=400.0).logarithmic(true).suffix("%").text("Forest scale"))
                .on_hover_text("Makes every tree, leaf, shrub and shadow bigger or smaller on the same drawn area. Use more for higher elevations.")
                .changed()
            {
                g.scale = pct / 100.0;
            }
            ui.horizontal(|ui| {
                for v in [25.0, 50.0, 100.0, 200.0, 400.0] {
                    if ui.small_button(format!("{v:.0}%")).clicked() {
                        g.scale = v / 100.0;
                    }
                }
            });
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_patch_shape(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Patch shape").default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate patch").on_hover_text("Random natural outline in the middle of the map").clicked() {
                    self.generate_patch();
                }
                if ui.button("Fill map").clicked() {
                    self.fill_map();
                }
            });
            let g = &mut self.project.gen;
            egui::ComboBox::from_label("Preset").selected_text("Apply...").show_ui(ui, |ui| {
                for (name, f) in PRESETS {
                    if ui.selectable_label(false, *name).clicked() {
                        f(g);
                    }
                }
            });
            egui::ComboBox::from_label("Pattern").selected_text(g.pattern.name()).show_ui(ui, |ui| {
                for p in Pattern::ALL {
                    ui.selectable_value(&mut g.pattern, p, p.name());
                }
            });
            ui.add(egui::Slider::new(&mut g.pattern_strength, 0.0..=1.0).text("Strength"));
            ui.add(egui::Slider::new(&mut g.pattern_scale, 50.0..=1000.0).text("Pattern size"));
            if g.pattern == Pattern::Branching {
                ui.add(egui::Slider::new(&mut g.branch_width, 0.05..=1.0).text("Branch width"));
            }
            ui.separator();
            ui.checkbox(&mut g.contain, "Fit the whole forest inside the drawn area")
                .on_hover_text("Pulls the generation in so its fall-off, grass fringe and stray trees all end before the line you drew. Painted trees are exempt.");
            ui.add_enabled_ui(g.contain, |ui| {
                let mut pct = g.contain_fit * 100.0;
                if ui.add(egui::Slider::new(&mut pct, 0.0..=200.0).suffix("%").text("Pull in"))
                    .on_hover_text("100% pulls in exactly as far as needed. More leaves a margin, less lets the fringe reach the line.")
                    .changed()
                {
                    g.contain_fit = pct / 100.0;
                }
                if let Some((fit, _)) = &self.render_info {
                    ui.small(format!("Pulled in by {:.0} px", fit));
                }
                ui.checkbox(&mut g.contain_clip, "Trim anything still crossing the line")
                    .on_hover_text("A safety net. Turn off to keep the pulled-in edge fully natural.");
                ui.checkbox(&mut g.contain_shadows, "Keep shadows inside too")
                    .on_hover_text("Off lets shadows fall outside the area, the way a real forest's would");
            });
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_edges(&mut self, ui: &mut egui::Ui) {
        let g = &mut self.project.gen;
        egui::CollapsingHeader::new("Edges").default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Seed");
                ui.add(egui::DragValue::new(&mut g.seed));
                if ui.button("Reroll (R)").clicked() {
                    g.seed = (g.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)) % 1_000_000;
                }
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut g.edge_mode, EdgeMode::Realistic, "Realistic");
                ui.radio_value(&mut g.edge_mode, EdgeMode::Blurred, "Blurred");
            });
            if g.edge_mode == EdgeMode::Blurred {
                ui.add(egui::Slider::new(&mut g.feather, 0.05..=1.0).text("Feather"));
            }
            ui.add(egui::Slider::new(&mut g.shape_softness, 5.0..=200.0).text("Outline softness"));
            ui.add(egui::Slider::new(&mut g.edge_warp, 0.0..=300.0).text("Outline warp"));
            ui.add(egui::Slider::new(&mut g.edge_noise, 0.0..=0.6).text("Outline noise"));
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_canopy(&mut self, ui: &mut egui::Ui) {
        let g = &mut self.project.gen;
        egui::CollapsingHeader::new("Canopy").default_open(true).show(ui, |ui| {
            ui.add(egui::Slider::new(&mut g.canopy_cover, 0.05..=1.0).text("Coverage"));
            ui.add(egui::Slider::new(&mut g.canopy_patch_size, 30.0..=500.0).text("Patch size"));
            ui.add(egui::Slider::new(&mut g.canopy_edge_depth, 0.0..=1.0).text("Edge thinning"));
            ui.add(egui::Slider::new(&mut g.crown_min, 3.0..=40.0).text("Crown min radius"));
            ui.add(egui::Slider::new(&mut g.crown_max, 4.0..=60.0).text("Crown max radius"));
            ui.add(egui::Slider::new(&mut g.gaps, 0.0..=1.0).text("Treefall gaps"));
            ui.add(egui::Slider::new(&mut g.gap_size, 10.0..=120.0).text("Gap size"));
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_undergrowth_and_fringe(&mut self, ui: &mut egui::Ui) {
        let g = &mut self.project.gen;
        egui::CollapsingHeader::new("Undergrowth and fringe").default_open(false).show(ui, |ui| {
            ui.add(egui::Slider::new(&mut g.grass_reach, 0.0..=1.0).text("Grass reach"));
            ui.add(egui::Slider::new(&mut g.grass_density, 0.0..=3.0).text("Grass density"));
            ui.add(egui::Slider::new(&mut g.shrub_density, 0.0..=3.0).text("Shrubs"));
            ui.add(egui::Slider::new(&mut g.lone_groups, 0.0..=5.0).text("Lone tree groups"));
            ui.add(egui::Slider::new(&mut g.lone_reach, 0.0..=1.0).text("Lone tree reach"));
            ui.add(egui::Slider::new(&mut g.interior_lone, 0.0..=1.0).text("Lone trees inside"));
            ui.add(egui::Slider::new(&mut g.pebbles, 0.0..=5.0).text("Pebble shrubs"));
            ui.add(egui::Slider::new(&mut g.haze_reach, 0.1..=1.0).text("Haze reach"));
            ui.add(egui::Slider::new(&mut g.haze_opacity, 0.0..=0.8).text("Haze opacity"));
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_forest_type(&mut self, ui: &mut egui::Ui) {
        // With procedural trees on, lists show only species that have a tree type, labelled with it.
        let all = self.project.gen.procedural_forms;
        let names: Vec<String> = self
            .project
            .species
            .iter()
            .map(|s| match s.tree_type(all) {
                Some(t) => format!("{} ({})", s.name, treegen_sprite::tree_type_label(t)),
                None => s.name.clone(),
            })
            .collect();
        let shown: Vec<usize> = (0..names.len()).filter(|&i| !forest_core::mushrooms::is_mushroom(&self.project.species[i].preset) && (!all || self.project.species[i].tree_type(true).is_some())).collect();
        let name_of = |i: usize| names.get(i).cloned().unwrap_or_else(|| "?".into());

        egui::CollapsingHeader::new("Forest type").default_open(true).show(ui, |ui| {
            let g = &mut self.project.gen;
            ui.checkbox(&mut g.procedural_forms, "Procedural trees")
                .on_hover_text("Grow every species as its procedural tree type. Lists then show only those species.");
            ui.label("What the generated forest is made of");
            egui::ComboBox::from_label("Canopy and lone trees").selected_text(name_of(g.main_species as usize)).show_ui(ui, |ui| {
                for &i in &shown {
                    let n = &names[i];
                    ui.selectable_value(&mut g.main_species, i as u8, n);
                }
            });
            egui::ComboBox::from_label("Shrubs and pebbles").selected_text(name_of(g.shrub_species as usize)).show_ui(ui, |ui| {
                for &i in &shown {
                    let n = &names[i];
                    ui.selectable_value(&mut g.shrub_species, i as u8, n);
                }
            });
            ui.separator();
            ui.label("Mix a second species into the canopy");
            egui::ComboBox::from_id_salt("mix").selected_text(name_of(g.mix_species as usize)).show_ui(ui, |ui| {
                for &i in &shown {
                    let n = &names[i];
                    ui.selectable_value(&mut g.mix_species, i as u8, n);
                }
            });
            ui.add(egui::Slider::new(&mut g.mix_amount, 0.0..=1.0).text("Amount"));
            ui.add(egui::Slider::new(&mut g.mix_scale, 40.0..=800.0).text("Clump size"));
            ui.small("Painted trees use the brush species below.");
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_which_trees(&mut self, ui: &mut egui::Ui) {
        // With procedural trees on, lists show only species that have a tree type, labelled with it.
        let all = self.project.gen.procedural_forms;
        let names: Vec<String> = self
            .project
            .species
            .iter()
            .map(|s| match s.tree_type(all) {
                Some(t) => format!("{} ({})", s.name, treegen_sprite::tree_type_label(t)),
                None => s.name.clone(),
            })
            .collect();
        let shown: Vec<usize> = (0..names.len()).filter(|&i| !all || self.project.species[i].tree_type(true).is_some()).collect();
        let name_of = |i: usize| names.get(i).cloned().unwrap_or_else(|| "?".into());


        egui::CollapsingHeader::new("Which trees").default_open(false).show(ui, |ui| {
            let g = &mut self.project.gen;
            ui.horizontal(|ui| {
                ui.checkbox(&mut g.procedural_trees, "Generated");
                ui.checkbox(&mut g.painted_trees, "Painted");
            });
            ui.small(format!("{} painted trees, {} erased areas", self.project.painted.len(), self.project.erased.len()));
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_brush(&mut self, ui: &mut egui::Ui) {
        // With procedural trees on, lists show only species that have a tree type, labelled with it.
        let all = self.project.gen.procedural_forms;
        let names: Vec<String> = self
            .project
            .species
            .iter()
            .map(|s| match s.tree_type(all) {
                Some(t) => format!("{} ({})", s.name, treegen_sprite::tree_type_label(t)),
                None => s.name.clone(),
            })
            .collect();
        let shown: Vec<usize> = (0..names.len()).filter(|&i| !all || self.project.species[i].tree_type(true).is_some()).collect();
        let name_of = |i: usize| names.get(i).cloned().unwrap_or_else(|| "?".into());



        egui::CollapsingHeader::new("Brush").default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (t, l) in [(Tool::TreeBrush, "Paint"), (Tool::TreeStamp, "Single"), (Tool::TreeErase, "Erase")] {
                    if ui.selectable_label(self.tool == t, l).clicked() {
                        self.tool = t;
                    }
                }
            });
            let b = &mut self.brush;
            egui::ComboBox::from_label("Species").selected_text(name_of(b.species as usize)).show_ui(ui, |ui| {
                for &i in &shown {
                    let n = &names[i];
                    ui.selectable_value(&mut b.species, i as u8, n);
                }
            });
            egui::ComboBox::from_label("Layer").selected_text(b.layer.name()).show_ui(ui, |ui| {
                for l in TreeLayer::ALL {
                    if ui.selectable_value(&mut b.layer, l, l.name()).changed() {
                        // Sensible sizes per layer, matching the generator.
                        let (lo, hi) = match l {
                            TreeLayer::Canopy => (10.0, 17.0),
                            TreeLayer::Shrub => (4.0, 9.0),
                            TreeLayer::Lone => (8.0, 15.0),
                            TreeLayer::Pebble => (2.0, 4.5),
                        };
                        b.size_min = lo;
                        b.size_max = hi;
                    }
                }
            });
            ui.add(egui::Slider::new(&mut b.radius, 5.0..=400.0).text("Brush size"));
            ui.add(egui::Slider::new(&mut b.size_min, 1.0..=80.0).text("Tree size min"));
            ui.add(egui::Slider::new(&mut b.size_max, 1.0..=80.0).text("Tree size max"));
            if b.size_max < b.size_min {
                b.size_max = b.size_min;
            }
            ui.add(egui::Slider::new(&mut b.density, 0.0..=1.0).text("Density"));
            ui.add(egui::Slider::new(&mut b.tone_jitter, 0.0..=0.4).text("Colour variation"));
            ui.checkbox(&mut b.replace_generated, "Painting replaces generated trees");
            ui.checkbox(&mut b.erase_generated, "Eraser removes generated trees too");
            ui.checkbox(&mut b.erase_layer_only, "Eraser only affects this layer");
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_species_editor(&mut self, ui: &mut egui::Ui) {
        // With procedural trees on, lists show only species that have a tree type, labelled with it.
        let all = self.project.gen.procedural_forms;
        let names: Vec<String> = self
            .project
            .species
            .iter()
            .map(|s| match s.tree_type(all) {
                Some(t) => format!("{} ({})", s.name, treegen_sprite::tree_type_label(t)),
                None => s.name.clone(),
            })
            .collect();
        let shown: Vec<usize> = (0..names.len()).filter(|&i| !all || self.project.species[i].tree_type(true).is_some()).collect();
        let name_of = |i: usize| names.get(i).cloned().unwrap_or_else(|| "?".into());




        egui::CollapsingHeader::new("Species editor").default_open(false).show(ui, |ui| {
            let sel = self.selected_species.min(self.project.species.len().saturating_sub(1));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("edit_species").selected_text(name_of(sel)).show_ui(ui, |ui| {
                    for &i in &shown {
                    let n = &names[i];
                        ui.selectable_value(&mut self.selected_species, i, n);
                    }
                });
                if ui.button("Duplicate").clicked() && sel < self.project.species.len() && self.project.species.len() < 255 {
                    let mut c = self.project.species[sel].clone();
                    c.name += " copy";
                    self.project.species.push(c);
                    self.selected_species = self.project.species.len() - 1;
                }
            });
            if let Some(sp) = self.project.species.get_mut(sel) {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut sp.name);
                });
                ui.separator();
                ui.separator();
                if !all
                    && ui
                        .checkbox(&mut sp.procedural, "Procedural tree")
                        .on_hover_text("Grow full trees with the procedural generator, five variations per type. Colours still come from this species.")
                        .changed()
                    && sp.procedural
                    && sp.preset.is_empty()
                {
                    sp.preset = "quaking_aspen".into();
                }
                if all || sp.procedural {
                    let text = if sp.preset.is_empty() { "None" } else { treegen_sprite::tree_type_label(&sp.preset) };
                    egui::ComboBox::from_label("Tree type").selected_text(text).show_ui(ui, |ui| {
                        for (key, label) in treegen_sprite::TREE_TYPES {
                            if ui.selectable_value(&mut sp.preset, key.to_string(), *label).changed() {
                                // picked by hand, so themes keep it
                                sp.procedural = true;
                            }
                        }
                    });
                    ui.small("The first render of a new type takes a few seconds while its trees are grown.");
                    ui.add(egui::Slider::new(&mut sp.height, 0.2..=3.0).text("Height (shadow length)"));
                } else {
                    ui.strong("Branching");
                    egui::ComboBox::from_label("Form").selected_text(sp.form.name()).show_ui(ui, |ui| {
                        for f in Form::ALL {
                            ui.selectable_value(&mut sp.form, f, f.name());
                        }
                    });
                    ui.add(egui::Slider::new(&mut sp.branchiness, 0.4..=2.0).text("Branchiness"))
                        .on_hover_text("More or fewer branches at every level");
                    ui.add(egui::Slider::new(&mut sp.clumpiness, 0.3..=2.5).text("Clump size"));
                    ui.add(egui::Slider::new(&mut sp.leaf_cover, 0.3..=2.0).text("Leaf cover"))
                        .on_hover_text("How densely leaves cover the branches");
                    ui.add(egui::Slider::new(&mut sp.height, 0.2..=3.0).text("Height (shadow length)"));
                    ui.separator();
                    ui.strong("Leaves");
                    ui.add(egui::Slider::new(&mut sp.leaf_size, 0.5..=8.0).suffix(" px").text("Leaf size"));
                    ui.add(egui::Slider::new(&mut sp.leaf_shape, 0.5..=6.0).text("Leaf length"));
                    ui.checkbox(&mut sp.needles, "Needles point outward");
                    ui.checkbox(&mut sp.bare, "Bare branches, no leaves");
                }
                ui.horizontal(|ui| {
                    ui.label("Bark");
                    ui.color_edit_button_rgb(&mut sp.bark);
                });
                ui.horizontal(|ui| {
                    ui.label("Shade to light");
                    for c in sp.ramp.iter_mut() {
                        ui.color_edit_button_rgb(c);
                    }
                });
            }
            if ui.button("Reset all species").clicked() {
                self.project.species = default_species();
            }
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_leaf_colour(&mut self, ui: &mut egui::Ui) {
        let l = &mut self.project.gen.leaf;
        egui::CollapsingHeader::new("Leaf colour").default_open(true).show(ui, |ui| {
            pct_slider(ui, &mut l.variety, 300.0, "Colour variety", "Each leaf lighter, darker, yellower or bluer");
            pct_slider(ui, &mut l.mottling, 300.0, "Mottling", "Broad lighter and darker patches");
            ui.add(egui::Slider::new(&mut l.mottle_size, 4.0..=40.0).suffix(" px").text("Mottling size"));
            pct_slider(ui, &mut l.dappled, 150.0, "Sun-dappled tops", "Sun-facing clumps warm up");
            pct_slider(ui, &mut l.tree_lean, 150.0, "Per-tree colour lean", "Some trees lean lime, some blue-green");
            if ui.button("Reset to my defaults").clicked() {
                *l = LeafStyle::default();
            }
            ui.small("Leaf size, shape and colours are per species, in the Trees tab.");
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_canopy_depth_and_clusters(&mut self, ui: &mut egui::Ui) {
        let l = &mut self.project.gen.leaf;
        let d = &mut self.project.gen.depth;
        egui::CollapsingHeader::new("Canopy depth and clusters").default_open(true).show(ui, |ui| {
            pct_slider(ui, &mut d.crown_tone, 200.0, "Patch tone", "Broad lighter and darker patches across crowns");
            ui.add(egui::Slider::new(&mut d.crown_tone_size, 12.0..=120.0).suffix(" px").text("Patch size"));
            pct_slider(ui, &mut d.crown_shade, 150.0, "Crown shading", "Crown tops lit, gaps between crowns dark");
            ui.add(egui::Slider::new(&mut d.shade_radius, 4.0..=40.0).suffix(" px").text("Shading reach"))
                .on_hover_text("How far away a crown is compared with to decide if it is high or low");
            pct_slider(ui, &mut d.height_var, 150.0, "Crown height variation", "Taller crowns shade the ones beside them");
            pct_slider(ui, &mut d.crown_var, 150.0, "Crown to crown difference", "Neighbouring crowns differ in brightness, so clusters follow crown outlines");
            pct_slider(ui, &mut d.cluster_tone, 200.0, "Clump colour", "How much whole leaf clumps share a colour");
            pct_slider(ui, &mut d.depth_strength, 200.0, "Depth darkening", "How much low and deep parts darken");
            ui.add(egui::Slider::new(&mut d.gain, 0.5..=2.0).text("Brightness"));
            ui.add(egui::Slider::new(&mut d.gamma, 0.4..=1.4).text("Shadow lift"))
                .on_hover_text("Below 1 lifts shadows and midtones");
            ui.horizontal(|ui| {
                if ui.button("Reset to my defaults").clicked() {
                    *d = DepthStyle::default();
                }
                if ui.button("CG2A match").on_hover_text("Settings tuned to CG2A's brightness spread across crowns").clicked() {
                    *d = DepthStyle::cg2a_match();
                }
            });
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_layers_and_colour_grading(&mut self, ui: &mut egui::Ui) {
        let l = &mut self.project.gen.leaf;
        let d = &mut self.project.gen.depth;
        let grading = &mut self.project.grading;
        egui::CollapsingHeader::new("Layers and colour grading").default_open(true).show(ui, |ui| {
            for k in LayerKind::ALL {
                let i = k.index();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut grading.layers[i].visible, "");
                    if ui.selectable_label(self.selected_layer == i, k.name()).clicked() {
                        self.selected_layer = i;
                    }
                });
            }
            ui.separator();
            let k = LayerKind::ALL[self.selected_layer];
            ui.strong(format!("{} grading", k.name()));
            grade_ui(ui, &mut grading.layers[self.selected_layer], k != LayerKind::GroundShadow);
            if ui.button("Reset layer").clicked() {
                grading.layers[self.selected_layer] = LayerGrade::default();
            }
            ui.separator();
            ui.strong("Whole forest grading");
            grade_ui(ui, &mut grading.global, false);
            if ui.button("Reset whole forest").clicked() {
                grading.global = LayerGrade::default();
            }
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_snow(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Snow").default_open(true).show(ui, |ui| {
            let sn = &mut self.project.gen.snow;
            ui.small("Snow on crowns and ground. Amount 0 turns it off.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Looks:");
                for (name, look) in snow_looks() {
                    if ui.button(name).clicked() {
                        *sn = look;
                    }
                }
            });
            ui.separator();
            pct_slider(ui, &mut sn.amount, 100.0, "Amount", "How much snow there is overall");
            egui::ComboBox::from_label("Where").selected_text(sn.mode.name()).show_ui(ui, |ui| {
                for m in SnowMode::ALL {
                    ui.selectable_value(&mut sn.mode, m, m.name());
                }
            });
            match sn.mode {
                SnowMode::Edge => {
                    ui.add(egui::Slider::new(&mut sn.reach, 20.0..=600.0).suffix(" px").text("Edge width"))
                        .on_hover_text("How far in from the outer edge the snow reaches");
                }
                SnowMode::Gradient => {
                    ui.horizontal(|ui| {
                        angle_dial(ui, &mut sn.angle);
                        ui.vertical(|ui| {
                            ui.label("Snow rises toward");
                            ui.add(egui::DragValue::new(&mut sn.angle).range(0.0..=360.0).suffix(" deg"));
                            ui.horizontal(|ui| {
                                for (lbl, a) in [("Top", 270.0), ("Right", 0.0), ("Bottom", 90.0), ("Left", 180.0)] {
                                    if ui.small_button(lbl).clicked() {
                                        sn.angle = a;
                                    }
                                }
                            });
                        });
                    });
                }
                SnowMode::Everywhere => {}
            }
            pct_slider(ui, &mut sn.tops, 100.0, "Clings to tops", "100% keeps snow on the upward facing tops of crowns, 0% covers crowns evenly");
            pct_slider(ui, &mut sn.ground, 100.0, "Ground cover", "How much of the grass and ground turns white");
            pct_slider(ui, &mut sn.patchiness, 100.0, "Patchiness", "Breaks the snow into patches");
            ui.horizontal(|ui| {
                ui.color_edit_button_rgb(&mut sn.tint);
                ui.label("Snow colour");
            });
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_fire(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Fire").default_open(true).show(ui, |ui| {
            let fi = &mut self.project.gen.fire;
            ui.small("Scorched and charred crowns, flames, ash and embers. Amount 0 turns it off.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Looks:");
                for (name, look) in fire_looks() {
                    if ui.button(name).clicked() {
                        *fi = look;
                    }
                }
            });
            ui.separator();
            pct_slider(ui, &mut fi.amount, 100.0, "Amount", "How much of the forest has burnt");
            egui::ComboBox::from_label("Where").selected_text(fi.mode.name()).show_ui(ui, |ui| {
                for m in FireMode::ALL {
                    ui.selectable_value(&mut fi.mode, m, m.name());
                }
            });
            match fi.mode {
                FireMode::Edge => {
                    ui.add(egui::Slider::new(&mut fi.reach, 20.0..=600.0).suffix(" px").text("Edge width"))
                        .on_hover_text("How far in from the outer edge the fire reaches");
                }
                FireMode::Front => {
                    ui.horizontal(|ui| {
                        angle_dial(ui, &mut fi.angle);
                        ui.vertical(|ui| {
                            ui.label("Fire comes from");
                            ui.add(egui::DragValue::new(&mut fi.angle).range(0.0..=360.0).suffix(" deg"));
                            ui.horizontal(|ui| {
                                for (lbl, a) in [("Top", 270.0), ("Right", 0.0), ("Bottom", 90.0), ("Left", 180.0)] {
                                    if ui.small_button(lbl).clicked() {
                                        fi.angle = a;
                                    }
                                }
                            });
                        });
                    });
                }
                FireMode::Everywhere => {}
            }
            pct_slider(ui, &mut fi.flames, 100.0, "Flames", "How much of the burnt area is still alight");
            pct_slider(ui, &mut fi.char, 100.0, "Charring", "How black the burnt crowns and ground get");
            pct_slider(ui, &mut fi.embers, 100.0, "Embers", "Glowing embers and beds on the ground");
            pct_slider(ui, &mut fi.glow, 100.0, "Glow", "Warm light the flames throw on nearby trees");
            pct_slider(ui, &mut fi.patchiness, 100.0, "Patchiness", "Breaks the fire into patches");
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_shadows(&mut self, ui: &mut egui::Ui) {
        let light = &mut self.project.light;
        let grading = &mut self.project.grading;
        egui::CollapsingHeader::new("Shadows").default_open(true).show(ui, |ui| {
            ui.checkbox(&mut light.shadows, "Cast shadows");
            ui.checkbox(&mut light.flat, "Flat light (no light direction)");
            ui.horizontal(|ui| {
                angle_dial(ui, &mut light.shadow_angle);
                ui.vertical(|ui| {
                    ui.label("Shadow direction");
                    ui.add(egui::DragValue::new(&mut light.shadow_angle).range(0.0..=360.0).suffix(" deg"));
                    ui.horizontal(|ui| {
                        for (lbl, a) in [("UL", 225.0), ("UR", 315.0), ("DL", 135.0), ("DR", 45.0)] {
                            if ui.small_button(lbl).on_hover_text("Shadows fall this way").clicked() {
                                light.shadow_angle = a;
                            }
                        }
                    });
                });
            });
            ui.add(egui::Slider::new(&mut light.elevation, 10.0..=90.0).text("Sun elevation"));
            ui.add(egui::Slider::new(&mut light.shadow_length, 0.2..=4.0).text("Shadow length"));
            ui.add(egui::Slider::new(&mut light.shadow_softness, 0.0..=4.0).text("Shadow softness"));
            ui.add(egui::Slider::new(&mut grading.shadow_opacity, 0.0..=1.0).text("Shadow opacity"));
            ui.horizontal(|ui| {
                ui.label("Shadow colour");
                ui.color_edit_button_rgb(&mut grading.shadow_color);
            });
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_export(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Export").default_open(true).show(ui, |ui| {
            ui.add(egui::Slider::new(&mut self.export_scale, 0.25..=4.0).text("Export scale"));
            ui.small(format!(
                "Output {} x {} px",
                (self.project.map_w as f32 * self.export_scale) as u32,
                (self.project.map_h as f32 * self.export_scale) as u32
            ));
            ui.checkbox(&mut self.variants.main, "With shadows");
            ui.checkbox(&mut self.variants.no_shadow, "Without shadows (_noshadow)");
            ui.checkbox(&mut self.variants.shadow_only, "Ground shadow only (_shadow)");
            if ui.button("Export PNG").clicked() {
                self.export();
            }
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_preview(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Preview").default_open(true).show(ui, |ui| {
            if self.gpu.is_some() {
                egui::ComboBox::from_label("Preview detail")
                    .selected_text(format!("{}%", (self.view_detail * 100.0) as u32))
                    .show_ui(ui, |ui| {
                        for s in [0.5, 1.0, 2.0] {
                            ui.selectable_value(&mut self.view_detail, s, format!("{}%", (s * 100.0) as u32));
                        }
                    });
                ui.small("Only the part on screen is drawn, at screen resolution. Zoom in to judge leaf detail.");
            } else {
                egui::ComboBox::from_label("Preview quality")
                    .selected_text(format!("{}%", (self.preview_scale * 100.0) as u32))
                    .show_ui(ui, |ui| {
                        for s in [0.25, 0.5, 1.0] {
                            if ui.selectable_value(&mut self.preview_scale, s, format!("{}%", (s * 100.0) as u32)).changed() {
                                self.gen_dirty = true;
                            }
                        }
                    });
                ui.small("Use 100% to judge leaf detail. At 50% each leaf is under a pixel.");
            }
        });
    }

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_my_defaults(&mut self, ui: &mut egui::Ui) {
        if self.edit_node.is_some() {
            ui.small("Select no zone to save or restore the project defaults.");
            return;
        }
        egui::CollapsingHeader::new("My defaults").default_open(false).show(ui, |ui| {
            ui.small("Saves every setting except shapes and painted trees, and uses them whenever the app starts.");
            ui.horizontal(|ui| {
                if ui.button("Save current settings as my defaults").clicked() {
                    self.status = match save_user_defaults(&self.project) {
                        Ok(p) => format!("Saved defaults to {}", p.display()),
                        Err(e) => format!("Could not save defaults: {e}"),
                    };
                }
            });
            if ui.button("Go back to the built-in defaults").clicked() {
                let p = &mut self.project;
                p.gen = GenParams::default();
                p.light = Light::default();
                p.grading = Grading::default();
                p.species = default_species();
                self.status = match clear_user_defaults() {
                    Ok(()) => "Built-in defaults restored, saved defaults removed".into(),
                    Err(e) => format!("Defaults restored, but the saved file stayed: {e}"),
                };
            }
        });
    }

    /// Apply the active tree tool at a map position.
    fn brush_at(&mut self, m: [f32; 2], first: bool) {
        match self.tool {
            Tool::TreeBrush => {
                paint_dab(&mut self.project, &self.brush, m[0], m[1], &mut self.paint_rng);
            }
            Tool::TreeStamp => {
                if first {
                    stamp_one(&mut self.project, &self.brush, m[0], m[1], &mut self.paint_rng);
                }
            }
            Tool::TreeErase => {
                erase_dab(&mut self.project, &self.brush, m[0], m[1]);
                self.painted_rendered = self.painted_rendered.min(self.project.painted.len());
            }
            _ => return,
        }
        self.paint_rev += 1;
        self.last_dab = Some(m);
    }

    // ---------------------------------------------------------------- canvas

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let origin = rect.min;
        let (mw, mh) = (self.project.map_w as f32, self.project.map_h as f32);
        if self.fit_pending {
            self.zoom = ((rect.width() - 40.0) / mw).min((rect.height() - 40.0) / mh).max(0.01);
            self.pan = (rect.size() - Vec2::new(mw, mh) * self.zoom) * 0.5;
            self.fit_pending = false;
        }

        // Zoom about the cursor.
        if let Some(hover) = resp.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let before = self.screen_to_map(origin, hover);
                self.zoom = (self.zoom * (scroll * 0.0015).exp()).clamp(0.01, 8.0);
                let after = self.map_to_screen(origin, before);
                self.pan += hover - after;
            }
        }
        let space = ui.input(|i| i.key_down(Key::Space));
        if resp.dragged_by(PointerButton::Middle) || (resp.dragged_by(PointerButton::Primary) && (self.tool == Tool::Pan || space)) {
            self.pan += resp.drag_delta();
        } else {
            match self.tool {
                Tool::Lasso | Tool::LassoCut => {
                    if resp.drag_started_by(PointerButton::Primary) {
                        self.stroke.clear();
                    }
                    if resp.dragged_by(PointerButton::Primary) {
                        if let Some(p) = resp.interact_pointer_pos() {
                            let m = self.screen_to_map(origin, p);
                            let far = self.stroke.last().map_or(true, |l| {
                                let d = ((l[0] - m[0]).powi(2) + (l[1] - m[1]).powi(2)).sqrt();
                                d * self.zoom > 3.0
                            });
                            if far {
                                self.stroke.push(m);
                            }
                        }
                    }
                    if resp.drag_stopped_by(PointerButton::Primary) {
                        let pts = chaikin(&std::mem::take(&mut self.stroke), 2);
                        self.add_shape(pts, self.tool.op());
                    }
                }
                Tool::Polygon | Tool::PolygonCut => {
                    if resp.clicked_by(PointerButton::Primary) {
                        if let Some(p) = resp.interact_pointer_pos() {
                            let m = self.screen_to_map(origin, p);
                            self.stroke.push(m);
                        }
                    }
                    if resp.clicked_by(PointerButton::Secondary) || resp.double_clicked() {
                        let pts = std::mem::take(&mut self.stroke);
                        self.add_shape(pts, self.tool.op());
                    }
                }
                Tool::Pan => {}
                Tool::Zones => self.zone_input(ui, &resp, origin),
                Tool::TreeBrush | Tool::TreeStamp | Tool::TreeErase => {
                    let pressed = resp.drag_started_by(PointerButton::Primary) || resp.clicked_by(PointerButton::Primary);
                    if pressed {
                        if let Some(p) = resp.interact_pointer_pos() {
                            self.push_undo();
                            let m = self.screen_to_map(origin, p);
                            self.brush_at(m, true);
                        }
                    } else if resp.dragged_by(PointerButton::Primary) && self.tool != Tool::TreeStamp {
                        if let (Some(p), Some(last)) = (resp.interact_pointer_pos(), self.last_dab) {
                            let m = self.screen_to_map(origin, p);
                            // Dabs at even spacing along the stroke, even when the mouse jumps.
                            let (dx, dy) = (m[0] - last[0], m[1] - last[1]);
                            let dist = (dx * dx + dy * dy).sqrt();
                            let step = self.brush.dab_step();
                            if dist >= step {
                                let n = (dist / step).floor() as usize;
                                for k in 1..=n {
                                    let t = k as f32 * step / dist;
                                    self.brush_at([last[0] + dx * t, last[1] + dy * t], false);
                                }
                            }
                        }
                    }
                    if resp.drag_stopped() || (pressed && !resp.dragged()) {
                        self.last_dab = None;
                    }
                }
            }
        }

        let painter = ui.painter_at(rect);
        let tl = self.screen_to_map(origin, rect.min);
        let br = self.screen_to_map(origin, rect.max);
        self.visible = Some([tl[0], tl[1], br[0], br[1]]);
        self.canvas_rect = Some(rect);
        self.pixels_per_point = ui.ctx().pixels_per_point();
        painter.rect_filled(rect, 0.0, Color32::from_gray(40));
        let map_rect = Rect::from_min_size(self.map_to_screen(origin, [0.0, 0.0]), Vec2::new(mw, mh) * self.zoom);
        let bg = Color32::from_rgb((self.background[0] * 255.0) as u8, (self.background[1] * 255.0) as u8, (self.background[2] * 255.0) as u8);
        painter.rect_filled(map_rect, 0.0, bg);
        self.draw_background(&painter, map_rect);
        let shown = if self.gpu_layers.is_some() { self.gpu_texture } else { self.texture.as_ref().map(|t| t.id()) };
        if let Some(id) = shown {
            let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            let mut shown_at = map_rect;
            match (&self.gpu_view, self.gpu_layers.is_some()) {
                (Some(v), true) => {
                    // the GPU image covers only part of the map
                    let at = |px: f32, py: f32| self.map_to_screen(origin, [px / v.scale, py / v.scale]);
                    let mut r = Rect::from_min_max(at(v.origin[0] as f32, v.origin[1] as f32), at((v.origin[0] + v.size[0]) as f32, (v.origin[1] + v.size[1]) as f32));
                    // At one render pixel per screen pixel, put the picture exactly on the pixel grid so
                    // it is not smeared by half a pixel.
                    let ppp = self.pixels_per_point;
                    if (v.scale / (self.zoom * ppp * self.view_detail) - 1.0).abs() < 0.01 && self.view_detail == 1.0 {
                        let min = Pos2::new((r.min.x * ppp).round() / ppp, (r.min.y * ppp).round() / ppp);
                        r = Rect::from_min_size(min, Vec2::new(v.size[0] as f32, v.size[1] as f32) / ppp);
                    }
                    painter.image(id, r, full, Color32::WHITE);
                    shown_at = r;
                }
                _ => {
                    painter.image(id, map_rect, full, Color32::WHITE);
                }
            }
            // zones with their own settings are layers over it
            let layer_ids: Vec<egui::TextureId> = if self.gpu_layers.is_some() {
                self.extra_gpu.iter().filter_map(|e| e.texture).collect()
            } else {
                self.extra_cpu.iter().filter_map(|(_, t)| t.as_ref().map(|t| t.id())).collect()
            };
            for lid in layer_ids {
                painter.image(lid, shown_at, full, Color32::WHITE);
            }
        }
        painter.rect_stroke(map_rect, 0.0, Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Outside);

        self.draw_zones(&painter, origin);
        if !self.stroke.is_empty() {
            let mut pts: Vec<Pos2> = self.stroke.iter().map(|p| self.map_to_screen(origin, *p)).collect();
            if matches!(self.tool, Tool::Polygon | Tool::PolygonCut) {
                if let Some(h) = resp.hover_pos() {
                    pts.push(h);
                }
            }
            let col = if self.tool.op() == ShapeOp::Add { Color32::from_rgb(140, 255, 140) } else { Color32::from_rgb(255, 120, 120) };
            painter.add(egui::Shape::line(pts, Stroke::new(2.0, col)));
        }
        if self.tool.is_brush() {
            let (grow, shrink) = ui.input(|i| (i.key_pressed(Key::CloseBracket), i.key_pressed(Key::OpenBracket)));
            if grow {
                self.brush.radius = (self.brush.radius * 1.2).min(400.0);
            }
            if shrink {
                self.brush.radius = (self.brush.radius / 1.2).max(5.0);
            }
            // Trees painted since the last preview, as outlines so painting feels immediate.
            let pending = Color32::from_rgba_unmultiplied(170, 230, 120, 160);
            for t in self.project.painted.iter().skip(self.painted_rendered) {
                painter.circle_stroke(self.map_to_screen(origin, [t.x, t.y]), t.r * self.zoom, Stroke::new(1.0, pending));
            }
            if let Some(h) = resp.hover_pos() {
                let col = if self.tool == Tool::TreeErase { Color32::from_rgb(255, 120, 120) } else { Color32::from_rgb(200, 255, 160) };
                let r = if self.tool == Tool::TreeStamp { self.brush.size_max } else { self.brush.radius };
                painter.circle_stroke(h, r * self.zoom, Stroke::new(1.5, col));
            }
        }
        if self.rendering {
            painter.text(rect.right_top() + Vec2::new(-10.0, 10.0), egui::Align2::RIGHT_TOP, "Rendering...", egui::FontId::proportional(14.0), Color32::WHITE);
        }
    }
}

impl eframe::App for TreeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        self.sync_background(&ctx);
        self.poll_worker(&ctx);
        self.run_shot(&ctx);

        {
            egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.status);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some((_, c)) = &self.render_info {
                            let how = if self.gpu_layers.is_some() { "GPU" } else { "CPU" };
                            ui.label(format!("{} ms ({how}) | canopy {} shrubs {} lone {} pebbles {}", self.render_ms, c[0], c[1], c[2], c[3]));
                        }
                    });
                });
            });
            egui::Panel::left("tools").resizable(false).default_size(185.0).show(ui, |ui| self.tool_panel(ui));
            egui::Panel::right("settings").resizable(true).default_size(360.0).size_range(280.0..=640.0).show(ui, |ui| self.with_zone_settings(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            self.canvas(ui);
        });
    }
}

/// The tree counts of every layer added up.
fn sum_counts(first: [usize; 4], rest: impl Iterator<Item = [usize; 4]>) -> [usize; 4] {
    rest.fold(first, |a, c| [a[0] + c[0], a[1] + c[1], a[2] + c[2], a[3] + c[3]])
}

/// Settings that carry over between projects, without any shapes or painted trees.
#[derive(Serialize, Deserialize)]
struct UserDefaults {
    gen: GenParams,
    light: Light,
    grading: Grading,
    species: Vec<Species>,
}

fn user_defaults_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("XDG_CONFIG_HOME")).map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("tree-inator").join("defaults.json"))
}

fn save_user_defaults(p: &Project) -> Result<PathBuf, String> {
    let path = user_defaults_path().ok_or("no settings folder found")?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let d = UserDefaults { gen: p.gen.clone(), light: p.light.clone(), grading: p.grading.clone(), species: p.species.clone() };
    std::fs::write(&path, serde_json::to_string_pretty(&d).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(path)
}

fn clear_user_defaults() -> Result<(), String> {
    match user_defaults_path() {
        Some(p) if p.exists() => std::fs::remove_file(p).map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}

fn apply_user_defaults(p: &mut Project) {
    let Some(path) = user_defaults_path() else { return };
    let Ok(text) = std::fs::read_to_string(path) else { return };
    if let Ok(d) = serde_json::from_str::<UserDefaults>(&text) {
        p.gen = d.gen;
        p.light = d.light;
        p.grading = d.grading;
        p.species = d.species;
    }
}

/// Ready-made snow looks. The first is off.
fn snow_looks() -> [(&'static str, SnowStyle); 6] {
    let d = SnowStyle::default;
    [
        ("Off", SnowStyle { amount: 0.0, ..d() }),
        ("Dusting", SnowStyle { amount: 0.6, tops: 0.95, ground: 0.3, ..d() }),
        ("Frosted", SnowStyle { amount: 0.7, tops: 0.6, ground: 0.7, ..d() }),
        ("Blanket", SnowStyle { amount: 1.0, tops: 0.15, ground: 1.0, patchiness: 0.3, ..d() }),
        ("Outer edge", SnowStyle { amount: 0.95, mode: SnowMode::Edge, reach: 200.0, ground: 0.8, ..d() }),
        ("Snow line", SnowStyle { amount: 0.95, mode: SnowMode::Gradient, angle: 270.0, ground: 0.8, patchiness: 0.7, ..d() }),
    ]
}

/// Ready-made fire looks. The first is off.
fn fire_looks() -> [(&'static str, FireStyle); 8] {
    let d = FireStyle::default;
    [
        ("Off", FireStyle { amount: 0.0, ..d() }),
        ("Wildfire patches", FireStyle { amount: 0.55, mode: FireMode::Everywhere, flames: 0.6, ..d() }),
        ("Burning edge", FireStyle { amount: 0.9, mode: FireMode::Edge, reach: 260.0, flames: 0.6, ..d() }),
        ("Fire front", FireStyle { amount: 0.7, mode: FireMode::Front, angle: 270.0, flames: 0.6, ..d() }),
        ("Inferno", FireStyle { amount: 0.62, flames: 1.0, glow: 1.0, embers: 0.8, ..d() }),
        ("Smouldering", FireStyle { amount: 0.55, flames: 0.15, embers: 1.0, glow: 0.4, char: 0.9, ..d() }),
        ("Burnt out", FireStyle { amount: 0.95, flames: 0.0, glow: 0.0, embers: 0.15, char: 0.9, ..d() }),
        ("Scorched", FireStyle { amount: 0.4, flames: 0.0, glow: 0.0, embers: 0.0, char: 0.4, ..d() }),
    ]
}

type Preset = (&'static str, fn(&mut GenParams));

/// Each preset only touches the settings that define its look.
const PRESETS: &[Preset] = &[
    ("CG2A dense forest", |g| {
        g.pattern = Pattern::Fragmented;
        g.pattern_strength = 0.5;
        g.pattern_scale = 300.0;
        g.canopy_cover = 0.88;
        g.gaps = 0.2;
        g.interior_lone = 0.0;
    }),
    ("Fragmented woodland", |g| {
        g.pattern = Pattern::Fragmented;
        g.pattern_strength = 0.9;
        g.pattern_scale = 260.0;
        g.canopy_cover = 0.75;
        g.gaps = 0.3;
        g.interior_lone = 0.1;
    }),
    ("Riparian / branching", |g| {
        g.pattern = Pattern::Branching;
        g.pattern_strength = 0.9;
        g.pattern_scale = 350.0;
        g.branch_width = 0.35;
        g.canopy_cover = 0.85;
        g.gaps = 0.0;
        g.interior_lone = 0.15;
    }),
    ("Open woodland", |g| {
        g.pattern = Pattern::Clumps;
        g.pattern_strength = 0.8;
        g.pattern_scale = 260.0;
        g.canopy_cover = 0.6;
        g.shrub_density = 1.5;
        g.interior_lone = 0.4;
        g.gaps = 0.0;
    }),
    ("Savanna", |g| {
        g.pattern = Pattern::Clumps;
        g.pattern_strength = 1.0;
        g.pattern_scale = 120.0;
        g.canopy_cover = 0.22;
        g.interior_lone = 1.0;
        g.lone_groups = 3.0;
        g.grass_reach = 1.0;
        g.gaps = 0.0;
    }),
    ("Old growth with gaps", |g| {
        g.pattern = Pattern::Lobed;
        g.pattern_strength = 0.5;
        g.pattern_scale = 400.0;
        g.canopy_cover = 0.92;
        g.gaps = 0.8;
        g.gap_size = 40.0;
        g.interior_lone = 0.0;
    }),
];

/// Grade sliders shared by layer and global grading.
fn grade_ui(ui: &mut egui::Ui, g: &mut LayerGrade, show_shadow_toggle: bool) {
    ui.add(egui::Slider::new(&mut g.opacity, 0.0..=1.0).text("Opacity"));
    ui.add(egui::Slider::new(&mut g.brightness, 0.2..=2.0).text("Brightness"));
    ui.add(egui::Slider::new(&mut g.contrast, 0.2..=2.5).text("Contrast"));
    ui.add(egui::Slider::new(&mut g.saturation, 0.0..=2.5).text("Saturation"));
    ui.add(egui::Slider::new(&mut g.hue, -180.0..=180.0).text("Hue"));
    ui.horizontal(|ui| {
        ui.label("Tint");
        ui.color_edit_button_rgb(&mut g.tint);
    });
    if show_shadow_toggle {
        ui.checkbox(&mut g.receive_shadow, "Show shadows on this layer");
    }
}

/// Round dial for the shadow direction. The arrow points where shadows fall.
fn angle_dial(ui: &mut egui::Ui, angle: &mut f32) {
    let size = 64.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let c = rect.center();
    if let Some(p) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let d = p - c;
            *angle = d.y.atan2(d.x).to_degrees().rem_euclid(360.0).round();
        }
    }
    let painter = ui.painter_at(rect);
    let r = size * 0.45;
    painter.circle(c, r, Color32::from_gray(35), Stroke::new(1.0, Color32::from_gray(120)));
    let a = angle.to_radians();
    let tip = c + Vec2::new(a.cos(), a.sin()) * r * 0.85;
    let sun = c - Vec2::new(a.cos(), a.sin()) * r * 0.7;
    painter.circle_filled(sun, 4.0, Color32::from_rgb(255, 220, 90));
    painter.arrow(c, tip - c, Stroke::new(2.0, Color32::from_gray(220)));
}

/// Chaikin corner cutting on a closed loop.
fn chaikin(pts: &[[f32; 2]], iterations: usize) -> Vec<[f32; 2]> {
    let mut p = pts.to_vec();
    if p.len() < 3 {
        return p;
    }
    for _ in 0..iterations {
        let n = p.len();
        let mut q = Vec::with_capacity(n * 2);
        for i in 0..n {
            let a = p[i];
            let b = p[(i + 1) % n];
            q.push([0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]]);
            q.push([0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]);
        }
        p = q;
    }
    p
}
