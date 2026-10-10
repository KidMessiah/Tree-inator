use forest_core::*;

fn project() -> Project {
    let mut p = Project { map_w: 512, map_h: 384, ..Default::default() };
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[60.0, 60.0], [450.0, 80.0], [420.0, 320.0], [90.0, 300.0]]));
    p
}

#[test]
fn deterministic_for_seed() {
    let p = project();
    let a = compose(&generate(&p, 0.5, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    let b = compose(&generate(&p, 0.5, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    assert_eq!(a, b);
}

#[test]
fn empty_project_is_transparent() {
    let p = Project { map_w: 128, map_h: 128, ..Default::default() };
    let r = generate(&p, 1.0, &|| false).unwrap();
    let rgba = compose(&r, &p.grading, ComposeOptions::default());
    assert!(rgba.chunks(4).all(|px| px[3] == 0));
}

#[test]
fn forest_has_canopy_and_layers() {
    let p = project();
    let r = generate(&p, 1.0, &|| false).unwrap();
    assert_eq!(r.layers.len(), LayerKind::ALL.len());
    assert!(r.tree_counts[0] > 50, "canopy trees: {}", r.tree_counts[0]);
    let canopy = &r.layers[LayerKind::Canopy.index()];
    let covered = canopy.alphas(r.h).filter(|a| *a > 0.5).count() as f32 / (r.w * r.h) as f32;
    assert!(covered > 0.1 && covered < 0.9, "canopy coverage {covered}");
}

#[test]
fn shadowless_has_no_shadow() {
    let mut p = project();
    p.light.shadows = false;
    let r = generate(&p, 0.5, &|| false).unwrap();
    assert!(r.layers.iter().all(|l| !l.has_shadow()));
    assert!(!r.layers[LayerKind::GroundShadow.index()].has_alpha());
}

#[test]
fn cancel_stops_generation() {
    assert!(generate(&project(), 0.5, &|| true).is_none());
}

#[test]
fn fbm_spread_matches_unit_scaling() {
    // unit() in the engine assumes an fBm spread of about 0.12.
    let mut v = Vec::new();
    for y in 0..200 {
        for x in 0..200 {
            v.push(forest_core::noise::fbm(x as f32 * 7.3, y as f32 * 5.1, 64.0, 5, 0.5, 3));
        }
    }
    let m = v.iter().sum::<f32>() / v.len() as f32;
    let sd = (v.iter().map(|a| (a - m) * (a - m)).sum::<f32>() / v.len() as f32).sqrt();
    println!("fbm mean {m} sd {sd}");
    assert!(sd > 0.07 && sd < 0.2, "sd {sd}");
}

#[test]
fn signed_distance_matches_brute_force() {
    use forest_core::field::{signed_distance, Field};
    let (w, h) = (37, 23);
    let mut rng = forest_core::rng::Rng::new(9);
    let mut m = Field::new(w, h);
    for v in m.data.iter_mut() {
        *v = (rng.f32() < 0.15) as u8 as f32;
    }
    let sdf = signed_distance(&m);
    for y in 0..h {
        for x in 0..w {
            let inside = m.get(x, y) > 0.5;
            let mut best = f32::MAX;
            for yy in 0..h {
                for xx in 0..w {
                    if (m.get(xx, yy) > 0.5) != inside {
                        let d = (((x as f32 - xx as f32).powi(2) + (y as f32 - yy as f32).powi(2)) as f32).sqrt();
                        best = best.min(d);
                    }
                }
            }
            let want = if inside { best - 0.5 } else { -(best - 0.5) };
            let got = sdf.get(x, y);
            assert!((got - want).abs() < 1e-3, "({x},{y}) got {got} want {want}");
        }
    }
}

#[test]
fn brush_paints_spaced_trees_and_erases() {
    let mut p = project();
    let b = BrushSettings { replace_generated: false, ..Default::default() };
    let mut rng = forest_core::rng::Rng::new(1);
    let n = paint_dab(&mut p, &b, 250.0, 200.0, &mut rng);
    assert!(n > 5, "painted {n}");
    // Painting the same spot again adds little: spacing is respected.
    let again = paint_dab(&mut p, &b, 250.0, 200.0, &mut rng);
    assert!(again < n / 2, "second dab {again} vs {n}");
    for (i, a) in p.painted.iter().enumerate() {
        for c in &p.painted[i + 1..] {
            let d = ((a.x - c.x).powi(2) + (a.y - c.y).powi(2)).sqrt();
            assert!(d >= 0.5 * (a.r + c.r), "trees overlap: {d}");
        }
    }
    let removed = erase_dab(&mut p, &b, 250.0, 200.0);
    assert_eq!(removed, n + again);
    assert_eq!(p.erased.len(), 1);
}

#[test]
fn painted_trees_render_and_fallback_switches_work() {
    let mut p = Project { map_w: 512, map_h: 384, ..Default::default() };
    let b = BrushSettings::default();
    let mut rng = forest_core::rng::Rng::new(2);
    paint_dab(&mut p, &b, 256.0, 192.0, &mut rng);
    let painted = p.painted.len();
    let r = generate(&p, 1.0, &|| false).unwrap();
    assert_eq!(r.tree_counts[0], painted, "painted trees render with no shape drawn");
    p.gen.painted_trees = false;
    let r = generate(&p, 1.0, &|| false).unwrap();
    assert_eq!(r.tree_counts[0], 0);
}

#[test]
fn erasing_removes_generated_trees_without_moving_others() {
    let mut p = project();
    let full = generate(&p, 0.5, &|| false).unwrap().tree_counts[0];
    p.erased.push([250.0, 190.0, 60.0]);
    let less = generate(&p, 0.5, &|| false).unwrap().tree_counts[0];
    assert!(less < full, "{less} vs {full}");
    p.gen.procedural_trees = false;
    assert_eq!(generate(&p, 0.5, &|| false).unwrap().tree_counts[0], 0);
}

#[test]
fn sprite_trees_render_and_stay_deterministic() {
    let mut p = project();
    let r1 = generate(&p, 1.0, &|| false).unwrap();
    let a = compose(&r1, &p.grading, ComposeOptions::default());
    let b = compose(&generate(&p, 1.0, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    assert_eq!(a, b, "same seed, same trees");
    p.gen.seed += 1;
    let other = compose(&generate(&p, 1.0, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    assert_ne!(a, other, "a different seed grows a different forest");
    assert!(r1.tree_counts[0] > 50);
    let covered = a.chunks(4).filter(|px| px[3] > 200).count();
    assert!(covered > a.len() / 4 / 10, "sprite canopy still covers the patch: {covered}");
}

#[test]
fn sprite_generator_makes_distinct_species() {
    use forest_core::wp::{make_sprite, SpriteSpec};
    let mk = |form, bare| SpriteSpec { form, branchiness: 1.0, clumpiness: 1.0, leaf_cover: 1.0, leaf_px: 2.6, leaf_elong: 1.3, along: false, bare };
    let oak = make_sprite(&mk(Form::Oak, false), 14.0, 1);
    let oak2 = make_sprite(&mk(Form::Oak, false), 14.0, 2);
    let dead = make_sprite(&mk(Form::Dead, true), 14.0, 1);
    let filled = |s: &forest_core::wp::Sprite| s.cls.iter().filter(|c| **c != 0).count();
    assert!(filled(&oak) > 200, "oak fills its crown: {}", filled(&oak));
    assert!(filled(&dead) < filled(&oak), "a bare tree covers less than a leafy one");
    assert!(dead.cls.iter().all(|c| *c != 1), "dead trees have no leaves");
    assert_ne!(oak.z, oak2.z, "different seeds give different trees");
    assert_eq!(make_sprite(&mk(Form::Oak, false), 14.0, 1).z, oak.z, "same seed, same tree");
    assert!(oak.depth.iter().cloned().fold(0.0, f32::max) <= 1.0);
}

#[test]
fn canopy_depth_adds_large_scale_brightness_variation() {
    // Brightness averaged over 24 px blocks should vary more with depth on than off.
    fn block_spread(rgba: &[u8], w: usize, h: usize) -> f32 {
        let b = 24;
        let mut means = Vec::new();
        for by in 0..h / b {
            for bx in 0..w / b {
                let (mut sum, mut n) = (0f32, 0f32);
                for y in by * b..by * b + b {
                    for x in bx * b..bx * b + b {
                        let i = (y * w + x) * 4;
                        if rgba[i + 3] > 250 {
                            sum += (rgba[i] as f32 + rgba[i + 1] as f32 + rgba[i + 2] as f32) / 3.0;
                            n += 1.0;
                        }
                    }
                }
                if n > (b * b) as f32 * 0.9 {
                    means.push(sum / n);
                }
            }
        }
        let m = means.iter().sum::<f32>() / means.len().max(1) as f32;
        (means.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / means.len().max(1) as f32).sqrt()
    }
    let mut p = Project { map_w: 512, map_h: 384, ..Default::default() };
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[0.0, 0.0], [512.0, 0.0], [512.0, 384.0], [0.0, 384.0]]));
    p.gen.canopy_cover = 1.0;
    p.gen.depth = DepthStyle::cg2a_match();
    let on = compose(&generate(&p, 1.0, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    p.gen.depth.crown_tone = 0.0;
    p.gen.depth.crown_shade = 0.0;
    p.gen.depth.height_var = 0.0;
    p.gen.depth.crown_var = 0.0;
    let off = compose(&generate(&p, 1.0, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    let (a, b) = (block_spread(&on, 512, 384), block_spread(&off, 512, 384));
    assert!(a > b * 1.4, "depth on {a} should vary more than off {b}");
}

#[test]
fn forest_type_changes_every_generated_tree() {
    let mut p = project();
    let broad = compose(&generate(&p, 1.0, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    p.gen.main_species = 1; // conifer
    let r = generate(&p, 1.0, &|| false).unwrap();
    let pine = compose(&r, &p.grading, ComposeOptions::default());
    assert_ne!(broad, pine, "a conifer forest looks different from a broadleaf one");
    // Conifer ramp is darker and bluer than broadleaf, so the canopy average green must drop.
    let mean_g = |v: &[u8]| {
        let (mut s, mut n) = (0f64, 0f64);
        for px in v.chunks(4) {
            if px[3] > 250 {
                s += px[1] as f64;
                n += 1.0;
            }
        }
        s / n.max(1.0)
    };
    assert!(mean_g(&pine) < mean_g(&broad), "conifer forest is darker: {} vs {}", mean_g(&pine), mean_g(&broad));
    // Tree counts are untouched: only the species changes.
    p.gen.main_species = 0;
    let base = generate(&p, 1.0, &|| false).unwrap();
    assert_eq!(base.tree_counts, r.tree_counts);
}

/// Pixels with any colour outside the box [x0, x1] x [y0, y1], allowing `slack` pixels of soft edge.
fn outside_box(rgba: &[u8], w: usize, h: usize, b: (usize, usize, usize, usize), slack: usize) -> usize {
    let mut n = 0;
    for y in 0..h {
        for x in 0..w {
            let outside = x + slack < b.0 || x > b.2 + slack || y + slack < b.1 || y > b.3 + slack;
            if outside && rgba[(y * w + x) * 4 + 3] > 0 {
                n += 1;
            }
        }
    }
    n
}

fn boxed() -> Project {
    let mut p = Project { map_w: 512, map_h: 384, ..Default::default() };
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[140.0, 110.0], [360.0, 110.0], [360.0, 270.0], [140.0, 270.0]]));
    p
}

fn render_rgba(p: &Project) -> Vec<u8> {
    compose(&generate(p, 1.0, &|| false).unwrap(), &p.grading, ComposeOptions::default())
}

#[test]
fn containment_keeps_everything_inside_the_drawn_area() {
    let b = (140, 110, 360, 270);
    let mut p = boxed();
    p.gen.contain = true;
    p.gen.contain_shadows = true;
    let out = outside_box(&render_rgba(&p), 512, 384, b, 2);
    assert_eq!(out, 0, "nothing may be drawn outside the outline, found {out} pixels");

    // The check is meaningful: without containment the fringe, haze and shadows do spill out.
    p.gen.contain = false;
    let spill = outside_box(&render_rgba(&p), 512, 384, b, 2);
    assert!(spill > 500, "without containment the forest spills outside: {spill}");
}

#[test]
fn shadows_can_be_left_to_fall_outside() {
    let b = (140, 110, 360, 270);
    let mut p = boxed();
    p.gen.contain_shadows = false;
    let spill = outside_box(&render_rgba(&p), 512, 384, b, 2);
    // The forest now ends well inside the line, so shadows may or may not reach past it. Trees, grass and haze
    // are held back either way: far less spill than with no containment at all.
    p.gen.contain = false;
    let all = outside_box(&render_rgba(&p), 512, 384, b, 2);
    assert!(spill < all / 2, "only shadows spill: {spill} against {all}");
}

#[test]
fn painted_trees_are_not_clipped() {
    let mut p = boxed();
    p.gen.procedural_trees = false;
    let mut rng = forest_core::rng::Rng::new(5);
    // Far outside the drawn box.
    paint_dab(&mut p, &BrushSettings { radius: 30.0, ..Default::default() }, 440.0, 330.0, &mut rng);
    assert!(!p.painted.is_empty());
    let rgba = render_rgba(&p);
    let seen = outside_box(&rgba, 512, 384, (140, 110, 360, 270), 2);
    assert!(seen > 100, "a tree painted outside the area still shows: {seen}");
}

#[test]
fn pulling_in_shrinks_the_forest() {
    let mut p = boxed();
    p.gen.contain_clip = false;
    let opaque = |v: &[u8]| v.chunks(4).filter(|px| px[3] > 200).count();
    p.gen.contain_fit = 0.0;
    let a = opaque(&render_rgba(&p));
    p.gen.contain_fit = 1.0;
    let b = opaque(&render_rgba(&p));
    assert!(b < a, "pulled in forest covers less: {b} vs {a}");
}

#[test]
fn pulling_in_alone_fits_without_trimming() {
    let b = (200, 150, 800, 600);
    let mut p = boxed();
    // A large shape, so the pull-in is not held back by the narrow-shape limit.
    p.map_w = 1024;
    p.map_h = 768;
    p.shapes[0].points = vec![[200.0, 150.0], [800.0, 150.0], [800.0, 600.0], [200.0, 600.0]];
    p.gen.contain_clip = false;
    p.gen.contain_shadows = false;
    p.gen.contain_fit = 0.0;
    let loose = outside_box(&render_rgba(&p), 1024, 768, b, 2);
    p.gen.contain_fit = 1.0;
    let r = generate(&p, 1.0, &|| false).unwrap();
    assert!(r.fit_inset > 0.0, "a pull-in was measured");
    let fit = outside_box(&compose(&r, &p.grading, ComposeOptions::default()), 1024, 768, b, 2);
    assert!(fit * 20 < loose, "fit leaves almost nothing outside: {fit} vs {loose}");
}

#[test]
fn forest_scale_changes_crown_size_not_the_drawn_area() {
    let mut p = Project { map_w: 1024, map_h: 768, ..Default::default() };
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[100.0, 100.0], [900.0, 100.0], [900.0, 650.0], [100.0, 650.0]]));
    p.gen.contain_clip = true;
    let normal = generate(&p, 1.0, &|| false).unwrap();
    p.gen.scale = 2.0;
    let big = generate(&p, 1.0, &|| false).unwrap();
    assert_eq!((normal.w, normal.h), (big.w, big.h), "same output size");
    assert!(big.trees.len() * 2 < normal.trees.len(), "fewer, larger trees: {} vs {}", big.trees.len(), normal.trees.len());
    let mean_r = |l: &forest_core::engine::RenderLayers| l.trees.iter().map(|t| t.r).sum::<f32>() / l.trees.len() as f32;
    assert!(mean_r(&big) > 1.6 * mean_r(&normal), "crowns are bigger");
    p.gen.scale = 0.5;
    let small = generate(&p, 1.0, &|| false).unwrap();
    assert!(small.trees.len() > normal.trees.len() * 2, "more, smaller trees");
}

#[test]
fn preset_trees_are_solid_and_deterministic() {
    use forest_core::treegen_sprite::build_sprite;
    let a = build_sprite("quaking_aspen", 0, -1.0, 40.0).expect("preset exists");
    let b = build_sprite("quaking_aspen", 0, -1.0, 40.0).unwrap();
    assert_eq!(a.cls, b.cls, "same variant, same tree");
    let c = build_sprite("quaking_aspen", 1, -1.0, 40.0).unwrap();
    assert_ne!(a.cls, c.cls, "variants differ");
    // Inside the crown the leaves leave few gaps.
    let n = a.size as f32;
    let mid = n / 2.0;
    let (mut inside, mut covered) = (0, 0);
    for y in 0..a.size {
        for x in 0..a.size {
            let (dx, dy) = (x as f32 + 0.5 - mid, y as f32 + 0.5 - mid);
            if dx * dx + dy * dy < 30.0 * 30.0 {
                inside += 1;
                covered += (a.cls[y * a.size + x] != 0) as u32;
            }
        }
    }
    assert!(covered as f32 / inside as f32 > 0.9, "dense crown: {covered} of {inside}");
    // The canopy comes from the generator: the raw tree is much sparser and shows more wood.
    let raw = build_sprite("quaking_aspen", 0, 0.0, 40.0).unwrap();
    let count = |s: &forest_core::wp::Sprite, c: u8| s.cls.iter().filter(|v| **v == c).count();
    assert!(count(&a, 1) > count(&raw, 1) * 3 / 2, "more leaves than the raw tree");
    assert!(count(&a, 2) * count(&raw, 1) < count(&raw, 2) * count(&a, 1), "less wood showing");
    // A palm keeps its open fronds when fill is off.
    let p = build_sprite("palm", 0, 0.0, 40.0).unwrap();
    assert!(p.cls.iter().filter(|c| **c != 0).count() > 100);
    assert!(build_sprite("not_a_tree", 0, 0.5, 40.0).is_none());
}

#[test]
fn themes_keep_a_procedural_tree_type() {
    let mut p = Project::default();
    let main = p.gen.main_species as usize;
    p.species[main].procedural = true;
    p.species[main].preset = "black_oak".into();
    p.gen.shrub_species = ((main + 1) % p.species.len()) as u8;
    let hell = themes().into_iter().find(|t| t.name == "Hellscape").expect("theme exists");
    p.apply_theme(&hell);
    let sp = &p.species[p.gen.main_species as usize];
    assert_eq!(sp.tree_type(false), Some("black_oak"), "tree type survives the theme");
    assert_eq!(sp.ramp, hell.species[0].ramp, "colours come from the theme");
    // A species that was not procedural takes the theme's own form.
    let shrub = &p.species[p.gen.shrub_species as usize];
    assert!(shrub.tree_type(false).is_none());
}

#[test]
fn gpu_shading_matches_cpu() {
    let Some(g) = forest_core::gpu::Gpu::headless() else {
        eprintln!("no GPU here, skipping");
        return;
    };
    let mut p = project();
    p.gen.snow.amount = 0.6;
    p.gen.fire = FireStyle { amount: 0.5, ..Default::default() };
    let opts = ComposeOptions { background: Some([0.8, 0.78, 0.7]), ..Default::default() };
    let cpu = compose(&generate(&p, 1.0, &|| false).unwrap(), &p.grading, opts);
    let plan = forest_core::engine::plan(&p, 1.0, &|| false).unwrap();
    let (w, h) = render_size(&p, 1.0);
    let st = forest_core::engine::stamp_region(&p, &plan, 1.0, forest_core::engine::Region { x0: 0, y0: 0, w, h }, &|| false).unwrap();
    // shading CPU stamps, and stamping on the GPU as well
    let shaded = g.compose_rgba(&g.shade(&p, &plan, &st), &p.grading, opts);
    let rendered = g.compose_rgba(&g.render(&p, &plan, 1.0, forest_core::engine::Region { x0: 0, y0: 0, w, h }), &p.grading, opts);
    for (name, gpu) in [("shaded", shaded), ("stamped and shaded", rendered)] {
        let worst = cpu.iter().zip(&gpu).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        let mean = cpu.iter().zip(&gpu).map(|(a, b)| (*a as i32 - *b as i32).abs() as f64).sum::<f64>() / cpu.len() as f64;
        assert!(worst <= 40 && mean < 0.2, "GPU {name} differs from CPU: worst {worst}, mean {mean:.3}");
    }
}

#[test]
fn gpu_view_matches_full_render() {
    let Some(g) = forest_core::gpu::Gpu::headless() else {
        eprintln!("no GPU here, skipping");
        return;
    };
    use forest_core::engine::Region;
    let p = project();
    let opts = ComposeOptions { background: Some([0.8, 0.78, 0.7]), ..Default::default() };
    let plan = forest_core::engine::plan(&p, 0.5, &|| false).unwrap();
    let scale = 1.0;
    let (w, h) = render_size(&p, scale);
    let full = g.compose_rgba(&g.render(&p, &plan, scale, Region { x0: 0, y0: 0, w, h }), &p.grading, opts);
    // a window in the middle, padded the way the app pads it
    let pad = forest_core::view_pad(&p, &plan, scale);
    let (cx0, cy0, cx1, cy1) = (w / 4, h / 4, w * 3 / 4, h * 3 / 4);
    let (x0, y0) = (cx0.saturating_sub(pad), cy0.saturating_sub(pad));
    let (x1, y1) = ((cx1 + pad).min(w), (cy1 + pad).min(h));
    let region = Region { x0, y0, w: x1 - x0, h: y1 - y0 };
    let part = g.compose_rgba(&g.render(&p, &plan, scale, region), &p.grading, opts);
    let (mut worst, mut sum, mut n) = (0, 0f64, 0usize);
    for y in cy0..cy1 {
        for x in cx0..cx1 {
            for c in 0..4 {
                let a = full[(y * w + x) * 4 + c] as i32;
                let b = part[((y - y0) * region.w + (x - x0)) * 4 + c] as i32;
                worst = worst.max((a - b).abs());
                sum += (a - b).abs() as f64;
                n += 1;
            }
        }
    }
    let mean = sum / n as f64;
    assert!(worst <= 8 && mean < 0.05, "viewport render differs: worst {worst}, mean {mean:.4}");
}

#[test]
fn mushrooms_grow_and_draw_on_both_paths() {
    use forest_core::mushrooms;
    for (name, _) in mushrooms::KINDS {
        for v in 0..5 {
            let sp = mushrooms::sprite(name, v, false).expect("baked sprite");
            assert!(sp.cls.iter().any(|c| *c == 1), "{name} {v} has a cap");
        }
    }
    let mut p = project();
    let base = generate(&p, 1.0, &|| false).unwrap().tree_counts;
    p.ensure_mushroom_species();
    p.gen.mushrooms = 2.0;
    p.gen.mushroom_mix = [1.0, 1.0, 1.0, 1.0, 1.0];
    let with = generate(&p, 1.0, &|| false).unwrap().tree_counts;
    assert!(with[0] > base[0], "mushrooms add trees: {base:?} -> {with:?}");
    if let Some(g) = forest_core::gpu::Gpu::headless() {
        let opts = ComposeOptions { background: Some([0.8, 0.78, 0.7]), ..Default::default() };
        let cpu = compose(&generate(&p, 1.0, &|| false).unwrap(), &p.grading, opts);
        let plan = forest_core::engine::plan(&p, 1.0, &|| false).unwrap();
        let (w, h) = render_size(&p, 1.0);
        let gpu = g.compose_rgba(&g.render(&p, &plan, 1.0, forest_core::engine::Region { x0: 0, y0: 0, w, h }), &p.grading, opts);
        let mean = cpu.iter().zip(&gpu).map(|(a, b)| (*a as i32 - *b as i32).abs() as f64).sum::<f64>() / cpu.len() as f64;
        assert!(mean < 0.25, "GPU differs from CPU with mushrooms: mean {mean:.3}");
    }
}

#[test]
fn brush_edits_through_the_cache_match_a_fresh_render() {
    let mut p = project();
    let mut cache = RenderCache::default();
    let opts = ComposeOptions::default();
    let first = cache.render(&p, 0.5, &|| false).unwrap();
    assert_eq!(compose(&first, &p.grading, opts), compose(&generate(&p, 0.5, &|| false).unwrap(), &p.grading, opts));
    // erase a patch and paint two trees, then render through the same cache
    p.erased.push([250.0, 190.0, 60.0]);
    p.painted.push(TreeRec { x: 150.0, y: 150.0, r: 18.0, h: 30.0, tone: 0.1, layer: TreeLayer::Canopy, species: 0, seed: 7 });
    p.painted.push(TreeRec { x: 330.0, y: 220.0, r: 10.0, h: 20.0, tone: -0.1, layer: TreeLayer::Shrub, species: 0, seed: 9 });
    let edited = cache.render(&p, 0.5, &|| false).unwrap();
    let fresh = generate(&p, 0.5, &|| false).unwrap();
    assert_eq!(compose(&edited, &p.grading, opts), compose(&fresh, &p.grading, opts));
    assert_eq!(edited.tree_counts, fresh.tree_counts);
    assert_eq!(edited.trees.len(), fresh.trees.len());
}

fn canopy_cover(p: &Project, scale: f32) -> (f32, Vec<u8>) {
    let r = generate(p, scale, &|| false).unwrap();
    let rgba = compose(&r, &p.grading, ComposeOptions::default());
    let n = r.layers[LayerKind::Canopy.index()].alphas(r.h).filter(|a| *a > 0.5).count();
    (n as f32 / (r.w * r.h) as f32, rgba)
}

#[test]
fn crown_shyness_cuts_gaps_and_can_be_switched_off() {
    let mut p = project();
    let (base, base_img) = canopy_cover(&p, 1.0);
    // off by default, and the gap setting alone does nothing
    p.gen.shyness_gap = 4.0;
    assert_eq!(canopy_cover(&p, 1.0).1, base_img);
    p.gen.crown_shyness = true;
    let (shy, shy_img) = canopy_cover(&p, 1.0);
    assert!(shy < base && shy > base * 0.7, "shyness trims some canopy: {base} -> {shy}");
    assert_ne!(shy_img, base_img);
    p.gen.crown_shyness = false;
    assert_eq!(canopy_cover(&p, 1.0).1, base_img);
}

#[test]
fn giants_only_shyness_leaves_other_crowns_alone() {
    let mut p = project();
    p.gen.crown_shyness = true;
    p.gen.shyness_giants_only = true;
    p.gen.shyness_gap = 4.0;
    // no giants: nothing is shy
    let off = {
        let mut q = p.clone();
        q.gen.crown_shyness = false;
        canopy_cover(&q, 1.0).1
    };
    assert_eq!(canopy_cover(&p, 1.0).1, off);
    // with giants, only they change
    p.ensure_mushroom_species();
    p.gen.giant_mushrooms = 0.3;
    p.gen.mushroom_mix = [1.0; 5];
    let giants = canopy_cover(&p, 1.0).1;
    p.gen.shyness_giants_only = false;
    let all = canopy_cover(&p, 1.0).1;
    p.gen.crown_shyness = false;
    let none = canopy_cover(&p, 1.0).1;
    let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).filter(|(x, y)| x != y).count();
    assert!(diff(&giants, &none) > 0, "giants get gaps");
    assert!(diff(&all, &none) > diff(&giants, &none), "everything shy changes more than giants only");
}

#[test]
fn shyness_toggle_through_the_cache_matches_a_fresh_render() {
    let mut p = project();
    let mut cache = RenderCache::default();
    let opts = ComposeOptions::default();
    let _ = cache.render(&p, 0.5, &|| false).unwrap();
    p.gen.crown_shyness = true;
    let toggled = cache.render(&p, 0.5, &|| false).unwrap();
    assert_eq!(compose(&toggled, &p.grading, opts), compose(&generate(&p, 0.5, &|| false).unwrap(), &p.grading, opts));
}

#[test]
fn crown_shyness_matches_on_gpu() {
    let Some(g) = forest_core::gpu::Gpu::headless() else { return };
    let mut p = project();
    p.gen.crown_shyness = true;
    p.gen.shyness_gap = 3.0;
    let opts = ComposeOptions { background: Some([0.8, 0.78, 0.7]), ..Default::default() };
    let cpu = compose(&generate(&p, 1.0, &|| false).unwrap(), &p.grading, opts);
    let plan = forest_core::engine::plan(&p, 1.0, &|| false).unwrap();
    let (w, h) = render_size(&p, 1.0);
    let gpu = g.compose_rgba(&g.render(&p, &plan, 1.0, forest_core::engine::Region { x0: 0, y0: 0, w, h }), &p.grading, opts);
    let mean = cpu.iter().zip(&gpu).map(|(a, b)| (*a as i32 - *b as i32).abs() as f64).sum::<f64>() / cpu.len() as f64;
    assert!(mean < 0.25, "GPU differs from CPU with shyness: mean {mean:.3}");
}

#[test]
fn dead_and_extra_trees_make_solid_deterministic_sprites_at_every_size() {
    use forest_core::{dead, extra, treegen_sprite};
    let mut names: Vec<String> = dead::DEAD_TYPES.iter().map(|(n, _)| n.to_string()).collect();
    names.extend(extra::names().iter().map(|n| n.to_string()));
    assert!(names.len() >= 19, "all the new types are registered: {names:?}");
    for name in &names {
        for r in [6.0f32, 12.0, 25.0, 80.0] {
            let a = treegen_sprite::sprite_for_radius(name, 1, r, -1.0).unwrap_or_else(|| panic!("{name} has a sprite"));
            let solid = a.cls.iter().filter(|c| **c != 0).count();
            assert!(solid > (r * r * 0.05) as usize, "{name} at radius {r} keeps its wood: {solid} pixels");
            let b = treegen_sprite::sprite_for_radius(name, 1, r, -1.0).unwrap();
            assert_eq!(a.cls, b.cls);
        }
    }
}

#[test]
fn dead_trees_lose_wood_as_they_decay_and_living_trees_are_unchanged() {
    use forest_core::treegen_sprite::build_sprite;
    let solid = |n: &str| build_sprite(n, 0, -1.0, 96.0).unwrap().cls.iter().filter(|c| **c != 0).count();
    let bare = solid("bare_oak");
    let dead = solid("dead_oak");
    let snag = solid("snag");
    assert!(bare > dead && dead > snag / 2, "wood falls with decay: {bare} {dead} {snag}");
    // a living preset still has its leaves
    assert!(build_sprite("black_oak", 0, -1.0, 96.0).unwrap().cls.iter().any(|c| *c == 1));
}

#[test]
fn a_background_picture_never_changes_the_render() {
    let mut p = project();
    let plain = compose(&generate(&p, 0.5, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    p.background_image = Some(BackgroundImage { path: "somewhere.png".into(), opacity: 0.5, visible: true });
    let with = compose(&generate(&p, 0.5, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    assert_eq!(plain, with);
    // the picture is kept out of the cache key, so changing it reuses the plan
    let mut cache = RenderCache::default();
    let _ = cache.render(&p, 0.5, &|| false).unwrap();
    p.background_image.as_mut().unwrap().opacity = 0.9;
    let again = cache.render(&p, 0.5, &|| false).unwrap();
    assert_eq!(compose(&again, &p.grading, ComposeOptions::default()), plain);
    // the project file round trips it, and old files without it load
    let json = serde_json::to_string(&p).unwrap();
    assert!(serde_json::from_str::<Project>(&json).unwrap().background_image.is_some());
    assert!(serde_json::from_str::<Project>("{\"map_w\": 100}").unwrap().background_image.is_none());
}

#[test]
fn premultiplied_preview_matches_straight_alpha() {
    let p = project();
    let r = generate(&p, 0.5, &|| false).unwrap();
    let straight = compose(&r, &p.grading, ComposeOptions::default());
    let pre = compose(&r, &p.grading, ComposeOptions { premultiplied: true, ..Default::default() });
    for (s, q) in straight.chunks(4).zip(pre.chunks(4)) {
        assert_eq!(s[3], q[3]);
        for k in 0..3 {
            let want = s[k] as f32 * s[3] as f32 / 255.0;
            assert!((want - q[k] as f32).abs() <= 1.5, "premultiplied {} vs {}", q[k], want);
        }
    }
}

fn two_zones() -> Project {
    let mut p = Project { map_w: 768, map_h: 384, ..Default::default() };
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[30.0, 40.0], [330.0, 40.0], [330.0, 340.0], [30.0, 340.0]]));
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[440.0, 40.0], [740.0, 40.0], [740.0, 340.0], [440.0, 340.0]]));
    p
}

#[test]
fn projects_without_own_zone_settings_are_one_layer() {
    let p = two_zones();
    assert_eq!(zone_passes(&p).len(), 1);
    let one = compose(&generate(&p, 0.5, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    let stack = compose_stack(&generate_stack(&p, 0.5, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    assert_eq!(one, stack);
}

#[test]
fn a_zone_with_its_own_settings_is_its_own_layer() {
    let mut p = two_zones();
    let mut z = ZoneSettings { gen: p.gen.clone(), species: p.species.clone() };
    z.gen.scale = 2.0;
    p.shapes[1].settings = Some(z);
    let passes = zone_passes(&p);
    assert_eq!(passes.len(), 2);
    assert_eq!(passes[0].shapes.len(), 1, "the shared layer keeps the other zone");
    assert!(passes[1].painted.is_empty());
    let opts = ComposeOptions::default();
    let stack = generate_stack(&p, 0.5, &|| false).unwrap();
    let both = compose_stack(&stack, &p.grading, opts);
    // the left zone, drawn alone, looks the same inside the stack, away from the other zone
    let mut left = p.clone();
    left.shapes.remove(1);
    let alone = compose(&generate(&left, 0.5, &|| false).unwrap(), &left.grading, opts);
    let (w, h) = render_size(&p, 0.5);
    let mut bad = Vec::new();
    for y in 0..h {
        for x in 0..(w * 4 / 10) {
            let i = (y * w + x) * 4;
            // colour under zero alpha does not show
            if both[i + 3] != alone[i + 3] || (both[i + 3] > 0 && both[i..i + 3] != alone[i..i + 3]) {
                bad.push((x, y, both[i..i + 4].to_vec(), alone[i..i + 4].to_vec()));
            }
        }
    }
    assert!(bad.is_empty(), "the left zone is unchanged by the layer next to it: {} differ, first {:?}, w {w}", bad.len(), bad.first());
    // the right zone differs from the same zone following the project settings
    let plain = compose(&generate(&two_zones(), 0.5, &|| false).unwrap(), &p.grading, opts);
    let mut diff = 0;
    for y in 0..h {
        for x in (w * 6 / 10)..w {
            let i = (y * w + x) * 4;
            diff += (both[i..i + 4] != plain[i..i + 4]) as usize;
        }
    }
    assert!(diff > 1000, "the right zone changed: {diff}");
    // hidden or cut zones follow the same rules: hiding the layer removes it
    p.shapes[1].visible = false;
    assert_eq!(zone_passes(&p).len(), 1);
}

#[test]
fn clearings_cut_every_layer() {
    let mut p = two_zones();
    p.shapes[1].settings = Some(ZoneSettings { gen: p.gen.clone(), species: p.species.clone() });
    p.shapes.push(Shape::new(ShapeOp::Subtract, vec![[500.0, 100.0], [680.0, 100.0], [680.0, 280.0], [500.0, 280.0]]));
    let passes = zone_passes(&p);
    assert_eq!(passes.len(), 2);
    assert!(passes[1].shapes.iter().any(|s| s.op == ShapeOp::Subtract));
    let img = compose_stack(&generate_stack(&p, 0.5, &|| false).unwrap(), &p.grading, ComposeOptions::default());
    let (w, _) = render_size(&p, 0.5);
    // the middle of the clearing has no canopy
    let (cx, cy) = ((590.0f32 * 0.5) as usize, (190.0f32 * 0.5) as usize);
    assert!(img[(cy * w + cx) * 4 + 3] < 40, "alpha in the clearing {}", img[(cy * w + cx) * 4 + 3]);
}

fn group(p: &mut Project, parent: Option<u32>, name: &str, over: serde_json::Value) -> u32 {
    let id = p.add_group(parent);
    let n = p.group_mut(id).unwrap();
    n.name = name.into();
    n.overrides = over;
    id
}

fn zone(p: &mut Project, group: Option<u32>, x: f32) -> usize {
    p.add_zone(ShapeOp::Add, vec![[x, 40.0], [x + 60.0, 40.0], [x + 60.0, 120.0], [x, 120.0]], group)
}

#[test]
fn a_group_change_reaches_groups_that_have_not_set_it() {
    use serde_json::json;
    let mut p = two_zones();
    let g = group(&mut p, None, "Forest", json!({"scale": 2.0, "depth": {"gain": 1.5}}));
    let a = group(&mut p, Some(g), "A", serde_json::Value::Null);
    let b = group(&mut p, Some(g), "B", json!({"scale": 3.0, "depth": {"crown_shade": 0.9}}));
    assert_eq!(p.effective(a).0.scale, 2.0, "A follows the group above it");
    assert_eq!(p.effective(b).0.scale, 3.0, "B keeps its own");
    // nested settings merge field by field: B gets the gain from above and keeps its own crown shade
    assert_eq!(p.effective(b).0.depth.gain, 1.5);
    assert_eq!(p.effective(b).0.depth.crown_shade, 0.9);
    // and a field nobody changed is still the project one
    assert_eq!(p.effective(b).0.depth.depth_strength, p.gen.depth.depth_strength);
    assert_eq!(p.effective(a).0.depth.crown_shade, p.gen.depth.crown_shade);
    // a change to the project reaches everything that has not set that field
    p.gen.canopy_cover = 0.33;
    assert_eq!(p.effective(a).0.canopy_cover, 0.33);
    assert_eq!(p.effective(b).0.canopy_cover, 0.33);
    // what a group would inherit
    assert_eq!(p.inherited(b).0.scale, 2.0);
    assert_eq!(p.inherited(g).0.scale, p.gen.scale);
}

#[test]
fn the_species_list_and_its_indexes_come_from_the_same_group() {
    use serde_json::json;
    let mut p = two_zones();
    let g = group(&mut p, None, "G", json!({"main_species": 1, "mix_species": 0, "shrub_species": 1}));
    let own: Vec<Species> = p.species.iter().take(3).cloned().collect();
    p.group_mut(g).unwrap().species = Some(own);
    let c = group(&mut p, Some(g), "C", json!({"main_species": 2}));
    let (gen, species) = p.effective(c);
    assert_eq!(species.len(), 3, "the inner group uses the list from above");
    assert_eq!((gen.main_species, gen.shrub_species), (2, 1));
    assert!(gen.main_species as usize <= species.len());
    // outside the group the project list stays
    let lone = group(&mut p, None, "Lone", serde_json::Value::Null);
    assert_eq!(p.effective(lone).1.len(), p.species.len());
}

#[test]
fn groups_draw_in_tree_order_and_hidden_groups_hide_what_they_hold() {
    use serde_json::json;
    let mut p = two_zones();
    p.shapes.clear();
    let g1 = group(&mut p, None, "G1", json!({"scale": 1.2}));
    let a = group(&mut p, Some(g1), "A", json!({"scale": 1.5}));
    let g2 = group(&mut p, Some(g1), "G2", json!({"scale": 1.7}));
    let b = group(&mut p, Some(g2), "B", json!({"scale": 2.0}));
    let c = group(&mut p, None, "C", json!({"scale": 2.5}));
    zone(&mut p, Some(g1), 5.0);
    for (l, x) in [(a, 70.0), (g2, 140.0), (b, 210.0), (c, 280.0)] {
        zone(&mut p, Some(l), x);
    }
    // a group first, then the groups inside it
    assert_eq!(p.draw_order(), vec![g1, a, g2, b, c]);
    let scales: Vec<f32> = zone_passes(&p).iter().map(|q| q.gen.scale).collect();
    assert_eq!(scales, vec![1.2, 1.5, 1.7, 2.0, 2.5]);
    p.group_mut(g2).unwrap().visible = false;
    let scales: Vec<f32> = zone_passes(&p).iter().map(|q| q.gen.scale).collect();
    assert_eq!(scales, vec![1.2, 1.5, 2.5], "a hidden group hides the groups inside it");
    p.group_mut(g1).unwrap().visible = false;
    let passes = zone_passes(&p);
    assert_eq!(passes.len(), 1, "only C is left, and nothing leaks into the project pass");
    assert_eq!(passes[0].gen.scale, 2.5);
    // groups that end up with the same settings are drawn together
    p.group_mut(g1).unwrap().visible = true;
    p.group_mut(g2).unwrap().visible = true;
    for id in [a, g2, b] {
        p.group_mut(id).unwrap().overrides = json!({"scale": 1.5});
    }
    p.group_mut(g1).unwrap().overrides = json!({"scale": 1.5});
    let passes = zone_passes(&p);
    assert_eq!(passes.len(), 2, "G1, A, G2 and B look the same, so they are one pass");
    assert_eq!(passes[0].shapes.len(), 4);
}

#[test]
fn a_clearing_cuts_its_group_and_what_is_inside_it() {
    use serde_json::json;
    let mut p = two_zones();
    p.shapes.clear();
    let g = group(&mut p, None, "G", json!({"scale": 1.5}));
    let inner = group(&mut p, Some(g), "Inner", json!({"scale": 2.0}));
    let other = group(&mut p, None, "Other", json!({"scale": 2.5}));
    zone(&mut p, Some(inner), 20.0);
    zone(&mut p, Some(other), 200.0);
    zone(&mut p, None, 120.0);
    let cut = |p: &mut Project, g: Option<u32>| p.add_zone(ShapeOp::Subtract, vec![[10.0, 10.0], [300.0, 10.0], [300.0, 30.0], [10.0, 30.0]], g);
    cut(&mut p, Some(g));
    let cuts = |q: &Project| q.shapes.iter().filter(|s| s.op == ShapeOp::Subtract).count();
    let by_scale = |p: &Project, scale: f32| zone_passes(p).into_iter().find(|q| q.gen.scale == scale).unwrap();
    assert_eq!(cuts(&by_scale(&p, 2.0)), 1, "the group clearing cuts the group inside it");
    assert_eq!(cuts(&by_scale(&p, 2.5)), 0, "and not another group");
    assert_eq!(zone_passes(&p)[0].shapes.iter().filter(|s| s.op == ShapeOp::Subtract).count(), 0, "nor the zones in no group");
    // a clearing in no group cuts everything
    cut(&mut p, None);
    assert_eq!(cuts(&by_scale(&p, 2.0)), 2);
    assert_eq!(cuts(&by_scale(&p, 2.5)), 1);
    assert_eq!(zone_passes(&p)[0].shapes.iter().filter(|s| s.op == ShapeOp::Subtract).count(), 1);
    // a hidden group takes its clearing with it
    p.group_mut(g).unwrap().visible = false;
    assert!(zone_passes(&p).iter().all(|q| q.shapes.iter().filter(|s| s.op == ShapeOp::Subtract).count() <= 1));
}

#[test]
fn a_new_zone_goes_into_the_group_picked() {
    let mut p = two_zones();
    let g = p.add_group(None);
    let i = p.add_zone(ShapeOp::Add, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], Some(g));
    assert_eq!(p.shapes[i].group, Some(g));
    let j = p.add_zone(ShapeOp::Subtract, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], Some(9999));
    assert_eq!(p.shapes[j].group, None, "a group that is not there is no group");
    let k = p.add_zone(ShapeOp::Add, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], None);
    assert_eq!(p.shapes[k].group, None);
}

#[test]
fn tree_edits_leave_no_dangling_ids() {
    use serde_json::json;
    let mut p = two_zones();
    p.shapes.clear();
    let g = group(&mut p, None, "G", json!({"scale": 2.0}));
    let a = group(&mut p, Some(g), "A", json!({"canopy_cover": 0.3}));
    let c = group(&mut p, None, "C", serde_json::Value::Null);
    let (z0, z1, z2) = (zone(&mut p, Some(g), 20.0), zone(&mut p, Some(a), 120.0), zone(&mut p, Some(c), 220.0));
    let check = |p: &Project| {
        assert!(p.groups.iter().all(|n| n.parent.map_or(true, |q| p.group(q).is_some())), "no group points at a missing parent");
        assert!(p.shapes.iter().all(|s| s.group.map_or(true, |l| p.group(l).is_some())), "no zone points at a missing group");
    };
    // a zone goes into a new group of its own, where it was
    let zg = p.group_zone(z2).unwrap();
    assert_eq!(p.shapes[z2].group, Some(zg));
    assert_eq!(p.group(zg).unwrap().parent, Some(c), "inside the group the zone was in");
    let zg2 = p.group_zone(z1).unwrap();
    assert_eq!(p.group(zg2).unwrap().parent, Some(a), "inside the group the zone was in");
    // wrapping a group, moving, and cycles
    let w = p.wrap_group(a).unwrap();
    assert_eq!((p.group(a).unwrap().parent, p.group(w).unwrap().parent), (Some(w), Some(g)));
    assert!(!p.move_into(w, Some(w)), "a group cannot go into itself");
    assert!(!p.move_into(g, Some(a)), "nor into what it holds");
    assert!(p.move_into(zg, Some(g)));
    check(&p);
    // ungroup: zones and groups move up to the group above
    assert!(p.ungroup(w));
    assert_eq!(p.group(a).unwrap().parent, Some(g));
    check(&p);
    assert!(p.ungroup(g));
    assert_eq!(p.shapes[z0].group, None, "zones fall to the project settings");
    assert!(p.group(a).unwrap().parent.is_none());
    check(&p);
    // delete takes everything inside with it
    let before = p.shapes.len();
    p.delete_group(a);
    assert!(p.group(a).is_none() && p.group(zg2).is_none());
    assert!(p.shapes.len() < before, "the zones in the deleted groups went too");
    check(&p);
    // moving up and down changes the drawing order
    let order = p.draw_order();
    if order.len() >= 2 {
        let first = order[0];
        assert!(p.nudge(first, 1));
        assert!(p.draw_order().iter().position(|x| *x == first) > Some(0));
    }
    p.delete_group(9999);
    assert!(!p.nudge(9999, 1));
}

#[test]
fn groups_round_trip_and_older_formats_load() {
    use serde_json::json;
    let mut p = two_zones();
    let g = group(&mut p, None, "G", json!({"scale": 2.0}));
    let a = group(&mut p, Some(g), "A", json!({"depth": {"gain": 1.4}}));
    p.shapes[0].group = Some(a);
    let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back.groups.len(), 2);
    assert_eq!(back.effective(a).0.depth.gain, 1.4);
    assert_eq!(back.effective(a).0.scale, 2.0);
    assert_eq!(back.shapes[0].group, Some(a));
    // a project from before groups
    let old = r#"{"shapes":[{"op":"Add","points":[[0,0],[10,0],[10,10]]}]}"#;
    let old = serde_json::from_str::<Project>(old).unwrap();
    assert!(old.groups.is_empty() && old.shapes[0].group.is_none());
    // the version with layers and groups: layers are groups now, and keep their zones
    let mut v = serde_json::to_value(two_zones()).unwrap();
    v.as_object_mut().unwrap().remove("groups");
    v["layers"] = json!([
        {"id": 1, "parent": null, "kind": "Group", "name": "Big forest", "visible": true, "collapsed": false, "overrides": {"scale": 2.0}, "species": null},
        {"id": 2, "parent": 1, "kind": "Layer", "name": "Oaks", "visible": true, "collapsed": false, "overrides": null, "species": null}
    ]);
    v["shapes"][1].as_object_mut().unwrap().remove("group");
    v["shapes"][1]["layer"] = json!(2);
    let loaded: Project = serde_json::from_value(v).unwrap();
    assert_eq!(loaded.groups.len(), 2);
    assert_eq!(loaded.shapes[1].group, Some(2));
    assert_eq!(loaded.effective(2).0.scale, 2.0, "the layer follows the group above it");
    assert_eq!(zone_passes(&loaded).len(), 2);
    // groups that held a full copy of the settings and were numbered by position
    let base = two_zones();
    let mut full = base.gen.clone();
    full.scale = 2.5;
    let mut v = serde_json::to_value(&base).unwrap();
    v.as_object_mut().unwrap().remove("groups");
    v["layers"] = json!([{"name": "Big", "settings": {"gen": full, "species": []}, "visible": true}]);
    v["shapes"][1].as_object_mut().unwrap().remove("group");
    v["shapes"][1]["layer"] = json!(0);
    let mut loaded: Project = serde_json::from_value(v).unwrap();
    loaded.migrate_groups();
    let id = loaded.groups[0].id;
    assert_eq!(loaded.shapes[1].group, Some(id));
    assert_eq!(loaded.effective(id).0.scale, 2.5);
    assert_eq!(loaded.groups[0].overrides, json!({"scale": 2.5}), "only what differs is kept");
    // a zone that carried its own settings becomes a group
    let mut legacy = two_zones();
    let mut z = ZoneSettings { gen: legacy.gen.clone(), species: legacy.species.clone() };
    z.gen.scale = 2.0;
    legacy.shapes[1].settings = Some(z);
    assert_eq!(zone_passes(&legacy).len(), 2);
}

#[test]
fn painted_trees_stay_on_top_of_zones_with_their_own_settings() {
    let mut p = two_zones();
    p.shapes[1].settings = Some(ZoneSettings { gen: p.gen.clone(), species: p.species.clone() });
    let opts = ComposeOptions::default();
    let without = compose_stack(&generate_stack(&p, 0.5, &|| false).unwrap(), &p.grading, opts);
    p.painted.push(TreeRec { x: 590.0, y: 190.0, r: 40.0, h: 80.0, tone: 0.0, layer: TreeLayer::Canopy, species: 0, seed: 3 });
    let passes = zone_passes(&p);
    assert_eq!(passes.len(), 3, "shared layer, own layer, painted layer");
    assert!(passes[0].painted.is_empty() && passes[1].painted.is_empty());
    let with = compose_stack(&generate_stack(&p, 0.5, &|| false).unwrap(), &p.grading, opts);
    let (w, _) = render_size(&p, 0.5);
    let mut changed = 0;
    for dy in -15i32..=15 {
        for dx in -15i32..=15 {
            let (x, y) = ((295 + dx) as usize, (95 + dy) as usize);
            let i = (y * w + x) * 4;
            changed += (with[i..i + 4] != without[i..i + 4]) as usize;
        }
    }
    assert!(changed > 400, "the painted tree shows over the zone: {changed} pixels changed");
    // without own-settings zones the painted trees stay in the one layer, as before
    let mut q = two_zones();
    q.painted.push(TreeRec { x: 590.0, y: 190.0, r: 40.0, h: 80.0, tone: 0.0, layer: TreeLayer::Canopy, species: 0, seed: 3 });
    assert_eq!(zone_passes(&q).len(), 1);
}

#[test]
fn gpu_premultiplied_and_stacked_layers_match_the_cpu() {
    let Some(g) = forest_core::gpu::Gpu::headless() else { return };
    let mut p = two_zones();
    let mut z = ZoneSettings { gen: p.gen.clone(), species: p.species.clone() };
    z.gen.scale = 1.6;
    p.shapes[1].settings = Some(z);
    let (w, h) = render_size(&p, 0.5);
    let region = forest_core::engine::Region { x0: 0, y0: 0, w, h };
    // premultiplied output of one layer, GPU against CPU
    let pass = &zone_passes(&p)[0];
    let pre = ComposeOptions { premultiplied: true, ..Default::default() };
    let cpu = compose(&generate(pass, 0.5, &|| false).unwrap(), &pass.grading, pre);
    let plan = forest_core::engine::plan(pass, 0.5, &|| false).unwrap();
    let gpu = g.compose_rgba(&g.render(pass, &plan, 0.5, region), &pass.grading, pre);
    let mean = cpu.iter().zip(&gpu).map(|(a, b)| (*a as i32 - *b as i32).abs() as f64).sum::<f64>() / cpu.len() as f64;
    assert!(mean < 0.25, "premultiplied GPU against CPU: mean {mean:.3}");
    // every pixel's colour is at most its alpha when premultiplied
    assert!(gpu.chunks(4).all(|px| px[..3].iter().all(|c| *c as u32 <= px[3] as u32 + 2)));
    // the whole stack, layer by layer on the GPU, against the CPU stack
    let opts = ComposeOptions { background: Some([0.8, 0.78, 0.7]), ..Default::default() };
    let cpu_stack = compose_stack(&generate_stack(&p, 0.5, &|| false).unwrap(), &p.grading, opts);
    let plain = ComposeOptions { background: None, ..opts };
    let mut gpu_stack = vec![0u8; w * h * 4];
    forest_core::stack::fill_background(&mut gpu_stack, [0.8, 0.78, 0.7]);
    for q in zone_passes(&p) {
        let plan = forest_core::engine::plan(&q, 0.5, &|| false).unwrap();
        forest_core::stack::over(&mut gpu_stack, &g.compose_rgba(&g.render(&q, &plan, 0.5, region), &q.grading, plain));
    }
    let mean = cpu_stack.iter().zip(&gpu_stack).map(|(a, b)| (*a as i32 - *b as i32).abs() as f64).sum::<f64>() / cpu_stack.len() as f64;
    assert!(mean < 0.4, "stacked layers on the GPU against the CPU: mean {mean:.3}");
}


/// What the settings panel does in one frame: it edits a copy of what the group uses, then the changes are kept.
fn edit_group(p: &mut Project, id: u32, change: impl FnOnce(&mut GenParams, &mut Vec<Species>), resets: &[&str]) {
    let (g0, s0) = p.effective(id);
    let (mut g1, mut s1) = (g0.clone(), s0.clone());
    change(&mut g1, &mut s1);
    let resets: Vec<String> = resets.iter().map(|k| k.to_string()).collect();
    p.record_edit(id, (&g0, &s0), (&g1, &s1), &resets);
}

#[test]
fn editing_a_group_keeps_only_what_was_changed() {
    use serde_json::json;
    let mut p = two_zones();
    let g = group(&mut p, None, "G", serde_json::Value::Null);
    let a = group(&mut p, Some(g), "A", serde_json::Value::Null);
    let b = group(&mut p, Some(g), "B", json!({"scale": 3.0}));
    // the user changes the tree size of the group and its crown shade, and nothing else
    edit_group(&mut p, g, |gen, _| {
        gen.scale = 2.0;
        gen.depth.crown_shade = 0.8;
    }, &[]);
    assert_eq!(p.group(g).unwrap().overrides, json!({"scale": 2.0f32, "depth": {"crown_shade": 0.8f32}}), "only the two changed settings");
    assert_eq!(p.effective(a).0.scale, 2.0, "A follows");
    assert_eq!(p.effective(b).0.scale, 3.0, "B kept its own size");
    assert_eq!(p.effective(b).0.depth.crown_shade, 0.8, "and still follows the rest");
    // a setting set to the same value the project has is still kept, because the user set it
    let project_cover = p.gen.canopy_cover;
    edit_group(&mut p, g, |gen, _| gen.canopy_cover = project_cover + 0.1, &[]);
    edit_group(&mut p, g, |gen, _| gen.canopy_cover = project_cover, &[]);
    assert_eq!(p.group(g).unwrap().overrides["canopy_cover"], json!(project_cover));
    // so a later change to the project no longer reaches it
    p.gen.canopy_cover = 0.2;
    assert_eq!(p.effective(a).0.canopy_cover, project_cover);
    // resetting puts a setting back to what the group inherits
    edit_group(&mut p, g, |gen, _| gen.canopy_cover = 0.2, &["canopy_cover"]);
    assert!(p.group(g).unwrap().overrides.get("canopy_cover").is_none());
    assert_eq!(p.effective(a).0.canopy_cover, 0.2, "follows the project again");
    // and the group inside can still be edited on its own without touching the group above
    edit_group(&mut p, a, |gen, _| gen.depth.gain = 1.9, &[]);
    assert_eq!(p.group(a).unwrap().overrides, json!({"depth": {"gain": 1.9f32}}));
    assert_eq!(p.group(g).unwrap().overrides.get("depth").and_then(|d| d.get("gain")), None);
}

#[test]
fn a_new_species_list_comes_with_its_indexes() {
    let mut p = two_zones();
    let g = group(&mut p, None, "G", serde_json::Value::Null);
    let a = group(&mut p, Some(g), "A", serde_json::Value::Null);
    let t = themes().into_iter().find(|t| t.name == "Autumn").unwrap();
    // a theme picked for the group: the panel applies it to a copy of what the group uses
    let (g0, s0) = p.effective(g);
    let mut tmp = p.clone();
    tmp.gen = g0.clone();
    tmp.species = s0.clone();
    tmp.apply_theme(&t);
    p.record_edit(g, (&g0, &s0), (&tmp.gen, &tmp.species), &[]);
    let node = p.group(g).unwrap();
    assert!(node.species.is_some(), "the group keeps the list the theme made");
    for k in SPECIES_KEYS {
        assert!(node.overrides.get(k).is_some(), "{k} goes with the list");
    }
    let (gen, species) = p.effective(a);
    assert_eq!(species.len(), tmp.species.len());
    for i in [gen.main_species, gen.mix_species, gen.shrub_species] {
        assert!((i as usize) < species.len(), "index {i} is inside the list");
    }
    // the group inside shows the theme species, not the project ones
    assert_eq!(species[gen.main_species as usize].name, tmp.species[tmp.gen.main_species as usize].name);
}


#[test]
fn editing_a_group_changes_the_render_key() {
    use serde_json::json;
    let mut p = two_zones();
    let g = group(&mut p, None, "G", serde_json::Value::Null);
    zone(&mut p, Some(g), 20.0);
    let before = p.render_key();
    edit_group(&mut p, g, |gen, _| gen.scale = 2.0, &[]);
    assert_ne!(p.render_key(), before, "a change to a group redraws what is in it");
    let again = p.render_key();
    p.group_mut(g).unwrap().visible = false;
    assert_ne!(p.render_key(), again, "so does hiding it");
    let hidden = p.render_key();
    p.group_mut(g).unwrap().overrides = json!({"scale": 3.0});
    assert_ne!(p.render_key(), hidden);
    // the picture under the forest is not part of it
    let mut q = p.clone();
    q.background_image = Some(BackgroundImage { path: "x.png".into(), opacity: 0.5, visible: true });
    assert_eq!(q.render_key(), p.render_key());
}

/// The average border pull-in over the cells inside a zone.
fn mean_pull_in(p: &Project, scale: f32, zone: usize) -> f32 {
    let plan = forest_core::engine::plan(p, scale, &|| false).unwrap();
    let (v, gw, gh, cs) = plan.pull_in();
    let pts = &p.shapes[zone].points;
    let inside = |x: f32, y: f32| {
        let (mut c, mut j) = (false, pts.len() - 1);
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[j]);
            if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                c = !c;
            }
            j = i;
        }
        c
    };
    let (mut sum, mut n) = (0.0, 0);
    for gy in 0..gh {
        for gx in 0..gw {
            if inside((gx * cs + cs / 2) as f32 / scale, (gy * cs + cs / 2) as f32 / scale) {
                sum += v[gy * gw + gx];
                n += 1;
            }
        }
    }
    sum / n.max(1) as f32
}

#[test]
fn separate_zones_are_fitted_to_the_border_on_their_own() {
    let rect = |x0: f32, x1: f32, y0: f32, y1: f32| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut p = Project { map_w: 1200, map_h: 600, ..Default::default() };
    p.shapes.push(Shape::new(ShapeOp::Add, rect(40.0, 500.0, 40.0, 560.0)));
    p.shapes.push(Shape::new(ShapeOp::Add, rect(580.0, 720.0, 200.0, 340.0)));
    let together = mean_pull_in(&p, 1.0, 1);
    let mut alone = p.clone();
    alone.shapes.remove(0);
    let apart = mean_pull_in(&alone, 1.0, 0);
    assert!((together - apart).abs() <= 0.02 * apart.max(1.0), "a small zone beside a big one is pulled in as if alone: {together:.2} against {apart:.2}");
    // zones that touch are one shape and are fitted as one
    p.shapes[1].points = rect(480.0, 720.0, 200.0, 340.0);
    let merged = mean_pull_in(&p, 1.0, 1);
    assert!(merged > 0.0);
}

#[test]
fn no_crown_crosses_the_drawn_line_in_a_thin_zone() {
    // a thin zone with a strong edge wobble: the fit alone cannot hold the forest, so crowns are kept off the line
    let mut p = Project { map_w: 800, map_h: 400, ..Default::default() };
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[60.0, 150.0], [740.0, 150.0], [740.0, 270.0], [60.0, 270.0]]));
    p.gen.edge_warp = 90.0;
    let plan = forest_core::engine::plan(&p, 0.5, &|| false).unwrap();
    assert!(plan.trees.len() > 20);
    for t in &plan.trees {
        let inside = (t.x - 60.0).min(740.0 - t.x).min(t.y - 150.0).min(270.0 - t.y);
        assert!(inside >= t.r - 1.0, "crown at ({:.0}, {:.0}) radius {:.0} reaches the line", t.x, t.y, t.r);
    }
}
