//! A picture to draw on, such as the map the forest is for. It is shown under the forest in the editor and is
//! never part of an export. Big pictures are shrunk to fit and cut into tiles, so they stay light on memory.

use super::*;

/// The longest side kept for display, in pixels.
const MAX_SIDE: u32 = 8192;
/// Tiles are cut to at most this many pixels a side.
const TILE: u32 = 4096;

/// A decoded picture cut into tiles, each with where it sits as fractions of the whole: [x0, y0, x1, y1].
pub(super) type Tiles = Vec<(egui::ColorImage, [f32; 4])>;

/// The longest side of the copy the wand reads.
const WAND_SIDE: u32 = 1536;

fn load_tiles(path: &str) -> Result<(Tiles, BgPixels), String> {
    let img = image::open(path).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    let k = (MAX_SIDE as f32 / w.max(h) as f32).min(1.0);
    let rgba = if k < 1.0 {
        let (nw, nh) = (((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1));
        image::imageops::resize(&img.to_rgba8(), nw, nh, image::imageops::FilterType::Triangle)
    } else {
        img.to_rgba8()
    };
    drop(img);
    let (w, h) = (rgba.width(), rgba.height());
    // a smaller copy kept in memory for the wand
    let ks = (WAND_SIDE as f32 / w.max(h) as f32).min(1.0);
    let small = if ks < 1.0 { image::imageops::resize(&rgba, ((w as f32 * ks) as u32).max(1), ((h as f32 * ks) as u32).max(1), image::imageops::FilterType::Triangle) } else { rgba.clone() };
    let pixels = BgPixels { w: small.width() as usize, h: small.height() as usize, rgba: small.into_raw() };
    let mut out = Vec::new();
    let mut y = 0;
    while y < h {
        let th = TILE.min(h - y);
        let mut x = 0;
        while x < w {
            let tw = TILE.min(w - x);
            let tile = image::imageops::crop_imm(&rgba, x, y, tw, th).to_image();
            let ci = egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], tile.as_raw());
            out.push((ci, [x as f32 / w as f32, y as f32 / h as f32, (x + tw) as f32 / w as f32, (y + th) as f32 / h as f32]));
            x += tw;
        }
        y += th;
    }
    Ok((out, pixels))
}

impl TreeApp {
    /// Is a picture showing under the forest?
    pub(super) fn background_showing(&self) -> bool {
        self.project.background_image.as_ref().is_some_and(|b| b.visible) && !self.bg_tiles.is_empty()
    }

    /// Keep the loaded picture in step with the project: load it when the path changes, drop it when it goes.
    pub(super) fn sync_background(&mut self, ctx: &egui::Context) {
        let want = self.project.background_image.as_ref().filter(|b| !b.path.is_empty()).map(|b| b.path.clone());
        if want != self.bg_loaded {
            self.bg_loaded = want.clone();
            self.bg_tiles.clear();
            self.bg_pixels = None;
            self.bg_rx = None;
            if let Some(path) = want {
                let (tx, rx) = channel();
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(load_tiles(&path));
                    ctx2.request_repaint();
                });
                self.bg_rx = Some(rx);
            }
            self.compose_dirty = true;
        }
        if let Some(rx) = &self.bg_rx {
            if let Ok(r) = rx.try_recv() {
                self.bg_rx = None;
                match r {
                    Ok((tiles, pixels)) => {
                        self.bg_pixels = Some(pixels);
                        self.bg_tiles = tiles.into_iter().map(|(ci, uv)| (ctx.load_texture("background", ci, egui::TextureOptions::LINEAR), uv)).collect();
                    }
                    Err(e) => self.status = format!("Could not load the background image: {e}"),
                }
                self.compose_dirty = true;
            }
        }
    }

    /// Choose a picture, make the map the same size as it, and show it under the forest.
    pub(super) fn import_background(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"]).pick_file() else {
            return;
        };
        let path = path.to_string_lossy().to_string();
        match image::image_dimensions(&path) {
            Ok((w, h)) => {
                self.push_undo();
                self.project.background_image = Some(BackgroundImage { path, ..Default::default() });
                self.match_map_to_background([w, h]);
                self.status = format!("Background picture {w} x {h}. The map is now the same size. It is not exported.");
            }
            Err(e) => self.status = format!("Could not read that picture: {e}"),
        }
    }

    pub(super) fn match_map_to_background(&mut self, size: [u32; 2]) {
        self.project.map_w = size[0].clamp(64, 16384);
        self.project.map_h = size[1].clamp(64, 16384);
        self.map_size_edit = [self.project.map_w, self.project.map_h];
        self.fit_pending = true;
    }

    /// The controls for the picture, in the Map and view section.
    pub(super) fn background_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Import map image").on_hover_text("Draw on top of a picture, such as the map the forest is for. It is not exported.").clicked() {
                self.import_background();
            }
            if self.project.background_image.is_some() && ui.button("Remove").clicked() {
                self.push_undo();
                self.project.background_image = None;
            }
        });
        let mut size_to_match = None;
        let loading = self.bg_rx.is_some();
        if let Some(b) = &mut self.project.background_image {
            let name = std::path::Path::new(&b.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            ui.small(format!("{name}{}", if loading { " (loading)" } else { "" }));
            ui.horizontal(|ui| {
                ui.checkbox(&mut b.visible, "Show");
                ui.add(egui::Slider::new(&mut b.opacity, 0.05..=1.0).text("Opacity"));
            });
            if ui.small_button("Match map size to picture").clicked() {
                size_to_match = image::image_dimensions(&b.path).ok();
            }
        }
        if let Some((w, h)) = size_to_match {
            self.push_undo();
            self.match_map_to_background([w, h]);
        }
    }

    /// Draw the picture on the canvas, over the map colour and under the forest.
    pub(super) fn draw_background(&self, painter: &egui::Painter, map_rect: Rect) {
        let Some(b) = &self.project.background_image else { return };
        if !b.visible {
            return;
        }
        let tint = Color32::WHITE.gamma_multiply(b.opacity.clamp(0.0, 1.0));
        let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        for (tex, uv) in &self.bg_tiles {
            let at = |fx: f32, fy: f32| map_rect.min + Vec2::new(fx * map_rect.width(), fy * map_rect.height());
            painter.image(tex.id(), Rect::from_min_max(at(uv[0], uv[1]), at(uv[2], uv[3])), full, tint);
        }
    }

    /// How the preview is composited: over the map colour, or with transparency when a picture sits under it.
    pub(super) fn preview_options(&self) -> ComposeOptions {
        if self.background_showing() {
            ComposeOptions { shadows: true, shadow_only: false, background: None, premultiplied: true }
        } else {
            ComposeOptions { shadows: true, shadow_only: false, background: Some(self.background), premultiplied: false }
        }
    }
}
