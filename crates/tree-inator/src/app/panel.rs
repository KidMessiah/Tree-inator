//! The theme picker: a grid of cards, each with a picture of the theme and its colours.

use super::*;

impl TreeApp {
    pub(super) fn theme_cards(&mut self, ui: &mut egui::Ui) {
        let list = themes();
        ui.small("Pick a theme, then fine tune. Tree types you picked stay.");
        let plain: Vec<usize> = (0..list.len()).filter(|&k| !is_satellite_theme(list[k].name)).collect();
        let matched: Vec<usize> = (0..list.len()).filter(|&k| is_satellite_theme(list[k].name)).collect();
        let mut picked = self.theme_grid(ui, &list, &plain, "theme_cards");
        if !matched.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Satellite Matched").strong());
            ui.small("Measured from aerial lidar and imagery of real forests: crown size, gaps, colour and shadow.");
            ui.add_space(2.0);
            picked = self.theme_grid(ui, &list, &matched, "theme_cards_sat").or(picked);
        }
        if let Some(k) = picked {
            self.project.apply_theme(&list[k]);
            self.theme = Some(list[k].name);
        }
    }

    /// One grid of theme cards for the given themes; the index of the one clicked.
    fn theme_grid(&mut self, ui: &mut egui::Ui, list: &[Theme], which: &[usize], id: &str) -> Option<usize> {
        let cols = 3;
        let gap = 6.0;
        let w = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).floor().max(70.0);
        let h = (w * 0.625).floor();
        let mut picked = None;
        egui::Grid::new(id).spacing([gap, gap]).show(ui, |ui| {
            for (n, &k) in which.iter().enumerate() {
                let t = &list[k];
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
                if (n + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        picked
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
