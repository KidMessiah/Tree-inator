//! Times a brush edit through the render cache against planning from scratch.
//! Usage: edit_bench project.json [scale]
use forest_core::*;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut p: Project = serde_json::from_str(&std::fs::read_to_string(&args[0]).unwrap()).unwrap();
    p.ensure_mushroom_species();
    let scale: f32 = args.get(1).map_or(0.5, |v| v.parse().unwrap());
    let mut cache = RenderCache::default();
    let t = Instant::now();
    cache.plan_only(&p, scale, &|| false).unwrap();
    println!("first plan:       {:.3}s", t.elapsed().as_secs_f32());
    for k in 0..3 {
        p.painted.push(TreeRec { x: 900.0 + 40.0 * k as f32, y: 900.0, r: 14.0, h: 24.0, tone: 0.0, layer: TreeLayer::Canopy, species: 0, seed: k });
        p.erased.push([1500.0, 1200.0 + 30.0 * k as f32, 50.0]);
        let t = Instant::now();
        let n = cache.plan_only(&p, scale, &|| false).unwrap().trees.len();
        println!("edit {k}:           {:.3}s  ({n} trees)", t.elapsed().as_secs_f32());
    }
    let t = Instant::now();
    let n = forest_core::engine::plan(&p, scale, &|| false).unwrap().trees.len();
    println!("fresh plan:       {:.3}s  ({n} trees)", t.elapsed().as_secs_f32());
}
