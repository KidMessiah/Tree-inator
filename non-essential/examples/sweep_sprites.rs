//! Builds every tree type at several sizes and variants and reports anything empty, missing or slow.
//! One at a time, so it stays light. Usage: sweep_sprites
use forest_core::treegen_sprite::{sprite_for_radius, TREE_TYPES, VARIANTS};
use std::time::Instant;

fn main() {
    let mut bad = 0;
    let mut total = 0;
    for (name, _) in TREE_TYPES {
        let heavy = name.starts_with("weeping_willow") || *name == "dead_willow";
        let radii: &[f32] = if heavy { &[4.0, 25.0] } else { &[1.0, 2.5, 6.0, 12.0, 25.0, 60.0, 140.0] };
        for v in 0..VARIANTS {
            for &r in radii {
                let t = Instant::now();
                total += 1;
                match sprite_for_radius(name, v, r, -1.0) {
                    None => {
                        println!("MISSING {name} v{v} r{r}");
                        bad += 1;
                    }
                    Some(s) => {
                        let solid = s.cls.iter().filter(|c| **c != 0).count();
                        if r >= 3.0 && solid == 0 {
                            println!("EMPTY {name} v{v} r{r}");
                            bad += 1;
                        }
                        if s.z.iter().any(|z| !z.is_finite()) || s.nx.iter().any(|z| !z.is_finite()) {
                            println!("NAN {name} v{v} r{r}");
                            bad += 1;
                        }
                    }
                }
                let ms = t.elapsed().as_millis();
                if ms > 15_000 {
                    println!("SLOW {name} v{v} r{r}: {ms} ms");
                }
            }
        }
        println!("{name} done");
    }
    println!("{total} sprites, {bad} problems");
}
