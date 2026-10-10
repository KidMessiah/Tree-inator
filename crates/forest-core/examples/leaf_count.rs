//! How many leaves a tree type grows, and how long it takes.
use forest_core::treegen::{generate, preset_by_name, Rand};
use std::time::Instant;

fn main() {
    let name = std::env::args().nth(1).unwrap_or("weeping_willow".into());
    let canopy = forest_core::treegen_sprite::canopy_for(&name);
    let preset = preset_by_name(&name).unwrap();
    let t = Instant::now();
    let geo = generate(preset, 5, &canopy);
    println!("{name}: {} leaves, {} branch curves, grown in {:.1}s", geo.leaves.len(), geo.splines.len(), t.elapsed().as_secs_f32());
    let t = Instant::now();
    let polys = geo.polygons(&mut Rand::new(1));
    println!("{} polygons in {:.1}s", polys.len(), t.elapsed().as_secs_f32());
}
