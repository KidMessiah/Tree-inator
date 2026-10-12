//! Writes a small test project: a Temperate forest on the left, and on the right a zone with its own
//! settings (Autumn colours, trees twice the size). Usage: make_zones out.json [layers] [map_w map_h]
use forest_core::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let layers: usize = args.get(1).map_or(1, |v| v.parse().unwrap());
    let (mw, mh): (u32, u32) = (args.get(2).map_or(2048, |v| v.parse().unwrap()), args.get(3).map_or(1024, |v| v.parse().unwrap()));
    let mut p = Project { map_w: mw, map_h: mh, ..Default::default() };
    p.ensure_mushroom_species();
    let t = themes().into_iter().find(|t| t.name == "Temperate").unwrap();
    p.apply_theme(&t);
    let (w, h) = (mw as f32, mh as f32);
    p.shapes.push(Shape::new(ShapeOp::Add, vec![[0.05 * w, 0.1 * h], [0.46 * w, 0.1 * h], [0.46 * w, 0.9 * h], [0.05 * w, 0.9 * h]]));
    for k in 0..layers {
        let x0 = 0.54 + k as f32 * 0.0;
        let (a, b) = (0.1 + k as f32 * 0.3, 0.1 + (k as f32 + 1.0) * 0.3 - 0.02);
        let mut z = Shape::new(ShapeOp::Add, vec![[x0 * w, a * h], [0.95 * w, a * h], [0.95 * w, b * h], [x0 * w, b * h]]);
        let mut q = p.clone();
        let theme = ["Autumn", "Frozen", "Jungle"][k % 3];
        q.apply_theme(&themes().into_iter().find(|t| t.name == theme).unwrap());
        q.gen.scale = 1.0 + 0.5 * (k as f32 + 1.0);
        let id = p.add_group(None);
        let (gv, qv) = (serde_json::to_value(&p.gen).unwrap(), serde_json::to_value(&q.gen).unwrap());
        let mut over = diff_value(&gv, &qv);
        if let Some(o) = over.as_object_mut() {
            // the species list and its indexes go together
            for key in SPECIES_KEYS {
                o.insert(key.to_string(), qv[key].clone());
            }
        }
        let node = p.group_mut(id).unwrap();
        node.name = format!("Layer {}", k + 1);
        node.overrides = over;
        node.species = Some(q.species);
        z.group = Some(id);
        p.shapes.push(z);
    }
    std::fs::write(&args[0], serde_json::to_string(&p).unwrap()).unwrap();
}
