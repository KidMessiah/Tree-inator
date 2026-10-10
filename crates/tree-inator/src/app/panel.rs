//! The settings panel: five sections (Shape, Forest, Look, Light and weather, Export). Each shows its main
//! controls first and keeps every other setting behind an "All ... settings" fold, so nothing is lost.

use super::simple::{choices, nearest, Slot};
use super::*;
use std::ops::RangeInclusive;

/// What differs from the defaults, for the dots and the "N changed" badges.
struct Diff {
    cur: serde_json::Value,
    def: serde_json::Value,
}

impl Diff {
    fn of<T: Serialize>(cur: &T, def: &T) -> Diff {
        Diff { cur: serde_json::to_value(cur).unwrap_or_default(), def: serde_json::to_value(def).unwrap_or_default() }
    }

    fn is(&self, key: &str) -> bool {
        self.cur.get(key) != self.def.get(key)
    }

    fn count(&self, keys: &[&str]) -> usize {
        keys.iter().filter(|k| self.is(k)).count()
    }
}

const SHAPE_KEYS: &[&str] = &["shape_softness", "edge_warp", "edge_noise", "pattern", "pattern_strength", "pattern_scale", "feather", "contain", "contain_fit", "contain_clip", "branch_width"];
const FOREST_KEYS: &[&str] = &[
    "mushrooms", "mushroom_mix", "giant_mushrooms", "crown_shyness", "shyness_gap", "shyness_giants_only",
    "scale", "canopy_cover", "crown_min", "crown_max", "gaps", "shrub_density", "grass_density", "lone_groups", "pebbles", "mix_amount", "mix_scale",
    "main_species", "mix_species", "shrub_species", "procedural_forms", "interior_lone",
];
const LOOK_KEYS: &[&str] = &["leaf", "depth", "grass_color"];
const WEATHER_KEYS: &[&str] = &["snow", "fire"];

/// A label with a dot in front when the setting differs from its default.
fn label(ui: &mut egui::Ui, changed: bool, text: &str, tip: &str) {
    let rich = if changed { egui::RichText::new(format!("\u{2022} {text}")).color(ui.visuals().hyperlink_color) } else { egui::RichText::new(format!("  {text}")) };
    ui.add_sized([112.0, 18.0], egui::Label::new(rich).truncate()).on_hover_text(tip);
}

fn slider(ui: &mut egui::Ui, changed: bool, text: &str, tip: &str, v: &mut f32, range: RangeInclusive<f32>) {
    ui.horizontal(|ui| {
        label(ui, changed, text, tip);
        ui.spacing_mut().slider_width = (ui.available_width() - 96.0).clamp(60.0, 420.0);
        ui.add(egui::Slider::new(v, range)).on_hover_text(tip);
    });
}

/// A 0..1 setting shown as a percentage.
fn pct(ui: &mut egui::Ui, changed: bool, text: &str, tip: &str, v: &mut f32, max: f32) {
    ui.horizontal(|ui| {
        label(ui, changed, text, tip);
        ui.spacing_mut().slider_width = (ui.available_width() - 96.0).clamp(60.0, 420.0);
        let mut p = *v * 100.0;
        if ui.add(egui::Slider::new(&mut p, 0.0..=max).suffix("%")).on_hover_text(tip).changed() {
            *v = p / 100.0;
        }
    });
}

impl TreeApp {
    pub(super) fn settings_panel(&mut self, ui: &mut egui::Ui) {
        // a layer or group is marked against what it inherits, the project against the defaults
        let gd = self.gen_base.clone().unwrap_or_default();
        let gen = Diff::of(&self.project.gen, &gd);
        let light = Diff::of(&self.project.light, &Light::default());
        let q = self.search.trim().to_lowercase();

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search settings").desired_width(ui.available_width() - 28.0));
            if ui.small_button("x").on_hover_text("Clear the search").clicked() {
                self.search.clear();
            }
        });
        ui.add_space(2.0);
        let shown = |title: &str, words: &str| q.is_empty() || title.to_lowercase().contains(&q) || words.contains(&q);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if shown("Shape", "outline edge softness warp noise pattern fragmented branching clumps lobed feather fit patch fill style") {
                let n = gen.count(SHAPE_KEYS);
                self.section(ui, "Shape", n, !q.is_empty(), SHAPE_KEYS, |app, ui| app.shape_main(ui, &gen));
            }
            if shown("Forest", "trees species mix density size scale crown shrubs lone clearings gaps grass canopy undergrowth seed variation") {
                let n = gen.count(FOREST_KEYS);
                self.section(ui, "Forest", n, !q.is_empty(), FOREST_KEYS, |app, ui| app.forest_main(ui, &gen));
            }
            if shown("Look", "theme colour color leaf variety brightness gain shading grading layers haze ground") {
                let n = gen.count(LOOK_KEYS);
                self.section(ui, "Look", n, !q.is_empty(), LOOK_KEYS, |app, ui| app.look_main(ui, &gen));
            }
            if shown("Light and weather", "sun shadow direction length softness elevation snow fire flat light") {
                let n = gen.count(WEATHER_KEYS) + light.count(&["shadow_angle", "elevation", "shadow_length", "shadow_softness", "shadows", "flat"]);
                self.section(ui, "Light and weather", n, !q.is_empty(), WEATHER_KEYS, |app, ui| app.light_main(ui, &gen, &light));
            }
            if shown("Export", "export png scale size preview detail defaults") {
                self.section(ui, "Export", 0, !q.is_empty(), &[], |app, ui| app.export_main(ui));
            }
            if !q.is_empty() && !["shape", "forest", "look", "light", "export"].iter().any(|s| s.contains(&q)) && false {
                ui.small("No settings match.");
            }
        });
    }

    /// One section: a header with a badge when something changed, the body, and a reset for the section.
    fn section(&mut self, ui: &mut egui::Ui, title: &str, changed: usize, force_open: bool, reset: &[&str], body: impl FnOnce(&mut Self, &mut egui::Ui)) {
        let mut head = egui::RichText::new(title).strong();
        if changed > 0 {
            head = egui::RichText::new(format!("{title}   {changed} changed")).strong();
        }
        let mut h = egui::CollapsingHeader::new(head).id_salt(("section", title)).default_open(title == "Forest");
        if force_open {
            h = h.open(Some(true));
        }
        h.show(ui, |ui| {
            body(self, ui);
            if !reset.is_empty() {
                ui.add_space(4.0);
                if ui.small_button("Reset this section").on_hover_text("Put every setting in this section back to its default").clicked() {
                    self.reset_gen_keys(reset);
                    if title == "Light and weather" {
                        self.project.light = Light::default();
                    }
                }
            }
        });
    }

    /// Put settings back: to the defaults for the project, to what a layer or group inherits for a node.
    fn reset_gen_keys(&mut self, keys: &[&str]) {
        let def = serde_json::to_value(self.gen_base.clone().unwrap_or_default()).unwrap_or_default();
        let mut cur = serde_json::to_value(&self.project.gen).unwrap_or_default();
        for k in keys {
            if let Some(v) = def.get(k) {
                cur[*k] = v.clone();
            }
            if self.gen_base.is_some() {
                self.pending_reset.push(k.to_string());
            }
        }
        if let Ok(g) = serde_json::from_value(cur) {
            self.project.gen = g;
        }
    }

    fn fold<R>(&mut self, ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut Self, &mut egui::Ui) -> R) {
        egui::CollapsingHeader::new(egui::RichText::new(title).weak()).id_salt(("fold", title)).default_open(false).show(ui, |ui| {
            body(self, ui);
        });
    }

    // ---------------------------------------------------------------- sections

    fn shape_main(&mut self, ui: &mut egui::Ui, d: &Diff) {
        ui.horizontal(|ui| {
            if ui.button("Generate patch").on_hover_text("Random natural outline in the middle of the map").clicked() {
                self.generate_patch();
            }
            if ui.button("Fill map").clicked() {
                self.fill_map();
            }
        });
        let g = &mut self.project.gen;
        ui.horizontal(|ui| {
            label(ui, false, "Edge style", "Starting points for the edge and pattern settings");
            egui::ComboBox::from_id_salt("edge_style").selected_text("Apply...").show_ui(ui, |ui| {
                for (name, f) in PRESETS {
                    if ui.selectable_label(false, *name).clicked() {
                        f(g);
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            label(ui, d.is("pattern"), "Pattern", "The overall shape of the forest edge");
            egui::ComboBox::from_id_salt("pattern").selected_text(g.pattern.name()).show_ui(ui, |ui| {
                for p in Pattern::ALL {
                    ui.selectable_value(&mut g.pattern, p, p.name());
                }
            });
        });
        slider(ui, d.is("pattern_strength"), "Pattern strength", "How strongly the pattern reshapes the drawn outline", &mut g.pattern_strength, 0.0..=1.0);
        slider(ui, d.is("shape_softness"), "Edge softness", "How gradually the forest thins at its edge", &mut g.shape_softness, 5.0..=200.0);
        slider(ui, d.is("edge_warp"), "Edge wobble", "How much the outline wanders", &mut g.edge_warp, 0.0..=300.0);
        ui.checkbox(&mut g.contain, "Keep everything inside the line I draw");
        self.fold(ui, "All shape settings", |app, ui| {
            app.sec_patch_shape(ui);
            app.sec_edges(ui);
        });
    }

    fn forest_main(&mut self, ui: &mut egui::Ui, d: &Diff) {
        ui.small("What grows, and how thick");
        ui.checkbox(&mut self.project.gen.procedural_forms, "Procedural trees")
            .on_hover_text("Every tree grows as a procedural tree type. Off brings back the built-in forms.");
        self.tree_type_picker(ui, Slot::Main, "Main trees");
        self.tree_type_picker(ui, Slot::Mix, "Mixed in");
        let g = &mut self.project.gen;
        pct(ui, d.is("mix_amount"), "Mix amount", "How much of the second tree is mixed into the canopy", &mut g.mix_amount, 100.0);
        self.tree_type_picker(ui, Slot::Shrub, "Shrubs");
        let g = &mut self.project.gen;
        let mut size = g.scale * 100.0;
        ui.horizontal(|ui| {
            label(ui, d.is("scale"), "Tree size", "Makes every tree, leaf, shrub and shadow bigger or smaller on the same drawn area");
            ui.spacing_mut().slider_width = (ui.available_width() - 96.0).clamp(60.0, 420.0);
            if ui.add(egui::Slider::new(&mut size, 10.0..=400.0).logarithmic(true).suffix("%")).changed() {
                g.scale = size / 100.0;
            }
        });
        slider(ui, d.is("canopy_cover"), "Density", "How much of the area the crowns cover", &mut g.canopy_cover, 0.05..=1.0);
        slider(ui, d.is("shrub_density"), "Shrubs amount", "How many shrubs grow under and around the trees", &mut g.shrub_density, 0.0..=3.0);
        slider(ui, d.is("lone_groups"), "Lone trees", "Small groups of trees outside the edge", &mut g.lone_groups, 0.0..=5.0);
        slider(ui, d.is("gaps"), "Clearings", "Gaps inside the forest where trees have fallen", &mut g.gaps, 0.0..=1.0);
        self.mushroom_controls(ui, d);
        self.shyness_controls(ui, d);
        let g = &mut self.project.gen;
        if ui.button("New variation").on_hover_text("Same settings, different trees (R)").clicked() {
            g.seed = g.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) % 1_000_000;
        }
        self.fold(ui, "All forest settings", |app, ui| {
            app.sec_size(ui);
            app.sec_canopy(ui);
            app.sec_undergrowth_and_fringe(ui);
            app.sec_forest_type(ui);
            app.sec_which_trees(ui);
            app.sec_species_editor(ui);
        });
    }

    /// Crown shyness: a thin gap where neighbouring crowns of similar height meet.
    fn shyness_controls(&mut self, ui: &mut egui::Ui, d: &Diff) {
        let g = &mut self.project.gen;
        ui.horizontal(|ui| {
            label(ui, d.is("crown_shyness"), "Crown shyness", "Thin gaps where neighbouring crowns of similar height meet, as in real canopies. Trees and mushrooms");
            ui.checkbox(&mut g.crown_shyness, "On");
        });
        if g.crown_shyness {
            slider(ui, d.is("shyness_gap"), "Gap width", "Width of the gaps between crowns, in map pixels. Small crowns get thinner gaps", &mut g.shyness_gap, 0.5..=6.0);
            ui.horizontal(|ui| {
                label(ui, d.is("shyness_giants_only"), "Only giants", "Only the giant mushrooms keep gaps; trees and small mushrooms overlap as before");
                ui.checkbox(&mut g.shyness_giants_only, "Giant mushrooms only");
            });
        }
    }

    /// Mushroom amount and the share of each kind.
    fn mushroom_controls(&mut self, ui: &mut egui::Ui, d: &Diff) {
        let g = &mut self.project.gen;
        slider(ui, d.is("mushrooms"), "Mushrooms", "How many mushroom clusters grow on open ground and in clearings. Off at 0", &mut g.mushrooms, 0.0..=4.0);
        pct(ui, d.is("giant_mushrooms"), "Giant mushrooms", "Share of the big trees that are mushrooms as tall as trees", &mut g.giant_mushrooms, 60.0);
        if g.mushrooms > 0.0 || g.giant_mushrooms > 0.0 {
            ui.horizontal_wrapped(|ui| {
                for (k, (_, name)) in forest_core::mushrooms::KINDS.iter().enumerate() {
                    let mut on = g.mushroom_mix[k] > 0.0;
                    if ui.checkbox(&mut on, *name).changed() {
                        g.mushroom_mix[k] = if on { 0.5 } else { 0.0 };
                    }
                }
            });
            if g.mushroom_mix.iter().sum::<f32>() <= 0.0 {
                g.mushroom_mix = [0.5; 5];
            }
            self.project.ensure_mushroom_species();
        }
    }

    fn look_main(&mut self, ui: &mut egui::Ui, d: &Diff) {
        self.theme_cards(ui);
        let g = &mut self.project.gen;
        pct(ui, false, "Colour variety", "Each leaf a little lighter, darker, yellower or bluer", &mut g.leaf.variety, 300.0);
        pct(ui, false, "Crown shading", "Crown tops lit, gaps between crowns dark", &mut g.depth.crown_shade, 150.0);
        slider(ui, false, "Brightness", "Overall brightness of the canopy", &mut g.depth.gain, 0.4..=2.0);
        ui.horizontal(|ui| {
            label(ui, false, "Background", "Colour behind the map in the editor");
            ui.color_edit_button_rgb(&mut self.background);
        });
        self.fold(ui, "All look settings", |app, ui| {
            app.sec_leaf_colour(ui);
            app.sec_canopy_depth_and_clusters(ui);
            app.sec_layers_and_colour_grading(ui);
        });
        let _ = d;
    }

    fn light_main(&mut self, ui: &mut egui::Ui, d: &Diff, ld: &Diff) {
        let light = &mut self.project.light;
        ui.checkbox(&mut light.shadows, "Cast shadows");
        let dirs = [("Up left", 225.0), ("Up right", 315.0), ("Down left", 135.0), ("Down right", 45.0)];
        let cur = dirs.iter().position(|dd| (dd.1 - light.shadow_angle).abs() < 0.5);
        if let Some(a) = choices(ui, "Shadows fall", &dirs, cur) {
            light.shadow_angle = a;
        }
        slider(ui, ld.is("shadow_length"), "Shadow length", "How far shadows reach", &mut light.shadow_length, 0.2..=4.0);
        slider(ui, ld.is("shadow_softness"), "Shadow softness", "How blurred shadow edges are", &mut light.shadow_softness, 0.0..=4.0);
        ui.add_space(4.0);
        let g = &mut self.project.gen;
        let looks = snow_looks();
        let sn = &mut g.snow;
        let now = serde_json::to_string(&*sn).unwrap_or_default();
        let current = if sn.amount <= 0.0 { Some(0) } else { looks.iter().position(|(_, l)| serde_json::to_string(l).unwrap_or_default() == now) };
        let opts: Vec<(&str, usize)> = looks.iter().enumerate().map(|(k, (n, _))| (*n, k)).collect();
        if let Some(k) = choices(ui, "Snow", &opts, current) {
            *sn = looks[k].1.clone();
        }
        pct(ui, d.is("snow"), "Snow amount", "How much snow there is overall", &mut sn.amount, 100.0);
        let looks = fire_looks();
        let fi = &mut g.fire;
        let now = serde_json::to_string(&*fi).unwrap_or_default();
        let current = if fi.amount <= 0.0 { Some(0) } else { looks.iter().position(|(_, l)| serde_json::to_string(l).unwrap_or_default() == now) };
        let opts: Vec<(&str, usize)> = looks.iter().enumerate().map(|(k, (n, _))| (*n, k)).collect();
        if let Some(k) = choices(ui, "Fire", &opts, current) {
            *fi = looks[k].1.clone();
        }
        pct(ui, d.is("fire"), "Fire amount", "How much of the forest has burnt", &mut fi.amount, 100.0);
        self.fold(ui, "All light and weather settings", |app, ui| {
            app.sec_shadows(ui);
            app.sec_snow(ui);
            app.sec_fire(ui);
        });
        let _ = nearest;
    }

    fn export_main(&mut self, ui: &mut egui::Ui) {
        self.sec_export(ui);
        self.fold(ui, "Preview and defaults", |app, ui| {
            app.sec_preview(ui);
            app.sec_my_defaults(ui);
        });
    }

    /// Theme cards: a baked picture of each theme on one generic layout, and its main colours.
    fn theme_cards(&mut self, ui: &mut egui::Ui) {
        let list = themes();
        ui.small("Pick a theme, then fine tune. Tree types you picked stay.");
        let cols = 3;
        let gap = 6.0;
        let w = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).floor().max(70.0);
        let h = (w * 0.625).floor();
        let mut picked = None;
        egui::Grid::new("theme_cards").spacing([gap, gap]).show(ui, |ui| {
            for (k, t) in list.iter().enumerate() {
                let tex = self.theme_texture(ui.ctx(), t.name);
                ui.vertical(|ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h + 22.0), Sense::click());
                    let sel = self.theme == Some(t.name);
                    let painter = ui.painter_at(rect);
                    let img = Rect::from_min_size(rect.min, Vec2::new(w, h));
                    match &tex {
                        Some(tx) => {
                            painter.image(tx.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        }
                        None => {
                            painter.rect_filled(img, 3.0, Color32::from_gray(60));
                        }
                    }
                    // the theme's colours along the bottom of the picture
                    let swatch = |c: [f32; 3]| Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
                    let cols: [Color32; 5] = [
                        swatch(t.species[0].ramp[2]),
                        swatch(t.species[1].ramp[2]),
                        swatch(t.species[2].ramp[2]),
                        Color32::from_rgb(t.grass[0], t.grass[1], t.grass[2]),
                        Color32::from_rgb(t.haze[0], t.haze[1], t.haze[2]),
                    ];
                    let sw = w / 5.0;
                    for (i, c) in cols.iter().enumerate() {
                        painter.rect_filled(Rect::from_min_size(Pos2::new(rect.min.x + sw * i as f32, img.max.y - 5.0), Vec2::new(sw, 5.0)), 0.0, *c);
                    }
                    let stroke = if sel { Stroke::new(2.0, ui.visuals().hyperlink_color) } else { Stroke::new(1.0, Color32::from_gray(80)) };
                    painter.rect_stroke(img, 3.0, stroke, egui::StrokeKind::Inside);
                    painter.text(Pos2::new(rect.min.x + 2.0, img.max.y + 3.0), egui::Align2::LEFT_TOP, t.name, egui::FontId::proportional(11.0), ui.visuals().text_color());
                    if resp.on_hover_text(t.note).clicked() {
                        picked = Some(k);
                    }
                });
                if (k + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        if let Some(k) = picked {
            self.project.apply_theme(&list[k]);
            self.theme = Some(list[k].name);
        }
    }

    fn theme_texture(&mut self, ctx: &egui::Context, name: &'static str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.theme_tex.get(name) {
            return Some(t.clone());
        }
        let bytes = super::theme_cards::CARDS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)?;
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        let tex = ctx.load_texture(format!("theme_{name}"), ci, egui::TextureOptions::LINEAR);
        self.theme_tex.insert(name, tex.clone());
        Some(tex)
    }
}
