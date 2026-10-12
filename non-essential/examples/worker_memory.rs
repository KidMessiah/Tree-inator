//! Keeps what the editor keeps for a project with layered zones: one render cache per layer with its plan, and the
//! GPU layers of a viewport render, all alive together. Run it under a memory guard to see the peak.
//! Usage: worker_memory project.json [scale]
use forest_core::engine::Region;
use forest_core::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut p: Project = serde_json::from_str(&std::fs::read_to_string(&args[0]).unwrap()).unwrap();
    p.ensure_mushroom_species();
    let scale: f32 = args.get(1).map_or(1.0, |v| v.parse().unwrap());
    let g = forest_core::gpu::Gpu::headless().expect("gpu");
    let passes = zone_passes(&p);
    let mut caches: Vec<RenderCache> = passes.iter().map(|_| RenderCache::default()).collect();
    let plan_scale = 0.5f32.min((40e6 / (p.map_w as f32 * p.map_h as f32).max(1.0)).sqrt());
    for (c, pass) in caches.iter_mut().zip(&passes) {
        c.plan_only(pass, plan_scale, &|| false).expect("plan");
    }
    let region = Region { x0: 1000, y0: 800, w: 3000, h: 2000 };
    let mut layers = Vec::new();
    for (c, pass) in caches.iter().zip(&passes) {
        layers.push(g.render(pass, c.current_plan().unwrap(), scale, region));
    }
    g.wait();
    println!("{} layers kept: plans and a {}x{} view each", layers.len(), region.w, region.h);
    std::thread::sleep(std::time::Duration::from_millis(1500));
}
