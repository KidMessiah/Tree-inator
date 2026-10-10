//! Headless renderer for testing the engine.
//! Usage: forest-cli [project.json] [-out FILE] [-scale S] [-bg] [-strips] [-shy GAP [-shygiants]] [-strokes FILE] [-trees FILE]
//! `-strips` renders strip by strip, the way the app exports, so memory stays bounded. `-gpu` shades and
//! composites on the GPU (`-gpushade` uses CPU stamps, `-gpu` stamps on the GPU too).

use forest_core::*;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut project_path = None;
    let mut theme: Option<String> = None;
    let mut out = PathBuf::from("out.png");
    let mut scale = 1.0f32;
    let mut background = false;
    let mut strips = false;
    let mut gpu = false;
    let mut gpu_shade_only = false;
    let mut shy: Option<f32> = None;
    let mut shy_giants = false;
    let mut strokes_path: Option<PathBuf> = None;
    let mut trees_path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-out" => {
                i += 1;
                out = args[i].clone().into();
            }
            "-scale" => {
                i += 1;
                scale = args[i].parse().expect("scale");
            }
            "-bg" => background = true,
            "-strips" => strips = true,
            "-gpu" => gpu = true,
            "-gpushade" => {
                gpu = true;
                gpu_shade_only = true;
            }
            "-shy" => {
                i += 1;
                shy = Some(args[i].parse().expect("gap"));
            }
            "-shygiants" => shy_giants = true,
            "-theme" => {
                i += 1;
                theme = Some(args[i].clone());
            }
            "-trees" => {
                i += 1;
                trees_path = Some(args[i].clone().into());
            }
            "-strokes" => {
                i += 1;
                strokes_path = Some(args[i].clone().into());
            }
            p => project_path = Some(PathBuf::from(p)),
        }
        i += 1;
    }

    let project: Project = match &project_path {
        Some(p) => serde_json::from_str(&std::fs::read_to_string(p).expect("read project")).expect("parse project"),
        None => test_project(),
    };

    let mut project = project;
    project.ensure_mushroom_species();
    if let Some(name) = &theme {
        let t = themes().into_iter().find(|t| t.name.eq_ignore_ascii_case(name)).expect("unknown theme");
        project.apply_theme(&t);
    }
    if let Some(gap) = shy {
        project.gen.crown_shyness = true;
        project.gen.shyness_gap = gap;
        project.gen.shyness_giants_only = shy_giants;
    }
    if let Some(sp) = &strokes_path {
        apply_strokes(&mut project, sp);
        println!("strokes: {} painted trees, {} erased areas", project.painted.len(), project.erased.len());
    }

    let t = Instant::now();
    if strips {
        let opts = ComposeOptions { background: background.then_some([214.0 / 255.0, 205.0 / 255.0, 180.0 / 255.0]), ..Default::default() };
        if gpu {
            let g = forest_core::gpu::Gpu::headless().expect("no usable GPU");
            export_strips_gpu(&g, &project, scale, &[(out.clone(), opts)], &|| false, &mut |_| {}).expect("export");
        } else {
            export_strips(&project, scale, &[(out.clone(), opts)], &|| false, &mut |_| {}).expect("export");
        }
        println!("strips: {:.2}s", t.elapsed().as_secs_f32());
        println!("wrote {}", out.display());
        return;
    }
    if gpu {
        let g = forest_core::gpu::Gpu::headless().expect("no usable GPU");
        println!("gpu ready: {:.2}s", t.elapsed().as_secs_f32());
        let (w, h) = render_size(&project, scale);
        let bgc = background.then_some([214.0 / 255.0, 205.0 / 255.0, 180.0 / 255.0]);
        let passes = zone_passes(&project);
        let mut rgba: Vec<u8> = Vec::new();
        for (pi, pr) in passes.iter().enumerate() {
            let t = Instant::now();
            let plan = forest_core::engine::plan(pr, scale, &|| false).unwrap();
            let region = forest_core::engine::Region { x0: 0, y0: 0, w, h };
            let layers = if gpu_shade_only {
                let stamped = forest_core::engine::stamp_region(pr, &plan, scale, region, &|| false).unwrap();
                println!("plan and stamps: {:.2}s", t.elapsed().as_secs_f32());
                let t = Instant::now();
                let layers = g.shade(pr, &plan, &stamped);
                g.wait();
                println!("gpu shade: {:.3}s", t.elapsed().as_secs_f32());
                layers
            } else {
                println!("plan: {:.2}s", t.elapsed().as_secs_f32());
                let t = Instant::now();
                let layers = g.render(pr, &plan, scale, region);
                g.wait();
                println!("gpu stamp and shade: {:.3}s", t.elapsed().as_secs_f32());
                layers
            };
            let t = Instant::now();
            let many = passes.len() > 1;
            let opts = ComposeOptions { background: if many { None } else { bgc }, ..Default::default() };
            let one = g.compose_rgba(&layers, &pr.grading, opts);
            println!("gpu compose and read back: {:.3}s", t.elapsed().as_secs_f32());
            if pi == 0 {
                rgba = one;
                if many {
                    if let Some(bg) = bgc {
                        let mut base = vec![0u8; rgba.len()];
                        forest_core::stack::fill_background(&mut base, bg);
                        forest_core::stack::over(&mut base, &rgba);
                        rgba = base;
                    }
                }
            } else {
                forest_core::stack::over(&mut rgba, &one);
            }
        }
        image::save_buffer(&out, &rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8).expect("save");
        println!("wrote {} ({} layers)", out.display(), passes.len());
        return;
    }
    let stack = generate_stack(&project, scale, &|| false).unwrap();
    let layers = &stack[0];
    println!(
        "generate: {:.2}s  {}x{}  canopy {} shrubs {} lone {} pebbles {}",
        t.elapsed().as_secs_f32(),
        layers.w,
        layers.h,
        layers.tree_counts[0],
        layers.tree_counts[1],
        layers.tree_counts[2],
        layers.tree_counts[3]
    );
    println!("fit: edge pulled in {:.0} px", layers.fit_inset);
    if let Some(tp) = &trees_path {
        std::fs::write(tp, serde_json::to_string(&layers.trees).expect("trees json")).expect("write trees");
        println!("trees: {} written to {}", layers.trees.len(), tp.display());
    }
    let t = Instant::now();
    let opts = ComposeOptions {
        background: background.then_some([214.0 / 255.0, 205.0 / 255.0, 180.0 / 255.0]),
        ..Default::default()
    };
    let rgba = compose_stack(&stack, &project.grading, opts);
    println!("compose: {:.3}s", t.elapsed().as_secs_f32());
    image::save_buffer(&out, &rgba, layers.w as u32, layers.h as u32, image::ExtendedColorType::Rgba8).expect("save");
    println!("wrote {}", out.display());
}

/// Same outline as the Python prototype, scaled to the default map.
fn test_project() -> Project {
    let mut p = Project::default();
    let k = 1.6;
    let pts = [(180., 260.), (560., 150.), (980., 220.), (1100., 480.), (860., 760.), (450., 720.), (220., 600.)];
    p.shapes.push(Shape::new(ShapeOp::Add, pts.iter().map(|&(x, y)| [x * k, y * k]).collect()));
    p
}

/// A recorded brush stroke: settings plus the cursor path in map pixels.
#[derive(serde::Deserialize)]
struct Stroke {
    brush: BrushSettings,
    points: Vec<[f32; 2]>,
}

/// Replays strokes through the same brush code the app uses, with dabs spaced along the path.
fn apply_strokes(project: &mut Project, path: &PathBuf) {
    let strokes: Vec<Stroke> = serde_json::from_str(&std::fs::read_to_string(path).expect("read strokes")).expect("parse strokes");
    let mut rng = forest_core::rng::Rng::new(0x7EE5);
    for st in strokes {
        let b = &st.brush;
        let dab = |p: &mut Project, x: f32, y: f32, rng: &mut forest_core::rng::Rng| match b.mode {
            BrushMode::Paint => {
                paint_dab(p, b, x, y, rng);
            }
            BrushMode::Single => stamp_one(p, b, x, y, rng),
            BrushMode::Erase => {
                erase_dab(p, b, x, y);
            }
        };
        let Some(first) = st.points.first() else { continue };
        dab(project, first[0], first[1], &mut rng);
        if b.mode == BrushMode::Single {
            for q in &st.points[1..] {
                dab(project, q[0], q[1], &mut rng);
            }
            continue;
        }
        let mut last = *first;
        let step = b.dab_step();
        for q in &st.points[1..] {
            let (dx, dy) = (q[0] - last[0], q[1] - last[1]);
            let dist = (dx * dx + dy * dy).sqrt();
            let n = (dist / step).floor() as usize;
            for k in 1..=n {
                let t = k as f32 * step / dist;
                dab(project, last[0] + dx * t, last[1] + dy * t, &mut rng);
            }
            if n > 0 {
                let t = n as f32 * step / dist;
                last = [last[0] + dx * t, last[1] + dy * t];
            }
        }
    }
}
