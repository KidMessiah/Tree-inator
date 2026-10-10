//! Zones: every drawn area of forest or clearing as a named, editable item. The list sits in the left rail
//! and the Zones tool edits the selected zone's points on the canvas.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum ZoneDrag {
    /// Moving one corner.
    Point(usize),
    /// Moving the whole zone.
    Move,
}

const HANDLE: f32 = 6.0;

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> (f32, f32) {
    let ab = b - a;
    let len2 = ab.length_sq().max(1e-6);
    let t = (((p - a).x * ab.x + (p - a).y * ab.y) / len2).clamp(0.0, 1.0);
    ((a + ab * t - p).length(), t)
}

fn inside(poly: &[Pos2], p: Pos2) -> bool {
    let mut c = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c
}

impl TreeApp {
    /// The name shown for a zone: its own, or "Forest 2" and "Clearing 1" by order.
    pub(super) fn zone_name(&self, i: usize) -> String {
        let z = &self.project.shapes[i];
        if !z.name.trim().is_empty() {
            return z.name.clone();
        }
        let same = self.project.shapes[..=i].iter().filter(|s| s.op == z.op).count();
        match z.op {
            ShapeOp::Add => format!("Forest {same}"),
            ShapeOp::Subtract => format!("Clearing {same}"),
        }
    }

    /// Where a new zone goes: the group picked, or the group of the zone picked, else no group.
    pub(super) fn placement_group(&self) -> Option<u32> {
        self.group_sel.or_else(|| self.zone_sel.and_then(|i| self.project.shapes.get(i)).and_then(|s| s.group)).filter(|g| self.project.group(*g).is_some())
    }

    /// Which group a zone is in: none, one of the groups, or a new one.
    fn zone_group_combo(&mut self, ui: &mut egui::Ui, i: usize) {
        let cur = self.project.zone_group(&self.project.shapes[i]);
        let label = cur.map(|id| self.project.path_name(id)).unwrap_or_else(|| "None (project settings)".into());
        let mut pick: Option<Option<u32>> = None;
        let mut new_group = false;
        // the groups by name with the groups above them, in drawing order
        let groups: Vec<(u32, String)> = self.project.draw_order().into_iter().map(|id| (id, self.project.path_name(id))).collect();
        ui.horizontal(|ui| {
            ui.label("Group");
            egui::ComboBox::from_id_salt("zone_group").selected_text(label).show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_none(), "None (project settings)").clicked() {
                    pick = Some(None);
                }
                for (id, name) in &groups {
                    if ui.selectable_label(cur == Some(*id), name).clicked() {
                        pick = Some(Some(*id));
                    }
                }
                ui.separator();
                if ui.selectable_label(false, "New group").clicked() {
                    new_group = true;
                }
            });
        })
        .response
        .on_hover_text("A zone in a group is drawn with the settings of the group. Zones in the same group share them.");
        if new_group {
            self.push_undo();
            let id = self.project.add_group(cur);
            self.project.shapes[i].group = Some(id);
            self.group_sel = None;
        } else if let Some(p) = pick {
            if p != cur {
                self.push_undo();
                self.project.shapes[i].group = p;
            }
        }
    }

    /// The rows of one group level: the groups on top, then the zones, each newest first.
    fn tree_rows(&mut self, ui: &mut egui::Ui, parent: Option<u32>, depth: usize) {
        let ids: Vec<u32> = self.project.groups.iter().filter(|n| n.parent == parent).map(|n| n.id).rev().collect();
        for id in ids {
            let Some(n) = self.project.group(id).cloned() else { continue };
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 14.0);
                if ui.small_button(if n.collapsed { "+" } else { "-" }).on_hover_text("Open or close the group").clicked() {
                    if let Some(m) = self.project.group_mut(id) {
                        m.collapsed = !m.collapsed;
                    }
                }
                let mut vis = n.visible;
                if ui.checkbox(&mut vis, "").on_hover_text("Show or hide the group and everything in it").changed() {
                    if let Some(m) = self.project.group_mut(id) {
                        m.visible = vis;
                    }
                }
                let count = match self.project.zones_under(id) {
                    1 => "1 zone".to_string(),
                    k => format!("{k} zones"),
                };
                let text = format!("[{}] ({count})", if n.name.is_empty() { "Group" } else { &n.name });
                if ui.selectable_label(self.group_sel == Some(id), egui::RichText::new(text).strong()).clicked() {
                    self.group_sel = if self.group_sel == Some(id) { None } else { Some(id) };
                    if self.group_sel.is_some() {
                        self.zone_sel = None;
                        self.prev_zone_sel = None;
                    }
                }
            });
            if !n.collapsed {
                self.tree_rows(ui, Some(id), depth + 1);
            }
        }
        // the zones of this level, newest on top
        let zones: Vec<usize> = (0..self.project.shapes.len()).filter(|&i| self.project.zone_group(&self.project.shapes[i]) == parent).rev().collect();
        for i in zones {
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 14.0 + 18.0);
                let z = &self.project.shapes[i];
                let mut vis = z.visible;
                if ui.checkbox(&mut vis, "").on_hover_text("Show or hide this zone").changed() {
                    self.push_undo();
                    self.project.shapes[i].visible = vis;
                }
                let (sign, col) = match self.project.shapes[i].op {
                    ShapeOp::Add => ("+", Color32::from_rgb(120, 220, 120)),
                    ShapeOp::Subtract => ("-", Color32::from_rgb(255, 120, 110)),
                };
                ui.label(egui::RichText::new(sign).color(col).strong());
                let name = self.zone_name(i);
                if ui.selectable_label(self.zone_sel == Some(i), name).clicked() {
                    self.zone_sel = if self.zone_sel == Some(i) { None } else { Some(i) };
                    if self.zone_sel.is_some() {
                        self.group_sel = None;
                    }
                }
            });
        }
    }

    /// The zones and the groups they are in, as a tree, in the left rail. New zones go into the group picked.
    pub(super) fn zones_ui(&mut self, ui: &mut egui::Ui) {
        if self.zone_sel.is_some_and(|i| i >= self.project.shapes.len()) {
            self.zone_sel = None;
        }
        if self.group_sel.is_some_and(|id| self.project.group(id).is_none()) {
            self.group_sel = None;
        }
        ui.label(egui::RichText::new("ZONES").small().weak());
        if self.project.shapes.is_empty() && self.project.groups.is_empty() {
            ui.small("Draw a forest with the lasso or polygon. Each shape becomes a zone. Put zones in a group to give them settings of their own.");
        }
        self.tree_rows(ui, None, 0);
        let place = self.placement_group();
        ui.horizontal_wrapped(|ui| {
            let hover = match place {
                Some(g) => format!("An empty group inside {}", self.project.path_name(g)),
                None => "An empty group".to_string(),
            };
            if ui.small_button("New group").on_hover_text(hover).clicked() {
                self.push_undo();
                self.group_sel = Some(self.project.add_group(place));
                self.zone_sel = None;
                self.prev_zone_sel = None;
            }
            if let Some(g) = place {
                ui.small(format!("New zones go in {}", self.project.path_name(g)));
            }
        });
        if let Some(id) = self.group_sel {
            self.group_actions(ui, id);
        }
        let Some(i) = self.zone_sel else {
            if self.group_sel.is_none() && !self.project.shapes.is_empty() {
                ui.small("Pick a zone or a group to edit it.");
            }
            return;
        };
        ui.separator();
        let n = self.project.shapes.len();
        let mut name = self.project.shapes[i].name.clone();
        if ui.add(egui::TextEdit::singleline(&mut name).hint_text(self.zone_name(i)).desired_width(f32::INFINITY)).changed() {
            self.project.shapes[i].name = name;
        }
        ui.horizontal(|ui| {
            let op = self.project.shapes[i].op;
            let label = if op == ShapeOp::Add { "Make clearing" } else { "Make forest" };
            if ui.small_button(label).on_hover_text("Switch between adding forest and cutting it away. A clearing in a group cuts that group and what is inside it.").clicked() {
                self.push_undo();
                self.project.shapes[i].op = if op == ShapeOp::Add { ShapeOp::Subtract } else { ShapeOp::Add };
            }
        });
        self.zone_group_combo(ui, i);
        ui.horizontal(|ui| {
            // the neighbours in the same group, in the order zones are drawn
            let same = self.project.zone_group(&self.project.shapes[i]);
            let next = (i + 1..n).find(|&k| self.project.zone_group(&self.project.shapes[k]) == same);
            let prev = (0..i).rev().find(|&k| self.project.zone_group(&self.project.shapes[k]) == same);
            if ui.add_enabled(next.is_some(), egui::Button::new("Up").small()).on_hover_text("Draw later, so it wins over zones below").clicked() {
                self.push_undo();
                let k = next.unwrap();
                self.project.shapes.swap(i, k);
                self.zone_sel = Some(k);
            }
            if ui.add_enabled(prev.is_some(), egui::Button::new("Down").small()).clicked() {
                self.push_undo();
                let k = prev.unwrap();
                self.project.shapes.swap(i, k);
                self.zone_sel = Some(k);
            }
            if ui.small_button("Duplicate").clicked() {
                self.push_undo();
                let mut copy = self.project.shapes[i].clone();
                for p in copy.points.iter_mut() {
                    p[0] += 30.0;
                    p[1] += 30.0;
                }
                copy.name = String::new();
                self.project.shapes.push(copy);
                self.zone_sel = Some(self.project.shapes.len() - 1);
            }
            if ui.small_button("Delete").clicked() {
                self.push_undo();
                self.project.shapes.remove(i);
                self.zone_sel = None;
            }
        });
        if ui.small_button("Edit points").on_hover_text("Switch to the zone editing tool").clicked() {
            self.tool = Tool::Zones;
        }
        if ui.small_button("Clear all zones").clicked() {
            self.push_undo();
            self.project.shapes.clear();
            self.zone_sel = None;
        }
    }

    /// What can be done with the group that is picked.
    fn group_actions(&mut self, ui: &mut egui::Ui, id: u32) {
        let Some(n) = self.project.group(id).cloned() else { return };
        ui.separator();
        let mut name = n.name.clone();
        if ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY)).changed() {
            if let Some(m) = self.project.group_mut(id) {
                m.name = name;
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Group it").on_hover_text("Wrap this group in a new group").clicked() {
                self.push_undo();
                self.group_sel = self.project.wrap_group(id);
            }
            if ui.small_button("Up").on_hover_text("Draw over the group above it").clicked() {
                self.push_undo();
                self.project.nudge(id, 1);
            }
            if ui.small_button("Down").on_hover_text("Draw under the group below it").clicked() {
                self.push_undo();
                self.project.nudge(id, -1);
            }
            if ui.small_button("Ungroup").on_hover_text("Take the group apart. Its zones and groups move up and follow the settings above.").clicked() {
                self.push_undo();
                self.project.ungroup(id);
                self.group_sel = None;
            }
            if ui.small_button("Delete").on_hover_text("Delete the group with every zone and group inside it").clicked() {
                self.push_undo();
                self.project.delete_group(id);
                self.group_sel = None;
                self.zone_sel = None;
            }
        });
        // move into another group, or out to the top
        let inside = self.project.descendants(id);
        let groups: Vec<(u32, String)> = self.project.groups.iter().filter(|g| g.id != id && !inside.contains(&g.id)).map(|g| (g.id, self.project.path_name(g.id))).collect();
        let here = n.parent.map(|p| self.project.path_name(p)).unwrap_or_else(|| "Top level".into());
        let mut move_to: Option<Option<u32>> = None;
        ui.horizontal(|ui| {
            ui.label("In");
            egui::ComboBox::from_id_salt("move_group").selected_text(here).show_ui(ui, |ui| {
                if ui.selectable_label(n.parent.is_none(), "Top level").clicked() {
                    move_to = Some(None);
                }
                for (gid, gname) in &groups {
                    if ui.selectable_label(n.parent == Some(*gid), gname).clicked() {
                        move_to = Some(Some(*gid));
                    }
                }
            });
        });
        if let Some(to) = move_to {
            if to != n.parent {
                self.push_undo();
                self.project.move_into(id, to);
            }
        }
        let inner = self.project.descendants(id).len();
        let reach = if inner > 0 { format!(" It reaches the {inner} groups inside it unless they set the same setting.") } else { String::new() };
        ui.small(format!("The settings panel edits this group.{reach} Draw a zone while it is picked to put the zone in it."));
        ui.add_space(4.0);
    }

    fn zone_screen(&self, i: usize, origin: Pos2) -> Vec<Pos2> {
        self.project.shapes[i].points.iter().map(|p| self.map_to_screen(origin, *p)).collect()
    }

    /// Select, drag corners, move, add corners (double click an edge) and remove corners (right click) of zones.
    pub(super) fn zone_input(&mut self, ui: &egui::Ui, resp: &egui::Response, origin: Pos2) {
        let Some(pointer) = resp.hover_pos().or(resp.interact_pointer_pos()) else { return };
        // what is under the pointer, for the cursor and for a click
        let mut over_point = None;
        let mut over_edge = None;
        if let Some(i) = self.zone_sel.filter(|&i| i < self.project.shapes.len()) {
            let pts = self.zone_screen(i, origin);
            over_point = pts.iter().position(|q| (*q - pointer).length() <= HANDLE + 2.0);
            if over_point.is_none() {
                for k in 0..pts.len() {
                    let (d, t) = dist_to_segment(pointer, pts[k], pts[(k + 1) % pts.len()]);
                    if d < 6.0 {
                        over_edge = Some((k, t));
                        break;
                    }
                }
            }
        }
        if over_point.is_some() || self.zone_drag.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        // remove a corner
        if resp.clicked_by(PointerButton::Secondary) {
            if let (Some(i), Some(k)) = (self.zone_sel, over_point) {
                if self.project.shapes[i].points.len() > 3 {
                    self.push_undo();
                    self.project.shapes[i].points.remove(k);
                }
            }
            return;
        }
        // add a corner on an edge
        if resp.double_clicked() {
            if let (Some(i), Some((k, _))) = (self.zone_sel, over_edge) {
                self.push_undo();
                let m = self.screen_to_map(origin, pointer);
                self.project.shapes[i].points.insert(k + 1, m);
                return;
            }
        }
        if resp.drag_started_by(PointerButton::Primary) {
            let sel_inside = self.zone_sel.is_some_and(|i| inside(&self.zone_screen(i, origin), pointer));
            if let (Some(_), Some(k)) = (self.zone_sel, over_point) {
                self.push_undo();
                self.zone_drag = Some(ZoneDrag::Point(k));
            } else if sel_inside {
                self.push_undo();
                self.zone_drag = Some(ZoneDrag::Move);
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let (Some(i), Some(d)) = (self.zone_sel, self.zone_drag) {
                let delta = resp.drag_delta() / self.zoom;
                match d {
                    ZoneDrag::Point(k) => {
                        if let Some(p) = self.project.shapes[i].points.get_mut(k) {
                            p[0] += delta.x;
                            p[1] += delta.y;
                        }
                    }
                    ZoneDrag::Move => {
                        for p in self.project.shapes[i].points.iter_mut() {
                            p[0] += delta.x;
                            p[1] += delta.y;
                        }
                    }
                }
            }
        }
        if resp.drag_stopped() {
            self.zone_drag = None;
        }
        // click to select: the top-most visible zone under the pointer, or none
        if resp.clicked_by(PointerButton::Primary) {
            let mut hit = None;
            for i in (0..self.project.shapes.len()).rev() {
                if inside(&self.zone_screen(i, origin), pointer) {
                    hit = Some(i);
                    break;
                }
            }
            self.zone_sel = hit;
        }
        if self.zone_sel.is_some() && ui.input(|i| i.key_pressed(Key::Delete)) {
            if let Some(i) = self.zone_sel.take() {
                self.push_undo();
                self.project.shapes.remove(i);
            }
        }
    }

    /// Outlines of every zone, with the selected one filled and its corners shown.
    pub(super) fn draw_zones(&self, painter: &egui::Painter, origin: Pos2) {
        for (i, s) in self.project.shapes.iter().enumerate() {
            let sel = self.zone_sel == Some(i);
            if !self.show_outlines && !sel {
                continue;
            }
            let pts = self.zone_screen(i, origin);
            let (r, g, b) = match s.op {
                ShapeOp::Add => (120, 255, 120),
                ShapeOp::Subtract => (255, 110, 110),
            };
            let alpha = if !s.visible { 60 } else if sel { 255 } else { 140 };
            let width = if sel { 2.0 } else { 1.0 };
            painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(width, Color32::from_rgba_unmultiplied(r, g, b, alpha))));
            if sel && self.tool == Tool::Zones {
                for q in &pts {
                    painter.rect_filled(Rect::from_center_size(*q, Vec2::splat(HANDLE * 2.0)), 2.0, Color32::WHITE);
                    painter.rect_stroke(Rect::from_center_size(*q, Vec2::splat(HANDLE * 2.0)), 2.0, Stroke::new(1.5, Color32::from_rgb(r, g, b)), egui::StrokeKind::Inside);
                }
            }
            if sel {
                let c = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / pts.len().max(1) as f32;
                painter.text(Pos2::new(c.x, c.y), egui::Align2::CENTER_CENTER, self.zone_name(i), egui::FontId::proportional(13.0), Color32::WHITE);
            }
        }
    }
}
