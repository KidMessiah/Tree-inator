//! Try canopy settings on one preset. Writes a PPM with one row per setting (three variants each) and prints
//! how much of each crown is gaps and how much is visible wood.
//!
//! canopy_lab OUT.ppm PRESET SPEC...   where SPEC is `raw`, `def`, or leaves,twigs,twig_len,leaf_size,inner,sky[,rise[,width[,limbs[,wood[,limb_base[,limb_rise[,light]]]]]]]

use forest_core::treegen::Canopy;
use forest_core::treegen_sprite::{build_sprite_with, canopy_for};
use std::io::Write;

fn parse(name: &str, s: &str) -> Canopy {
    match s {
        "raw" => Canopy::RAW,
        "def" => canopy_for(name),
        _ if s.starts_with("def+") => Canopy { light: s[4..].parse().unwrap(), ..canopy_for(name) },
        _ => {
            let v: Vec<f64> = s.split(',').map(|x| x.parse().unwrap()).collect();
            Canopy { leaves: v[0], twigs: v[1], twig_len: v[2], leaf_size: v[3], inner: v[4], sky: v[5], rise: *v.get(6).unwrap_or(&0.0), leaf_width: *v.get(7).unwrap_or(&1.0), blossom_size: 1.0, limbs: *v.get(8).unwrap_or(&1.0), wood: *v.get(9).unwrap_or(&1.0), limb_base: *v.get(10).unwrap_or(&1.0), limb_rise: *v.get(11).unwrap_or(&0.0), light: *v.get(12).unwrap_or(&0.0) }
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (out, name) = (&a[1], &a[2]);
    let r0 = 60.0f32;
    let mut rows: Vec<Vec<forest_core::wp::Sprite>> = Vec::new();
    for spec in &a[3..] {
        let c = parse(name, spec);
        let t = std::time::Instant::now();
        let row: Vec<_> = (0..3).map(|v| build_sprite_with(name, v, &c, r0).unwrap()).collect();
        let mut line = format!("{spec:>28} {:5.1}s ", t.elapsed().as_secs_f32());
        for sp in &row {
            let (gaps, wood) = metrics(sp, r0);
            line += &format!(" gaps {:4.1}% wood {:4.1}% |", gaps * 100.0, wood * 100.0);
        }
        println!("{line}");
        // mean height by distance from the centre, as a share of the tallest point
        let sp = &row[0];
        let mid = sp.size as f32 / 2.0;
        let mut bins = [(0f32, 0u32); 6];
        for y in 0..sp.size {
            for x in 0..sp.size {
                let i = y * sp.size + x;
                if sp.cls[i] != 0 {
                    let d = ((x as f32 + 0.5 - mid).powi(2) + (y as f32 + 0.5 - mid).powi(2)).sqrt() / r0;
                    let b = ((d * 5.0) as usize).min(5);
                    bins[b].0 += sp.z[i] / sp.zmax;
                    bins[b].1 += 1;
                }
            }
        }
        println!("{:>36}", bins.iter().map(|(z, n)| format!("{:.2}", z / (*n).max(1) as f32)).collect::<Vec<_>>().join(" "));
        rows.push(row);
    }
    let cell = rows[0][0].size;
    let (w, h) = (cell * 3, cell * rows.len());
    let mut img = vec![[214u8, 202, 176]; w * h];
    for (ri, row) in rows.iter().enumerate() {
        for (ci, sp) in row.iter().enumerate() {
            for y in 0..sp.size.min(cell) {
                for x in 0..sp.size.min(cell) {
                    let i = y * sp.size + x;
                    if sp.cls[i] == 0 {
                        continue;
                    }
                    let (nx, ny) = (sp.nx[i], sp.ny[i]);
                    let l = ((-0.5 * nx - 0.6 * ny + 1.0) / (nx * nx + ny * ny + 1.0).sqrt() / 1.25).clamp(0.2, 1.1);
                    let d = 0.45 + 0.75 * sp.depth[i];
                    let c = if sp.cls[i] == 2 {
                        [0.42, 0.30, 0.20]
                    } else {
                        let t = 0.6 + 0.8 * sp.tone[i];
                        [0.30 * t, 0.50 * t, 0.18 * t]
                    };
                    let px = &mut img[(ri * cell + y) * w + ci * cell + x];
                    *px = c.map(|v| ((v * l * d).clamp(0.0, 1.0) * 255.0) as u8);
                }
            }
        }
    }
    let mut f = std::fs::File::create(out).unwrap();
    write!(f, "P6\n{w} {h}\n255\n").unwrap();
    f.write_all(&img.concat()).unwrap();
}

/// Gaps: empty pixels inside the crown outline (the filled shape after closing small holes). Wood: bark share
/// of what is drawn.
fn metrics(sp: &forest_core::wp::Sprite, r0: f32) -> (f32, f32) {
    let n = sp.size;
    let occ: Vec<bool> = sp.cls.iter().map(|c| *c != 0).collect();
    let rad = (0.12 * r0) as i32;
    let morph = |src: &[bool], grow: bool| -> Vec<bool> {
        let mut out = vec![!grow; n * n];
        for y in 0..n as i32 {
            for x in 0..n as i32 {
                let mut hit = !grow;
                'o: for dy in -rad..=rad {
                    for dx in -rad..=rad {
                        if dx * dx + dy * dy > rad * rad {
                            continue;
                        }
                        let (xx, yy) = (x + dx, y + dy);
                        let v = xx >= 0 && yy >= 0 && xx < n as i32 && yy < n as i32 && src[yy as usize * n + xx as usize];
                        if grow && v {
                            hit = true;
                            break 'o;
                        }
                        if !grow && !v {
                            hit = false;
                            break 'o;
                        }
                    }
                }
                out[y as usize * n + x as usize] = hit;
            }
        }
        out
    };
    let closed = morph(&morph(&occ, true), false);
    let (mut inside, mut empty, mut drawn, mut bark) = (0u32, 0u32, 0u32, 0u32);
    for i in 0..n * n {
        if closed[i] {
            inside += 1;
            empty += !occ[i] as u32;
        }
        if occ[i] {
            drawn += 1;
            bark += (sp.cls[i] == 2) as u32;
        }
    }
    (empty as f32 / inside.max(1) as f32, bark as f32 / drawn.max(1) as f32)
}
