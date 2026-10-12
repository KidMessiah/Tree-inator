//! Draws a project the way the editor does, with the plan at a coarser scale than the picture, and counts how
//! much of the forest lands outside the zones. Usage: view_spill project.json out.png plan_scale draw_scale
use forest_core::engine::{plan, Region};
use forest_core::*;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut p: Project = serde_json::from_str(&std::fs::read_to_string(&a[0]).unwrap()).unwrap();
    p.ensure_mushroom_species();
    p.light.shadows = false;
    let (ps, ds): (f32, f32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let g = forest_core::gpu::Gpu::headless().expect("gpu");
    let (w, h) = render_size(&p, ds);
    let pl = plan(&p, ps, &|| false).unwrap();
    let layers = g.render(&p, &pl, ds, Region { x0: 0, y0: 0, w, h });
    let rgba = g.compose_rgba(&layers, &p.grading, ComposeOptions::default());
    // pixels covered, and how many of them are farther than 4 px outside every zone
    let inside = |x: f32, y: f32| {
        p.shapes.iter().any(|z| {
            let pts = &z.points;
            let (mut c, mut j) = (false, pts.len() - 1);
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[j]);
                if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                    c = !c;
                }
                j = i;
            }
            c
        })
    };
    let near = |x: f32, y: f32| (-1..=1).any(|dy| (-1..=1).any(|dx| inside(x + dx as f32 * 4.0 / ds, y + dy as f32 * 4.0 / ds)));
    let (mut covered, mut out) = (0usize, 0usize);
    let mut vis = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if rgba[i * 4 + 3] > 8 {
                covered += 1;
                let (mx, my) = ((x as f32 + 0.5) / ds, (y as f32 + 0.5) / ds);
                if !near(mx, my) {
                    out += 1;
                    vis[i * 3] = 255;
                } else {
                    vis[i * 3 + 1] = 120;
                }
            }
        }
    }
    println!("plan at {ps}, drawn at {ds}: {covered} covered pixels, {out} outside the zones ({:.2}%)", 100.0 * out as f32 / covered.max(1) as f32);
    let file = std::io::BufWriter::new(std::fs::File::create(&a[1]).unwrap());
    let mut enc = png::Encoder::new(file, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&vis).unwrap();
}
