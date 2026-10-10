//! Awkward inputs: nothing here should panic, hang or produce nonsense. Small maps keep it quick.
use forest_core::*;

fn small() -> Project {
    let mut p = Project { map_w: 256, map_h: 192, ..Default::default() };
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[20.0, 20.0], [230.0, 30.0], [220.0, 170.0], [30.0, 160.0]]));
    p
}

fn render(p: &Project, scale: f32) -> Vec<u8> {
    compose_stack(&generate_stack(p, scale, &|| false).expect("renders"), &p.grading, ComposeOptions::default())
}

#[test]
fn empty_and_tiny_projects_render() {
    let p = Project { map_w: 64, map_h: 64, ..Default::default() };
    assert!(render(&p, 1.0).chunks(4).all(|px| px[3] == 0));
    let mut t = small();
    t.map_w = 64;
    t.map_h = 64;
    let _ = render(&t, 1.0);
    let _ = render(&small(), 0.25);
}

#[test]
fn bad_shapes_are_ignored_or_survived() {
    let mut p = small();
    p.shapes.push(Shape::new(ShapeOp::Add, vec![]));
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[10.0, 10.0]]));
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[10.0, 10.0], [50.0, 50.0]]));
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[0.0, 0.0], [100.0, 100.0], [200.0, 200.0]]));
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[-5000.0, -5000.0], [9000.0, -4000.0], [3000.0, 9000.0]]));
    p.shapes.push(Shape::new(ShapeOp::Subtract, vec![[100.0, 100.0], [100.0, 100.0], [100.0, 100.0]]));
    let _ = render(&p, 0.5);
}

#[test]
fn out_of_range_indexes_do_not_panic() {
    let mut p = small();
    p.gen.main_species = 250;
    p.gen.shrub_species = 251;
    p.gen.mix_species = 252;
    p.gen.mix_amount = 0.5;
    p.shapes[0].group = Some(7);
    p.painted.push(TreeRec { x: 100.0, y: 100.0, r: 12.0, h: 20.0, tone: 0.0, layer: TreeLayer::Canopy, species: 200, seed: 1 });
    let _ = render(&p, 0.5);
    // a layer that is not there is treated as no layer
    assert_eq!(zone_passes(&p).len(), 1);
    // a layer with an empty species list renders with the project one
    let id = p.add_group(None);
    p.group_mut(id).unwrap().species = Some(Vec::new());
    p.shapes[0].group = Some(id);
    let _ = render(&p, 0.5);
    // a layer whose settings make no sense falls back to the project ones
    p.group_mut(id).unwrap().overrides = serde_json::json!({"scale": "wide", "canopy_cover": [1, 2]});
    let _ = render(&p, 0.5);
}

#[test]
fn extreme_settings_render() {
    for (scale, density, size) in [(0.5, 0.0, 0.1), (0.5, 1.0, 0.5), (1.0, 1.0, 4.0), (0.25, 0.05, 1.0)] {
        let mut p = small();
        p.gen.canopy_cover = density;
        p.gen.scale = size;
        let _ = render(&p, scale);
    }
    let mut p = small();
    p.gen.crown_shyness = true;
    p.gen.shyness_gap = 40.0;
    p.gen.giant_mushrooms = 1.0;
    p.gen.mushrooms = 4.0;
    p.ensure_mushroom_species();
    p.gen.mushroom_mix = [1.0; 5];
    let _ = render(&p, 0.5);
    p.gen.shyness_gap = 0.0;
    p.gen.shyness_giants_only = true;
    let _ = render(&p, 0.5);
}

#[test]
fn every_theme_and_tree_type_renders_and_survives_a_round_trip() {
    for t in themes() {
        let mut p = small();
        p.apply_theme(&t);
        let json = serde_json::to_string(&p).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.species.len(), p.species.len(), "{}", t.name);
        // the theme's species have tree types that exist
        for sp in &t.species {
            if let Some(ty) = sp.tree_type(true) {
                assert!(treegen::preset_by_name(ty).is_some() || dead::is_dead(ty), "{}: {ty}", t.name);
            }
        }
        let img = render(&back, 0.5);
        assert!(img.chunks(4).any(|px| px[3] > 0), "{} draws something", t.name);
    }
}

#[test]
fn groups_and_painted_trees_survive_odd_edits() {
    let mut p = small();
    let g = p.add_group(None);
    let ids: Vec<u32> = (0..4).map(|k| p.add_group(if k % 2 == 0 { Some(g) } else { None })).collect();
    p.group_mut(ids[1]).unwrap().visible = false;
    p.shapes[0].group = Some(ids[0]);
    p.add_zone(ShapeOp::Add, vec![[60.0, 60.0], [120.0, 60.0], [90.0, 120.0]], Some(ids[2]));
    p.add_zone(ShapeOp::Add, vec![[140.0, 60.0], [200.0, 60.0], [170.0, 120.0]], Some(ids[3]));
    p.add_zone(ShapeOp::Subtract, vec![[80.0, 80.0], [150.0, 80.0], [120.0, 140.0]], Some(ids[1]));
    p.painted.push(TreeRec { x: 90.0, y: 90.0, r: 14.0, h: 24.0, tone: 0.0, layer: TreeLayer::Canopy, species: 0, seed: 2 });
    p.erased.push([100.0, 100.0, 30.0]);
    let _ = render(&p, 0.5);
    p.ungroup(g);
    p.delete_group(ids[2]);
    p.delete_group(9999);
    assert!(!p.nudge(9999, 1));
    let _ = render(&p, 0.5);
    for id in ids {
        p.delete_group(id);
    }
    assert!(p.shapes.iter().all(|s| s.group.is_none()));
    let _ = render(&p, 0.5);
}

#[test]
fn a_project_with_no_species_still_renders() {
    let mut p = small();
    p.species.clear();
    let _ = render(&p, 0.5);
}

#[test]
fn cancelling_a_render_stops_it() {
    let p = small();
    assert!(generate(&p, 0.5, &|| true).is_none());
}
