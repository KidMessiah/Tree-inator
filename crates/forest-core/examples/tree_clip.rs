//! Counts planned trees whose crown crosses the drawn line, which the clip would cut.
//! Usage: tree_clip project.json [plan_scale]
use forest_core::*;

fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = (dx * dx + dy * dy).max(1e-6);
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l).clamp(0.0, 1.0);
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut p: Project = serde_json::from_str(&std::fs::read_to_string(&a[0]).unwrap()).unwrap();
    p.ensure_mushroom_species();
    let scale: f32 = a.get(1).map_or(0.5, |v| v.parse().unwrap());
    let pl = forest_core::engine::plan(&p, scale, &|| false).unwrap();
    let (mut n, mut cross, mut outside, mut deep) = (0, 0, 0, 0);
    let mut over: Vec<f32> = Vec::new();
    for t in &pl.trees {
        let mut best = f32::MAX;
        let mut inside = false;
        for z in &p.shapes {
            let pts = &z.points;
            let mut j = pts.len() - 1;
            let mut c = false;
            for i in 0..pts.len() {
                let (u, v) = (pts[i], pts[j]);
                best = best.min(seg_dist([t.x, t.y], u, v));
                if (u[1] > t.y) != (v[1] > t.y) && t.x < (v[0] - u[0]) * (t.y - u[1]) / (v[1] - u[1]) + u[0] {
                    c = !c;
                }
                j = i;
            }
            inside |= c;
        }
        n += 1;
        if !inside {
            outside += 1;
        } else if best < t.r {
            cross += 1;
            over.push(t.r - best);
        } else {
            deep += 1;
        }
    }
    over.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if !over.is_empty() {
        println!("crown past the line: median {:.1}, 90% {:.1}, max {:.1} map px", over[over.len() / 2], over[over.len() * 9 / 10], over[over.len() - 1]);
    }
    println!("{n} trees: {deep} clear of the line, {cross} crown crosses it, {outside} centre outside");
}
