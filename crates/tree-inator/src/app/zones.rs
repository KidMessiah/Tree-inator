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
    /// What can be done with the zone that is picked.
    pub(super) fn zone_actions(&mut self, ui: &mut egui::Ui, i: usize) {
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
            self.tool = Tool::Select;
        }
        if ui.small_button("Clear all zones").clicked() {
            self.push_undo();
            self.project.shapes.clear();
            self.zone_sel = None;
        }
    }

    /// What can be done with the group that is picked.
    pub(super) fn group_actions(&mut self, ui: &mut egui::Ui, id: u32) {
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
            let locked = self.zone_sel.is_some_and(|i| self.zone_locked(i));
            if locked && (over_point.is_some() || sel_inside) {
                self.status = "That zone is locked. Unlock it in the zone list to edit it.".into();
            } else if let (Some(_), Some(k)) = (self.zone_sel, over_point) {
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
                        let moved = self.project.shapes[i].points.get(k).map(|p| [p[0] + delta.x, p[1] + delta.y]);
                        if let Some(mut m) = moved {
                            if self.snap {
                                if let Some(sp) = self.snap_point(m, Some(i)) {
                                    m = sp;
                                }
                            }
                            self.project.shapes[i].points[k] = m;
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
            let multi = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            match (hit, multi) {
                (Some(h), true) if self.zone_sel.is_some() => {
                    if self.zone_sel == Some(h) {
                        self.zone_sel = self.zone_multi.pop();
                    } else if let Some(k) = self.zone_multi.iter().position(|&z| z == h) {
                        self.zone_multi.remove(k);
                    } else {
                        self.zone_multi.push(h);
                    }
                }
                (h, _) => {
                    self.zone_sel = h;
                    self.zone_multi.clear();
                }
            }
            if hit.is_some() {
                self.group_sel = None;
            }
        }
        if self.zone_sel.is_some() && ui.input(|i| i.key_pressed(Key::Delete)) {
            let mut gone: Vec<usize> = self.selected_zones().into_iter().filter(|&i| !self.zone_locked(i)).collect();
            if !gone.is_empty() {
                self.push_undo();
                gone.sort_unstable_by(|a, b| b.cmp(a));
                for i in gone {
                    self.project.shapes.remove(i);
                }
                self.zone_sel = None;
                self.zone_multi.clear();
            }
        }
    }

    /// Outlines of every zone, with the selected one filled and its corners shown.
    pub(super) fn draw_zones(&self, painter: &egui::Painter, origin: Pos2) {
        for (i, s) in self.project.shapes.iter().enumerate() {
            let sel = self.selected_zones().contains(&i);
            if !self.show_outlines && !sel {
                continue;
            }
            let pts = self.zone_screen(i, origin);
            let (r, g, b) = if sel {
                (120, 210, 255)
            } else if s.mask {
                (220, 90, 90)
            } else {
                match s.op {
                    ShapeOp::Add => (120, 255, 120),
                    ShapeOp::Subtract => (255, 110, 110),
                }
            };
            let alpha = if !s.visible { 60 } else if sel { 255 } else { 140 };
            let width = if sel { 2.0 } else { 1.0 };
            painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(width, Color32::from_rgba_unmultiplied(r, g, b, alpha))));
            if self.zone_sel == Some(i) && self.tool == Tool::Select && !self.zone_locked(i) && pts.len() <= 120 {
                for q in &pts {
                    painter.rect_filled(Rect::from_center_size(*q, Vec2::splat(HANDLE * 2.0)), 2.0, Color32::WHITE);
                    painter.rect_stroke(Rect::from_center_size(*q, Vec2::splat(HANDLE * 2.0)), 2.0, Stroke::new(1.5, Color32::from_rgb(r, g, b)), egui::StrokeKind::Inside);
                }
            }
            if s.mask {
                // hatching, so a no-trees mask reads at a glance
                let (mnx, mny, mxx, mxy) = (pts.iter().map(|p| p.x).fold(f32::MAX, f32::min), pts.iter().map(|p| p.y).fold(f32::MAX, f32::min), pts.iter().map(|p| p.x).fold(f32::MIN, f32::max), pts.iter().map(|p| p.y).fold(f32::MIN, f32::max));
                let clip = painter.with_clip_rect(Rect::from_min_max(Pos2::new(mnx, mny), Pos2::new(mxx, mxy)).intersect(painter.clip_rect()));
                let mut x = mnx - (mxy - mny);
                while x < mxx {
                    clip.line_segment([Pos2::new(x, mxy), Pos2::new(x + (mxy - mny), mny)], Stroke::new(0.8, Color32::from_rgba_unmultiplied(220, 90, 90, 90)));
                    x += 9.0;
                }
            }
            if self.zone_sel == Some(i) {
                let c = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / pts.len().max(1) as f32;
                painter.text(Pos2::new(c.x, c.y), egui::Align2::CENTER_CENTER, self.zone_name(i), egui::FontId::proportional(13.0), Color32::WHITE);
            }
        }
    }
}
