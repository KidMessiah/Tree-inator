//! Stand age. A forest drawn at "age 0" is shown as drawn: mature trees at the sizes of the settings. With an age in
//! years, crowns grow along a curve that levels off at the largest crown the tree type really reaches, so a thousand
//! year old stand is never made of screen-sized trees. A young stand is open and full of shrubs, a mature one is
//! closed, and an old one opens up again with gaps and a wider spread of heights.
//!
//! Sizes are in generated map pixels. One pixel stands for 0.45 m at forest size 100% (the scale the themes are
//! built to), so a crown radius in metres divides by 0.45.

use crate::params::{GenParams, Project};

/// Metres per generated map pixel at forest size 100%.
pub const METRES_PER_PX: f32 = 0.45;

/// The largest crown radius, in metres, that a tree type reaches in the wild. Types not listed use 8 m.
pub fn max_crown_radius_m(preset: &str) -> f32 {
    match preset {
        "quaking_aspen" => 4.5,
        "silver_birch" => 5.0,
        "lombardy_poplar" => 2.5,
        "acer" | "sassafras" => 7.0,
        "black_oak" | "cambridge_oak" => 11.0,
        "black_tupelo" => 8.0,
        "live_oak" => 18.0,
        "sphere_tree" | "apple" => 4.5,
        "hill_cherry" => 6.0,
        "weeping_willow" | "weeping_willow_o" => 10.0,
        "balsam_fir" | "spruce" => 4.0,
        "douglas_fir" => 7.0,
        "small_pine" => 6.5,
        "european_larch" => 4.5,
        "juniper" => 4.5,
        "bald_cypress" => 7.0,
        "umbrella_acacia" => 9.0,
        "palm" | "fan_palm" => 3.5,
        "bamboo" => 1.8,
        "baobab" => 11.0,
        "banyan" => 28.0,
        "mangrove" => 7.0,
        "eucalyptus" => 8.5,
        "ginkgo" => 8.0,
        "joshua_tree" => 3.0,
        "dragon_blood" => 6.0,
        "krummholz" => 3.0,
        _ => 8.0,
    }
}

/// Years a tree type needs to reach about 95 percent of its full crown. Pioneers are quick, oaks and conifers slow.
pub fn years_to_grow(preset: &str) -> f32 {
    match preset {
        "quaking_aspen" | "silver_birch" | "lombardy_poplar" | "weeping_willow" | "weeping_willow_o" | "umbrella_acacia" | "eucalyptus" | "bamboo" | "apple" | "hill_cherry" => 30.0,
        "black_oak" | "cambridge_oak" | "live_oak" | "baobab" | "banyan" | "douglas_fir" | "bald_cypress" => 120.0,
        "balsam_fir" | "spruce" | "small_pine" | "european_larch" | "juniper" | "krummholz" | "dragon_blood" | "joshua_tree" => 90.0,
        _ => 60.0,
    }
}

/// The size of a tree's crown at an age in years, relative to the crown as drawn, which is a mature tree at 60 percent of
/// the years it takes to grow. Younger trees are smaller, older ones keep growing more and more slowly, about 1.4 times
/// the drawn size in the end. The settings cap it at the type's real largest crown.
pub fn growth(age: f32, preset: &str) -> f32 {
    if age <= 0.0 {
        return 1.0;
    }
    let t95 = years_to_grow(preset);
    let g = |t: f32| 1.0 - (-2.0 * t / t95).exp();
    (g(age) / g(0.6 * t95)).clamp(0.08, 3.0)
}

/// The largest crown radius of a type in generated map pixels.
pub fn max_crown_px(preset: &str) -> f32 {
    max_crown_radius_m(preset) / METRES_PER_PX
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn main_preset(p: &Project) -> String {
    p.species.get(p.gen.main_species as usize).map(|s| s.preset.clone()).unwrap_or_default()
}

impl Project {
    /// The project with its stand age worked into the settings. With no age, or once already applied, it is the same
    /// project. Crowns follow the growth curve of the main tree type, capped at that type's largest crown; the cover,
    /// shrubs, gaps and height spread follow the stage of the stand.
    pub fn aged(&self) -> Project {
        let g = &self.gen;
        if g.age <= 0.0 || g.age_applied {
            return self.clone();
        }
        let preset = main_preset(self);
        let grow = growth(g.age, &preset);
        // how far past maturity the stand is, 0 at maturity to 1 once very old
        let old = smooth(0.7, 2.6, g.age / years_to_grow(&preset));
        let cap = max_crown_px(&preset);
        let mut q = self.clone();
        let a: &mut GenParams = &mut q.gen;
        a.crown_max = (g.crown_max * grow).min(cap).max(1.5);
        a.crown_min = (g.crown_min * grow).min(a.crown_max * 0.9).max(1.0);
        let young = grow.min(1.0);
        a.canopy_cover = (g.canopy_cover * (0.4 + 0.6 * (young * 1.6).min(1.0))).clamp(0.02, 1.0);
        a.shrub_density = g.shrub_density * (1.0 + 1.6 * (1.0 - young));
        a.gaps = (g.gaps + 0.35 * old).min(1.0);
        a.gap_size = g.gap_size * (1.0 + 0.8 * old);
        a.depth.height_var = g.depth.height_var * (0.5 + 0.5 * young) * (1.0 + 0.9 * old);
        a.age_applied = true;
        q
    }
}
