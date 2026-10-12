//! Writes a picture of how far the border fit pulls the forest in from the drawn line: brighter is further in.
//! Usage: pull_in_map project.json out.png [scale]
use forest_core::*;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut p: Project = serde_json::from_str(&std::fs::read_to_string(&a[0]).unwrap()).unwrap();
    p.ensure_mushroom_species();
    let scale: f32 = a.get(2).map_or(0.5, |v| v.parse().unwrap());
    let plan = forest_core::engine::plan(&p, scale, &|| false).unwrap();
    let (v, gw, gh, cs) = plan.pull_in();
    let max = v.iter().cloned().fold(1.0f32, f32::max);
    println!("grid {gw}x{gh}, cell {cs}, largest pull-in {max:.1}");
    // the average pull-in over the cells inside each zone
    let inside = |pts: &[[f32; 2]], x: f32, y: f32| {
        let mut c = false;
        let mut j = pts.len() - 1;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[j]);
            if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                c = !c;
            }
            j = i;
        }
        c
    };
    let means: Vec<String> = p
        .shapes
        .iter()
        .map(|z| {
            let (mut sum, mut n) = (0.0f32, 0);
            for gy in 0..gh {
                for gx in 0..gw {
                    let (x, y) = ((gx * cs + cs / 2) as f32 / scale, (gy * cs + cs / 2) as f32 / scale);
                    if inside(&z.points, x, y) {
                        sum += v[gy * gw + gx];
                        n += 1;
                    }
                }
            }
            format!("{:.1}", if n > 0 { sum / n as f32 } else { 0.0 })
        })
        .collect();
    println!("mean pull-in inside each zone: {}", means.join(" "));
    let mut img = vec![0u8; gw * gh * 3];
    for i in 0..gw * gh {
        let g = (v[i] / max * 255.0) as u8;
        img[i * 3] = g;
        img[i * 3 + 1] = g;
        img[i * 3 + 2] = g;
    }
    // the zone outlines in red
    for z in p.shapes.iter().filter(|z| z.visible) {
        for k in 0..z.points.len() {
            let (p0, p1) = (z.points[k], z.points[(k + 1) % z.points.len()]);
            let n = 400;
            for t in 0..=n {
                let f = t as f32 / n as f32;
                let (x, y) = ((p0[0] + (p1[0] - p0[0]) * f) * scale / cs as f32, (p0[1] + (p1[1] - p0[1]) * f) * scale / cs as f32);
                if x >= 0.0 && y >= 0.0 && (x as usize) < gw && (y as usize) < gh {
                    let i = (y as usize * gw + x as usize) * 3;
                    img[i] = 255;
                    img[i + 1] = 40;
                    img[i + 2] = 40;
                }
            }
        }
    }
    let file = std::io::BufWriter::new(std::fs::File::create(&a[1]).unwrap());
    let mut enc = png::Encoder::new(file, gw as u32, gh as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&img).unwrap();
}
