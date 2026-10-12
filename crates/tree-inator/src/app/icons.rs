//! Small icons drawn with the painter, so they need no font or image files.

use super::*;

pub(super) fn tool_icon(painter: &egui::Painter, r: Rect, tool: Tool, col: Color32) {
    let c = r.center();
    let u = r.width() / 24.0;
    let p = |x: f32, y: f32| Pos2::new(c.x + x * u, c.y + y * u);
    let s = Stroke::new(1.6, col);
    match tool {
        Tool::Select => {
            painter.add(egui::Shape::convex_polygon(vec![p(-5.0, -8.0), p(-5.0, 7.0), p(-1.5, 3.5), p(1.5, 9.0), p(4.0, 8.0), p(1.0, 2.5), p(6.0, 2.5)], col, Stroke::NONE));
        }
        Tool::Lasso => {
            painter.circle_stroke(p(0.0, -2.0), 7.0 * u, s);
            painter.line_segment([p(-3.0, 4.0), p(-6.0, 9.0)], s);
            painter.circle_filled(p(-6.5, 9.5), 1.6 * u, col);
        }
        Tool::Polygon => {
            let pts = [p(0.0, -8.0), p(8.0, -2.0), p(5.0, 8.0), p(-5.0, 8.0), p(-8.0, -2.0)];
            for i in 0..5 {
                painter.line_segment([pts[i], pts[(i + 1) % 5]], s);
                painter.circle_filled(pts[i], 1.8 * u, col);
            }
        }
        Tool::Wand => {
            painter.line_segment([p(-8.0, 8.0), p(2.0, -2.0)], Stroke::new(2.2, col));
            for (dx, dy) in [(5.0, -8.0), (8.0, -3.0), (2.0, -9.0), (8.0, 3.0)] {
                painter.line_segment([p(dx - 2.0, dy), p(dx + 2.0, dy)], Stroke::new(1.3, col));
                painter.line_segment([p(dx, dy - 2.0), p(dx, dy + 2.0)], Stroke::new(1.3, col));
            }
        }
        Tool::Line => {
            let pts: Vec<Pos2> = (0..=12).map(|i| p(-9.0 + 18.0 * i as f32 / 12.0, 4.0 * (i as f32 * 0.9).sin())).collect();
            painter.add(egui::Shape::line(pts, Stroke::new(2.0, col)));
            painter.circle_filled(p(-9.0, 0.0), 2.0 * u, col);
            painter.circle_filled(p(9.0, 4.0 * (12.0f32 * 0.9).sin()), 2.0 * u, col);
        }
        Tool::Brush => {
            painter.circle_stroke(p(0.0, 0.0), 9.0 * u, s);
            for (x, y) in [(-3.0, -3.0), (3.0, -2.0), (0.0, 3.0), (-4.0, 3.0), (4.0, 4.0)] {
                painter.circle_filled(p(x, y), 1.5 * u, col);
            }
        }
        Tool::Single => {
            painter.circle_stroke(p(0.0, -2.0), 6.0 * u, s);
            painter.line_segment([p(0.0, 4.0), p(0.0, 9.0)], Stroke::new(2.0, col));
        }
        Tool::Eraser => {
            painter.add(egui::Shape::convex_polygon(vec![p(-8.0, 3.0), p(-1.0, -7.0), p(8.0, -1.0), p(1.0, 9.0)], Color32::TRANSPARENT, s));
            painter.line_segment([p(-4.0, -2.0), p(5.0, 4.0)], Stroke::new(1.2, col));
        }
        Tool::Mask => {
            let r2 = Rect::from_center_size(c, Vec2::splat(18.0 * u));
            painter.rect_stroke(r2, 2.0, s, egui::StrokeKind::Inside);
            for i in -2..=2 {
                painter.line_segment([p(i as f32 * 4.0 - 5.0, 8.0), p(i as f32 * 4.0 + 5.0, -8.0)], Stroke::new(1.0, col));
            }
        }
        Tool::Pan => {
            for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                painter.line_segment([p(0.0, 0.0), p(dx * 9.0, dy * 9.0)], s);
                painter.line_segment([p(dx * 9.0, dy * 9.0), p(dx * 6.0 + dy * 2.5, dy * 6.0 + dx * 2.5)], s);
                painter.line_segment([p(dx * 9.0, dy * 9.0), p(dx * 6.0 - dy * 2.5, dy * 6.0 - dx * 2.5)], s);
            }
        }
    }
}

pub(super) fn eye_icon(painter: &egui::Painter, r: Rect, on: bool, col: Color32) {
    let c = r.center();
    if on {
        painter.add(egui::Shape::line(
            vec![Pos2::new(c.x - 6.0, c.y), Pos2::new(c.x - 3.0, c.y - 3.5), Pos2::new(c.x + 3.0, c.y - 3.5), Pos2::new(c.x + 6.0, c.y), Pos2::new(c.x + 3.0, c.y + 3.5), Pos2::new(c.x - 3.0, c.y + 3.5), Pos2::new(c.x - 6.0, c.y)],
            Stroke::new(1.2, col),
        ));
        painter.circle_filled(c, 2.0, col);
    } else {
        painter.line_segment([Pos2::new(c.x - 6.0, c.y), Pos2::new(c.x + 6.0, c.y)], Stroke::new(1.2, col.gamma_multiply(0.5)));
    }
}

pub(super) fn lock_icon(painter: &egui::Painter, r: Rect, on: bool, col: Color32) {
    let c = r.center();
    let col = if on { col } else { col.gamma_multiply(0.3) };
    painter.rect_filled(Rect::from_center_size(Pos2::new(c.x, c.y + 2.0), Vec2::new(9.0, 6.0)), 1.0, col);
    painter.circle_stroke(Pos2::new(c.x, c.y - 1.5), 3.0, Stroke::new(1.2, col));
}
