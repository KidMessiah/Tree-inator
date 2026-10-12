//! Compare mode: the map is split, the left side keeps a snapshot of where you were, and only the right side changes
//! as you edit. Revert puts every setting back; Keep makes what you see on the right the new snapshot.

use super::*;
use eframe::wgpu;

/// A copy of the picture on screen and of the settings that made it.
pub(super) struct Snapshot {
    textures: Vec<(wgpu::Texture, egui::TextureId)>,
    /// How many of `textures` are zone layers over the first one.
    scale: f32,
    origin: [usize; 2],
    size: [usize; 2],
    project: Project,
    zoom: f32,
    pan: Vec2,
}

impl TreeApp {
    fn copy_image(&self, img: &forest_core::gpu::GpuImage) -> Option<(wgpu::Texture, egui::TextureId)> {
        let rs = self.render_state.as_ref()?;
        let size = wgpu::Extent3d { width: img.w as u32, height: img.h as u32, depth_or_array_layers: 1 };
        let tex = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("snapshot"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mut enc = rs.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo { texture: &img.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            size,
        );
        rs.queue.submit([enc.finish()]);
        let view = tex.create_view(&Default::default());
        let id = rs.renderer.write().register_native_texture(&rs.device, &view, wgpu::FilterMode::Linear);
        Some((tex, id))
    }

    pub(super) fn free_snapshot(&mut self) {
        if let (Some(s), Some(rs)) = (self.snapshot.take(), &self.render_state) {
            for (_, id) in &s.textures {
                rs.renderer.write().free_texture(id);
            }
        }
    }

    /// Keep the picture as it is now, with the settings that made it.
    pub(super) fn take_snapshot(&mut self) {
        let (Some(img), Some(v)) = (self.gpu_image.as_ref(), self.gpu_view.as_ref()) else {
            self.status = "Compare needs the finished preview; try again in a moment".into();
            return;
        };
        let mut textures = Vec::new();
        if let Some(t) = self.copy_image(img) {
            textures.push(t);
        }
        for e in &self.extra_gpu {
            if let Some(i) = &e.image {
                if let Some(t) = self.copy_image(i) {
                    textures.push(t);
                }
            }
        }
        if textures.is_empty() {
            return;
        }
        let snap = Snapshot { textures, scale: v.scale, origin: v.origin, size: v.size, project: self.real_project(), zoom: self.zoom, pan: self.pan };
        self.free_snapshot();
        self.snapshot = Some(snap);
    }

    /// Put every setting back to what the snapshot had.
    fn revert_to_snapshot(&mut self) {
        let Some(p) = self.snapshot.as_ref().map(|s| s.project.clone()) else { return };
        self.push_undo();
        let p = &p;
        self.project.gen = p.gen.clone();
        self.project.species = p.species.clone();
        self.project.light = p.light.clone();
        self.project.grading = p.grading.clone();
        // a group keeps its own settings, if it is still there
        for g in &p.groups {
            if let Some(now) = self.project.group_mut(g.id) {
                now.overrides = g.overrides.clone();
                now.species = g.species.clone();
            }
        }
        self.status = "Settings put back to the snapshot".into();
    }

    /// The settings that differ from the snapshot, by name.
    fn changed_since_snapshot(&self) -> Vec<String> {
        let Some(s) = &self.snapshot else { return Vec::new() };
        let now = self.real_project();
        let mut out = Vec::new();
        let mut walk = |scope: &str, a: &serde_json::Value, b: &serde_json::Value| {
            let d = forest_core::diff_value(a, b);
            fn paths(prefix: &str, v: &serde_json::Value, out: &mut Vec<String>) {
                match v {
                    serde_json::Value::Object(m) => {
                        for (k, x) in m {
                            let p = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                            paths(&p, x, out);
                        }
                    }
                    serde_json::Value::Null => {}
                    _ => out.push(prefix.to_string()),
                }
            }
            let mut ps = Vec::new();
            paths("", &d, &mut ps);
            for p in ps {
                out.push(super::tuning::setting_label(scope, &p));
            }
        };
        let val = |x: &dyn erased::Ser| x.to_value();
        walk("gen", &val(&s.project.gen), &val(&now.gen));
        walk("light", &val(&s.project.light), &val(&now.light));
        walk("grading", &val(&s.project.grading), &val(&now.grading));
        if serde_json::to_value(&s.project.species).ok() != serde_json::to_value(&now.species).ok() {
            out.push("Species colours or types".to_string());
        }
        out.sort();
        out.dedup();
        out
    }

    pub(super) fn compare_bar(&mut self, ui: &mut egui::Ui) {
        let changed = self.changed_since_snapshot();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Compare").strong());
            ui.label(egui::RichText::new(format!("{} settings changed since the snapshot", changed.len())).weak());
            if !changed.is_empty() {
                let list = if changed.len() > 6 { format!("{}, and {} more", changed[..6].join(", "), changed.len() - 6) } else { changed.join(", ") };
                ui.label(egui::RichText::new(list).small().color(Color32::from_rgb(255, 200, 90)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close compare").clicked() {
                    self.compare = false;
                    self.free_snapshot();
                }
                if ui.add(egui::Button::new(egui::RichText::new("Keep changes").strong()).fill(Color32::from_rgb(40, 110, 70))).on_hover_text("Make what you see on the right the new snapshot").clicked() {
                    self.take_snapshot();
                }
                if ui.add(egui::Button::new("Revert to snapshot").fill(Color32::from_rgb(120, 60, 50))).on_hover_text("Put every setting back to what the left side shows").clicked() {
                    self.revert_to_snapshot();
                }
                if ui.button("New snapshot here").on_hover_text("Replace the left side with the picture as it is now").clicked() {
                    self.take_snapshot();
                }
            });
        });
    }

    /// Self test: with compare on from the environment, take the first snapshot when the preview is ready, then apply the
    /// settings named in TREE_INATOR_EXTRA ("canopy_cover=0.4;depth.gain=1.3") so the right side differs.
    pub(super) fn compare_test_hooks(&mut self) {
        if !self.compare || self.snapshot.is_some() || self.rendering || self.gen_dirty || self.gpu_view.is_none() || self.gpu_image.is_none() {
            return;
        }
        self.take_snapshot();
        let Ok(extra) = std::env::var("TREE_INATOR_EXTRA") else { return };
        let mut g = serde_json::to_value(&self.project.gen).unwrap_or_default();
        for kv in extra.split(';') {
            let Some((path, val)) = kv.split_once('=') else { continue };
            let Ok(x) = val.parse::<f64>() else { continue };
            let mut cur = &mut g;
            let mut it = path.split('.').peekable();
            while let Some(k) = it.next() {
                if it.peek().is_none() {
                    if let Some(o) = cur.as_object_mut() {
                        o.insert(k.to_string(), serde_json::json!(x));
                    }
                    break;
                }
                match cur.get_mut(k) {
                    Some(n) => cur = n,
                    None => break,
                }
            }
        }
        if let Ok(new) = serde_json::from_value(g) {
            self.project.gen = new;
        }
    }

    /// The pictures making up the live view, bottom first, with where each goes on screen.
    pub(super) fn live_images(&self, origin: Pos2, map_rect: Rect) -> Vec<(egui::TextureId, Rect)> {
        let mut out = Vec::new();
        let shown = if self.gpu_layers.is_some() { self.gpu_texture } else { self.texture.as_ref().map(|t| t.id()) };
        let Some(id) = shown else { return out };
        let mut shown_at = map_rect;
        if let (Some(v), true) = (&self.gpu_view, self.gpu_layers.is_some()) {
            // the GPU image covers only part of the map
            let at = |px: f32, py: f32| self.map_to_screen(origin, [px / v.scale, py / v.scale]);
            let mut r = Rect::from_min_max(at(v.origin[0] as f32, v.origin[1] as f32), at((v.origin[0] + v.size[0]) as f32, (v.origin[1] + v.size[1]) as f32));
            // At one render pixel per screen pixel, put the picture exactly on the pixel grid so it is not smeared.
            let ppp = self.pixels_per_point;
            if (v.scale / (self.zoom * ppp * self.view_detail) - 1.0).abs() < 0.01 && self.view_detail == 1.0 {
                let min = Pos2::new((r.min.x * ppp).round() / ppp, (r.min.y * ppp).round() / ppp);
                r = Rect::from_min_size(min, Vec2::new(v.size[0] as f32, v.size[1] as f32) / ppp);
            }
            shown_at = r;
        }
        out.push((id, shown_at));
        // zones with their own settings are layers over it
        let layer_ids: Vec<egui::TextureId> = if self.gpu_layers.is_some() { self.extra_gpu.iter().filter_map(|e| e.texture).collect() } else { self.extra_cpu.iter().filter_map(|(_, t)| t.as_ref().map(|t| t.id())).collect() };
        for lid in layer_ids {
            out.push((lid, shown_at));
        }
        out
    }

    fn snapshot_images(&self, origin: Pos2) -> Vec<(egui::TextureId, Rect)> {
        let Some(s) = &self.snapshot else { return Vec::new() };
        let at = |px: f32, py: f32| self.map_to_screen(origin, [px / s.scale, py / s.scale]);
        let r = Rect::from_min_max(at(s.origin[0] as f32, s.origin[1] as f32), at((s.origin[0] + s.size[0]) as f32, (s.origin[1] + s.size[1]) as f32));
        s.textures.iter().map(|(_, id)| (*id, r)).collect()
    }

    /// Draw the map picture: the live view, or with compare on the snapshot to the left of the divider and the live
    /// view to its right. Returns where the divider is, if there is one.
    pub(super) fn draw_picture(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, origin: Pos2, map_rect: Rect) {
        let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        let bg = Color32::from_rgb((self.background[0] * 255.0) as u8, (self.background[1] * 255.0) as u8, (self.background[2] * 255.0) as u8);
        let live = self.live_images(origin, map_rect);
        let split_on = self.compare && self.snapshot.is_some();
        let sx = rect.min.x + rect.width() * self.split;
        let sides: Vec<(Rect, Vec<(egui::TextureId, Rect)>)> = if split_on {
            vec![
                (Rect::from_min_max(rect.min, Pos2::new(sx, rect.max.y)), self.snapshot_images(origin)),
                (Rect::from_min_max(Pos2::new(sx, rect.min.y), rect.max), live),
            ]
        } else {
            vec![(rect, live)]
        };
        for (clip, images) in sides {
            let p = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
            p.rect_filled(map_rect, 0.0, bg);
            self.draw_background(&p, map_rect);
            for (id, r) in images {
                p.image(id, r, full, Color32::WHITE);
            }
        }
        if !split_on {
            return;
        }
        painter.line_segment([Pos2::new(sx, rect.min.y), Pos2::new(sx, rect.max.y)], Stroke::new(2.0, Color32::WHITE));
        let handle = Rect::from_center_size(Pos2::new(sx, rect.center().y), Vec2::new(18.0, 44.0));
        painter.rect_filled(handle, 9.0, Color32::WHITE);
        let c = handle.center();
        for dx in [-1.0f32, 1.0] {
            painter.add(egui::Shape::convex_polygon(vec![Pos2::new(c.x + dx * 2.0, c.y - 5.0), Pos2::new(c.x + dx * 2.0, c.y + 5.0), Pos2::new(c.x + dx * 6.0, c.y)], Color32::BLACK, Stroke::NONE));
        }
        let r = ui.interact(handle.expand(6.0), ui.id().with("compare_split"), Sense::drag());
        if r.dragged() {
            self.split = ((sx + r.drag_delta().x - rect.min.x) / rect.width()).clamp(0.05, 0.95);
        }
        for (pos, al, txt) in [(rect.left_top() + Vec2::new(10.0, 8.0), egui::Align2::LEFT_TOP, "Snapshot"), (rect.right_top() + Vec2::new(-10.0, 8.0), egui::Align2::RIGHT_TOP, "Now (editing)")] {
            let galley = painter.layout_no_wrap(txt.to_string(), egui::FontId::proportional(12.0), Color32::WHITE);
            let box_ = al.anchor_size(pos, galley.size()).expand(4.0);
            painter.rect_filled(box_, 4.0, Color32::from_black_alpha(170));
            painter.text(pos, al, txt, egui::FontId::proportional(12.0), Color32::WHITE);
        }
        if let Some(s) = &self.snapshot {
            if (s.zoom - self.zoom).abs() > 1e-3 || (s.pan - self.pan).length() > 1.0 {
                painter.text(Pos2::new(rect.left() + 10.0, rect.top() + 34.0), egui::Align2::LEFT_TOP, "The snapshot covers the view it was taken in. Press New snapshot here after moving or zooming.", egui::FontId::proportional(11.0), Color32::from_rgb(255, 200, 90));
            }
        }
    }
}

mod erased {
    /// Anything that can become a JSON value, so the three settings objects can share a loop.
    pub trait Ser {
        fn to_value(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn to_value(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap_or_default()
        }
    }
}
