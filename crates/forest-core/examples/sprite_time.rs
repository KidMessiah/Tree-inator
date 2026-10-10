//! Times building one sprite of a tree type at a few radii.
use forest_core::treegen_sprite::build_sprite;
use std::time::Instant;

fn main() {
    let name = std::env::args().nth(1).unwrap_or("dead_oak".into());
    for r in [6.0f32, 12.0, 25.0, 50.0, 96.0] {
        let t = Instant::now();
        let s = build_sprite(&name, 1, -1.0, r).unwrap();
        println!("{name} r0 {r}: {:.0} ms, {} px", t.elapsed().as_secs_f32() * 1000.0, s.size);
    }
}
