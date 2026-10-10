//! Background render thread. Previews are cancelled when a newer one arrives.

use forest_core::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

pub enum Job {
    Preview { project: Project, scale: f32, id: u64 },
    /// Render only the part of the map on screen: `scale` render pixels per map pixel, `view` the map
    /// rectangle (x0, y0, x1, y1) that has to come out exact.
    View { project: Project, scale: f32, view: [f32; 4], id: u64 },
    Export { project: Project, scale: f32, path: PathBuf, variants: Variants },
}

#[derive(Clone, Copy)]
pub struct Variants {
    pub main: bool,
    pub no_shadow: bool,
    pub shadow_only: bool,
}

pub enum Msg {
    /// `extra` holds the layers of zones with their own settings, bottom first, to draw over `layers`.
    Preview { layers: Arc<RenderLayers>, extra: Vec<Arc<RenderLayers>>, id: u64, ms: u128 },
    GpuPreview { layers: Arc<forest_core::gpu::GpuLayers>, extra: Vec<Arc<forest_core::gpu::GpuLayers>>, id: u64, ms: u128 },
    /// A viewport render: its scale and where its top left pixel sits in render pixels.
    GpuView { layers: Arc<forest_core::gpu::GpuLayers>, extra: Vec<Arc<forest_core::gpu::GpuLayers>>, id: u64, ms: u128, scale: f32, origin: [usize; 2], core: [f32; 4] },
    Status(String),
    Exported(Result<Vec<PathBuf>, String>),
}

pub fn spawn(rx: Receiver<Job>, tx: Sender<Msg>, latest: Arc<AtomicU64>, ctx: eframe::egui::Context, gpu: Option<Arc<forest_core::gpu::Gpu>>) {
    std::thread::spawn(move || {
        let send = |m: Msg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        // Previews reuse the plan and stamps when only light, colour, snow or fire changed.
        // one cache per layer of zones, so changing one zone only redoes that zone
        let mut caches: Vec<RenderCache> = vec![RenderCache::default()];
        while let Ok(first) = rx.recv() {
            // Drain the queue; only the newest preview matters.
            let mut jobs = vec![first];
            while let Ok(j) = rx.try_recv() {
                jobs.push(j);
            }
            let last_preview = jobs.iter().rposition(|j| matches!(j, Job::Preview { .. } | Job::View { .. }));
            for (i, job) in jobs.into_iter().enumerate() {
                match job {
                    Job::Preview { project, scale, id } => {
                        if Some(i) != last_preview {
                            continue;
                        }
                        let t = Instant::now();
                        let latest = latest.clone();
                        let cancelled = move || latest.load(Ordering::Relaxed) != id;
                        let passes = zone_passes(&project);
                        caches.resize_with(passes.len(), RenderCache::default);
                        share_stamp_memory(&mut caches);
                        if let Some(g) = &gpu {
                            // only the plan comes from the CPU; stamping and shading run on the GPU
                            let mut all = Vec::new();
                            for (pi, pass) in passes.iter().enumerate() {
                                let Some(plan) = caches[pi].plan_only(pass, scale, &cancelled) else { break };
                                let (w, h) = render_size(pass, scale);
                                let region = forest_core::engine::Region { x0: 0, y0: 0, w, h };
                                let mut layers = g.render(pass, plan, scale, region);
                                layers.trees = plan.trees.clone();
                                all.push(Arc::new(layers));
                            }
                            if all.len() == passes.len() {
                                let layers = all.remove(0);
                                send(Msg::GpuPreview { layers, extra: all, id, ms: t.elapsed().as_millis() });
                            }
                        } else {
                            let mut all = Vec::new();
                            for (pi, pass) in passes.iter().enumerate() {
                                let Some(layers) = caches[pi].render(pass, scale, &cancelled) else { break };
                                all.push(Arc::new(layers));
                            }
                            if all.len() == passes.len() {
                                let layers = all.remove(0);
                                send(Msg::Preview { layers, extra: all, id, ms: t.elapsed().as_millis() });
                            }
                        }
                    }
                    Job::View { project, scale, view, id } => {
                        if Some(i) != last_preview {
                            continue;
                        }
                        let Some(g) = &gpu else { continue };
                        let t = Instant::now();
                        let latest = latest.clone();
                        let cancelled = move || latest.load(Ordering::Relaxed) != id;
                        // the plan stays at a coarse fixed scale; only the drawing follows the zoom
                        let (mw, mh) = (project.map_w as f32, project.map_h as f32);
                        let plan_scale = 0.5f32.min((40e6 / (mw * mh).max(1.0)).sqrt());
                        let passes = zone_passes(&project);
                        caches.resize_with(passes.len(), RenderCache::default);
                        let mut planned = true;
                        for (pi, pass) in passes.iter().enumerate() {
                            if caches[pi].plan_only(pass, plan_scale, &cancelled).is_none() {
                                planned = false;
                                break;
                            }
                        }
                        if planned {
                            // every layer is drawn over the same region, with the widest margin any of them needs
                            let pad = passes.iter().zip(&caches).map(|(p, c)| view_pad(p, c.current_plan().expect("planned"), scale)).max().unwrap_or(0) as f32;
                            let px = |v: f32, hi: f32| (v * scale).clamp(0.0, hi * scale);
                            let x0 = (px(view[0], mw) - pad).max(0.0).floor() as usize;
                            let y0 = (px(view[1], mh) - pad).max(0.0).floor() as usize;
                            let x1 = ((px(view[2], mw) + pad).min(mw * scale).ceil() as usize).max(x0 + 8);
                            let y1 = ((px(view[3], mh) + pad).min(mh * scale).ceil() as usize).max(y0 + 8);
                            let region = forest_core::engine::Region { x0, y0, w: x1 - x0, h: y1 - y0 };
                            let mut all = Vec::new();
                            for (pass, cache) in passes.iter().zip(&caches) {
                                let plan = cache.current_plan().expect("planned");
                                let mut layers = g.render(pass, plan, scale, region);
                                layers.trees = plan.trees.clone();
                                all.push(Arc::new(layers));
                            }
                            let layers = all.remove(0);
                            send(Msg::GpuView { layers, extra: all, id, ms: t.elapsed().as_millis(), scale, origin: [x0, y0], core: view });
                        }
                    }
                    Job::Export { project, scale, path, variants } => {
                        send(Msg::Status("Exporting...".into()));
                        // give the preview's kept work back while the export runs
                        caches.iter_mut().for_each(|c| c.clear());
                        let res = export(gpu.as_deref(), &project, scale, &path, variants, &mut |k| send(Msg::Status(format!("Exporting {:.0}%", k * 100.0))));
                        send(Msg::Exported(res));
                    }
                }
            }
        }
    });
}

/// Write the chosen variants strip by strip, so even very large maps fit in memory.
fn export(gpu: Option<&forest_core::gpu::Gpu>, p: &Project, scale: f32, path: &PathBuf, v: Variants, progress: &mut dyn FnMut(f32)) -> Result<Vec<PathBuf>, String> {
    let stem = path.with_extension("");
    let stem = stem.to_string_lossy();
    let base = ComposeOptions { shadows: p.light.shadows, shadow_only: false, background: None, premultiplied: false };
    let mut outputs = Vec::new();
    if v.main {
        outputs.push((PathBuf::from(format!("{stem}.png")), base));
    }
    if v.no_shadow {
        outputs.push((PathBuf::from(format!("{stem}_noshadow.png")), ComposeOptions { shadows: false, ..base }));
    }
    if v.shadow_only {
        outputs.push((PathBuf::from(format!("{stem}_shadow.png")), ComposeOptions { shadows: true, shadow_only: true, ..base }));
    }
    match gpu {
        Some(g) => export_strips_gpu(g, p, scale, &outputs, &|| false, progress)?,
        None => export_strips(p, scale, &outputs, &|| false, progress)?,
    }
    Ok(outputs.into_iter().map(|o| o.0).collect())
}

/// Layers share the memory kept for stamps, so many layers never hold more than one would.
fn share_stamp_memory(caches: &mut [RenderCache]) {
    let share = forest_core::render::KEEP_STAMPS_BYTES / caches.len().max(1);
    for c in caches.iter_mut() {
        c.set_stamp_budget(share);
    }
}
