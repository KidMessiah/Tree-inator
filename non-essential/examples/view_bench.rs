//! Times viewport renders on the GPU with a kept plan: full detail against half detail.
//! Usage: view_bench project.json [width height]
use forest_core::engine::{plan, Region};
use forest_core::*;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut p: Project = serde_json::from_str(&std::fs::read_to_string(&args[0]).unwrap()).unwrap();
    p.ensure_mushroom_species();
    let (vw, vh): (f32, f32) = (args.get(1).map_or(2560.0, |v| v.parse().unwrap()), args.get(2).map_or(1440.0, |v| v.parse().unwrap()));
    let g = forest_core::gpu::Gpu::headless().expect("gpu");
    let t = Instant::now();
    let pl = plan(&p, 0.5, &|| false).unwrap();
    println!("plan {:.2}s", t.elapsed().as_secs_f32());
    for scale in [1.0f32, 0.5, 0.25] {
        let pad = view_pad(&p, &pl, scale) as f32;
        let region = Region { x0: 2000, y0: 1500, w: (vw + 2.0 * pad) as usize, h: (vh + 2.0 * pad) as usize };
        for run in 0..3 {
            let t = Instant::now();
            let _ = g.render(&p, &pl, scale, region);
            g.wait();
            println!("view {}x{} pad {} at scale {scale}: run {run} {:.3}s", region.w, region.h, pad, t.elapsed().as_secs_f32());
        }
    }
}
