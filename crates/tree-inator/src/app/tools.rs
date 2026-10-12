//! The drawing tools: the tool strip, the options bar, the species strip, and what each tool does. Zones are
//! combined with the plane geometry in `forest_core::geom`.

use super::icons::{eye_icon, lock_icon, tool_icon};
use super::*;
use forest_core::geom::{self, BoolOp, Ring, Shape as Region};

/// A picture kept in memory at a modest size, for the wand to read.
pub(super) struct BgPixels {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tool {
    Select,
    Lasso,
    Polygon,
    Wand,
    Line,
    Brush,
    Single,
    Eraser,
    Mask,
    Pan,
}

impl Tool {
    pub(super) const ALL: [Tool; 10] = [Tool::Select, Tool::Lasso, Tool::Polygon, Tool::Wand, Tool::Line, Tool::Brush, Tool::Single, Tool::Eraser, Tool::Mask, Tool::Pan];

    pub(super) fn is_brush(self) -> bool {
        matches!(self, Tool::Brush | Tool::Single | Tool::Eraser)
    }

    fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select and edit",
            Tool::Lasso => "Lasso",
            Tool::Polygon => "Polygon",
            Tool::Wand => "Wand: trace the map",
            Tool::Line => "Line: paths and tree lines",
            Tool::Brush => "Tree brush",
            Tool::Single => "Single tree",
            Tool::Eraser => "Tree eraser",
            Tool::Mask => "No-trees mask",
            Tool::Pan => "Pan",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::Lasso => "L",
            Tool::Polygon => "P",
            Tool::Wand => "W",
            Tool::Line => "N",
            Tool::Brush => "B",
            Tool::Single => "T",
            Tool::Eraser => "E",
            Tool::Mask => "M",
            Tool::Pan => "H",
        }
    }

    pub(super) fn from_key(k: Key) -> Option<Tool> {
        Some(match k {
            Key::V => Tool::Select,
            Key::L => Tool::Lasso,
            Key::P => Tool::Polygon,
            Key::W => Tool::Wand,
            Key::N => Tool::Line,
            Key::B => Tool::Brush,
            Key::T => Tool::Single,
            Key::E => Tool::Eraser,
            Key::M => Tool::Mask,
            Key::H => Tool::Pan,
            _ => return None,
        })
    }

    pub(super) fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Click a zone to select it. Ctrl or Shift click adds more. Drag its corners or the zone. Double click an edge to add a corner, right click a corner to remove it. Delete removes the zone.",
            Tool::Lasso => "Drag a loop around the area. Hold Alt to subtract, Shift+Alt to intersect.",
            Tool::Polygon => "Click corners. Enter, right click or a double click closes the shape. Escape cancels.",
            Tool::Wand => "Click a region of the imported map to trace it. Alt subtracts, Shift+Alt intersects.",
            Tool::Line => "Click points along the path. Enter, right click or a double click finishes it.",
            Tool::Brush => "Drag to scatter trees. [ and ] change the size. 1 to 9 pick the species.",
            Tool::Single => "Click to place one tree. 1 to 9 pick the species.",
            Tool::Eraser => "Drag over trees to remove them.",
            Tool::Mask => "Drag a loop around ground that must stay free of trees: roads, camps, labels.",
            Tool::Pan => "Drag to move the map. Wheel zooms. Space+drag pans from any tool.",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Mode {
    Add,
    Subtract,
    Intersect,
}

/// A row of the zone list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RowId {
    Zone(usize),
    Group(u32),
}

/// What the line tool does with the path it was given.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum LineUse {
    Cut,
    Trees,
    Split,
}

fn rings_of(z: &forest_core::Shape) -> Ring {
    z.points.clone()
}

impl TreeApp {
    // ------------------------------------------------------------------ the strip and the bars

    pub(super) fn tool_strip(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        for t in Tool::ALL {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 40.0), Sense::click());
            let sel = self.tool == t;
            let bg = if sel { Color32::from_rgb(34, 92, 140) } else if resp.hovered() { Color32::from_gray(52) } else { Color32::TRANSPARENT };
            ui.painter().rect_filled(rect, 5.0, bg);
            tool_icon(ui.painter(), rect.shrink(6.0), t, if sel { Color32::WHITE } else { Color32::from_gray(205) });
            ui.painter().text(rect.right_bottom() + Vec2::new(-3.0, -2.0), egui::Align2::RIGHT_BOTTOM, t.key(), egui::FontId::proportional(9.0), Color32::from_gray(150));
            if resp.on_hover_text(format!("{}  ({})\n{}", t.name(), t.key(), t.hint())).clicked() {
                self.set_tool(t);
            }
            if matches!(t, Tool::Select | Tool::Wand | Tool::Line | Tool::Mask) {
                ui.add_space(2.0);
            }
        }
    }

    pub(super) fn set_tool(&mut self, t: Tool) {
        if self.tool != t {
            self.tool = t;
            self.stroke.clear();
            self.line_use = LineUse::Cut;
            self.wand_preview = None;
            self.status.clear();
        }
    }

    /// What a gesture does right now: the mode picked, changed while Alt (subtract) or Shift+Alt (intersect) is held.
    pub(super) fn mode_now(&self, ctx: &egui::Context) -> Mode {
        let m = ctx.input(|i| i.modifiers);
        if m.alt && m.shift {
            Mode::Intersect
        } else if m.alt {
            Mode::Subtract
        } else {
            self.mode
        }
    }

    pub(super) fn options_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(self.tool.name()).strong());
            ui.separator();
            if matches!(self.tool, Tool::Lasso | Tool::Polygon | Tool::Wand) {
                for (m, l, tip) in [(Mode::Add, "Add", "Grow the forest"), (Mode::Subtract, "Subtract", "Cut a clearing (hold Alt)"), (Mode::Intersect, "Intersect", "Keep only the overlap with the zones (hold Shift+Alt)")] {
                    ui.selectable_value(&mut self.mode, m, l).on_hover_text(tip);
                }
                ui.separator();
            }
            match self.tool {
                Tool::Select => self.select_options(ui),
                Tool::Lasso | Tool::Polygon => {
                    opt_slider(ui, "Smooth", &mut self.draw_smooth, 0.0..=1.0).on_hover_text("Rounds the outline you draw and removes jitter");
                    ui.checkbox(&mut self.snap, "Snap to zone edges").on_hover_text("Points snap to other zones' corners and edges, so touching zones leave no gap");
                }
                Tool::Wand => {
                    opt_slider(ui, "Tolerance", &mut self.wand_tol, 1.0..=100.0).on_hover_text("How different from the clicked colour a pixel may be and still be included");
                    opt_slider(ui, "Smooth", &mut self.draw_smooth, 0.0..=1.0);
                    ui.checkbox(&mut self.wand_contiguous, "Contiguous").on_hover_text("Only the region connected to the click. Off takes every similar colour on the map.");
                    if self.bg_pixels.is_none() {
                        ui.label(egui::RichText::new("Import a map image in the Map tab first").color(Color32::from_rgb(255, 200, 90)));
                    }
                }
                Tool::Line => {
                    opt_slider(ui, "Width", &mut self.line_width, 2.0..=300.0);
                    ui.selectable_value(&mut self.line_use, LineUse::Cut, "Cut a path").on_hover_text("A clearing along the line, for roads and streams");
                    ui.selectable_value(&mut self.line_use, LineUse::Trees, "Trees along it").on_hover_text("A row of trees, for hedges and avenues");
                    if self.line_use == LineUse::Trees {
                        opt_slider(ui, "Spacing", &mut self.line_spacing, 4.0..=120.0);
                    }
                    if self.line_use == LineUse::Split {
                        ui.label(egui::RichText::new("Draw a line across the selected zone to cut it in two").color(Color32::from_rgb(255, 200, 90)));
                    }
                    ui.checkbox(&mut self.snap, "Snap");
                }
                Tool::Brush | Tool::Eraser | Tool::Single => {
                    opt_slider(ui, "Size", &mut self.brush.radius, 5.0..=400.0);
                    if self.tool == Tool::Brush {
                        opt_slider(ui, "Density", &mut self.brush.density, 0.0..=1.0);
                    }
                    if self.tool == Tool::Eraser {
                        ui.checkbox(&mut self.brush.erase_generated, "Generated trees too");
                        ui.checkbox(&mut self.brush.erase_layer_only, "This layer only");
                    } else {
                        ui.checkbox(&mut self.brush.replace_generated, "Replace generated trees");
                    }
                }
                Tool::Mask => {
                    ui.label(egui::RichText::new("Hatched areas stay free of trees and ground cover. Shown in the zone list as masks.").weak());
                }
                Tool::Pan => {
                    ui.label(egui::RichText::new(self.tool.hint()).weak());
                }
            }
        });
    }

    fn select_options(&mut self, ui: &mut egui::Ui) {
        let sel = self.selected_zones();
        let two = sel.len() >= 2;
        let one = !sel.is_empty();
        if ui.add_enabled(two, egui::Button::new("Union").small()).on_hover_text("Merge the selected zones into one").clicked() {
            self.union_selected();
        }
        if ui.add_enabled(two, egui::Button::new("Subtract").small()).on_hover_text("Cut the other selected zones out of the first one picked").clicked() {
            self.subtract_selected();
        }
        if ui.add_enabled(two, egui::Button::new("Intersect").small()).on_hover_text("Keep only where the selected zones overlap").clicked() {
            self.intersect_selected();
        }
        if ui.add_enabled(one, egui::Button::new("Split with line").small()).on_hover_text("Draw a line across the zone to cut it in two").clicked() {
            self.line_use = LineUse::Split;
            self.tool = Tool::Line;
            self.stroke.clear();
        }
        if ui.add_enabled(one, egui::Button::new("Smooth").small()).on_hover_text("Round the corners of the selected zones").clicked() {
            self.smooth_selected();
        }
        if ui.add_enabled(one, egui::Button::new("Simplify").small()).on_hover_text("Remove points that do not change the shape").clicked() {
            self.simplify_selected();
        }
        ui.separator();
        ui.label("Grow or shrink");
        ui.add(egui::DragValue::new(&mut self.grow_px).range(1.0..=400.0).suffix(" px"));
        if ui.add_enabled(one, egui::Button::new("Grow").small()).clicked() {
            self.offset_selected(self.grow_px);
        }
        if ui.add_enabled(one, egui::Button::new("Shrink").small()).clicked() {
            self.offset_selected(-self.grow_px);
        }
    }

    pub(super) fn species_strip(&mut self, ui: &mut egui::Ui) {
        let list: Vec<usize> = (0..self.project.species.len()).filter(|&i| !forest_core::mushrooms::is_mushroom(&self.project.species[i].preset) && self.project.species[i].tree_type().is_some()).collect();
        let mut edit: Option<usize> = None;
        let mut dup: Option<usize> = None;
        let mut slot: Option<(usize, u8)> = None;
        let mut add: Option<&'static str> = None;
        egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Species").weak());
                ui.menu_button("+ Add", |ui| {
                    ui.set_min_width(190.0);
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for (key, label) in forest_core::treegen_sprite::TREE_TYPES.iter().filter(|t| !forest_core::mushrooms::is_mushroom(t.0)) {
                            if ui.button(*label).clicked() {
                                add = Some(*key);
                                ui.close();
                            }
                        }
                    });
                });
                for (n, &i) in list.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(56.0, 44.0), Sense::click());
                    let sel = self.brush.species as usize == i;
                    ui.painter().rect_filled(rect, 5.0, if sel { Color32::from_rgb(34, 92, 140) } else { Color32::from_gray(38) });
                    let c = |v: [f32; 3]| Color32::from_rgb((v[0] * 255.0) as u8, (v[1] * 255.0) as u8, (v[2] * 255.0) as u8);
                    match self.species_texture(ui.ctx(), i) {
                        Some(tex) => {
                            let img = Rect::from_center_size(rect.center() + Vec2::new(0.0, -6.0), Vec2::splat(30.0));
                            ui.painter().image(tex.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        }
                        None => {
                            let sp = &self.project.species[i];
                            ui.painter().circle_filled(rect.center() + Vec2::new(0.0, -6.0), 11.0, c(sp.ramp[1]));
                            ui.painter().circle_filled(rect.center() + Vec2::new(-3.0, -9.0), 4.5, c(sp.ramp[3]));
                        }
                    }
                    let sp = &self.project.species[i];
                    let g = &self.project.gen;
                    // marks the species that fill the main, mix and shrub roles
                    let roles: String = [(g.main_species as usize, 'M'), (g.mix_species as usize, 'X'), (g.shrub_species as usize, 'S')].iter().filter(|r| r.0 == i).map(|r| r.1).collect();
                    let name = sp.name.chars().take(9).collect::<String>();
                    ui.painter().text(rect.center() + Vec2::new(0.0, 15.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(9.5), Color32::from_gray(210));
                    if n < 9 {
                        ui.painter().text(rect.left_top() + Vec2::new(4.0, 2.0), egui::Align2::LEFT_TOP, format!("{}", n + 1), egui::FontId::proportional(9.0), Color32::from_gray(150));
                    }
                    if !roles.is_empty() {
                        ui.painter().text(rect.right_top() + Vec2::new(-4.0, 2.0), egui::Align2::RIGHT_TOP, roles, egui::FontId::proportional(9.0), Color32::from_rgb(150, 200, 255));
                    }
                    let tip = format!("{}  (key {})\nDouble-click or right-click to edit", sp.name, n + 1);
                    let resp = resp.on_hover_text(tip);
                    if resp.double_clicked() {
                        edit = Some(i);
                    } else if resp.clicked() {
                        self.brush.species = i as u8;
                        if !self.tool.is_brush() {
                            self.set_tool(Tool::Brush);
                        }
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Edit").clicked() {
                            edit = Some(i);
                            ui.close();
                        }
                        if ui.button("Duplicate").clicked() {
                            dup = Some(i);
                            ui.close();
                        }
                        ui.separator();
                        for (label, role) in [("Use as main species", 0u8), ("Use as mix species", 1), ("Use as shrub species", 2)] {
                            if ui.button(label).clicked() {
                                slot = Some((i, role));
                                ui.close();
                            }
                        }
                    });
                }
            });
        });
        if let Some(key) = add {
            if self.project.species.len() < 255 {
                self.project.species.push(forest_core::Species::for_type(key));
                self.brush.species = (self.project.species.len() - 1) as u8;
                edit = Some(self.project.species.len() - 1);
            }
        }
        if let Some(i) = dup {
            if self.project.species.len() < 255 {
                let mut c = self.project.species[i].clone();
                c.name += " copy";
                self.project.species.push(c);
                edit = Some(self.project.species.len() - 1);
            }
        }
        if let Some((i, role)) = slot {
            let g = &mut self.project.gen;
            match role {
                0 => g.main_species = i as u8,
                1 => g.mix_species = i as u8,
                _ => g.shrub_species = i as u8,
            }
        }
        if edit.is_some() {
            self.species_edit = edit;
        }
        self.species_window(ui.ctx());
    }

    /// The window for changing one species: name, tree type, height, colours and canopy fill.
    fn species_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.species_edit else { return };
        if i >= self.project.species.len() {
            self.species_edit = None;
            return;
        }
        let mut open = true;
        let tex = self.species_texture(ctx, i);
        egui::Window::new("Species").open(&mut open).resizable(false).default_width(300.0).show(ctx, |ui| {
            if let Some(t) = tex {
                ui.vertical_centered(|ui| ui.image((t.id(), Vec2::splat(96.0))));
            }
            let sp = &mut self.project.species[i];
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut sp.name);
            });
            let text = if sp.preset.is_empty() { "None" } else { forest_core::treegen_sprite::tree_type_label(&sp.preset) };
            egui::ComboBox::from_label("Tree type").selected_text(text).show_ui(ui, |ui| {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (key, label) in forest_core::treegen_sprite::TREE_TYPES.iter().filter(|t| !forest_core::mushrooms::is_mushroom(t.0)) {
                        if ui.selectable_value(&mut sp.preset, key.to_string(), *label).changed() {
                            sp.procedural = true;
                        }
                    }
                });
            });
            ui.small("The first render of a new type takes a few seconds while its trees are grown.");
            ui.add(egui::Slider::new(&mut sp.height, 0.2..=3.0).text("Height (shadow length)"));
            ui.add(egui::Slider::new(&mut sp.canopy_fill, -1.0..=1.0).text("Canopy fill (below 0 uses the type default)"));
            ui.horizontal(|ui| {
                ui.label("Bark");
                ui.color_edit_button_rgb(&mut sp.bark);
            });
            ui.horizontal(|ui| {
                ui.label("Shade to light");
                for c in sp.ramp.iter_mut() {
                    ui.color_edit_button_rgb(c);
                }
            });
            if ui.button("Reset colours and height to the type default").clicked() {
                let d = forest_core::Species::for_type(&sp.preset);
                sp.ramp = d.ramp;
                sp.bark = d.bark;
                sp.height = d.height;
            }
        });
        if !open {
            self.species_edit = None;
        }
    }

    /// A small picture of one tree of species `i`, shaded from above-left with the species colours.
    fn species_texture(&mut self, ctx: &egui::Context, i: usize) -> Option<egui::TextureHandle> {
        let sp = &self.project.species[i];
        let key = format!("{}|{}|{:?}", sp.name, sp.preset, sp.ramp);
        if let Some(t) = self.species_tex.get(&key) {
            return Some(t.clone());
        }
        let sprite = forest_core::treegen_sprite::sprite_for_radius(&sp.preset, 1, 16.0, -1.0)?;
        let n = sprite.size;
        // crop to the part that is drawn so thin logs and small stumps still fill the picture
        let (mut x0, mut y0, mut x1, mut y1) = (n, n, 0, 0);
        for y in 0..n {
            for x in 0..n {
                if sprite.cls[y * n + x] != 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if x1 < x0 {
            return None;
        }
        let side = (x1 - x0).max(y1 - y0) + 3;
        let (cx, cy) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
        let mut px = vec![0u8; side * side * 4];
        for oy in 0..side {
            for ox in 0..side {
                let x = (cx + ox as f32 - side as f32 / 2.0 + 0.5).floor() as isize;
                let y = (cy + oy as f32 - side as f32 / 2.0 + 0.5).floor() as isize;
                if x < 0 || y < 0 || x >= n as isize || y >= n as isize {
                    continue;
                }
                let k = y as usize * n + x as usize;
                if sprite.cls[k] == 0 {
                    continue;
                }
                let light = (0.5 - 0.5 * (sprite.nx[k] + sprite.ny[k]) * 1.2).clamp(0.0, 1.0);
                let t = (sprite.tone[k] * 0.55 + light * 0.45 + 0.1 * sprite.depth[k]).clamp(0.0, 1.0) * 3.0;
                let kk = (t.floor() as usize).min(2);
                let f = t - kk as f32;
                let o = (oy * side + ox) * 4;
                for ch in 0..3 {
                    px[o + ch] = ((sp.ramp[kk][ch] * (1.0 - f) + sp.ramp[kk + 1][ch] * f).clamp(0.0, 1.0) * 255.0) as u8;
                }
                px[o + 3] = 255;
            }
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([side, side], &px);
        let tex = ctx.load_texture("species", img, egui::TextureOptions::LINEAR);
        self.species_tex.insert(key, tex.clone());
        Some(tex)
    }

    /// Keys 1 to 9 pick a species while a tree tool is active.
    pub(super) fn species_key(&mut self, n: usize) {
        let list: Vec<usize> = (0..self.project.species.len()).filter(|&i| !forest_core::mushrooms::is_mushroom(&self.project.species[i].preset) && self.project.species[i].tree_type().is_some()).take(9).collect();
        if let Some(&i) = list.get(n) {
            self.brush.species = i as u8;
        }
    }

    // ------------------------------------------------------------------ zones from shapes

    /// Add a drawn outline in the current mode.
    pub(super) fn commit_ring(&mut self, ring: Ring, mode: Mode, mask: bool) {
        self.commit_regions(vec![vec![ring]], mode, mask);
    }

    /// Add shapes (an outline with holes) in a mode. Add makes forest, with the holes as clearings; Subtract makes
    /// clearings; Intersect keeps only the overlap of the zones with the shapes.
    pub(super) fn commit_regions(&mut self, regions: Vec<Region>, mode: Mode, mask: bool) {
        let regions: Vec<Region> = regions.into_iter().filter(|r| r.first().is_some_and(|o| o.len() >= 3)).collect();
        if regions.is_empty() {
            return;
        }
        self.push_undo();
        let group = self.placement_group();
        match mode {
            Mode::Add if !mask => {
                for r in regions {
                    self.project.add_zone(ShapeOp::Add, r[0].clone(), group);
                    for hole in r.iter().skip(1) {
                        self.project.add_zone(ShapeOp::Subtract, hole.clone(), group);
                    }
                }
            }
            Mode::Add | Mode::Subtract => {
                for r in regions {
                    let i = self.project.add_zone(ShapeOp::Subtract, r[0].clone(), if mask { None } else { group });
                    if mask {
                        self.project.shapes[i].mask = true;
                        self.project.shapes[i].name = "No trees".into();
                    }
                }
            }
            Mode::Intersect => {
                let rings: Vec<Ring> = regions.into_iter().flatten().collect();
                self.intersect_with(&rings);
            }
        }
        self.zone_sel = Some(self.project.shapes.len().saturating_sub(1)).filter(|_| mode != Mode::Intersect);
    }

    /// Keep only the part of the forest zones that lies inside `region`.
    fn intersect_with(&mut self, region: &[Ring]) {
        let group = self.placement_group();
        let targets: Vec<usize> = (0..self.project.shapes.len())
            .filter(|&i| {
                let z = &self.project.shapes[i];
                z.op == ShapeOp::Add && z.visible && !self.zone_locked(i) && (group.is_none() || self.project.zone_group(z) == group)
            })
            .rev()
            .collect();
        for i in targets {
            let shapes = geom::boolean(&[rings_of(&self.project.shapes[i])], region, BoolOp::Intersect);
            self.replace_zone(i, shapes);
        }
    }

    /// Put shapes in the place of zone `i`: the first outline takes its place, further outlines become copies of
    /// it, and holes become clearings in its group. No shapes removes the zone.
    fn replace_zone(&mut self, i: usize, shapes: Vec<Region>) {
        let proto = self.project.shapes[i].clone();
        let mut shapes = shapes.into_iter().filter(|s| s.first().is_some_and(|o| o.len() >= 3));
        let Some(first) = shapes.next() else {
            self.project.shapes.remove(i);
            return;
        };
        self.project.shapes[i].points = first[0].clone();
        let mut extra: Vec<forest_core::Shape> = Vec::new();
        for hole in first.iter().skip(1) {
            extra.push(forest_core::Shape { op: ShapeOp::Subtract, points: hole.clone(), name: String::new(), ..proto.clone() });
        }
        for s in shapes {
            extra.push(forest_core::Shape { points: s[0].clone(), ..proto.clone() });
            for hole in s.iter().skip(1) {
                extra.push(forest_core::Shape { op: ShapeOp::Subtract, points: hole.clone(), name: String::new(), ..proto.clone() });
            }
        }
        self.project.shapes.extend(extra);
    }

    // ------------------------------------------------------------------ selection

    pub(super) fn selected_zones(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.zone_sel.into_iter().chain(self.zone_multi.iter().copied()).filter(|&i| i < self.project.shapes.len()).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    pub(super) fn zone_locked(&self, i: usize) -> bool {
        let Some(z) = self.project.shapes.get(i) else { return false };
        if z.locked {
            return true;
        }
        let mut g = self.project.zone_group(z);
        while let Some(id) = g {
            let Some(n) = self.project.group(id) else { break };
            if n.locked {
                return true;
            }
            g = n.parent;
        }
        false
    }

    pub(super) fn selftest_op(&mut self, op: &str) {
        match op {
            "union" => self.union_selected(),
            "subtract" => self.subtract_selected(),
            "intersect" => self.intersect_selected(),
            "smooth" => self.smooth_selected(),
            "simplify" => self.simplify_selected(),
            "grow" => self.offset_selected(self.grow_px),
            "shrink" => self.offset_selected(-self.grow_px),
            _ => {}
        }
    }

    fn union_selected(&mut self) {
        let sel: Vec<usize> = self.selected_zones().into_iter().filter(|&i| !self.zone_locked(i)).collect();
        if sel.len() < 2 {
            return;
        }
        self.push_undo();
        let rings: Vec<Ring> = sel.iter().map(|&i| rings_of(&self.project.shapes[i])).collect();
        let result = geom::boolean_solid(&rings, &[], BoolOp::Union);
        let keep = *sel.last().unwrap_or(&0);
        for &i in sel.iter().rev() {
            if i != keep {
                self.project.shapes.remove(i);
            }
        }
        let keep = sel.iter().filter(|&&i| i < keep).count().min(keep) + (keep - sel.iter().filter(|&&i| i < keep).count());
        let at = keep.saturating_sub(sel.len() - 1).min(self.project.shapes.len().saturating_sub(1));
        self.replace_zone(at, result);
        self.zone_multi.clear();
        self.zone_sel = Some(at.min(self.project.shapes.len().saturating_sub(1)));
    }

    fn subtract_selected(&mut self) {
        let Some(primary) = self.zone_sel.filter(|&i| i < self.project.shapes.len()) else { return };
        let others: Vec<usize> = self.selected_zones().into_iter().filter(|&i| i != primary).collect();
        if others.is_empty() || self.zone_locked(primary) {
            return;
        }
        self.push_undo();
        let cutters: Vec<Ring> = others.iter().map(|&i| rings_of(&self.project.shapes[i])).collect();
        let result = geom::boolean_solid(&[rings_of(&self.project.shapes[primary])], &cutters, BoolOp::Subtract);
        self.replace_zone(primary, result);
        // the cutters go; indices above the primary shift down
        let mut gone: Vec<usize> = others.into_iter().filter(|&i| i != primary).collect();
        gone.sort_unstable_by(|a, b| b.cmp(a));
        for i in gone {
            if i < self.project.shapes.len() {
                self.project.shapes.remove(i);
            }
        }
        self.zone_multi.clear();
        self.zone_sel = None;
    }

    fn intersect_selected(&mut self) {
        let sel: Vec<usize> = self.selected_zones().into_iter().filter(|&i| !self.zone_locked(i)).collect();
        if sel.len() < 2 {
            return;
        }
        self.push_undo();
        let mut acc: Vec<Ring> = vec![rings_of(&self.project.shapes[sel[0]])];
        for &i in &sel[1..] {
            let r = geom::boolean_solid(&acc, &[rings_of(&self.project.shapes[i])], BoolOp::Intersect);
            acc = r.into_iter().flatten().collect();
        }
        let result = geom::boolean_solid(&acc, &[], BoolOp::Union);
        let first = sel[0];
        for &i in sel.iter().skip(1).rev() {
            self.project.shapes.remove(i);
        }
        self.replace_zone(first, result);
        self.zone_multi.clear();
        self.zone_sel = None;
    }

    fn smooth_selected(&mut self) {
        let sel: Vec<usize> = self.selected_zones().into_iter().filter(|&i| !self.zone_locked(i)).collect();
        if sel.is_empty() {
            return;
        }
        self.push_undo();
        for i in sel {
            self.project.shapes[i].points = geom::smooth_ring(&self.project.shapes[i].points, 1);
        }
    }

    fn simplify_selected(&mut self) {
        let sel: Vec<usize> = self.selected_zones().into_iter().filter(|&i| !self.zone_locked(i)).collect();
        if sel.is_empty() {
            return;
        }
        self.push_undo();
        for i in sel {
            let s = geom::simplify_ring(&self.project.shapes[i].points, 2.0);
            if s.len() >= 3 {
                self.project.shapes[i].points = s;
            }
        }
    }

    /// Move the outline of the selected zones out (positive) or in (negative).
    fn offset_selected(&mut self, d: f32) {
        let sel: Vec<usize> = self.selected_zones().into_iter().filter(|&i| !self.zone_locked(i)).collect();
        if sel.is_empty() || d == 0.0 {
            return;
        }
        self.push_undo();
        for &i in sel.iter().rev() {
            let ring = rings_of(&self.project.shapes[i]);
            let mut closed = ring.clone();
            closed.push(ring[0]);
            let band = geom::buffer_path(&closed, d.abs() * 2.0);
            let band_rings: Vec<Ring> = band.into_iter().flatten().collect();
            let result = geom::boolean(&[ring], &band_rings, if d > 0.0 { BoolOp::Union } else { BoolOp::Subtract });
            // a grown zone keeps only its outer outline; the hollow inside the band is not a hole
            let result: Vec<Region> = if d > 0.0 { result.into_iter().map(|mut s| { s.truncate(1); s }).collect() } else { result };
            self.replace_zone(i, result);
        }
    }

    // ------------------------------------------------------------------ snapping

    /// The nearest zone corner, or point on a zone edge, to a map point, within a few screen pixels.
    pub(super) fn snap_point(&self, m: [f32; 2], skip: Option<usize>) -> Option<[f32; 2]> {
        let reach = 10.0 / self.zoom.max(1e-3);
        let edge_reach = 7.0 / self.zoom.max(1e-3);
        let mut best: Option<(f32, [f32; 2])> = None;
        for (i, z) in self.project.shapes.iter().enumerate() {
            if Some(i) == skip || !z.visible {
                continue;
            }
            for p in &z.points {
                let d = ((p[0] - m[0]).powi(2) + (p[1] - m[1]).powi(2)).sqrt();
                if d <= reach && best.map_or(true, |(b, _)| d < b) {
                    best = Some((d, *p));
                }
            }
        }
        if best.is_some() {
            return best.map(|b| b.1);
        }
        for (i, z) in self.project.shapes.iter().enumerate() {
            if Some(i) == skip || !z.visible || z.points.len() < 2 {
                continue;
            }
            let n = z.points.len();
            for k in 0..n {
                let (a, b) = (z.points[k], z.points[(k + 1) % n]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l2 = (dx * dx + dy * dy).max(1e-6);
                let t = (((m[0] - a[0]) * dx + (m[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
                let q = [a[0] + dx * t, a[1] + dy * t];
                let d = ((q[0] - m[0]).powi(2) + (q[1] - m[1]).powi(2)).sqrt();
                if d <= edge_reach && best.map_or(true, |(b, _)| d < b) {
                    best = Some((d, q));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// A dragged loop becomes an outline: jitter removed, corners rounded.
    pub(super) fn finish_lasso(&mut self, raw: Vec<[f32; 2]>, ctx: &egui::Context) {
        if raw.len() < 3 {
            return;
        }
        let eps = (0.8 + self.draw_smooth * 3.0) / self.zoom.max(1e-3);
        let simple = geom::simplify_ring(&raw, eps);
        let pts = geom::smooth_ring(if simple.len() >= 3 { &simple } else { &raw }, 1 + (self.draw_smooth * 2.0).round() as usize);
        let mask = self.tool == Tool::Mask;
        let mode = if mask { Mode::Subtract } else { self.mode_now(ctx) };
        self.commit_ring(pts, mode, mask);
    }

    pub(super) fn finish_polygon(&mut self, pts: Vec<[f32; 2]>, ctx: &egui::Context) {
        if pts.len() < 3 {
            return;
        }
        let pts = if self.draw_smooth > 0.55 { geom::smooth_ring(&pts, ((self.draw_smooth - 0.5) * 4.0).round() as usize) } else { pts };
        let mode = self.mode_now(ctx);
        self.commit_ring(pts, mode, false);
    }

    // ------------------------------------------------------------------ the wand

    /// The region of the map picture like the one under a map point, as shapes in map pixels.
    pub(super) fn wand_regions(&self, m: [f32; 2]) -> Vec<Region> {
        let Some(bg) = &self.bg_pixels else { return Vec::new() };
        let (mw, mh) = (self.project.map_w as f32, self.project.map_h as f32);
        let (px, py) = ((m[0] / mw * bg.w as f32) as i64, (m[1] / mh * bg.h as f32) as i64);
        if px < 0 || py < 0 || px as usize >= bg.w || py as usize >= bg.h {
            return Vec::new();
        }
        let shapes = geom::wand(&bg.rgba, bg.w, bg.h, (px as usize, py as usize), self.wand_tol, self.wand_contiguous, self.draw_smooth);
        let (sx, sy) = (mw / bg.w as f32, mh / bg.h as f32);
        shapes.into_iter().map(|s| s.into_iter().map(|r| r.into_iter().map(|p| [p[0] * sx, p[1] * sy]).collect()).collect()).collect()
    }

    pub(super) fn wand_click(&mut self, m: [f32; 2], mode: Mode) {
        if self.bg_pixels.is_none() {
            self.status = "The wand reads the map image: import one in the Map tab first".into();
            return;
        }
        let regions = self.wand_regions(m);
        if regions.is_empty() {
            self.status = "Nothing to trace there".into();
            return;
        }
        self.commit_regions(regions, mode, false);
    }

    // ------------------------------------------------------------------ the line

    pub(super) fn finish_line(&mut self) {
        let path = std::mem::take(&mut self.stroke);
        if path.len() < 2 {
            return;
        }
        match self.line_use {
            LineUse::Cut => {
                let band = geom::buffer_path(&path, self.line_width);
                self.commit_regions(band, Mode::Subtract, false);
                if let Some(z) = self.project.shapes.last_mut() {
                    z.name = "Path".into();
                }
            }
            LineUse::Trees => self.trees_along(&path),
            LineUse::Split => {
                let sel: Vec<usize> = self.selected_zones().into_iter().filter(|&i| !self.zone_locked(i)).collect();
                if sel.is_empty() {
                    return;
                }
                self.push_undo();
                let cut = geom::buffer_path(&path, 2.0);
                let cut_rings: Vec<Ring> = cut.into_iter().flatten().collect();
                for &i in sel.iter().rev() {
                    let result = geom::boolean_solid(&[rings_of(&self.project.shapes[i])], &cut_rings, BoolOp::Subtract);
                    self.replace_zone(i, result);
                }
                self.zone_multi.clear();
                self.line_use = LineUse::Cut;
                self.tool = Tool::Select;
            }
        }
    }

    /// A row of single trees along a path, with a little jitter.
    fn trees_along(&mut self, path: &[[f32; 2]]) {
        self.push_undo();
        let mut rng = forest_core::rng::Rng::new(self.paint_rev.wrapping_mul(7919).wrapping_add(path.len() as u64));
        let spacing = self.line_spacing.max(2.0);
        let mut carry = 0.0f32;
        let mut b = self.brush.clone();
        b.mode = BrushMode::Single;
        for w in path.windows(2) {
            let (a, c) = (w[0], w[1]);
            let (dx, dy) = (c[0] - a[0], c[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-3 {
                continue;
            }
            let mut t = carry;
            while t <= len {
                let (x, y) = (a[0] + dx / len * t + rng.normal(0.0, spacing * 0.12), a[1] + dy / len * t + rng.normal(0.0, spacing * 0.12));
                stamp_one(&mut self.project, &b, x, y, &mut rng);
                t += spacing * rng.range(0.85, 1.15);
            }
            carry = t - len;
        }
        self.paint_rev += 1;
    }

    // ------------------------------------------------------------------ reference map overlays

    /// The scale bar, the grid and the calibration points over the map.
    pub(super) fn draw_reference_overlays(&self, painter: &egui::Painter, origin: Pos2, map_rect: Rect) {
        let Some(b) = &self.project.background_image else { return };
        if b.px_per_m > 0.0 && b.grid {
            let step = 100.0 * b.px_per_m * self.zoom;
            if step > 12.0 {
                let mut x = map_rect.min.x;
                while x < map_rect.max.x {
                    painter.line_segment([Pos2::new(x, map_rect.min.y), Pos2::new(x, map_rect.max.y)], Stroke::new(0.6, Color32::from_white_alpha(46)));
                    x += step;
                }
                let mut y = map_rect.min.y;
                while y < map_rect.max.y {
                    painter.line_segment([Pos2::new(map_rect.min.x, y), Pos2::new(map_rect.max.x, y)], Stroke::new(0.6, Color32::from_white_alpha(46)));
                    y += step;
                }
            }
        }
        if b.px_per_m > 0.0 && b.scale_bar {
            let len = 100.0 * b.px_per_m * self.zoom;
            let a = map_rect.left_bottom() + Vec2::new(14.0, -18.0);
            for (p, q) in [(a, a + Vec2::new(len, 0.0)), (a + Vec2::new(0.0, -4.0), a + Vec2::new(0.0, 4.0)), (a + Vec2::new(len, -4.0), a + Vec2::new(len, 4.0))] {
                painter.line_segment([p, q], Stroke::new(3.0, Color32::BLACK));
                painter.line_segment([p, q], Stroke::new(1.5, Color32::WHITE));
            }
            painter.text(a + Vec2::new(len + 8.0, 0.0), egui::Align2::LEFT_CENTER, "100 m", egui::FontId::proportional(11.0), Color32::WHITE);
        }
        for p in &self.calib {
            let s = self.map_to_screen(origin, *p);
            painter.circle_filled(s, 4.0, Color32::from_rgb(255, 120, 220));
            painter.circle_stroke(s, 6.0, Stroke::new(1.0, Color32::WHITE));
        }
        if self.calib.len() == 2 {
            painter.line_segment([self.map_to_screen(origin, self.calib[0]), self.map_to_screen(origin, self.calib[1])], Stroke::new(1.5, Color32::from_rgb(255, 120, 220)));
        }
    }

    // ------------------------------------------------------------------ the zone list

    /// The rows in list order: groups, then the zones at each level, newest first.
    pub(super) fn flat_rows(&self) -> Vec<(RowId, usize)> {
        fn walk(app: &TreeApp, parent: Option<u32>, depth: usize, out: &mut Vec<(RowId, usize)>) {
            let ids: Vec<u32> = app.project.groups.iter().filter(|n| n.parent == parent).map(|n| n.id).rev().collect();
            for id in ids {
                out.push((RowId::Group(id), depth));
                if app.project.group(id).is_some_and(|n| !n.collapsed) {
                    walk(app, Some(id), depth + 1, out);
                }
            }
            for i in (0..app.project.shapes.len()).rev() {
                if app.project.zone_group(&app.project.shapes[i]) == parent {
                    out.push((RowId::Zone(i), depth));
                }
            }
        }
        let mut out = Vec::new();
        walk(self, None, 0, &mut out);
        out
    }

    fn row_visible(&self, r: RowId) -> bool {
        match r {
            RowId::Zone(i) => self.project.shapes.get(i).is_some_and(|z| z.visible),
            RowId::Group(g) => self.project.group(g).is_some_and(|n| n.visible),
        }
    }

    fn row_locked(&self, r: RowId) -> bool {
        match r {
            RowId::Zone(i) => self.project.shapes.get(i).is_some_and(|z| z.locked),
            RowId::Group(g) => self.project.group(g).is_some_and(|n| n.locked),
        }
    }

    pub(super) fn zones_panel(&mut self, ui: &mut egui::Ui) {
        if self.zone_sel.is_some_and(|i| i >= self.project.shapes.len()) {
            self.zone_sel = None;
        }
        self.zone_multi.retain(|&i| i < self.project.shapes.len());
        if self.group_sel.is_some_and(|id| self.project.group(id).is_none()) {
            self.group_sel = None;
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Zones").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+ Group").on_hover_text("A new empty group").clicked() {
                    let place = self.placement_group();
                    self.push_undo();
                    self.group_sel = Some(self.project.add_group(place));
                    self.zone_sel = None;
                    self.prev_zone_sel = None;
                }
                if ui.small_button("Merge").on_hover_text("Merge the selected zones into one").clicked() {
                    self.union_selected();
                }
            });
        });
        ui.separator();
        if self.project.shapes.is_empty() && self.project.groups.is_empty() {
            ui.label(egui::RichText::new("Draw a forest with the lasso or polygon, or trace the map with the wand. Each shape becomes a zone. Put zones in a group to give them settings of their own.").small().weak());
        }
        egui::ScrollArea::vertical().id_salt("zone_rows").max_height((ui.available_height() - 190.0).max(120.0)).auto_shrink([false, true]).show(ui, |ui| {
            let rows = self.flat_rows();
            for (id, depth) in rows {
                self.zone_row(ui, id, depth);
            }
        });
        if let Some(pick) = self.row_drop.take() {
            self.apply_row_drop(pick.0, pick.1);
        }
        ui.add_space(4.0);
        ui.separator();
        self.zone_footer(ui);
    }

    fn zone_row(&mut self, ui: &mut egui::Ui, id: RowId, depth: usize) {
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click_and_drag());
        let selected = match id {
            RowId::Zone(i) => self.selected_zones().contains(&i),
            RowId::Group(g) => self.group_sel == Some(g),
        };
        if selected {
            ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(34, 70, 104));
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, 3.0, Color32::from_gray(44));
        }
        let drop_here = self.row_drag.is_some() && resp.hovered() && self.row_drag != Some(id);
        if drop_here {
            ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.5, Color32::from_rgb(120, 200, 255)), egui::StrokeKind::Inside);
        }
        let x0 = rect.min.x + 4.0 + depth as f32 * 14.0;
        let cy = rect.center().y;
        for k in 0..3 {
            ui.painter().circle_filled(Pos2::new(rect.min.x + 5.0, cy - 4.0 + k as f32 * 4.0), 0.9, Color32::from_gray(110));
        }
        let visible = self.row_visible(id);
        let locked = self.row_locked(id);
        let name_col = if visible { Color32::from_gray(225) } else { Color32::from_gray(110) };
        let label: String;
        let mut count = String::new();
        match id {
            RowId::Group(g) => {
                let n = self.project.group(g).cloned().unwrap_or_default();
                let tri = if n.collapsed { vec![Pos2::new(x0 + 6.0, cy - 4.0), Pos2::new(x0 + 6.0, cy + 4.0), Pos2::new(x0 + 11.0, cy)] } else { vec![Pos2::new(x0 + 5.0, cy - 2.0), Pos2::new(x0 + 12.0, cy - 2.0), Pos2::new(x0 + 8.5, cy + 3.0)] };
                ui.painter().add(egui::Shape::convex_polygon(tri, Color32::from_gray(180), Stroke::NONE));
                label = if n.name.is_empty() { "Group".to_string() } else { n.name.clone() };
                count = format!("{}", self.project.zones_under(g));
            }
            RowId::Zone(i) => {
                let z = &self.project.shapes[i];
                let chip = Rect::from_min_size(Pos2::new(x0 + 4.0, cy - 7.0), Vec2::new(16.0, 14.0));
                let col = if z.mask { Color32::from_rgb(220, 90, 90) } else if z.op == ShapeOp::Add { Color32::from_rgb(120, 190, 110) } else { Color32::from_rgb(225, 150, 90) };
                if z.mask {
                    ui.painter().rect_stroke(chip, 2.0, Stroke::new(1.0, col), egui::StrokeKind::Inside);
                    for k in 0..4 {
                        ui.painter().line_segment([Pos2::new(chip.min.x + k as f32 * 4.0, chip.max.y), Pos2::new(chip.min.x + 4.0 + k as f32 * 4.0, chip.min.y)], Stroke::new(1.0, col));
                    }
                } else {
                    ui.painter().rect_filled(chip, 2.0, col.gamma_multiply(if visible { 1.0 } else { 0.3 }));
                }
                label = self.zone_name(i);
            }
        }
        let tx = x0 + 26.0;
        if self.renaming == Some(id) {
            let edit = Rect::from_min_size(Pos2::new(tx - 2.0, rect.min.y + 2.0), Vec2::new((rect.max.x - tx - 90.0).max(40.0), 22.0));
            let mut ui2 = ui.new_child(egui::UiBuilder::new().max_rect(edit));
            let mut text = label.clone();
            let te = ui2.add(egui::TextEdit::singleline(&mut text));
            te.request_focus();
            match id {
                RowId::Zone(i) => self.project.shapes[i].name = text,
                RowId::Group(g) => {
                    if let Some(n) = self.project.group_mut(g) {
                        n.name = text;
                    }
                }
            }
            if te.lost_focus() {
                self.renaming = None;
            }
        } else {
            ui.painter().text(Pos2::new(tx, cy), egui::Align2::LEFT_CENTER, &label, egui::FontId::proportional(13.0), name_col);
            if !count.is_empty() {
                ui.painter().text(Pos2::new(rect.max.x - 96.0, cy), egui::Align2::RIGHT_CENTER, count, egui::FontId::proportional(10.5), Color32::from_gray(120));
            }
        }
        // eye, lock and solo
        let bx = |k: f32| Rect::from_center_size(Pos2::new(rect.max.x - 68.0 + k * 22.0, cy), Vec2::splat(20.0));
        let (eye_r, lock_r, solo_r) = (bx(0.0), bx(1.0), bx(2.0));
        let mut toggled = false;
        if ui.interact(eye_r, ui.id().with(("eye", id_key(id))), Sense::click()).on_hover_text("Show or hide").clicked() {
            self.push_undo();
            match id {
                RowId::Zone(i) => self.project.shapes[i].visible = !visible,
                RowId::Group(g) => {
                    if let Some(n) = self.project.group_mut(g) {
                        n.visible = !visible;
                    }
                }
            }
            toggled = true;
        }
        if ui.interact(lock_r, ui.id().with(("lock", id_key(id))), Sense::click()).on_hover_text("Lock: stops accidental edits").clicked() {
            match id {
                RowId::Zone(i) => self.project.shapes[i].locked = !locked,
                RowId::Group(g) => {
                    if let Some(n) = self.project.group_mut(g) {
                        n.locked = !locked;
                    }
                }
            }
            toggled = true;
        }
        let soloed = self.solo == Some(id);
        if ui.interact(solo_r, ui.id().with(("solo", id_key(id))), Sense::click()).on_hover_text("Solo: show only this").clicked() {
            self.solo = if soloed { None } else { Some(id) };
            toggled = true;
        }
        let col = Color32::from_gray(200);
        eye_icon(ui.painter(), eye_r, visible, col);
        lock_icon(ui.painter(), lock_r, locked, col);
        ui.painter().text(solo_r.center(), egui::Align2::CENTER_CENTER, "S", egui::FontId::proportional(11.0), if soloed { Color32::from_rgb(240, 200, 80) } else { Color32::from_gray(90) });

        // clicks: select, ctrl or shift adds, double click renames, groups fold
        if resp.double_clicked() {
            self.renaming = Some(id);
        } else if resp.clicked() && !toggled {
            let multi = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            self.pick_row(id, multi);
        }
        if resp.drag_started() {
            self.row_drag = Some(id);
        }
        if resp.drag_stopped() {
            if let Some(from) = self.row_drag.take() {
                if let Some(pos) = ui.ctx().pointer_latest_pos() {
                    if rect.contains(pos) && from != id {
                        self.row_drop = Some((from, id));
                    }
                }
            }
        }
        // a drop on another row: the drag ends over the row that is hovered then
        if self.row_drag.is_some() && ui.input(|i| i.pointer.any_released()) && resp.hovered() && self.row_drag != Some(id) {
            if let Some(from) = self.row_drag.take() {
                self.row_drop = Some((from, id));
            }
        }
        resp.context_menu(|ui| self.row_menu(ui, id));
    }

    fn pick_row(&mut self, id: RowId, multi: bool) {
        match id {
            RowId::Group(g) => {
                if let Some(n) = self.project.group_mut(g) {
                    if self.group_sel == Some(g) {
                        n.collapsed = !n.collapsed;
                    }
                }
                self.group_sel = Some(g);
                self.zone_sel = None;
                self.zone_multi.clear();
                self.prev_zone_sel = None;
            }
            RowId::Zone(i) => {
                self.group_sel = None;
                if multi && self.zone_sel.is_some() {
                    if self.zone_sel == Some(i) {
                        self.zone_sel = self.zone_multi.pop();
                    } else if let Some(k) = self.zone_multi.iter().position(|&z| z == i) {
                        self.zone_multi.remove(k);
                    } else {
                        self.zone_multi.push(i);
                    }
                } else {
                    self.zone_sel = Some(i);
                    self.zone_multi.clear();
                }
            }
        }
    }

    fn row_menu(&mut self, ui: &mut egui::Ui, id: RowId) {
        match id {
            RowId::Zone(i) => {
                if ui.button("Rename").clicked() {
                    self.renaming = Some(id);
                    ui.close();
                }
                if ui.button("Duplicate").clicked() {
                    self.push_undo();
                    let mut copy = self.project.shapes[i].clone();
                    for p in copy.points.iter_mut() {
                        p[0] += 30.0;
                        p[1] += 30.0;
                    }
                    copy.name = String::new();
                    self.project.shapes.push(copy);
                    self.zone_sel = Some(self.project.shapes.len() - 1);
                    ui.close();
                }
                if ui.button("Fit view to this").clicked() {
                    self.fit_to_zone(i);
                    ui.close();
                }
                if ui.button("Copy settings").on_hover_text("Copy the settings of the group this zone is in").clicked() {
                    self.copy_settings_from(self.project.zone_group(&self.project.shapes[i]));
                    ui.close();
                }
                if ui.button("Paste settings").clicked() {
                    self.paste_settings_to(self.project.zone_group(&self.project.shapes[i]));
                    ui.close();
                }
                let op = self.project.shapes[i].op;
                if ui.button(if op == ShapeOp::Add { "Make clearing" } else { "Make forest" }).clicked() {
                    self.push_undo();
                    self.project.shapes[i].op = if op == ShapeOp::Add { ShapeOp::Subtract } else { ShapeOp::Add };
                    ui.close();
                }
                if ui.button("Delete").clicked() {
                    self.push_undo();
                    self.project.shapes.remove(i);
                    self.zone_sel = None;
                    self.zone_multi.clear();
                    ui.close();
                }
            }
            RowId::Group(g) => {
                if ui.button("Rename").clicked() {
                    self.renaming = Some(id);
                    ui.close();
                }
                if ui.button("Group it").clicked() {
                    self.push_undo();
                    self.group_sel = self.project.wrap_group(g);
                    ui.close();
                }
                if ui.button("Copy settings").clicked() {
                    self.copy_settings_from(Some(g));
                    ui.close();
                }
                if ui.button("Paste settings").clicked() {
                    self.paste_settings_to(Some(g));
                    ui.close();
                }
                if ui.button("Ungroup").clicked() {
                    self.push_undo();
                    self.project.ungroup(g);
                    self.group_sel = None;
                    ui.close();
                }
                if ui.button("Delete group and its zones").clicked() {
                    self.push_undo();
                    self.project.delete_group(g);
                    self.group_sel = None;
                    self.zone_sel = None;
                    ui.close();
                }
            }
        }
    }

    /// A row dropped on another: a zone onto a group goes into it; a zone onto a zone takes its place and group;
    /// a group onto a group goes into it.
    pub(super) fn selftest_drop(&mut self, from: RowId, onto: RowId) {
        self.apply_row_drop(from, onto);
    }

    fn apply_row_drop(&mut self, from: RowId, onto: RowId) {
        self.push_undo();
        match (from, onto) {
            (RowId::Zone(i), RowId::Group(g)) => self.project.shapes[i].group = Some(g),
            (RowId::Zone(i), RowId::Zone(j)) => {
                let group = self.project.zone_group(&self.project.shapes[j]);
                let mut z = self.project.shapes.remove(i);
                z.group = group;
                let at = if j > i { j } else { j }.min(self.project.shapes.len());
                self.project.shapes.insert(at, z);
                self.zone_sel = Some(at);
            }
            (RowId::Group(a), RowId::Group(b)) => {
                self.project.move_into(a, Some(b));
            }
            (RowId::Group(a), RowId::Zone(j)) => {
                let target = self.project.zone_group(&self.project.shapes[j]);
                self.project.move_into(a, target);
            }
        }
    }

    fn zone_footer(&mut self, ui: &mut egui::Ui) {
        let sel = self.selected_zones();
        ui.label(egui::RichText::new(format!("{} selected", sel.len() + self.group_sel.map_or(0, |_| 1))).weak());
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("Duplicate").small()).clicked() {
                if let Some(&i) = sel.first() {
                    self.push_undo();
                    let mut copy = self.project.shapes[i].clone();
                    for p in copy.points.iter_mut() {
                        p[0] += 30.0;
                        p[1] += 30.0;
                    }
                    copy.name = String::new();
                    self.project.shapes.push(copy);
                    self.zone_sel = Some(self.project.shapes.len() - 1);
                    self.zone_multi.clear();
                }
            }
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("Group").small()).on_hover_text("Put the selected zones in a new group").clicked() {
                self.push_undo();
                let place = self.placement_group();
                let g = self.project.add_group(place);
                for &i in &sel {
                    self.project.shapes[i].group = Some(g);
                }
                self.group_sel = Some(g);
                self.zone_sel = None;
                self.zone_multi.clear();
            }
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("Fit view").small()).clicked() {
                if let Some(&i) = sel.first() {
                    self.fit_to_zone(i);
                }
            }
            if ui.small_button("Clear painted trees").clicked() && !(self.project.painted.is_empty() && self.project.erased.is_empty()) {
                self.push_undo();
                self.project.painted.clear();
                self.project.erased.clear();
                self.paint_rev += 1;
            }
        });
        if let Some(id) = self.group_sel {
            self.group_actions(ui, id);
        } else if let Some(i) = self.zone_sel {
            self.zone_actions(ui, i);
        }
    }

    pub(super) fn fit_to_zone(&mut self, i: usize) {
        let Some(z) = self.project.shapes.get(i) else { return };
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &z.points {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let Some(rect) = self.canvas_rect else { return };
        let (w, h) = ((x1 - x0).max(10.0), (y1 - y0).max(10.0));
        self.zoom = ((rect.width() - 80.0) / w).min((rect.height() - 80.0) / h).clamp(0.02, 8.0);
        let c = Vec2::new((x0 + x1) * 0.5, (y0 + y1) * 0.5);
        self.pan = rect.size() * 0.5 - c * self.zoom;
    }

    fn copy_settings_from(&mut self, group: Option<u32>) {
        let (gen, species) = match group {
            Some(g) => self.project.effective(g),
            None => (self.project.gen.clone(), self.project.species.clone()),
        };
        self.settings_clip = Some((gen, species));
        self.status = "Settings copied".into();
    }

    fn paste_settings_to(&mut self, group: Option<u32>) {
        let Some((gen, species)) = self.settings_clip.clone() else {
            self.status = "Copy settings from a zone or group first".into();
            return;
        };
        self.push_undo();
        match group {
            Some(g) => {
                let (before_gen, before_species) = self.project.effective(g);
                self.project.record_edit(g, (&before_gen, &before_species), (&gen, &species), &[]);
            }
            None => {
                self.project.gen = gen;
                self.project.species = species;
            }
        }
        self.status = "Settings pasted".into();
    }
}

fn id_key(id: RowId) -> (u8, u64) {
    match id {
        RowId::Zone(i) => (0, i as u64),
        RowId::Group(g) => (1, g as u64),
    }
}

/// A label then a slider, so options read left to right.
fn opt_slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) -> egui::Response {
    ui.label(label);
    ui.add(egui::Slider::new(v, range))
}
