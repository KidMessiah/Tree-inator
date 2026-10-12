//! Grade and composite layers into straight-alpha RGBA8.

use crate::engine::{LayerKind, RenderLayers};
use crate::params::{Grading, LayerGrade};
use rayon::prelude::*;

#[derive(Clone, Copy)]
pub struct ComposeOptions {
    pub shadows: bool,
    /// Only the cast shadows, for placing on a separate layer elsewhere.
    pub shadow_only: bool,
    /// Opaque background colour, or None for transparency.
    pub background: Option<[f32; 3]>,
    /// Colour multiplied by alpha, the way the editor's texture draws it. For PNG leave this off.
    pub premultiplied: bool,
    /// Which layers are drawn, one bit per `LayerKind::index`. All of them by default.
    pub layers: u8,
}

impl Default for ComposeOptions {
    fn default() -> Self {
        ComposeOptions { shadows: true, shadow_only: false, background: None, premultiplied: false, layers: ALL_LAYERS }
    }
}

pub const ALL_LAYERS: u8 = 0x7F;

/// The layers an export can write on their own, without the ground shadow, which has its own file:
/// name suffix and layer mask. Ground is haze and grass, underbrush is shrubs and pebbles, canopy is
/// the forest crowns and the lone trees.
pub const LAYER_GROUPS: [(&str, u8); 3] = [("ground", 0b0000101), ("underbrush", 0b1001000), ("canopy", 0b0110000)];

fn grade(c: [f32; 3], g: &LayerGrade, hue_rot: Option<(f32, f32)>) -> [f32; 3] {
    let mut c = [c[0] * g.tint[0], c[1] * g.tint[1], c[2] * g.tint[2]];
    if let Some((cs, sn)) = hue_rot {
        // Rotate chroma in YIQ space.
        let y = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        let i = 0.596 * c[0] - 0.274 * c[1] - 0.322 * c[2];
        let q = 0.211 * c[0] - 0.523 * c[1] + 0.312 * c[2];
        let (i, q) = (i * cs - q * sn, i * sn + q * cs);
        c = [y + 0.956 * i + 0.621 * q, y - 0.272 * i - 0.647 * q, y - 1.106 * i + 1.703 * q];
    }
    let lum = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    let pivot = 0.25;
    for v in c.iter_mut() {
        *v = lum + (*v - lum) * g.saturation;
        *v *= g.brightness;
        *v = (*v - pivot) * g.contrast + pivot;
        *v = v.clamp(0.0, 1.0);
    }
    c
}

fn hue_of(g: &LayerGrade) -> Option<(f32, f32)> {
    (g.hue.abs() > 0.01).then(|| {
        let a = g.hue.to_radians();
        (a.cos(), a.sin())
    })
}

pub fn compose(r: &RenderLayers, grading: &Grading, opts: ComposeOptions) -> Vec<u8> {
    let n = r.w * r.h;
    let default = LayerGrade::default();
    let grades: Vec<&LayerGrade> = r
        .layers
        .iter()
        .map(|l| grading.layers.get(l.kind.index()).unwrap_or(&default))
        .collect();
    let hues: Vec<_> = grades.iter().map(|g| hue_of(g)).collect();
    let gh = hue_of(&grading.global);
    let sc = grading.shadow_color;
    let so = grading.shadow_opacity;
    let w = r.w;
    let mut out = vec![0u8; n * 4];
    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for (x, px) in row.chunks_mut(4).enumerate() {
            let mut acc = [0f32; 3];
            let mut a_acc = 0f32;
            if let Some(bg) = opts.background {
                acc = bg;
                a_acc = 1.0;
            }
            for (li, l) in r.layers.iter().enumerate() {
                let g = grades[li];
                if !g.visible || opts.layers & (1 << l.kind.index()) == 0 {
                    continue;
                }
                let Some(p) = l.get(x, y) else { continue };
                let (c, a) = if l.kind == LayerKind::GroundShadow {
                    if !opts.shadows {
                        continue;
                    }
                    (sc, p.alpha * so * g.opacity)
                } else {
                    if opts.shadow_only {
                        continue;
                    }
                    let a = p.alpha * g.opacity;
                    if a <= 0.0 {
                        continue;
                    }
                    let mut c = grade(p.rgb, g, hues[li]);
                    c = grade(c, &grading.global, gh);
                    if opts.shadows && g.receive_shadow && l.has_shadow() {
                        let k = 1.0 - so * 0.92 * p.shadow;
                        c = [c[0] * k, c[1] * k, c[2] * k];
                    }
                    (c, a)
                };
                if a <= 0.0 {
                    continue;
                }
                for k in 0..3 {
                    acc[k] = c[k] * a + acc[k] * (1.0 - a);
                }
                a_acc = a + a_acc * (1.0 - a);
            }
            let a = a_acc.clamp(0.0, 1.0);
            let un = if opts.premultiplied { 1.0 } else if a > 1e-4 { 1.0 / a } else { 0.0 };
            for k in 0..3 {
                px[k] = ((acc[k] * un).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            px[3] = (a * 255.0 + 0.5) as u8;
        }
    });
    out
}
