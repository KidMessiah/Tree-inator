//! Time a first preview, then previews after changing only light or snow, through the render cache.
//! cache_bench PROJECT.json

use forest_core::*;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("project path");
    let mut p: Project = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut cache = RenderCache::default();
    let mut run = |p: &Project, label: &str| {
        let t = Instant::now();
        cache.render(p, 1.0, &|| false).unwrap();
        println!("{label:>22}: {:.2}s", t.elapsed().as_secs_f32());
    };
    run(&p, "first");
    p.light.shadow_angle += 40.0;
    run(&p, "light changed");
    p.gen.snow.amount = 0.8;
    run(&p, "snow changed");
    p.gen.canopy_cover = 0.6;
    run(&p, "density changed");
}
