//! Project and parameter types. All sizes are in map (native CG2A) pixels.

use crate::groups::Group;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeOp {
    Add,
    Subtract,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shape {
    pub op: ShapeOp,
    pub points: Vec<[f32; 2]>,
    /// A name for the zone list.
    #[serde(default)]
    pub name: String,
    /// Hidden zones stay in the project but are left out of the forest.
    #[serde(default = "yes")]
    pub visible: bool,
    /// The group this zone is in, by the group id in `Project::groups`. None follows the project settings.
    #[serde(default, alias = "layer")]
    pub group: Option<u32>,
    /// Settings of a zone that was a layer by itself, from before layers could hold several zones. Projects are
    /// moved over to `layers` when they are loaded, so this is only ever read.
    #[serde(default, skip_serializing)]
    pub settings: Option<ZoneSettings>,
    /// A locked zone cannot be moved or edited by accident.
    #[serde(default)]
    pub locked: bool,
    /// A clearing that is a no-trees mask: kept in the zone list apart from the forest, drawn hatched.
    #[serde(default)]
    pub mask: bool,
}

/// A full copy of a layer's settings, as older project files held it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZoneSettings {
    pub gen: GenParams,
    pub species: Vec<Species>,
}

fn yes() -> bool {
    true
}

impl Shape {
    pub fn new(op: ShapeOp, points: Vec<[f32; 2]>) -> Shape {
        Shape { op, points, name: String::new(), visible: true, group: None, settings: None, locked: false, mask: false }
    }
}

/// Macro shape of the patch, applied on top of the drawn outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    /// Follow the drawn outline as is.
    Plain,
    /// Solid core whose edge breaks into islands and fragments.
    Fragmented,
    /// Branching fingers, like forest following streams and valleys.
    Branching,
    /// Separate clumps with open ground between.
    Clumps,
    /// Deep lobes and bays cut into the outline.
    Lobed,
}

impl Pattern {
    pub const ALL: [Pattern; 5] = [Pattern::Plain, Pattern::Fragmented, Pattern::Branching, Pattern::Clumps, Pattern::Lobed];
    pub fn name(self) -> &'static str {
        match self {
            Pattern::Plain => "Plain",
            Pattern::Fragmented => "Fragmented edge",
            Pattern::Branching => "Branching fingers",
            Pattern::Clumps => "Clumps",
            Pattern::Lobed => "Lobed",
        }
    }
}

/// Which height layer a tree belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreeLayer {
    Shrub,
    Canopy,
    Lone,
    Pebble,
}

impl TreeLayer {
    pub const ALL: [TreeLayer; 4] = [TreeLayer::Shrub, TreeLayer::Canopy, TreeLayer::Lone, TreeLayer::Pebble];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            TreeLayer::Shrub => "Shrubs",
            TreeLayer::Canopy => "Canopy",
            TreeLayer::Lone => "Lone trees",
            TreeLayer::Pebble => "Pebbles",
        }
    }
    /// Crown height for a radius, the same rule the generator uses.
    pub fn height(self, r: f32) -> f32 {
        match self {
            TreeLayer::Shrub => r * 0.8 + 4.0,
            TreeLayer::Canopy => r * 1.3 + 18.0,
            TreeLayer::Lone => r * 1.4 + 12.0,
            TreeLayer::Pebble => r + 3.0,
        }
    }
}

/// A single tree. Painted trees are stored like this in the project.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TreeRec {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub h: f32,
    pub tone: f32,
    pub layer: TreeLayer,
    pub species: u8,
    pub seed: u32,
}

/// One kind of tree: how it branches and what colour its leaves are.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Species {
    pub name: String,
    /// Multiplies the tree's height, which sets how long its shadow is.
    pub height: f32,
    /// Leaf colour ramp, shadow to light, 0..1.
    pub ramp: [[f32; 3]; 4],
    /// Bark, stone or wood colour for the solid parts.
    pub bark: [f32; 3],
    /// Marks a tree type picked by hand, which themes keep.
    pub procedural: bool,
    /// Which procedural tree type (acer, apple, balsam_fir, palm, ...), see `treegen_sprite::TREE_TYPES`.
    pub preset: String,
    /// Canopy strength for a procedural tree: below zero the type's own, 0 the raw generator.
    pub canopy_fill: f32,
    /// The built-in form of older project files, read only to pick a tree type for species saved without one.
    #[serde(default, skip_serializing, alias = "form")]
    legacy_form: String,
}

fn rgb(c: [[u8; 3]; 4]) -> [[f32; 3]; 4] {
    c.map(|v| v.map(|x| x as f32 / 255.0))
}

impl Species {
    /// A new species of tree type `key` with colours and height that suit the kind of tree.
    pub fn for_type(key: &str) -> Species {
        let label = crate::treegen_sprite::tree_type_label(key).to_string();
        let (ramp, bark, height) = match key {
            "fallen_log" => ([[110, 106, 96], [150, 146, 134], [185, 180, 166], [210, 205, 190]], [150, 144, 130], 0.3),
            "stump" => ([[70, 54, 36], [104, 82, 56], [140, 112, 76], [176, 146, 100]], [60, 46, 34], 0.3),
            k if crate::dead::is_dead(k) => ([[44, 42, 38], [78, 74, 66], [112, 108, 98], [150, 146, 134]], [90, 84, 76], 1.0),
            "balsam_fir" | "douglas_fir" | "small_pine" | "spruce" | "juniper" | "krummholz" => ([[18, 40, 30], [34, 70, 50], [56, 100, 70], [90, 136, 96]], [50, 40, 32], if key == "krummholz" { 0.6 } else { 1.4 }),
            "european_larch" | "ginkgo" => ([[60, 74, 20], [110, 130, 36], [160, 180, 60], [210, 220, 100]], [60, 46, 34], 1.2),
            "apple" | "hill_cherry" => ([[120, 60, 80], [200, 110, 140], [236, 160, 184], [250, 210, 224]], [60, 46, 34], 0.9),
            "acer" => ([[60, 14, 10], [130, 30, 20], [190, 60, 30], [230, 110, 50]], [60, 46, 34], 0.8),
            _ => ([[26, 44, 20], [52, 82, 32], [92, 128, 48], [140, 170, 76]], [60, 46, 34], 1.0),
        };
        Species { name: label, preset: key.to_string(), procedural: true, height, ramp: rgb(ramp), bark: bark.map(|v| v as f32 / 255.0), ..Species::default() }
    }

    /// The procedural tree type this species grows as.
    pub fn tree_type(&self) -> Option<&str> {
        if !self.preset.is_empty() {
            Some(&self.preset)
        } else {
            None
        }
    }

    /// Mushrooms carry their own colouring in the sprite and skip the leaf styling.
    pub fn plain_colours(&self) -> bool {
        self.preset.starts_with("mushroom_")
    }

    /// Give this species the procedural tree type that suits it, if it has none yet.
    pub fn assign_tree_type(&mut self) {
        if self.preset.is_empty() {
            self.preset = default_tree_type(&self.name, &self.legacy_form).to_string();
        }
    }
}

/// The procedural tree type that best matches a species, by name first, then by the built-in form older
/// files stored, and black tupelo when nothing fits.
pub fn default_tree_type(name: &str, legacy_form: &str) -> &'static str {
    let n = name.to_lowercase();
    let by_name: &[(&str, &str)] = &[
        ("sakura", "hill_cherry"),
        ("cherry", "hill_cherry"),
        ("blossom", "hill_cherry"),
        ("azalea", "apple"),
        ("bamboo", "bamboo"),
        ("redwood", "douglas_fir"),
        ("hemlock", "douglas_fir"),
        ("larch", "european_larch"),
        ("fern", "fan_palm"),
        ("palm", "palm"),
        ("mangrove", "weeping_willow_o"),
        ("juniper", "small_pine"),
        ("olive", "sphere_tree"),
        ("autumn", "sassafras"),
        ("golden", "quaking_aspen"),
        ("broadleaf", "quaking_aspen"),
        ("dead", "dead_oak"),
    ];
    if let Some((_, t)) = by_name.iter().find(|(k, _)| n.contains(k)) {
        return t;
    }
    match legacy_form {
        "Spruce" => "balsam_fir",
        "Sage" => "quaking_aspen",
        "Bush" | "Baobab" => "black_oak",
        "Birch" => "silver_birch",
        "Pine" => "small_pine",
        "Willow" => "weeping_willow",
        "Cypress" => "lombardy_poplar",
        "Acacia" => "acer",
        "Palm" => "palm",
        "TreeFern" => "fan_palm",
        "Cedar" => "douglas_fir",
        "Gnarled" => "cambridge_oak",
        "Dead" => "dead_oak",
        _ => "black_tupelo",
    }
}

impl Default for Species {
    fn default() -> Self {
        Species {
            name: "Broadleaf".into(),
            height: 1.0,
            ramp: rgb([[22, 36, 12], [48, 70, 24], [82, 110, 40], [128, 156, 64]]),
            bark: [60.0 / 255.0, 46.0 / 255.0, 34.0 / 255.0],
            procedural: false,
            preset: String::new(),
            canopy_fill: -1.0,
            legacy_form: String::new(),
        }
    }
}

/// The five mushroom species: cap colours from the ramp, warts in the bark colour.
pub fn mushroom_species() -> Vec<Species> {
    let base = Species { procedural: true, height: 1.0, ..Species::default() };
    let mk = |name: &str, kind: &str, ramp: [[u8; 3]; 4], wart: [u8; 3]| Species {
        name: name.into(),
        preset: kind.into(),
        ramp: rgb(ramp),
        bark: wart.map(|v| v as f32 / 255.0),
        canopy_fill: 0.0,
        ..base.clone()
    };
    vec![
        mk("Mushroom: redcap", "mushroom_redcap", [[150, 24, 22], [204, 42, 30], [228, 100, 40], [238, 176, 76]], [246, 238, 218]),
        mk("Mushroom: brown cap", "mushroom_brown", [[88, 52, 28], [140, 90, 48], [190, 144, 92], [214, 182, 132]], [214, 190, 150]),
        mk("Mushroom: violet cap", "mushroom_violet", [[66, 40, 90], [128, 86, 150], [176, 140, 190], [206, 180, 214]], [226, 210, 232]),
        mk("Mushroom: funnel cap", "mushroom_funnel", [[168, 112, 26], [224, 170, 52], [240, 206, 100], [246, 228, 150]], [250, 238, 190]),
        mk("Mushroom: puffball", "mushroom_puffball", [[196, 184, 156], [228, 220, 198], [238, 232, 212], [240, 234, 216]], [168, 154, 124]),
    ]
}

pub fn default_species() -> Vec<Species> {
    let b = Species::default();
    let mut v = vec![
        b.clone(),
        Species { name: "Conifer".into(), height: 1.3, preset: "balsam_fir".into(),
            ramp: rgb([[14, 30, 20], [30, 54, 36], [52, 82, 52], [80, 112, 70]]), ..b.clone() },
        Species { name: "Sage".into(), preset: "quaking_aspen".into(),
            ramp: rgb([[30, 44, 34], [62, 84, 64], [100, 126, 98], [150, 170, 140]]), ..b.clone() },
        Species { name: "Autumn".into(),
            ramp: rgb([[50, 22, 8], [120, 52, 14], [186, 96, 26], [232, 164, 60]]), ..b.clone() },
        Species { name: "Golden".into(), ramp: rgb([[44, 38, 8], [104, 92, 20], [170, 150, 40], [224, 206, 92]]), ..b.clone() },
        Species { name: "Dead".into(), preset: "dead_oak".into(),
            ramp: rgb([[44, 42, 38], [78, 74, 66], [112, 108, 98], [150, 146, 134]]), ..b.clone() },
        Species { name: "Bush".into(), preset: "black_oak".into(), height: 0.5, ..b.clone() },
        Species { name: "Birch".into(), preset: "silver_birch".into(), height: 1.1,
            ramp: rgb([[34, 52, 18], [76, 104, 34], [126, 156, 58], [176, 200, 96]]), ..b.clone() },
        Species { name: "Pine".into(), preset: "small_pine".into(), height: 1.4,
            ramp: rgb([[16, 34, 22], [34, 62, 40], [58, 92, 56], [92, 124, 78]]), ..b.clone() },
        Species { name: "Willow".into(), preset: "weeping_willow".into(),
            ramp: rgb([[40, 56, 20], [86, 112, 40], [134, 160, 64], [180, 200, 100]]), ..b.clone() },
        Species { name: "Cypress".into(), preset: "lombardy_poplar".into(), height: 1.4,
            ramp: rgb([[12, 28, 18], [26, 50, 32], [44, 74, 46], [70, 100, 62]]), ..b.clone() },
        Species { name: "Acacia".into(), preset: "acer".into(), height: 0.8,
            ramp: rgb([[54, 54, 22], [100, 100, 36], [150, 146, 58], [196, 188, 94]]), ..b },
    ];
    v.iter_mut().for_each(Species::assign_tree_type);
    v.extend(mushroom_species());
    v
}

/// A colour theme: a few themed species plus ground and haze colours. No dead trees in any of them.
pub struct Theme {
    pub name: &'static str,
    pub note: &'static str,
    /// Main canopy, the species mixed into it, and the shrubs.
    pub species: [Species; 3],
    pub mix_amount: f32,
    pub grass: [u8; 3],
    pub accent: [u8; 3],
    pub haze: [u8; 3],
}

fn themed(name: &str, preset: &str, height: f32, ramp: [[u8; 3]; 4]) -> Species {
    Species { name: name.into(), preset: preset.into(), height, ramp: rgb(ramp), ..Species::default() }
}

/// A species that grows as one of the procedural tree types, with the colours of its leaves and its wood.
fn typed(name: &str, preset: &str, ramp: [[u8; 3]; 4], bark: [u8; 3], height: f32) -> Species {
    Species { name: name.into(), preset: preset.into(), height, ramp: rgb(ramp), bark: bark.map(|v| v as f32 / 255.0), ..Species::default() }
}

/// How many mushrooms a theme grows and which kinds (redcap, brown, violet, funnel, puffball).
pub fn theme_mushrooms(name: &str) -> (f32, [f32; 5]) {
    match name {
        "Feywild" => (1.6, [0.1, 0.0, 0.6, 0.3, 0.0]),
        "Fungal" => (2.6, [0.3, 0.1, 0.3, 0.2, 0.1]),
        "Shadowfell" => (1.1, [0.0, 0.1, 0.8, 0.0, 0.1]),
        "Blighted" => (1.2, [0.0, 0.2, 0.3, 0.0, 0.5]),
        "Bloodwood" => (1.2, [0.8, 0.1, 0.0, 0.0, 0.1]),
        "Silverwood" => (1.0, [0.0, 0.1, 0.5, 0.0, 0.4]),
        "Crystal Grove" => (1.2, [0.0, 0.0, 0.7, 0.3, 0.0]),
        "Celestial" => (0.8, [0.0, 0.0, 0.4, 0.5, 0.1]),
        "Astral" => (0.9, [0.0, 0.0, 0.7, 0.2, 0.1]),
        "Swamp" => (1.4, [0.1, 0.3, 0.0, 0.2, 0.4]),
        _ => (0.0, [0.0; 5]),
    }
}

/// Themes made of bare or sparse trees set how much of the ground the crowns cover, so the wood and the
/// ground show. Other themes leave the density as it is.
pub fn theme_canopy(name: &str) -> Option<f32> {
    match name {
        "Dead Forest" => Some(0.5),
        "Bare Winter" => Some(0.55),
        "Burnt Forest" => Some(0.45),
        "Baobab Plains" => Some(0.35),
        "Mojave" => Some(0.3),
        "Dragon Blood Isle" => Some(0.5),
        "Eucalyptus Woodland" => Some(0.55),
        "Windswept Heath" => Some(0.6),
        "Juniper Highlands" => Some(0.4),
        "Acacia Plains" => Some(0.3),
        _ => None,
    }
}

/// Share of the canopy that is giant mushrooms in a theme.
pub fn theme_giants(name: &str) -> f32 {
    match name {
        "Fungal" => 0.32,
        "Feywild" => 0.12,
        "Bloodwood" => 0.08,
        "Shadowfell" => 0.07,
        "Blighted" => 0.06,
        "Silverwood" | "Crystal Grove" | "Astral" => 0.05,
        "Celestial" => 0.04,
        "Swamp" => 0.04,
        _ => 0.0,
    }
}

/// Themes made from aerial lidar and hyperspectral imagery (non-essential/research): species colours, ground colours and
/// the stand's own structure settings, read from `assets/satellite_themes.json`.
fn satellite_data() -> &'static serde_json::Value {
    static DATA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../assets/satellite_themes.json")).unwrap_or(serde_json::Value::Null))
}

/// The satellite themes in the order the data file gives them, with their names and notes kept for the whole run.
fn satellite_list() -> &'static [(&'static str, &'static str)] {
    static LIST: std::sync::OnceLock<Vec<(&'static str, &'static str)>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        let mut v: Vec<(i64, &'static str, &'static str)> = Vec::new();
        if let Some(m) = satellite_data().as_object() {
            for (name, d) in m {
                let note = d.get("note").and_then(|n| n.as_str()).unwrap_or("");
                let order = d.get("order").and_then(|o| o.as_i64()).unwrap_or(i64::MAX);
                v.push((order, Box::leak(name.clone().into_boxed_str()), Box::leak(note.to_string().into_boxed_str())));
            }
        }
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|(_, n, d)| (n, d)).collect()
    })
}

/// True for a theme measured from aerial lidar and imagery.
pub fn is_satellite_theme(name: &str) -> bool {
    satellite_data().get(name).is_some()
}

fn satellite_theme(name: &'static str, note: &'static str) -> Option<Theme> {
    let d = satellite_data().get(name)?;
    let col3 = |v: &serde_json::Value| -> [u8; 3] {
        let a: Vec<u8> = v.as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0).min(255) as u8).collect()).unwrap_or_default();
        [a.first().copied().unwrap_or(0), a.get(1).copied().unwrap_or(0), a.get(2).copied().unwrap_or(0)]
    };
    let mut species: Vec<Species> = Vec::new();
    for s in d.get("species")?.as_array()? {
        let ramp: Vec<[u8; 3]> = s.get("ramp")?.as_array()?.iter().map(col3).collect();
        if ramp.len() != 4 {
            return None;
        }
        species.push(typed(s.get("name")?.as_str()?, s.get("preset")?.as_str()?, [ramp[0], ramp[1], ramp[2], ramp[3]], col3(s.get("bark")?), s.get("height")?.as_f64()? as f32));
    }
    let species: [Species; 3] = species.try_into().ok()?;
    Some(Theme { name, note, species, mix_amount: d.get("mix_amount")?.as_f64()? as f32, grass: col3(d.get("grass")?), accent: col3(d.get("accent")?), haze: col3(d.get("haze")?) })
}

/// How the other themes borrow the natural look of a measured stand: which stand, how much of its look to take (0 keeps
/// the app's own, 1 takes all of it), and how far to mute and darken the species colours. See `assets/natural_looks.json`.
fn look_data() -> &'static serde_json::Value {
    static DATA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../assets/natural_looks.json")).unwrap_or(serde_json::Value::Null))
}

fn look_entry(name: &str) -> Option<&'static serde_json::Value> {
    let d = look_data();
    d.get("themes").and_then(|t| t.get(name)).or_else(|| d.get("default"))
}

/// The settings that make a stand look natural and not sizes or counts: how much crowns differ from each other, how
/// they are shaded, and how the light plays on the leaves.
const LOOK_DEPTH: [&str; 12] = ["gain", "gamma", "crown_tone", "crown_shade", "height_var", "cluster_tone", "crown_var", "depth_strength", "weave", "canopy_ao", "translucency", "lumps"];
const LOOK_LEAF: [&str; 5] = ["mottling", "mottle_size", "tree_lean", "dappled", "variety"];

/// The natural look a plain theme takes from a measured stand, as a sparse settings object.
pub fn natural_look(name: &str) -> Option<serde_json::Value> {
    let e = look_entry(name)?;
    let src = theme_settings(e.get("from")?.as_str()?)?;
    let k = e.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let def = serde_json::to_value(GenParams::default()).ok()?;
    let mut out = serde_json::Map::new();
    for (group, keys) in [("depth", &LOOK_DEPTH[..]), ("leaf", &LOOK_LEAF[..])] {
        let mut o = serde_json::Map::new();
        for key in keys {
            let (Some(d), Some(t)) = (def.get(group).and_then(|g| g.get(*key)).and_then(|v| v.as_f64()), src.get(group).and_then(|g| g.get(*key)).and_then(|v| v.as_f64())) else { continue };
            o.insert((*key).into(), serde_json::json!(d + (t - d) * k));
        }
        out.insert(group.into(), serde_json::Value::Object(o));
    }
    // layout: how open the forest is and how its trees group, taken from the stand at its own strength
    let lay = e.get("layout").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let take = |key: &str| -> Option<(f64, f64)> { Some((def.get(key)?.as_f64()?, src.get(key)?.as_f64()?)) };
    if lay > 0.0 {
        for key in ["gaps", "gap_size", "canopy_patch_size", "shrub_density", "grass_density", "crown_lean"] {
            if let Some((d, t)) = take(key) {
                out.insert(key.into(), serde_json::json!(d + (t - d) * lay));
            }
        }
    }
    // tree size: the stand's crowns at its strength, then scaled for what the theme's trees are. The forest size setting
    // is left to the user.
    let c = e.get("crown").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let (lo, hi) = (def.get("crown_min").and_then(|v| v.as_f64()).unwrap_or(10.0), def.get("crown_max").and_then(|v| v.as_f64()).unwrap_or(17.0));
    let (slo, shi) = (src.get("crown_min").and_then(|v| v.as_f64()).unwrap_or(lo), src.get("crown_max").and_then(|v| v.as_f64()).unwrap_or(hi));
    out.insert("crown_min".into(), serde_json::json!((lo + (slo - lo) * lay) * c));
    out.insert("crown_max".into(), serde_json::json!((hi + (shi - hi) * lay) * c));
    Some(serde_json::Value::Object(out))
}

/// Mute and darken a species ramp the way real canopy is: foliage seen from the air is greyer and darker than the
/// colours a palette starts with.
fn natural_ramp(name: &str, sp: &mut Species) {
    let Some(e) = look_entry(name) else { return };
    let sat = e.get("sat").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    let val = e.get("val").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
    for c in sp.ramp.iter_mut() {
        let grey = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        for v in c.iter_mut() {
            *v = ((grey + (*v - grey) * sat) * val).clamp(0.0, 1.0);
        }
    }
    // colour variation: the stand's own steps of lightness, saturation and hue from dark leaves to lit ones, laid over the
    // theme's colours, so a violet or a crimson canopy varies the way a real one does
    let tone = e.get("tone").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let Some(stand) = e.get("from").and_then(|f| f.as_str()).and_then(|f| satellite_data().get(f)).and_then(|d| d.get("species")).and_then(|s| s.get(0)).and_then(|s| s.get("ramp")).and_then(|r| r.as_array()) else { return };
    if tone <= 0.0 || stand.len() != 4 {
        return;
    }
    let st: Vec<[f32; 3]> = stand.iter().map(|c| { let a: Vec<f32> = c.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32 / 255.0).collect()).unwrap_or_default(); [a.first().copied().unwrap_or(0.0), a.get(1).copied().unwrap_or(0.0), a.get(2).copied().unwrap_or(0.0)] }).collect();
    let hsv_st: Vec<[f32; 3]> = st.iter().map(|c| rgb_to_hsv(*c)).collect();
    let mean_h = hsv_st.iter().map(|h| h[0]).sum::<f32>() / 4.0;
    for (i, c) in sp.ramp.iter_mut().enumerate() {
        let mut h = rgb_to_hsv(*c);
        let s = hsv_st[i];
        h[2] = h[2] + (s[2] - h[2]) * tone;
        h[1] = h[1].min(h[1] + (s[1] * 1.3 + 0.12 - h[1]) * tone);
        h[0] = (h[0] + (s[0] - mean_h) * 0.6 * tone).rem_euclid(1.0);
        *c = hsv_to_rgb(h);
    }
}

fn rgb_to_hsv(c: [f32; 3]) -> [f32; 3] {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let d = mx - mn;
    let h = if d < 1e-6 { 0.0 } else if mx == c[0] { ((c[1] - c[2]) / d).rem_euclid(6.0) } else if mx == c[1] { (c[2] - c[0]) / d + 2.0 } else { (c[0] - c[1]) / d + 4.0 };
    [h / 6.0, if mx <= 1e-6 { 0.0 } else { d / mx }, mx]
}

fn hsv_to_rgb(h: [f32; 3]) -> [f32; 3] {
    let (hh, s, v) = (h[0].rem_euclid(1.0) * 6.0, h[1].clamp(0.0, 1.0), h[2].clamp(0.0, 1.0));
    let (i, f) = (hh.floor(), hh - hh.floor());
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// The structure settings a satellite theme carries, as a sparse settings object, or None for other themes.
pub fn theme_settings(name: &str) -> Option<serde_json::Value> {
    satellite_data().get(name)?.get("gen").cloned()
}

pub fn themes() -> Vec<Theme> {
    let mut v = base_themes();
    v.extend(satellite_list().iter().filter_map(|(n, note)| satellite_theme(n, note)));
    v
}

fn base_themes() -> Vec<Theme> {
    vec![
        Theme {
            name: "Temperate",
            note: "Ordinary green woodland",
            species: [
                themed("Broadleaf", "quaking_aspen", 1.0, [[22, 36, 12], [48, 70, 24], [82, 110, 40], [128, 156, 64]]),
                themed("Conifer", "balsam_fir", 1.3, [[14, 30, 20], [30, 54, 36], [52, 82, 52], [80, 112, 70]]),
                themed("Bush", "black_oak", 0.5, [[22, 36, 12], [48, 70, 24], [82, 110, 40], [128, 156, 64]]),
            ],
            mix_amount: 0.0,
            grass: [70, 86, 40],
            accent: [96, 110, 50],
            haze: [55, 61, 34],
        },
        Theme {
            name: "Autumn",
            note: "Rust and gold leaves",
            species: [
                themed("Autumn Oak", "sassafras", 1.0, [[50, 22, 8], [120, 52, 14], [186, 96, 26], [232, 164, 60]]),
                themed("Golden Birch", "quaking_aspen", 1.1, [[44, 38, 8], [104, 92, 20], [170, 150, 40], [224, 206, 92]]),
                themed("Rust Bush", "black_oak", 0.5, [[56, 26, 10], [110, 54, 18], [170, 96, 30], [212, 150, 62]]),
            ],
            mix_amount: 0.4,
            grass: [96, 84, 40],
            accent: [150, 80, 30],
            haze: [74, 56, 30],
        },
        Theme {
            name: "Hellscape",
            note: "Petrified grey stone trees on ash",
            species: [
                themed("Petrified Oak", "dead_oak", 1.0, [[26, 22, 22], [62, 54, 50], [104, 92, 84], [152, 138, 122]]),
                themed("Stone Spire", "balsam_fir", 1.3, [[20, 18, 20], [46, 40, 42], [82, 72, 70], [124, 110, 102]]),
                themed("Slag Thorn", "black_oak", 0.5, [[30, 24, 22], [70, 56, 48], [114, 90, 72], [162, 130, 98]]),
            ],
            mix_amount: 0.3,
            grass: [72, 64, 60],
            accent: [150, 72, 38],
            haze: [66, 52, 46],
        },
        Theme {
            name: "Ember Waste",
            note: "Charred trees, ember glow",
            species: [
                themed("Cinder Oak", "burnt_oak", 1.0, [[18, 8, 8], [58, 16, 12], [128, 36, 16], [214, 92, 28]]),
                themed("Ashen Spruce", "burnt_pine", 1.3, [[16, 12, 14], [40, 24, 26], [80, 38, 34], [132, 62, 46]]),
                themed("Ember Thorn", "black_oak", 0.5, [[20, 8, 8], [70, 18, 12], [150, 44, 18], [232, 112, 34]]),
            ],
            mix_amount: 0.35,
            grass: [58, 36, 32],
            accent: [222, 92, 30],
            haze: [74, 28, 20],
        },
        Theme {
            name: "Feywild",
            note: "Lush violet, teal and pink",
            species: [
                themed("Fey Oak", "hill_cherry", 1.0, [[30, 22, 74], [74, 52, 136], [126, 92, 190], [196, 156, 232]]),
                themed("Fey Willow", "weeping_willow", 1.0, [[16, 60, 70], [30, 110, 112], [70, 168, 150], [150, 224, 190]]),
                themed("Glow Shrub", "apple", 0.5, [[70, 24, 80], [140, 46, 140], [214, 90, 180], [250, 168, 220]]),
            ],
            mix_amount: 0.4,
            grass: [54, 124, 100],
            accent: [226, 124, 204],
            haze: [40, 70, 96],
        },
        Theme {
            name: "Shadowfell",
            note: "Drained grey and cold purple",
            species: [
                themed("Gloom Oak", "black_tupelo", 1.0, [[14, 14, 20], [36, 34, 50], [66, 62, 88], [108, 102, 130]]),
                themed("Gloom Cypress", "lombardy_poplar", 1.3, [[10, 12, 16], [24, 28, 38], [44, 50, 64], [74, 80, 98]]),
                themed("Gloom Thicket", "black_oak", 0.5, [[14, 14, 18], [34, 32, 44], [60, 56, 76], [96, 90, 116]]),
            ],
            mix_amount: 0.3,
            grass: [44, 46, 54],
            accent: [116, 104, 160],
            haze: [30, 30, 42],
        },
        Theme {
            name: "Frozen",
            note: "Frosted pines and pale birch",
            species: [
                themed("Frost Spruce", "balsam_fir", 1.3, [[24, 44, 52], [60, 92, 100], [130, 164, 170], [214, 232, 236]]),
                themed("Frost Birch", "silver_birch", 1.1, [[50, 62, 70], [100, 118, 128], [170, 188, 196], [228, 240, 244]]),
                themed("Frost Bush", "black_oak", 0.5, [[44, 58, 66], [92, 112, 122], [160, 182, 190], [222, 236, 240]]),
            ],
            mix_amount: 0.3,
            grass: [150, 166, 172],
            accent: [238, 244, 248],
            haze: [120, 140, 150],
        },
        Theme {
            name: "Swamp",
            note: "Murky olive and weeping willow",
            species: [
                themed("Bog Willow", "weeping_willow", 1.0, [[22, 28, 12], [54, 64, 24], [96, 106, 40], [142, 150, 66]]),
                themed("Mire Oak", "black_tupelo", 1.0, [[18, 30, 20], [40, 62, 38], [72, 96, 56], [110, 132, 80]]),
                themed("Reed Bush", "black_oak", 0.5, [[30, 36, 14], [66, 76, 28], [110, 120, 46], [156, 164, 74]]),
            ],
            mix_amount: 0.35,
            grass: [60, 70, 34],
            accent: [88, 72, 40],
            haze: [46, 56, 30],
        },
        Theme {
            name: "Savanna",
            note: "Dry acacia and sage",
            species: [
                themed("Acacia", "acer", 0.8, [[54, 54, 22], [100, 100, 36], [150, 146, 58], [196, 188, 94]]),
                themed("Dry Sage", "quaking_aspen", 1.0, [[56, 58, 34], [100, 104, 62], [148, 150, 96], [196, 194, 138]]),
                themed("Scrub", "black_oak", 0.5, [[60, 56, 26], [110, 100, 40], [160, 146, 62], [204, 190, 100]]),
            ],
            mix_amount: 0.3,
            grass: [136, 120, 66],
            accent: [170, 120, 70],
            haze: [100, 88, 50],
        },
        Theme {
            name: "Jungle",
            note: "Deep saturated green",
            species: [
                themed("Jungle Oak", "black_tupelo", 1.0, [[10, 36, 16], [20, 74, 30], [40, 120, 44], [96, 176, 70]]),
                themed("Jungle Willow", "weeping_willow", 1.0, [[12, 42, 30], [24, 86, 56], [50, 134, 80], [110, 190, 120]]),
                themed("Fern Bush", "fan_palm", 0.5, [[14, 44, 18], [30, 88, 34], [58, 138, 52], [118, 190, 86]]),
            ],
            mix_amount: 0.35,
            grass: [44, 96, 44],
            accent: [116, 176, 60],
            haze: [28, 64, 34],
        },
        Theme {
            name: "Crystal Grove",
            note: "Glassy cyan and violet, glittering ground",
            species: [
                themed("Crystal Cypress", "lombardy_poplar", 1.3, [[30, 60, 84], [70, 130, 160], [140, 205, 224], [230, 248, 255]]),
                themed("Glass Birch", "silver_birch", 1.1, [[60, 50, 110], [110, 100, 180], [170, 160, 230], [226, 222, 255]]),
                themed("Shard Bush", "black_oak", 0.5, [[20, 70, 90], [40, 120, 150], [90, 180, 210], [180, 236, 252]]),
            ],
            mix_amount: 0.35,
            grass: [60, 80, 100],
            accent: [200, 240, 255],
            haze: [70, 100, 130],
        },
        Theme {
            name: "Fungal",
            note: "Giant caps in teal and magenta, glowing spores",
            species: [
                themed("Cap Tree", "acer", 0.8, [[50, 20, 70], [110, 40, 120], [180, 70, 170], [236, 140, 214]]),
                themed("Spore Cluster", "black_oak", 0.5, [[10, 60, 66], [20, 120, 120], [60, 190, 170], [140, 240, 210]]),
                themed("Glow Cap", "black_oak", 0.5, [[70, 26, 60], [150, 50, 110], [220, 100, 170], [255, 190, 226]]),
            ],
            mix_amount: 0.4,
            grass: [34, 24, 40],
            accent: [90, 240, 200],
            haze: [40, 30, 60],
        },
        Theme {
            name: "Celestial",
            note: "Gold and ivory leaves on pale ground",
            species: [
                themed("Gilded Birch", "ginkgo", 1.1, [[90, 70, 20], [170, 140, 50], [230, 200, 100], [255, 240, 180]]),
                themed("Ivory Willow", "weeping_willow", 1.0, [[150, 140, 110], [210, 200, 170], [240, 232, 208], [255, 252, 236]]),
                themed("Halo Bush", "black_oak", 0.5, [[120, 100, 40], [200, 170, 80], [244, 220, 130], [255, 246, 200]]),
            ],
            mix_amount: 0.35,
            grass: [200, 190, 150],
            accent: [255, 224, 120],
            haze: [220, 210, 170],
        },
        Theme {
            name: "Astral",
            note: "Deep indigo trees, starry ground",
            species: [
                themed("Night Cypress", "lombardy_poplar", 1.3, [[8, 12, 40], [20, 30, 80], [44, 60, 130], [90, 110, 190]]),
                themed("Star Willow", "weeping_willow", 1.0, [[14, 20, 60], [34, 50, 110], [70, 90, 170], [140, 160, 230]]),
                themed("Dusk Bush", "black_oak", 0.5, [[10, 14, 44], [28, 38, 90], [56, 74, 140], [110, 130, 200]]),
            ],
            mix_amount: 0.3,
            grass: [18, 22, 50],
            accent: [235, 240, 255],
            haze: [24, 30, 70],
        },
        Theme {
            name: "Blighted",
            note: "Sickly yellow-green and bruised purple",
            species: [
                themed("Blight Willow", "weeping_willow", 1.0, [[40, 44, 10], [90, 100, 20], [150, 160, 40], [200, 210, 80]]),
                themed("Rot Oak", "stag_oak", 1.0, [[40, 20, 50], [80, 40, 96], [126, 70, 140], [170, 110, 180]]),
                themed("Toxic Bush", "black_oak", 0.5, [[30, 50, 10], [70, 110, 20], [130, 180, 40], [190, 240, 80]]),
            ],
            mix_amount: 0.35,
            grass: [58, 60, 30],
            accent: [170, 230, 40],
            haze: [60, 70, 30],
        },
        Theme {
            name: "Bloodwood",
            note: "Lush deep crimson leaves",
            species: [
                themed("Bloodwood", "black_tupelo", 1.0, [[60, 8, 16], [110, 18, 28], [168, 34, 44], [220, 80, 80]]),
                themed("Crimson Birch", "silver_birch", 1.1, [[80, 14, 24], [140, 30, 40], [196, 60, 64], [240, 120, 116]]),
                themed("Redthorn", "black_oak", 0.5, [[50, 10, 18], [100, 20, 30], [150, 34, 44], [200, 64, 72]]),
            ],
            mix_amount: 0.35,
            grass: [30, 40, 28],
            accent: [150, 30, 36],
            haze: [40, 30, 28],
        },
        Theme {
            name: "Silverwood",
            note: "Silver-blue leaves, pale cool ground",
            species: [
                themed("Silverleaf", "silver_birch", 1.1, [[40, 56, 70], [86, 110, 130], [150, 176, 196], [220, 236, 246]]),
                themed("Moon Willow", "weeping_willow", 1.0, [[50, 66, 84], [100, 128, 150], [164, 192, 212], [226, 240, 250]]),
                themed("Silver Bush", "black_oak", 0.5, [[40, 56, 66], [84, 108, 124], [140, 166, 184], [206, 226, 238]]),
            ],
            mix_amount: 0.35,
            grass: [86, 104, 116],
            accent: [226, 240, 252],
            haze: [70, 90, 110],
        },
        Theme {
            name: "Mediterranean",
            note: "Olive and stone pine on dry tan ground",
            species: [
                themed("Stone Pine", "small_pine", 1.3, [[34, 48, 28], [66, 84, 46], [108, 124, 74], [156, 168, 110]]),
                themed("Olive", "sphere_tree", 1.0, [[60, 70, 50], [104, 118, 84], [150, 164, 122], [196, 206, 166]]),
                themed("Maquis", "black_oak", 0.5, [[50, 58, 34], [92, 104, 56], [140, 150, 84], [184, 190, 120]]),
            ],
            mix_amount: 0.3,
            grass: [150, 130, 84],
            accent: [190, 160, 100],
            haze: [110, 100, 70],
        },
        Theme {
            name: "Taiga",
            note: "Dark boreal spruce, lichen on mossy ground",
            species: [
                themed("Boreal Spruce", "balsam_fir", 1.3, [[10, 26, 24], [24, 52, 46], [44, 82, 70], [74, 114, 96]]),
                themed("Scots Pine", "small_pine", 1.3, [[20, 38, 26], [44, 70, 44], [76, 106, 66], [112, 140, 92]]),
                themed("Bilberry", "black_oak", 0.5, [[30, 44, 28], [64, 84, 44], [104, 124, 66], [150, 164, 100]]),
            ],
            mix_amount: 0.3,
            grass: [78, 74, 56],
            accent: [170, 180, 150],
            haze: [60, 70, 60],
        },
        Theme {
            name: "Cherry Blossom",
            note: "Pink and white blossom, petals on green",
            species: [
                themed("Sakura", "hill_cherry", 1.0, [[120, 60, 80], [200, 110, 140], [240, 170, 196], [255, 222, 232]]),
                themed("White Blossom", "hill_cherry", 1.1, [[150, 130, 140], [220, 200, 210], [246, 236, 240], [255, 250, 252]]),
                themed("Azalea", "apple", 0.5, [[90, 40, 70], [170, 70, 120], [226, 120, 170], [252, 190, 214]]),
            ],
            mix_amount: 0.35,
            grass: [90, 130, 70],
            accent: [255, 210, 225],
            haze: [120, 100, 100],
        },
        Theme {
            name: "Redwood",
            note: "Very dark, wet temperate rainforest",
            species: [
                themed("Redwood", "douglas_fir", 1.3, [[6, 24, 14], [14, 50, 28], [28, 84, 44], [56, 120, 70]]),
                themed("Hemlock", "douglas_fir", 1.3, [[10, 34, 24], [24, 64, 44], [46, 98, 66], [84, 136, 96]]),
                themed("Sword Fern", "fan_palm", 0.5, [[10, 40, 20], [22, 82, 34], [44, 128, 52], [90, 176, 84]]),
            ],
            mix_amount: 0.3,
            grass: [40, 52, 30],
            accent: [90, 120, 56],
            haze: [20, 44, 30],
        },
        Theme {
            name: "Badlands",
            note: "Sparse scrub on red rock",
            species: [
                themed("Juniper", "small_pine", 1.0, [[50, 60, 34], [90, 104, 56], [132, 146, 84], [176, 186, 120]]),
                themed("Mesquite", "acer", 0.8, [[60, 56, 28], [106, 98, 44], [150, 140, 70], [194, 182, 110]]),
                themed("Sagebrush", "black_oak", 0.5, [[70, 76, 56], [116, 124, 92], [160, 168, 130], [204, 210, 170]]),
            ],
            mix_amount: 0.3,
            grass: [160, 92, 60],
            accent: [200, 130, 86],
            haze: [130, 80, 56],
        },
        Theme {
            name: "Volcanic",
            note: "Tough green trees on black ash with glowing cracks",
            species: [
                themed("Ash Pine", "small_pine", 1.3, [[24, 40, 24], [50, 74, 38], [86, 112, 58], [126, 150, 86]]),
                themed("Scorch Sage", "quaking_aspen", 1.0, [[50, 44, 34], [90, 80, 56], [132, 116, 80], [176, 160, 112]]),
                themed("Lava Scrub", "black_oak", 0.5, [[40, 34, 26], [84, 66, 40], [130, 96, 52], [176, 130, 70]]),
            ],
            mix_amount: 0.3,
            grass: [40, 36, 36],
            accent: [255, 110, 30],
            haze: [54, 40, 36],
        },
        Theme {
            name: "Alpine",
            note: "Cold conifers, bare rock and snow patches",
            species: [
                themed("Mountain Spruce", "balsam_fir", 1.3, [[16, 34, 36], [34, 64, 62], [64, 98, 94], [104, 138, 130]]),
                themed("Dwarf Juniper", "small_pine", 1.3, [[26, 44, 40], [54, 80, 70], [90, 116, 102], [132, 156, 140]]),
                themed("Alpine Heath", "black_oak", 0.5, [[50, 60, 50], [96, 110, 92], [142, 156, 134], [188, 200, 178]]),
            ],
            mix_amount: 0.3,
            grass: [128, 130, 128],
            accent: [240, 244, 246],
            haze: [130, 146, 156],
        },
        Theme {
            name: "Bamboo",
            note: "Fresh yellow-green eastern forest",
            species: [
                themed("Bamboo Grove", "bamboo", 1.0, [[30, 60, 18], [70, 120, 34], [130, 180, 60], [190, 226, 100]]),
                themed("Tea Birch", "silver_birch", 1.1, [[44, 76, 26], [96, 140, 50], [150, 192, 78], [204, 232, 120]]),
                themed("Dwarf Bamboo", "bamboo", 0.5, [[36, 70, 22], [84, 130, 40], [140, 190, 64], [196, 236, 106]]),
            ],
            mix_amount: 0.3,
            grass: [92, 130, 50],
            accent: [150, 180, 70],
            haze: [60, 90, 44],
        },
        Theme {
            name: "Mangrove",
            note: "Teal-green tidal forest on wet ground",
            species: [
                themed("Mangrove", "weeping_willow_o", 1.0, [[14, 50, 40], [30, 90, 66], [60, 134, 92], [110, 180, 130]]),
                themed("Buttonwood", "black_tupelo", 1.0, [[24, 56, 34], [52, 96, 56], [90, 136, 84], [134, 176, 120]]),
                themed("Salt Shrub", "black_oak", 0.5, [[30, 60, 50], [66, 100, 80], [110, 140, 112], [156, 184, 150]]),
            ],
            mix_amount: 0.35,
            grass: [86, 92, 60],
            accent: [64, 120, 110],
            haze: [50, 80, 64],
        },
        Theme {
            name: "Dead Forest",
            note: "Standing dead oaks and stag-headed trees on grey ground",
            species: [
                typed("Dead Oak", "dead_oak", [[44, 42, 38], [78, 74, 66], [112, 108, 98], [150, 146, 134]], [150, 142, 128], 1.0),
                typed("Stag Oak", "stag_oak", [[34, 40, 16], [66, 78, 28], [104, 116, 44], [146, 156, 70]], [118, 106, 90], 1.0),
                typed("Dry Scrub", "bare_oak", [[44, 42, 38], [78, 74, 66], [112, 108, 98], [150, 146, 134]], [96, 82, 66], 0.5),
            ],
            mix_amount: 0.3,
            grass: [64, 60, 48],
            accent: [84, 76, 56],
            haze: [52, 50, 40],
        },
        Theme {
            name: "Bare Winter",
            note: "Leafless trees and pale birches in winter",
            species: [
                typed("Winter Oak", "bare_oak", [[44, 42, 38], [78, 74, 66], [112, 108, 98], [150, 146, 134]], [86, 68, 52], 1.0),
                typed("Winter Birch", "dead_birch", [[44, 42, 38], [78, 74, 66], [112, 108, 98], [150, 146, 134]], [196, 192, 178], 1.0),
                typed("Bare Brush", "bare_oak", [[44, 42, 38], [78, 74, 66], [112, 108, 98], [150, 146, 134]], [76, 62, 48], 0.5),
            ],
            mix_amount: 0.3,
            grass: [112, 114, 104],
            accent: [140, 142, 134],
            haze: [96, 100, 100],
        },
        Theme {
            name: "Burnt Forest",
            note: "Charred trunks and snapped limbs on black ground",
            species: [
                typed("Burnt Oak", "burnt_oak", [[30, 28, 26], [60, 56, 52], [96, 90, 84], [140, 130, 120]], [92, 86, 80], 1.0),
                typed("Burnt Pine", "burnt_pine", [[30, 28, 26], [60, 56, 52], [96, 90, 84], [140, 130, 120]], [84, 78, 74], 1.1),
                typed("Charred Brush", "burnt_oak", [[30, 28, 26], [60, 56, 52], [96, 90, 84], [140, 130, 120]], [76, 70, 66], 0.5),
            ],
            mix_amount: 0.4,
            grass: [38, 34, 32],
            accent: [64, 52, 44],
            haze: [58, 44, 38],
        },
        Theme {
            name: "Baobab Plains",
            note: "Fat baobabs and flat acacias on dry grass",
            species: [
                typed("Baobab", "baobab", [[34, 56, 18], [64, 96, 30], [104, 138, 48], [150, 178, 80]], [140, 120, 98], 1.0),
                themed("Acacia", "acer", 0.8, [[40, 62, 22], [78, 106, 36], [124, 150, 56], [170, 190, 92]]),
                themed("Dry Scrub", "black_oak", 0.5, [[60, 66, 28], [110, 112, 44], [160, 154, 66], [206, 196, 104]]),
            ],
            mix_amount: 0.35,
            grass: [150, 130, 74],
            accent: [176, 150, 84],
            haze: [120, 104, 66],
        },
        Theme {
            name: "Banyan Grove",
            note: "Huge spreading figs over shaded ground",
            species: [
                typed("Banyan", "banyan", [[10, 36, 16], [22, 70, 30], [42, 110, 48], [80, 152, 76]], [98, 86, 72], 1.1),
                themed("Fig", "black_tupelo", 1.0, [[18, 52, 22], [38, 88, 38], [70, 126, 56], [114, 166, 88]]),
                themed("Fern Bush", "fan_palm", 0.5, [[20, 60, 26], [44, 100, 40], [80, 140, 60], [128, 180, 90]]),
            ],
            mix_amount: 0.3,
            grass: [54, 92, 44],
            accent: [80, 120, 56],
            haze: [40, 70, 40],
        },
        Theme {
            name: "Stilt Mangrove",
            note: "Mangroves standing on arching roots in wet ground",
            species: [
                typed("Stilt Mangrove", "mangrove", [[14, 50, 40], [30, 90, 66], [60, 134, 92], [110, 180, 130]], [96, 76, 58], 0.9),
                themed("Buttonwood", "black_tupelo", 1.0, [[24, 56, 34], [52, 96, 56], [90, 136, 84], [134, 176, 120]]),
                themed("Salt Shrub", "black_oak", 0.5, [[30, 60, 50], [66, 100, 80], [110, 140, 112], [156, 184, 150]]),
            ],
            mix_amount: 0.3,
            grass: [70, 84, 60],
            accent: [64, 110, 100],
            haze: [46, 74, 62],
        },
        Theme {
            name: "Dragon Blood Isle",
            note: "Umbrella trees with blue-green blades on dry soil",
            species: [
                typed("Dragon Blood Tree", "dragon_blood", [[12, 52, 56], [26, 96, 96], [56, 142, 134], [110, 190, 176]], [122, 108, 100], 0.9),
                themed("Fern Palm", "fan_palm", 1.0, [[20, 60, 34], [44, 100, 52], [80, 140, 74], [128, 180, 110]]),
                themed("Spiky Shrub", "black_oak", 0.5, [[16, 54, 54], [34, 94, 90], [66, 136, 124], [116, 182, 164]]),
            ],
            mix_amount: 0.25,
            grass: [150, 128, 92],
            accent: [180, 150, 100],
            haze: [110, 96, 70],
        },
        Theme {
            name: "Mojave",
            note: "Joshua trees and sparse scrub on pale sand",
            species: [
                typed("Joshua Tree", "joshua_tree", [[44, 52, 28], [82, 96, 44], [122, 138, 66], [166, 180, 98]], [132, 106, 80], 0.8),
                themed("Yucca Scrub", "quaking_aspen", 1.0, [[54, 62, 34], [96, 108, 56], [140, 152, 82], [184, 194, 118]]),
                themed("Creosote", "black_oak", 0.5, [[50, 60, 34], [88, 102, 54], [128, 142, 80], [172, 182, 116]]),
            ],
            mix_amount: 0.15,
            grass: [182, 156, 110],
            accent: [198, 172, 126],
            haze: [150, 130, 96],
        },
        Theme {
            name: "Eucalyptus Woodland",
            note: "Silver-green gums with open crowns and dry grass",
            species: [
                typed("Gum Tree", "eucalyptus", [[30, 50, 44], [60, 90, 76], [104, 136, 114], [156, 180, 156]], [168, 156, 138], 1.2),
                themed("Wattle", "acer", 0.8, [[56, 70, 28], [104, 118, 42], [156, 164, 62], [206, 208, 98]]),
                themed("Bottlebrush", "black_oak", 0.5, [[34, 56, 40], [66, 96, 64], [108, 138, 92], [156, 180, 128]]),
            ],
            mix_amount: 0.3,
            grass: [120, 118, 70],
            accent: [150, 140, 80],
            haze: [90, 92, 60],
        },
        Theme {
            name: "Golden Ginkgo",
            note: "Ginkgos and maples in gold and red",
            species: [
                typed("Ginkgo", "ginkgo", [[88, 64, 8], [170, 130, 16], [226, 184, 40], [248, 226, 110]], [98, 82, 66], 1.2),
                typed("Red Maple", "acer", [[60, 14, 10], [130, 30, 18], [196, 62, 26], [236, 122, 52]], [88, 66, 52], 1.0),
                themed("Gold Shrub", "black_oak", 0.5, [[84, 60, 10], [150, 112, 20], [208, 164, 44], [240, 210, 100]]),
            ],
            mix_amount: 0.35,
            grass: [104, 98, 52],
            accent: [150, 130, 50],
            haze: [90, 76, 40],
        },
        Theme {
            name: "Windswept Heath",
            note: "Bent krummholz and pines over heather",
            species: [
                typed("Krummholz", "krummholz", [[16, 36, 28], [32, 64, 46], [56, 96, 64], [88, 128, 92]], [78, 66, 54], 0.5),
                themed("Scots Pine", "small_pine", 1.3, [[18, 40, 26], [38, 70, 44], [64, 102, 64], [100, 138, 92]]),
                themed("Heather", "black_oak", 0.5, [[56, 36, 60], [96, 64, 100], [140, 100, 140], [184, 150, 176]]),
            ],
            mix_amount: 0.25,
            grass: [96, 96, 70],
            accent: [128, 100, 120],
            haze: [76, 80, 64],
        },
        Theme {
            name: "Juniper Highlands",
            note: "Round, shaggy junipers and pinyon on dry high ground",
            species: [
                themed("Juniper", "juniper", 0.8, [[34, 50, 44], [62, 86, 72], [96, 120, 100], [140, 160, 136]]),
                themed("Pinyon", "small_pine", 1.2, [[30, 46, 34], [56, 80, 56], [88, 112, 80], [130, 150, 112]]),
                themed("Sagebrush", "black_oak", 0.4, [[70, 80, 62], [110, 122, 98], [150, 160, 130], [190, 198, 166]]),
            ],
            mix_amount: 0.3,
            grass: [138, 124, 92],
            accent: [160, 140, 100],
            haze: [100, 92, 66],
        },
        Theme {
            name: "Spruce Spires",
            note: "Dark, steep spruce with fir between",
            species: [
                themed("Spruce", "spruce", 1.5, [[10, 30, 26], [24, 58, 48], [46, 92, 72], [84, 128, 104]]),
                themed("Fir", "balsam_fir", 1.3, [[14, 34, 28], [30, 62, 50], [52, 94, 74], [88, 128, 104]]),
                themed("Dwarf Juniper", "juniper", 0.4, [[30, 48, 42], [56, 80, 70], [88, 112, 98], [128, 150, 134]]),
            ],
            mix_amount: 0.25,
            grass: [60, 74, 60],
            accent: [78, 92, 70],
            haze: [40, 52, 44],
        },
        Theme {
            name: "Cypress Bayou",
            note: "Russet bald cypress over dark water and tupelo",
            species: [
                themed("Bald Cypress", "bald_cypress", 1.3, [[44, 40, 16], [92, 84, 30], [150, 120, 50], [196, 150, 70]]),
                themed("Water Tupelo", "black_tupelo", 1.0, [[22, 36, 20], [48, 70, 36], [84, 108, 56], [128, 150, 84]]),
                themed("Buttonbush", "black_oak", 0.5, [[30, 44, 24], [62, 84, 42], [100, 122, 64], [140, 160, 94]]),
            ],
            mix_amount: 0.35,
            grass: [70, 78, 48],
            accent: [96, 80, 48],
            haze: [52, 58, 34],
        },
        Theme {
            name: "Live Oak Hammock",
            note: "Wide, spreading live oaks with cabbage palms",
            species: [
                themed("Live Oak", "live_oak", 1.1, [[22, 34, 22], [44, 66, 42], [78, 104, 70], [124, 148, 106]]),
                themed("Cabbage Palm", "palm", 1.0, [[30, 52, 28], [60, 92, 44], [96, 132, 66], [140, 170, 100]]),
                themed("Wax Myrtle", "black_oak", 0.5, [[28, 44, 24], [58, 84, 40], [94, 120, 62], [134, 160, 92]]),
            ],
            mix_amount: 0.2,
            grass: [84, 96, 52],
            accent: [112, 118, 70],
            haze: [60, 68, 40],
        },
        Theme {
            name: "Acacia Plains",
            note: "Flat-topped acacia over golden grass",
            species: [
                themed("Umbrella Acacia", "umbrella_acacia", 0.9, [[54, 56, 24], [98, 100, 38], [150, 146, 60], [196, 188, 96]]),
                themed("Baobab", "baobab", 1.3, [[60, 66, 30], [104, 112, 46], [150, 158, 70], [190, 196, 104]]),
                themed("Thorn Scrub", "black_oak", 0.4, [[62, 58, 28], [112, 104, 44], [160, 148, 66], [204, 190, 104]]),
            ],
            mix_amount: 0.1,
            grass: [168, 148, 86],
            accent: [190, 160, 96],
            haze: [130, 112, 66],
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeMode {
    Realistic,
    Blurred,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GenParams {
    pub seed: u64,
    /// Fit the whole generation, fall-off and stray trees included, inside the drawn area.
    /// Painted trees are exempt.
    pub contain: bool,
    /// Size of everything generated: crowns, leaves, shrubs, stray trees, shadows. 1 is normal.
    pub scale: f32,
    /// How much of the measured pull-in to apply. 1 fits exactly, more leaves a margin, 0 does not pull in.
    pub contain_fit: f32,
    /// Trim anything that still crosses the line after pulling in.
    pub contain_clip: bool,
    /// Also keep shadows inside. Off lets shadows fall outside, like a real forest.
    pub contain_shadows: bool,
    pub edge_mode: EdgeMode,
    /// Blurred mode feather width, in depth units.
    pub feather: f32,
    /// Blur radius applied to the drawn shape before depth is computed.
    pub shape_softness: f32,
    /// How far noise pushes the outline in and out.
    pub edge_warp: f32,
    pub edge_noise: f32,

    pub canopy_cover: f32,
    pub canopy_patch_size: f32,
    /// How strongly trees gather in clusters with open ground between them, 0 (even scatter) to 1.
    pub clump: f32,
    /// Typical radius of one cluster of trees, in generated map pixels.
    pub clump_size: f32,
    /// Depth at which canopy reaches full coverage. Larger means a wider thinning band.
    pub canopy_edge_depth: f32,
    pub crown_min: f32,
    pub crown_max: f32,

    pub shrub_density: f32,
    pub grass_reach: f32,
    pub grass_density: f32,
    pub lone_groups: f32,
    pub lone_reach: f32,
    pub pebbles: f32,
    /// How many mushroom clusters grow on open ground, 0 for none.
    #[serde(default)]
    pub mushrooms: f32,
    /// Share of each kind among the mushrooms, in `mushrooms::KINDS` order.
    #[serde(default)]
    pub mushroom_mix: [f32; 5],
    /// Share of the canopy trees that are giant mushrooms the size of trees, 0 for none.
    #[serde(default)]
    pub giant_mushrooms: f32,
    /// How far crowns lean away from crowded neighbours toward open space, 0 (none) to 1.
    pub crown_lean: f32,
    /// Stand age in years, 0 for the forest as drawn. Crowns grow along each tree type's curve up to its real largest size.
    pub age: f32,
    /// Set once the age has been worked into the settings, so it is never applied twice.
    #[serde(skip)]
    pub age_applied: bool,
    /// Crown shyness: thin gaps where neighbouring crowns of similar height meet.
    pub crown_shyness: bool,
    /// Gap width in map pixels (smaller crowns get proportionally thinner gaps).
    pub shyness_gap: f32,
    /// Only the giant mushrooms are shy.
    pub shyness_giants_only: bool,
    pub haze_reach: f32,
    pub haze_opacity: f32,

    pub pattern: Pattern,
    pub pattern_strength: f32,
    /// Feature size of the pattern in pixels.
    pub pattern_scale: f32,
    /// Width of branches in Branching, 0..1.
    pub branch_width: f32,
    /// Treefall gaps cut into the canopy only.
    pub gaps: f32,
    pub gap_size: f32,
    /// Lone trees scattered inside the patch too, not just at the fringe (savanna, open woodland).
    pub interior_lone: f32,

    /// Fallback switches: trees from the generator, and trees from the brush.
    pub procedural_trees: bool,
    pub painted_trees: bool,
    /// Second species mixed into generated canopy in clumps.
    pub mix_species: u8,
    pub mix_amount: f32,
    pub mix_scale: f32,
    /// Species of generated canopy and lone trees, and of generated shrubs and pebbles.
    pub main_species: u8,
    pub shrub_species: u8,
    /// Ground cover and haze colours, 0..1 per channel. Themes change these.
    pub grass_color: [f32; 3],
    /// Patches and flecks of this colour break up the ground cover: embers, glow, snow, leaf litter.
    pub grass_accent: [f32; 3],
    pub haze_color: [f32; 3],

    pub leaf: LeafStyle,
    pub depth: DepthStyle,
    pub snow: SnowStyle,
    pub fire: FireStyle,
}

/// Large-scale light, shade and tone for the canopy. 1.0 = 100 percent.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DepthStyle {
    /// Broad lighter and darker patches across crowns.
    pub crown_tone: f32,
    pub crown_tone_size: f32,
    /// Crown tops lit, gaps between crowns dark.
    pub crown_shade: f32,
    /// Distance, in map pixels, over which a crown counts as higher or lower than its surroundings.
    pub shade_radius: f32,
    /// Random crown height, so taller crowns shade lower ones.
    pub height_var: f32,
    /// Colour shared by whole leaf clumps.
    pub cluster_tone: f32,
    /// How much low and deep parts darken.
    pub depth_strength: f32,
    /// Tone curve on the final shade. Below 1 lifts the shadows and midtones.
    pub gamma: f32,
    /// Overall brightness of the canopy.
    pub gain: f32,
    /// How different neighbouring crowns look, so clusters follow crown shapes.
    pub crown_var: f32,
    /// Canopy weave: 0 is off. Crowns of similar height interleave at their edges instead of one dome hiding
    /// the next, as neighbouring crowns do in aerial imagery.
    pub weave: f32,
    /// Ambient occlusion: how much darker the places between and below crowns get, from the shape of the canopy
    /// surface. 0 is off.
    pub canopy_ao: f32,
    /// Light shining through thin leaves on the side of a crown away from the sun, as a glow. 0 is off.
    pub translucency: f32,
    /// How much the lumps of a crown (clumps of leaves and branches, not only its overall dome) change how the
    /// light falls on it. 0 is off.
    pub lumps: f32,
    /// Canopy smoothing: 0 is off. Flattens each crown into a lobe of one tone with crisp edges, as in aerial
    /// photographs. 1 reaches about 60 percent of a crown radius.
    pub smooth: f32,
}

impl Default for DepthStyle {
    /// The user's picked settings.
    fn default() -> Self {
        DepthStyle { crown_tone: 0.1, crown_tone_size: 28.0, crown_shade: 0.0, shade_radius: 14.0, height_var: 1.5, cluster_tone: 1.0, depth_strength: 1.0, gamma: 1.03, gain: 1.2, crown_var: 0.35, weave: 0.0, canopy_ao: 0.0, translucency: 0.0, lumps: 0.0, smooth: 0.0 }
    }
}

impl DepthStyle {
    /// Tuned so the brightness spread across crowns matches the CG2A files.
    pub fn cg2a_match() -> Self {
        DepthStyle { crown_tone: 0.5, crown_tone_size: 48.0, crown_shade: 0.35, shade_radius: 14.0, height_var: 0.5, cluster_tone: 1.0, depth_strength: 1.0, gamma: 0.8, gain: 1.2, crown_var: 0.35, weave: 0.0, canopy_ao: 0.0, translucency: 0.0, lumps: 0.0, smooth: 0.0 }
    }
}

/// Where the snow lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnowMode {
    /// Evenly across the whole forest.
    Everywhere,
    /// Heaviest on the outer fringe, thinning toward the middle.
    Edge,
    /// A snow line across the map: heavy on one side, none on the other.
    Gradient,
}

impl SnowMode {
    pub const ALL: [SnowMode; 3] = [SnowMode::Everywhere, SnowMode::Edge, SnowMode::Gradient];
    pub fn name(self) -> &'static str {
        match self {
            SnowMode::Everywhere => "Everywhere",
            SnowMode::Edge => "Outer edge",
            SnowMode::Gradient => "Snow line across the map",
        }
    }
}

/// Where a fire is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FireMode {
    /// Patches across the whole forest.
    Everywhere,
    /// Burning in from the outer fringe.
    Edge,
    /// A fire front across the map: burnt out on one side, burning in the middle, untouched beyond.
    Front,
}

impl FireMode {
    pub const ALL: [FireMode; 3] = [FireMode::Everywhere, FireMode::Edge, FireMode::Front];
    pub fn name(self) -> &'static str {
        match self {
            FireMode::Everywhere => "Patches everywhere",
            FireMode::Edge => "Outer edge",
            FireMode::Front => "Fire front across the map",
        }
    }
}

/// A forest fire: scorched and charred crowns, flames, ash and embers. 0 amount is off.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FireStyle {
    pub amount: f32,
    pub mode: FireMode,
    /// Share of the burnt area that is still in flames, 0..1.
    pub flames: f32,
    /// How black the burnt crowns and ground get, 0..1.
    pub char: f32,
    /// Glowing embers on the ground, 0..1.
    pub embers: f32,
    /// Warm light the flames throw on their surroundings, 0..1.
    pub glow: f32,
    /// Fringe width in map pixels for the outer edge mode.
    pub reach: f32,
    /// Direction the fire comes from, in degrees (270 is the top of the map).
    pub angle: f32,
    pub patchiness: f32,
}

impl Default for FireStyle {
    fn default() -> Self {
        FireStyle { amount: 0.0, mode: FireMode::Everywhere, flames: 0.5, char: 0.8, embers: 0.5, glow: 0.6, reach: 220.0, angle: 270.0, patchiness: 0.6 }
    }
}

/// Snow on crowns and ground. 0 amount is off.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SnowStyle {
    pub amount: f32,
    pub mode: SnowMode,
    /// How much of the ground cover turns white.
    pub ground: f32,
    /// 1 keeps snow on the upward facing tops of crowns, 0 covers crowns evenly.
    pub tops: f32,
    /// Fringe width in map pixels for the outer edge mode.
    pub reach: f32,
    /// Direction the snow line rises toward, in degrees (270 is the top of the map).
    pub angle: f32,
    /// Patchiness of the snow cover.
    pub patchiness: f32,
    pub tint: [f32; 3],
}

impl Default for SnowStyle {
    fn default() -> Self {
        SnowStyle {
            amount: 0.0,
            mode: SnowMode::Everywhere,
            ground: 0.8,
            tops: 0.6,
            reach: 140.0,
            angle: 270.0,
            patchiness: 0.5,
            tint: [0.93, 0.96, 1.0],
        }
    }
}

/// Leaf colour settings for the generated trees. 1.0 = 100 percent.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LeafStyle {
    /// Each leaf a little lighter, darker, yellower or bluer.
    pub variety: f32,
    /// Broad lighter and darker patches.
    pub mottling: f32,
    /// Mottling patch size in map pixels.
    pub mottle_size: f32,
    /// Sun-facing clumps warm up.
    pub dappled: f32,
    /// Each tree leans lime or blue-green.
    pub tree_lean: f32,
}

impl Default for LeafStyle {
    /// The user's picked settings.
    fn default() -> Self {
        LeafStyle { variety: 2.0, mottling: 1.5, mottle_size: 14.0, dappled: 0.4, tree_lean: 0.5 }
    }
}

impl Default for GenParams {
    fn default() -> Self {
        GenParams {
            seed: 1,
            contain: true,
            scale: 1.0,
            contain_fit: 1.3,
            contain_clip: true,
            contain_shadows: true,
            edge_mode: EdgeMode::Realistic,
            feather: 0.35,
            shape_softness: 60.0,
            edge_warp: 90.0,
            edge_noise: 0.18,
            canopy_cover: 0.82,
            canopy_patch_size: 160.0,
            clump: 0.0,
            clump_size: 70.0,
            canopy_edge_depth: 0.45,
            crown_min: 10.0,
            crown_max: 17.0,
            shrub_density: 1.0,
            grass_reach: 0.7,
            grass_density: 1.0,
            lone_groups: 1.0,
            lone_reach: 0.45,
            pebbles: 1.0,
            mushrooms: 0.0,
            mushroom_mix: [0.0; 5],
            giant_mushrooms: 0.0,
            crown_lean: 0.0,
            age: 0.0,
            age_applied: false,
            crown_shyness: false,
            shyness_gap: 1.6,
            shyness_giants_only: false,
            haze_reach: 0.75,
            haze_opacity: 0.26,
            pattern: Pattern::Fragmented,
            pattern_strength: 0.6,
            pattern_scale: 300.0,
            branch_width: 0.35,
            gaps: 0.0,
            gap_size: 30.0,
            interior_lone: 0.0,
            procedural_trees: true,
            painted_trees: true,
            mix_species: 2,
            mix_amount: 0.0,
            mix_scale: 220.0,
            main_species: 0,
            shrub_species: 0,
            grass_color: [70.0 / 255.0, 86.0 / 255.0, 40.0 / 255.0],
            grass_accent: [96.0 / 255.0, 110.0 / 255.0, 50.0 / 255.0],
            haze_color: [55.0 / 255.0, 61.0 / 255.0, 34.0 / 255.0],
            leaf: LeafStyle::default(),
            snow: SnowStyle::default(),
            fire: FireStyle::default(),
            depth: DepthStyle::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Light {
    /// Direction shadows fall, degrees. 0 = right, 90 = down. CG2A uses 45.
    pub shadow_angle: f32,
    /// Sun elevation in degrees, used for crown shading.
    pub elevation: f32,
    pub shadow_length: f32,
    pub shadow_softness: f32,
    pub shadows: bool,
    /// Remove directional crown shading, keep only ambient occlusion.
    pub flat: bool,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            shadow_angle: 45.0,
            elevation: 39.0,
            shadow_length: 1.0,
            shadow_softness: 1.0,
            shadows: true,
            flat: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerGrade {
    pub visible: bool,
    pub opacity: f32,
    /// Hue rotation in degrees.
    pub hue: f32,
    pub saturation: f32,
    pub brightness: f32,
    pub contrast: f32,
    /// Multiplies the layer colour, for tinting.
    pub tint: [f32; 3],
    /// Whether this layer shows shadows cast onto it.
    pub receive_shadow: bool,
}

impl Default for LayerGrade {
    fn default() -> Self {
        LayerGrade {
            visible: true,
            opacity: 1.0,
            hue: 0.0,
            saturation: 1.0,
            brightness: 1.0,
            contrast: 1.0,
            tint: [1.0, 1.0, 1.0],
            receive_shadow: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Grading {
    pub global: LayerGrade,
    pub layers: Vec<LayerGrade>,
    pub shadow_opacity: f32,
    pub shadow_color: [f32; 3],
}

impl Default for Grading {
    fn default() -> Self {
        Grading {
            global: LayerGrade::default(),
            layers: vec![LayerGrade::default(); crate::engine::LayerKind::ALL.len()],
            shadow_opacity: 0.6,
            shadow_color: [30.0 / 255.0, 24.0 / 255.0, 11.0 / 255.0],
        }
    }
}

/// A picture to draw on, such as the map the forest is for. It is shown under the forest in the editor and
/// never goes into an export.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BackgroundImage {
    pub path: String,
    pub opacity: f32,
    pub visible: bool,
    /// Map pixels per metre, from calibrating on two points a known distance apart. 0 when not set.
    #[serde(default)]
    pub px_per_m: f32,
    /// Draw a scale bar, and a grid every 100 metres, over the map in the editor.
    #[serde(default)]
    pub scale_bar: bool,
    #[serde(default)]
    pub grid: bool,
    /// Keep the picture from being moved or clicked by accident.
    #[serde(default)]
    pub locked: bool,
}

impl Default for BackgroundImage {
    fn default() -> Self {
        BackgroundImage { path: String::new(), opacity: 1.0, visible: true, px_per_m: 0.0, scale_bar: false, grid: false, locked: true }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub map_w: u32,
    pub map_h: u32,
    pub shapes: Vec<Shape>,
    pub gen: GenParams,
    pub light: Light,
    pub grading: Grading,
    pub species: Vec<Species>,
    /// Trees placed with the brush.
    pub painted: Vec<TreeRec>,
    /// Circles [x, y, r] where generated trees are removed.
    pub erased: Vec<[f32; 3]>,
    /// A picture under the forest in the editor, not part of the render.
    pub background_image: Option<BackgroundImage>,
    /// Groups of zones, and the settings they change.
    #[serde(alias = "layers")]
    pub groups: Vec<Group>,
}

impl Project {
    /// The zones in the order they are painted into the shape mask: forests first, then every clearing, so a
    /// clearing or a no-trees mask holds whenever it was drawn.
    pub fn shapes_clearings_last(&self) -> impl Iterator<Item = &Shape> {
        self.shapes.iter().filter(|s| s.op == ShapeOp::Add).chain(self.shapes.iter().filter(|s| s.op != ShapeOp::Add))
    }

    /// Make sure the mushroom species exist, adding any that a saved project lacks.
    pub fn ensure_mushroom_species(&mut self) {
        for m in mushroom_species() {
            let full = self.species.len() >= 255;
            match self.species.iter_mut().find(|s| s.preset == m.preset) {
                // the built-in mushroom species follow the baked sprites' colours
                Some(s) if s.name == m.name => {
                    s.ramp = m.ramp;
                    s.bark = m.bark;
                }
                Some(_) => {}
                None if !full => self.species.push(m),
                None => {}
            }
        }
    }

    /// Switch the forest to a colour theme: add its species if they are missing, then select them.
    /// Use a theme's species and colours. A slot (main, mix, shrub) that was a procedural tree keeps its tree
    /// type and takes the theme's colours, so any tree type works in any theme.
    pub fn apply_theme(&mut self, t: &Theme) {
        self.ensure_mushroom_species();
        let g = &self.gen;
        let kept: Vec<Option<String>> = [g.main_species, g.mix_species, g.shrub_species]
            .iter()
            .map(|&i| self.species.get(i as usize).and_then(|s| if s.procedural { s.tree_type().map(str::to_string) } else { None }))
            .collect();
        let mut idx = [0u8; 3];
        for (k, sp) in t.species.iter().enumerate() {
            let mut sp = sp.clone();
            if !is_satellite_theme(t.name) {
                natural_ramp(t.name, &mut sp);
            }
            if let Some(ty) = &kept[k] {
                sp.procedural = true;
                sp.preset = ty.clone();
            }
            let sp = &sp;
            let i = match self.species.iter().position(|s| s.name == sp.name) {
                Some(i) => {
                    self.species[i] = sp.clone();
                    i
                }
                None if self.species.len() < 255 => {
                    self.species.push(sp.clone());
                    self.species.len() - 1
                }
                None => 0,
            };
            idx[k] = i as u8;
        }
        let g = &mut self.gen;
        g.main_species = idx[0];
        g.mix_species = idx[1];
        g.shrub_species = idx[2];
        g.mix_amount = t.mix_amount;
        (g.mushrooms, g.mushroom_mix) = theme_mushrooms(t.name);
        g.giant_mushrooms = theme_giants(t.name);
        if let Some(c) = theme_canopy(t.name) {
            g.canopy_cover = c;
        }
        g.grass_color = t.grass.map(|v| v as f32 / 255.0);
        g.grass_accent = t.accent.map(|v| v as f32 / 255.0);
        g.haze_color = t.haze.map(|v| v as f32 / 255.0);
        // a satellite theme also carries the structure and look of the stand it was measured from
        if let Some(over) = theme_settings(t.name).or_else(|| natural_look(t.name)) {
            let mut cur = serde_json::to_value(&*g).unwrap_or_default();
            crate::groups::merge_value(&mut cur, &over);
            if let Ok(new) = serde_json::from_value(cur) {
                *g = new;
            }
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        Project {
            map_w: 2048,
            map_h: 1536,
            shapes: Vec::new(),
            gen: GenParams::default(),
            light: Light::default(),
            grading: Grading::default(),
            species: default_species(),
            painted: Vec::new(),
            erased: Vec::new(),
            background_image: None,
            groups: Vec::new(),
        }
    }
}
