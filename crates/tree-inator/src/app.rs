mod background;
mod compare;
mod icons;
mod selftest;
mod panel;
mod simple;
mod theme_cards;
mod tools;
mod tuning;
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

use tools::{BgPixels, LineUse, Mode, RowId, Tool};

/// Everything the user edits by hand, for undo.
#[derive(Clone)]
struct Snap {
    shapes: Vec<Shape>,
    groups: Vec<Group>,
    painted: Vec<TreeRec>,
    erased: Vec<[f32; 3]>,
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
    export_format: forest_core::render::ExportFormat,
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
    bg_rx: Option<Receiver<Result<(background::Tiles, BgPixels), String>>>,
    zone_sel: Option<usize>,
    search: String,
    /// Drawing tool state.
    mode: Mode,
    snap: bool,
    draw_smooth: f32,
    wand_tol: f32,
    wand_contiguous: bool,
    wand_preview: Option<(Instant, [i32; 2], Vec<Vec<Vec<[f32; 2]>>>)>,
    line_width: f32,
    line_use: LineUse,
    line_spacing: f32,
    grow_px: f32,
    /// Zones picked in addition to `zone_sel`, for combining them.
    zone_multi: Vec<usize>,
    /// The row shown on its own, ignoring what else is visible.
    solo: Option<RowId>,
    renaming: Option<RowId>,
    row_drag: Option<RowId>,
    row_drop: Option<(RowId, RowId)>,
    bg_pixels: Option<BgPixels>,
    /// Calibrating the map's scale: two points a known distance apart.
    calib: Vec<[f32; 2]>,
    calibrating: bool,
    calib_metres: f32,
    settings_clip: Option<(GenParams, Vec<Species>)>,
    selftest_done: bool,
    selftest_log: Vec<String>,
    /// The tab of the tuning panel, and which tabs have their Fine Tuning open.
    tab: tuning::Tab,
    /// Compare mode: a snapshot on the left of a divider, the live picture on the right.
    compare: bool,
    snapshot: Option<compare::Snapshot>,
    split: f32,
    fine_open: [bool; 8],
    theme_tex: std::collections::HashMap<&'static str, egui::TextureHandle>,
    species_tex: std::collections::HashMap<String, egui::TextureHandle>,
    /// The species open in the editor window, if any.
    species_edit: Option<usize>,
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
            variants: Variants { main: true, no_shadow: false, shadow_only: false, layers: false },
            export_format: forest_core::render::ExportFormat::Png,
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
            search: std::env::var("TREE_INATOR_SEARCH").unwrap_or_default(),
            mode: Mode::Add,
            snap: true,
            draw_smooth: 0.4,
            wand_tol: 32.0,
            wand_contiguous: true,
            wand_preview: None,
            line_width: 24.0,
            line_use: LineUse::Cut,
            line_spacing: 18.0,
            grow_px: 20.0,
            zone_multi: Vec::new(),
            solo: None,
            renaming: None,
            row_drag: None,
            row_drop: None,
            bg_pixels: None,
            calib: Vec::new(),
            calibrating: false,
            calib_metres: 100.0,
            settings_clip: None,
            selftest_done: false,
            selftest_log: Vec::new(),
            tab: tuning::Tab::from_env(),
            compare: std::env::var_os("TREE_INATOR_COMPARE").is_some(),
            snapshot: None,
            split: 0.5,
            fine_open: [std::env::var_os("TREE_INATOR_FINE").is_some(); 8],
            theme_tex: Default::default(),
            species_tex: Default::default(),
            species_edit: None,
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
    /// What the preview draws: the project, or with a row on solo only that row's zones.
    fn preview_project(&self) -> Project {
        let mut p = self.real_project();
        let Some(solo) = self.solo else { return p };
        let keep: Vec<usize> = match solo {
            RowId::Zone(i) => vec![i],
            RowId::Group(g) => {
                let mut ids = p.descendants(g);
                ids.push(g);
                (0..p.shapes.len()).filter(|&i| p.zone_group(&p.shapes[i]).is_some_and(|z| ids.contains(&z))).collect()
            }
        };
        for (i, z) in p.shapes.iter_mut().enumerate() {
            if !keep.contains(&i) {
                z.visible = false;
            }
        }
        p
    }

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
            self.tuning_panel(ui);
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

        self.tuning_panel(ui);

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
        let gen_key = format!("{}{}{:?}", self.project.render_key(), self.paint_rev, self.solo);
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
                let _ = self.tx.send(Job::View { project: self.preview_project(), scale, view, id: self.job_id });
            } else {
                let _ = self.tx.send(Job::Preview { project: self.preview_project(), scale: self.preview_scale, id: self.job_id });
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
                let over = ComposeOptions { shadows: true, shadow_only: false, background: None, premultiplied: true, layers: forest_core::compose::ALL_LAYERS };
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
                let over = ComposeOptions { shadows: true, shadow_only: false, background: None, premultiplied: true, layers: forest_core::compose::ALL_LAYERS };
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
        if enter && matches!(self.tool, Tool::Polygon) {
            let pts = std::mem::take(&mut self.stroke);
            self.finish_polygon(pts, ctx);
        }
        if enter && self.tool == Tool::Line {
            self.finish_line();
        }
        // tool keys and the species keys
        let (tool, number) = ctx.input(|i| {
            if i.modifiers.command || i.modifiers.alt {
                return (None, None);
            }
            let t = Tool::ALL.iter().find_map(|_| None::<Tool>).or_else(|| {
                [Key::V, Key::L, Key::P, Key::W, Key::N, Key::B, Key::T, Key::E, Key::M, Key::H].into_iter().find(|k| i.key_pressed(*k)).and_then(Tool::from_key)
            });
            let n = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9].iter().position(|k| i.key_pressed(*k));
            (t, n)
        });
        if let Some(t) = tool {
            self.set_tool(t);
        }
        if let Some(n) = number {
            self.species_key(n);
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
        let fmt = self.export_format;
        let ext = fmt.extension().unwrap_or("png");
        let Some(path) = rfd::FileDialog::new().add_filter(fmt.label(), &[ext]).set_file_name(format!("forest.{ext}")).save_file() else {
            return;
        };
        let _ = self.tx.send(Job::Export { project: self.real_project(), scale: self.export_scale, path, variants: self.variants, format: fmt });
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
                if ui.button("Export").clicked() {
                    self.export();
                }
                let can = self.gpu.is_some();
                if ui.add_enabled(can, egui::Button::new("Compare").selected(self.compare)).on_hover_text("Split the map: a snapshot on the left, what you are editing on the right").on_disabled_hover_text("Compare needs the GPU preview").clicked() {
                    self.compare = !self.compare;
                    if self.compare {
                        self.take_snapshot();
                    } else {
                        self.free_snapshot();
                    }
                }
            });
        });
    }

    // ---------------------------------------------------------------- tabs

    #[allow(unused_variables, unused_mut)]
    pub(super) fn sec_species_editor(&mut self, ui: &mut egui::Ui) {
        // Lists show the species with their tree type.
        let names: Vec<String> = self
            .project
            .species
            .iter()
            .map(|s| match s.tree_type() {
                Some(t) => format!("{} ({})", s.name, treegen_sprite::tree_type_label(t)),
                None => s.name.clone(),
            })
            .collect();
        let shown: Vec<usize> = (0..names.len()).filter(|&i| self.project.species[i].tree_type().is_some()).collect();
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
            ui.checkbox(&mut self.variants.layers, "Layers: ground, underbrush, canopy (_ground, _underbrush, _canopy)");
            egui::ComboBox::from_label("Format").selected_text(self.export_format.label()).show_ui(ui, |ui| {
                for f in forest_core::render::ExportFormat::ALL {
                    ui.selectable_value(&mut self.export_format, f, f.label());
                }
            });
            ui.small(match self.export_format {
                forest_core::render::ExportFormat::Png => "Quick to write.",
                forest_core::render::ExportFormat::PngSmall => "A smaller file, slower to write.",
                forest_core::render::ExportFormat::Tiles => "A folder of 1024 px tiles with an index file. Any map size fits.",
                forest_core::render::ExportFormat::Webp => "Lossless and usually less than half the size of PNG. Needs the whole picture in memory.",
                forest_core::render::ExportFormat::Jpeg => "Small files without transparency. Needs the whole picture in memory.",
            });
            if ui.button("Export").clicked() {
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
            Tool::Brush => {
                paint_dab(&mut self.project, &self.brush, m[0], m[1], &mut self.paint_rng);
            }
            Tool::Single => {
                if first {
                    stamp_one(&mut self.project, &self.brush, m[0], m[1], &mut self.paint_rng);
                }
            }
            Tool::Eraser => {
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
            if self.calibrating {
                if resp.clicked_by(PointerButton::Primary) {
                    if let Some(p) = resp.interact_pointer_pos() {
                        if self.calib.len() >= 2 {
                            self.calib.clear();
                        }
                        self.calib.push(self.screen_to_map(origin, p));
                    }
                }
            } else {
                let ctx = ui.ctx().clone();
                match self.tool {
                    Tool::Lasso | Tool::Mask => {
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
                            let raw = std::mem::take(&mut self.stroke);
                            self.finish_lasso(raw, &ctx);
                        }
                    }
                    Tool::Polygon | Tool::Line => {
                        // a press that moved a little counts as a drag in egui, so take its release as a click too
                        if resp.clicked_by(PointerButton::Primary) || resp.drag_stopped_by(PointerButton::Primary) {
                            if let Some(p) = resp.interact_pointer_pos() {
                                let mut m = self.screen_to_map(origin, p);
                                if self.snap {
                                    if let Some(sp) = self.snap_point(m, None) {
                                        m = sp;
                                    }
                                }
                                self.stroke.push(m);
                            }
                        }
                        if resp.clicked_by(PointerButton::Secondary) || resp.double_clicked() {
                            let pts = std::mem::take(&mut self.stroke);
                            if self.tool == Tool::Line {
                                self.stroke = pts;
                                self.finish_line();
                            } else {
                                self.finish_polygon(pts, &ctx);
                            }
                        }
                    }
                    Tool::Wand => {
                        if resp.clicked_by(PointerButton::Primary) {
                            if let Some(p) = resp.interact_pointer_pos() {
                                let m = self.screen_to_map(origin, p);
                                let mode = self.mode_now(&ctx);
                                self.wand_click(m, mode);
                                self.wand_preview = None;
                            }
                        }
                    }
                    Tool::Pan => {}
                    Tool::Select => self.zone_input(ui, &resp, origin),
                    Tool::Brush | Tool::Single | Tool::Eraser => {
                        let pressed = resp.drag_started_by(PointerButton::Primary) || resp.clicked_by(PointerButton::Primary);
                        if pressed {
                            if let Some(p) = resp.interact_pointer_pos() {
                                self.push_undo();
                                let m = self.screen_to_map(origin, p);
                                self.brush_at(m, true);
                            }
                        } else if resp.dragged_by(PointerButton::Primary) && self.tool != Tool::Single {
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
        }

        let painter = ui.painter_at(rect);
        let tl = self.screen_to_map(origin, rect.min);
        let br = self.screen_to_map(origin, rect.max);
        self.visible = Some([tl[0], tl[1], br[0], br[1]]);
        self.canvas_rect = Some(rect);
        self.pixels_per_point = ui.ctx().pixels_per_point();
        painter.rect_filled(rect, 0.0, Color32::from_gray(40));
        let map_rect = Rect::from_min_size(self.map_to_screen(origin, [0.0, 0.0]), Vec2::new(mw, mh) * self.zoom);
        self.draw_picture(ui, &painter, rect, origin, map_rect);
        painter.rect_stroke(map_rect, 0.0, Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Outside);

        self.draw_zones(&painter, origin);
        self.draw_reference_overlays(&painter, origin, map_rect);
        let ctx = ui.ctx().clone();
        let mode_now = self.mode_now(&ctx);
        let mode_col = match mode_now {
            Mode::Add => Color32::from_rgb(140, 255, 140),
            Mode::Subtract => Color32::from_rgb(255, 120, 120),
            Mode::Intersect => Color32::from_rgb(255, 210, 90),
        };
        if !self.stroke.is_empty() {
            let mut pts: Vec<Pos2> = self.stroke.iter().map(|p| self.map_to_screen(origin, *p)).collect();
            if matches!(self.tool, Tool::Polygon | Tool::Line) {
                if let Some(h) = resp.hover_pos() {
                    pts.push(h);
                }
            }
            let col = if self.tool == Tool::Line {
                Color32::from_rgb(255, 220, 90)
            } else if self.tool == Tool::Mask {
                Color32::from_rgb(220, 90, 90)
            } else {
                mode_col
            };
            if self.tool == Tool::Line {
                // the band the path will make
                painter.add(egui::Shape::line(pts.clone(), Stroke::new(self.line_width * self.zoom, Color32::from_rgba_unmultiplied(255, 255, 255, 30))));
            }
            painter.add(egui::Shape::line(pts.clone(), Stroke::new(2.0, col)));
            for q in &pts {
                painter.circle_filled(*q, 3.0, col);
            }
        }
        if let Some(h) = resp.hover_pos() {
            // a corner or edge of another zone the next point would snap to
            if self.snap && matches!(self.tool, Tool::Polygon | Tool::Line | Tool::Lasso) {
                let m = self.screen_to_map(origin, h);
                if let Some(sp) = self.snap_point(m, None) {
                    let c = self.map_to_screen(origin, sp);
                    painter.add(egui::Shape::convex_polygon(vec![c + Vec2::new(0.0, -8.0), c + Vec2::new(8.0, 0.0), c + Vec2::new(0.0, 8.0), c + Vec2::new(-8.0, 0.0)], Color32::TRANSPARENT, Stroke::new(1.6, Color32::from_rgb(255, 120, 220))));
                }
            }
            if self.tool == Tool::Wand && self.bg_pixels.is_some() && !self.calibrating {
                let m = self.screen_to_map(origin, h);
                let cell = [(m[0] / 3.0) as i32, (m[1] / 3.0) as i32];
                let stale = self.wand_preview.as_ref().map_or(true, |(t, c, _)| *c != cell && t.elapsed() > Duration::from_millis(110));
                if stale {
                    let regions = self.wand_regions(m);
                    self.wand_preview = Some((Instant::now(), cell, regions));
                    ctx.request_repaint_after(Duration::from_millis(120));
                }
                if let Some((_, _, regions)) = &self.wand_preview {
                    for r in regions {
                        for (k, ring) in r.iter().enumerate() {
                            let pts: Vec<Pos2> = ring.iter().map(|p| self.map_to_screen(origin, *p)).collect();
                            let col = if k == 0 { mode_col } else { Color32::from_rgb(255, 255, 255) };
                            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.8, col)));
                        }
                    }
                }
                painter.circle_stroke(h, 5.0, Stroke::new(1.5, Color32::WHITE));
                painter.text(h + Vec2::new(14.0, 12.0), egui::Align2::LEFT_TOP, format!("Tolerance {:.0}  |  click to {}", self.wand_tol, match mode_now { Mode::Add => "add", Mode::Subtract => "subtract", Mode::Intersect => "intersect" }), egui::FontId::proportional(11.0), Color32::WHITE);
            }
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
                let col = if self.tool == Tool::Eraser { Color32::from_rgb(255, 120, 120) } else { Color32::from_rgb(200, 255, 160) };
                let r = if self.tool == Tool::Single { self.brush.size_max } else { self.brush.radius };
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
        self.compare_test_hooks();
        self.run_selftest_actions(&ctx);
        self.run_shot(&ctx);

        {
            egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
            if self.compare {
                egui::Panel::top("compare").show(ui, |ui| self.compare_bar(ui));
            }
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let msg = if self.status.is_empty() { self.tool.hint().to_string() } else { self.status.clone() };
                    ui.label(msg);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some((_, c)) = &self.render_info {
                            let how = if self.gpu_layers.is_some() { "GPU" } else { "CPU" };
                            ui.label(format!("{} ms ({how}) | canopy {} shrubs {} lone {} pebbles {}", self.render_ms, c[0], c[1], c[2], c[3]));
                        }
                    });
                });
            });
            egui::Panel::top("options").show(ui, |ui| self.options_bar(ui));
            egui::Panel::left("tools").resizable(false).exact_size(52.0).show(ui, |ui| self.tool_strip(ui));
            egui::Panel::left("zones").resizable(true).default_size(250.0).size_range(210.0..=420.0).show(ui, |ui| self.zones_panel(ui));
            egui::Panel::right("settings").resizable(true).default_size(420.0).size_range(300.0..=700.0).show(ui, |ui| self.with_zone_settings(ui));
            egui::Panel::bottom("species").show(ui, |ui| self.species_strip(ui));
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

