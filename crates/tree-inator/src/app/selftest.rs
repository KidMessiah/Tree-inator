//! Self test: actions from TREE_INATOR_ACTIONS run once when the preview is ready, so the drawing tools can be
//! checked without a mouse. Actions are separated by `|`: tool=wand, mode=add, wand=900,500, polygon=x,y;x,y;x,y,
//! lasso=..., line=..., select=0, multi=1, op=union.

use super::*;

fn pts(v: &str) -> Vec<[f32; 2]> {
    v.split(';')
        .filter_map(|p| {
            let (x, y) = p.split_once(',')?;
            Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
        })
        .collect()
}

impl TreeApp {
    fn selftest_dump(&self, path: &str) {
        let mut out = String::new();
        for l in &self.selftest_log {
            out += &format!("{l}
");
        }
        for (i, z) in self.project.shapes.iter().enumerate() {
            let n = z.points.len();
            let mut area = 0.0f32;
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for k in 0..n {
                let (a, b) = (z.points[k], z.points[(k + 1) % n]);
                area += a[0] * b[1] - b[0] * a[1];
                for d in 0..2 {
                    lo[d] = lo[d].min(a[d]);
                    hi[d] = hi[d].max(a[d]);
                }
            }
            out += &format!("zone {i} name={:?} op={:?} mask={} locked={} vis={} group={:?} pts={n} area={:.0} bbox=({:.0},{:.0})-({:.0},{:.0})
", z.name, z.op, z.mask, z.locked, z.visible, z.group, area.abs() / 2.0, lo[0], lo[1], hi[0], hi[1]);
        }
        for g in &self.project.groups {
            out += &format!("group {} name={:?} parent={:?} locked={}
", g.id, g.name, g.parent, g.locked);
        }
        out += &format!("ngroups={} sel={:?} multi={:?} solo={:?} painted={}
", self.project.groups.len(), self.zone_sel, self.zone_multi, self.solo, self.project.painted.len());
        if let Some(b) = &self.project.background_image {
            out += &format!("bg px_per_m={:.4}
", b.px_per_m);
        }
        let _ = std::fs::write(path, out);
    }

    pub(super) fn run_selftest_actions(&mut self, ctx: &egui::Context) {
        if self.selftest_done || self.rendering || self.gen_dirty || self.gpu_view.is_none() {
            return;
        }
        let Ok(script) = std::env::var("TREE_INATOR_ACTIONS") else {
            self.selftest_done = true;
            return;
        };
        // wait for the map picture, when there is one, so the wand has pixels
        if self.project.background_image.is_some() && self.bg_pixels.is_none() {
            return;
        }
        self.selftest_done = true;
        for act in script.split('|') {
            let Some((k, v)) = act.split_once('=') else { continue };
            match k.trim() {
                "tool" => {
                    if let Some(t) = Tool::ALL.into_iter().find(|t| format!("{t:?}").eq_ignore_ascii_case(v.trim())) {
                        self.set_tool(t);
                    }
                }
                "mode" => {
                    self.mode = match v.trim() {
                        "sub" | "subtract" => Mode::Subtract,
                        "int" | "intersect" => Mode::Intersect,
                        _ => Mode::Add,
                    }
                }
                "tol" => self.wand_tol = v.trim().parse().unwrap_or(32.0),
                "snap" => self.snap = v.trim() != "0",
                "width" => self.line_width = v.trim().parse().unwrap_or(24.0),
                "lineuse" => {
                    self.line_use = match v.trim() {
                        "trees" => LineUse::Trees,
                        "split" => LineUse::Split,
                        _ => LineUse::Cut,
                    }
                }
                "wand" => {
                    if let Some(p) = pts(v).first() {
                        let mode = self.mode;
                        self.wand_click(*p, mode);
                    }
                }
                "polygon" => {
                    let p = pts(v);
                    self.finish_polygon(p, ctx);
                }
                "lasso" => {
                    let p = pts(v);
                    self.finish_lasso(p, ctx);
                }
                "line" => {
                    self.stroke = pts(v);
                    self.finish_line();
                }
                "select" => {
                    self.zone_sel = v.trim().parse().ok();
                    self.zone_multi.clear();
                }
                "multi" => {
                    if let Ok(i) = v.trim().parse() {
                        self.zone_multi.push(i);
                    }
                }
                "op" => self.selftest_op(v.trim()),
                "lock" => {
                    if let Ok(i) = v.trim().parse::<usize>() {
                        if let Some(z) = self.project.shapes.get_mut(i) {
                            z.locked = !z.locked;
                        }
                    }
                }
                "lockgroup" => {
                    if let Ok(g) = v.trim().parse::<u32>() {
                        if let Some(n) = self.project.group_mut(g) {
                            n.locked = !n.locked;
                        }
                    }
                }
                "selgroup" => {
                    self.group_sel = v.trim().parse().ok();
                    self.zone_sel = None;
                }
                "solo" => self.solo = v.trim().parse().ok().map(RowId::Zone),
                "newgroup" => {
                    let g = self.project.add_group(None);
                    self.group_sel = Some(g);
                }
                "drop" => {
                    // zone onto group: drop=z:0>g:1
                    let parse = |t: &str| -> Option<RowId> {
                        let (k, n) = t.split_once(':')?;
                        let n: usize = n.parse().ok()?;
                        Some(if k == "g" { RowId::Group(n as u32) } else { RowId::Zone(n) })
                    };
                    if let Some((a, b)) = v.split_once('>') {
                        if let (Some(a), Some(b)) = (parse(a.trim()), parse(b.trim())) {
                            self.selftest_drop(a, b);
                        }
                    }
                }
                "undo" => self.undo(),
                "redo" => self.redo(),
                "calib" => {
                    // calib=x1,y1;x2,y2;metres
                    let parts: Vec<&str> = v.split(';').collect();
                    if parts.len() == 3 {
                        let p = pts(&format!("{};{}", parts[0], parts[1]));
                        let m: f32 = parts[2].trim().parse().unwrap_or(100.0);
                        if p.len() == 2 {
                            let px = ((p[0][0] - p[1][0]).powi(2) + (p[0][1] - p[1][1]).powi(2)).sqrt();
                            if let Some(b) = self.project.background_image.as_mut() {
                                b.px_per_m = px / m;
                                b.scale_bar = true;
                                b.grid = true;
                            }
                        }
                    }
                }
                "snap_at" => {
                    if let Some(p) = pts(v).first() {
                        let r = self.snap_point(*p, None);
                        self.selftest_log.push(format!("snap_at {:?} -> {:?}", p, r));
                    }
                }
                "dump" => self.selftest_dump(v.trim()),
                _ => {}
            }
        }
    }
}
