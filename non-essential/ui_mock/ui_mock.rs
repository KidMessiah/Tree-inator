//! A mock-up of the reworked editor: toolbox with modes, tool options bar, layer-style zone panel, tabbed settings,
//! reference image controls and the new drawing tools. It uses the real engine for the forest picture but none of the
//! editor's logic; every control is live in the mock only. Run: cargo run --release --bin ui_mock
//! MOCK_TOOL=wand MOCK_TAB=look MOCK_SHOT=out.png renders one state to a picture and quits.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use forest_core::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Select,
    Lasso,
    Polygon,
    Wand,
    Line,
    Brush,
    Single,
    Eraser,
    Mask,
    Pan,
}

impl Tool {
    const ALL: [Tool; 10] = [Tool::Select, Tool::Lasso, Tool::Polygon, Tool::Wand, Tool::Line, Tool::Brush, Tool::Single, Tool::Eraser, Tool::Mask, Tool::Pan];
    fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select and edit",
            Tool::Lasso => "Lasso",
            Tool::Polygon => "Polygon",
            Tool::Wand => "Wand: trace the reference map",
            Tool::Line => "Line: paths, rivers, tree lines",
            Tool::Brush => "Tree brush",
            Tool::Single => "Single tree",
            Tool::Eraser => "Tree eraser",
            Tool::Mask => "No-trees mask",
            Tool::Pan => "Pan",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::Lasso => "L",
            Tool::Polygon => "P",
            Tool::Wand => "W",
            Tool::Line => "N",
            Tool::Brush => "B",
            Tool::Single => "T",
            Tool::Eraser => "E",
            Tool::Mask => "M",
            Tool::Pan => "H",
        }
    }
    fn from_name(s: &str) -> Tool {
        Tool::ALL.into_iter().find(|t| format!("{t:?}").eq_ignore_ascii_case(s)).unwrap_or(Tool::Select)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Add,
    Subtract,
    Intersect,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Theme,
    Shape,
    Trees,
    Colour,
    Ground,
    Weather,
    Map,
    Export,
}

impl Tab {
    const ALL: [Tab; 8] = [Tab::Theme, Tab::Shape, Tab::Trees, Tab::Colour, Tab::Ground, Tab::Weather, Tab::Map, Tab::Export];
    fn name(self) -> &'static str {
        match self {
            Tab::Theme => "Theme",
            Tab::Shape => "Shape",
            Tab::Trees => "Trees",
            Tab::Colour => "Colour & light",
            Tab::Ground => "Ground",
            Tab::Weather => "Weather",
            Tab::Map => "Map",
            Tab::Export => "Export",
        }
    }
    fn blurb(self) -> &'static str {
        match self {
            Tab::Shape => "How the forest sits on the map: how thick, how clumped, how ragged its edge is.",
            Tab::Trees => "The trees themselves: size, variety, how crowns overlap.",
            Tab::Colour => "Colour, brightness, shading and shadows.",
            Tab::Ground => "Grass, undergrowth, haze and stones between the trees.",
            Tab::Weather => "Snow and fire laid over the forest.",
            _ => "",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Slider,
    Percent,
    Check,
}

/// One setting in the tuning panel. `mixed` ones are the plain-language sliders at the top of a tab; the rest are the
/// individual tweaks under Fine Tuning. `aliases` are the other words people use for it, for the search.
struct Def {
    id: &'static str,
    label: &'static str,
    tab: Tab,
    mixed: bool,
    tip: &'static str,
    aliases: &'static [&'static str],
    lo: f32,
    hi: f32,
    def: f32,
    kind: Kind,
}

#[allow(clippy::too_many_arguments)]
fn d(id: &'static str, label: &'static str, tab: Tab, mixed: bool, kind: Kind, lo: f32, hi: f32, def: f32, tip: &'static str, aliases: &'static [&'static str]) -> Def {
    Def { id, label, tab, mixed, tip, aliases, lo, hi, def, kind }
}

fn defs() -> Vec<Def> {
    use Kind::*;
    use Tab::*;
    vec![
        // ---- Shape
        d("denseness", "Denseness", Shape, true, Percent, 0.1, 1.0, 0.69, "How much of the ground the crowns cover. Low is sparse woodland with the ground showing, high is a closed canopy. Moves Density and Clearings together.", &["density", "thick", "thin", "sparse", "crowded", "cover", "coverage", "fuller", "denser", "fewer trees", "more trees"]),
        d("clumpiness", "Clumpiness", Shape, true, Percent, 0.0, 1.0, 0.3, "Smooth and even at 0, groups of trees with gaps between at 1. Moves clump size, clearings and how patchy the cover is.", &["patchy", "clusters", "groves", "uneven", "natural", "random", "grouping", "clustering"]),
        d("raggedness", "Edge raggedness", Shape, true, Percent, 0.0, 1.0, 0.4, "How rough the forest's outer edge is. Low gives a tidy fringe, high gives a broken, wandering edge with stray trees.", &["edge", "border", "fringe", "outline", "rough", "wobble", "messy", "boundary"]),
        d("openness", "Openness", Shape, true, Percent, 0.0, 1.0, 0.2, "Clearings inside the forest where trees have fallen or never grew. More openness means more and larger glades.", &["clearings", "gaps", "glades", "holes", "meadows", "open", "treefall"]),
        d("pattern_strength", "Pattern strength", Shape, false, Slider, 0.0, 1.0, 0.5, "How strongly the chosen pattern (fragmented, branching, clumps, lobed) reshapes the drawn outline. 0 keeps your outline exactly.", &["shape", "outline", "style"]),
        d("edge_softness", "Edge softness", Shape, false, Slider, 5.0, 200.0, 60.0, "How gradually the forest thins at its edge, in map pixels. Small values give a sharp wall of trees, large values a long soft fade.", &["feather", "fade", "blur", "transition"]),
        d("edge_wobble", "Edge wobble", Shape, false, Slider, 0.0, 300.0, 90.0, "How far the outline wanders from the line you drew. Anything past the line is still clipped if Keep inside is on.", &["warp", "wander", "irregular"]),
        d("branch_width", "Branch width", Shape, false, Slider, 0.05, 1.0, 0.4, "For the Branching pattern: how wide the forest ribbons are. Narrow gives thin river-like fingers, wide gives thick arms.", &["fingers", "ribbons", "river"]),
        d("gaps", "Clearing amount", Shape, false, Slider, 0.0, 1.0, 0.2, "How many clearings appear inside the forest.", &["gaps", "glades", "holes"]),
        d("gap_size", "Clearing size", Shape, false, Slider, 4.0, 80.0, 24.0, "Typical width of a clearing, in generated map pixels.", &["glade size", "hole size"]),
        d("contain", "Keep everything inside my line", Shape, false, Check, 0.0, 1.0, 1.0, "Trees, ground cover and haze stop before the line you drew, with a margin so nothing is cut by it. Turn off to let the forest spill past.", &["clip", "constrain", "boundary", "spill", "overflow", "inside"]),
        d("contain_fit", "Margin from line", Shape, false, Slider, 0.0, 2.0, 1.3, "How far inside your line the forest stays. 1 leaves just enough room for crowns and ground cover, higher leaves a wider empty border.", &["inset", "padding", "gap", "pull in", "fit"]),
        // ---- Trees
        d("tree_size", "Tree size", Trees, true, Percent, 0.1, 4.0, 0.9, "Makes every tree, leaf, shrub and shadow bigger or smaller on the same drawn area. Use it to match your map's scale.", &["scale", "bigger trees", "smaller trees", "crown size", "zoom", "size", "larger", "tiny", "huge"]),
        d("variety", "Species mix", Trees, true, Percent, 0.0, 1.0, 0.12, "How much of the second tree type is mixed into the canopy, in patches.", &["mix", "second species", "diversity", "mixed forest", "different trees"]),
        d("maturity", "Maturity", Trees, true, Percent, 0.0, 1.5, 0.4, "Young, even stands at 0 to old, uneven ones at 1 and above: taller emergents, more difference between neighbouring crowns.", &["age", "old growth", "young", "height variation", "tall trees", "uneven"]),
        d("wildness", "Wildness", Trees, true, Percent, 0.0, 1.0, 0.3, "Orderly woodland at 0 to untidy, wild forest at 1: more lone trees, stray shrubs, fallen gaps.", &["messy", "natural", "tidy", "untamed", "random"]),
        d("crown_min", "Smallest crown", Trees, false, Slider, 2.0, 40.0, 10.0, "Radius of the smallest canopy tree, in generated map pixels.", &["minimum", "small trees"]),
        d("crown_max", "Largest crown", Trees, false, Slider, 4.0, 60.0, 17.0, "Radius of the largest canopy tree, in generated map pixels.", &["maximum", "big trees"]),
        d("height_var", "Crown height variation", Trees, false, Percent, 0.0, 1.5, 0.4, "Random crown heights, so taller crowns shade the ones beside them. Low is flat and uniform.", &["tall", "short", "emergent", "relief"]),
        d("weave", "Canopy weave", Trees, false, Percent, 0.0, 1.0, 0.45, "Neighbouring crowns of similar height interleave at their edges instead of one dome hiding the next, as in aerial photos. 0 is off.", &["interleave", "overlap", "merge", "blend crowns", "overlapping"]),
        d("shyness", "Crown shyness gap", Trees, false, Slider, 0.0, 6.0, 0.0, "Thin gaps where crowns of similar height meet, as in real canopies. 0 is off. Small crowns get thinner gaps.", &["gap between trees", "crown gap", "separation", "spacing"]),
        d("shrubs", "Shrubs", Trees, false, Slider, 0.0, 3.0, 1.0, "How many shrubs grow under and around the trees.", &["bushes", "undergrowth", "understory"]),
        d("lone", "Lone trees", Trees, false, Slider, 0.0, 5.0, 1.0, "Small groups of trees standing outside the forest edge.", &["outliers", "strays", "scattered", "isolated"]),
        d("giants", "Giant mushrooms", Trees, false, Percent, 0.0, 0.6, 0.0, "Share of the big trees that are mushrooms as tall as trees.", &["fungus", "toadstool", "fantasy"]),
        // ---- Colour and light
        d("brightness", "Brightness", Colour, true, Slider, 0.4, 2.0, 1.0, "Overall brightness of the canopy. Raises or lowers all crown colours together.", &["lighter", "darker", "exposure", "light", "dark", "dim", "bright"]),
        d("contrast", "Contrast", Colour, true, Slider, 0.4, 1.4, 1.0, "Below 1 lifts the shadows and flattens the look, above 1 deepens shadows and sharpens crown shapes.", &["gamma", "shadow lift", "punchy", "flat", "harsh", "soft"]),
        d("warmth", "Warmth", Colour, true, Slider, -1.0, 1.0, 0.0, "Shifts all colours toward yellow-orange (warm, summer) or blue-green (cool, damp).", &["colour temperature", "yellow", "blue", "cool", "warm", "autumn", "tint", "cold"]),
        d("lushness", "Lushness", Colour, true, Percent, 0.0, 1.5, 0.6, "How varied and rich the leaf colours are. Low is uniform and dull, high is lively with mottled patches.", &["saturation", "vibrant", "colourful", "colorful", "variety", "dull", "vivid"]),
        d("colour_variety", "Colour variety", Colour, false, Percent, 0.0, 3.0, 0.6, "Each leaf a little lighter, darker, yellower or bluer.", &["colour noise", "leaf variation", "mottling"]),
        d("patch_tone", "Patch tone", Colour, false, Percent, 0.0, 2.0, 0.5, "Broad lighter and darker patches across crowns.", &["blotches", "tone"]),
        d("crown_shade", "Crown shading", Colour, false, Percent, 0.0, 1.5, 0.3, "Crown tops lit, gaps between crowns dark. Higher gives more depth between neighbouring crowns.", &["depth", "ambient occlusion", "dark gaps", "shading"]),
        d("shadow_length", "Shadow length", Colour, false, Slider, 0.2, 4.0, 1.0, "How far cast shadows reach. Long shadows suggest low sun.", &["long shadows", "short shadows", "sun height", "evening", "noon"]),
        d("shadow_soft", "Shadow softness", Colour, false, Slider, 0.0, 4.0, 1.0, "How blurred shadow edges are. Hard edges look like clear midday sun.", &["blur", "diffuse", "overcast", "penumbra"]),
        d("shadows", "Cast shadows", Colour, false, Check, 0.0, 1.0, 1.0, "Turn shadows on trees and ground off or on. Off is useful when you add shadows on your own map.", &["shadow", "no shadows", "drop shadow"]),
        // ---- Ground
        d("undergrowth", "Undergrowth", Ground, true, Percent, 0.0, 2.0, 0.8, "How much grass, shrubs and stones fill the ground under and around the trees.", &["grass", "shrubs", "forest floor", "bushes", "bare", "ground cover"]),
        d("haze_mix", "Haze", Ground, true, Percent, 0.0, 1.0, 0.26, "A soft darkening of the ground and canopy edge, like moss and shade, around the forest.", &["mist", "fog", "smoke", "atmosphere", "shade"]),
        d("grass_density", "Grass density", Ground, false, Slider, 0.0, 2.0, 1.0, "Amount of grass tufts between the trees.", &["lawn", "turf"]),
        d("grass_reach", "Grass reach", Ground, false, Slider, 0.1, 1.0, 0.7, "How far from the forest edge grass spreads outward.", &["spread", "fringe"]),
        d("haze_opacity", "Haze opacity", Ground, false, Slider, 0.0, 0.8, 0.26, "Strength of the haze.", &["mist amount", "fog amount"]),
        d("pebbles", "Stones", Ground, false, Slider, 0.0, 5.0, 1.0, "Small round shrubs and stones scattered on the ground.", &["rocks", "pebbles", "boulders"]),
        // ---- Weather
        d("snow", "Snow", Weather, true, Percent, 0.0, 1.0, 0.0, "How much snow lies on the forest overall.", &["winter", "frost", "white", "cold", "ice"]),
        d("fire", "Burn", Weather, true, Percent, 0.0, 1.0, 0.0, "How much of the forest has burnt: charred ground, embers and flames.", &["burnt", "wildfire", "ash", "flames", "burn"]),
        d("snow_tops", "Snow on crowns", Weather, false, Percent, 0.0, 1.0, 0.6, "How much snow settles on the crowns versus the ground.", &["canopy snow"]),
        d("snow_reach", "Snow line reach", Weather, false, Slider, 10.0, 400.0, 140.0, "For edge and gradient snow: how far the snow reaches in from the edge.", &["snowline"]),
        d("fire_glow", "Ember glow", Weather, false, Percent, 0.0, 1.0, 0.5, "Orange glow around flames and embers.", &["embers", "light", "heat"]),
    ]
}

fn lev(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + (a[i - 1] != b[j - 1]) as usize);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// How well one query word matches a field: whole word, start of a word, inside a word, or a typo away.
fn word_score(q: &str, field: &str) -> f32 {
    let mut best = 0.0f32;
    for w in field.split(|c: char| !c.is_alphanumeric()) {
        if w.is_empty() {
            continue;
        }
        let w = w.to_lowercase();
        let s = if w == q {
            1.0
        } else if w.starts_with(q) && q.len() >= 2 {
            0.85
        } else if q.len() >= 3 && w.contains(q) {
            0.6
        } else if q.len() >= 4 && lev(q, &w) <= 1 {
            0.65
        } else if q.len() >= 6 && lev(q, &w) <= 2 {
            0.5
        } else {
            0.0
        };
        best = best.max(s);
    }
    best
}

/// Settings ranked for a query: every word must match somewhere, in the label, the aliases or the tip; labels count
/// most. A whole alias phrase ("bigger trees") typed out beats word by word matches. Returns the setting, its score and
/// the alias that matched, if one did.
fn search(defs: &[Def], query: &str) -> Vec<(usize, f32, Option<&'static str>)> {
    let q = query.to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, df) in defs.iter().enumerate() {
        let mut total = 0.0;
        let mut hit_alias = None;
        let mut ok = true;
        for w in &words {
            let l = word_score(w, df.label);
            let (mut a, mut a_which) = (0.0f32, None);
            for al in df.aliases {
                let s = word_score(w, al);
                if s > a {
                    a = s;
                    a_which = Some(*al);
                }
            }
            let t = word_score(w, df.tip);
            let best = l.max(a * 0.95).max(t * 0.4);
            if best <= 0.0 {
                ok = false;
                break;
            }
            if a * 0.95 > l && a_which.is_some() {
                hit_alias = a_which;
            }
            total += best;
        }
        if !ok {
            continue;
        }
        for al in df.aliases {
            if al.to_lowercase() == q {
                total += words.len() as f32;
                hit_alias = Some(*al);
            }
        }
        let mut score = total / words.len() as f32;
        if !df.mixed {
            score *= 0.97;
        }
        out.push((i, score, hit_alias));
    }
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(9);
    out
}

struct Row {
    name: String,
    group: bool,
    indent: u8,
    visible: bool,
    locked: bool,
    solo: bool,
    collapsed: bool,
    selected: bool,
    trees: u32,
    colour: Color32,
    mask: bool,
}

fn row(name: &str, group: bool, indent: u8, trees: u32, colour: Color32) -> Row {
    Row { name: name.into(), group, indent, visible: true, locked: false, solo: false, collapsed: false, selected: false, trees, colour, mask: false }
}

struct Mock {
    tool: Tool,
    mode: Mode,
    tab: Tab,
    rows: Vec<Row>,
    renaming: Option<usize>,
    species: usize,
    forest: Option<egui::TextureHandle>,
    reference: Option<egui::TextureHandle>,
    theme_names: Vec<(String, [Color32; 4])>,
    theme: usize,
    // tool options
    brush_size: f32,
    density: f32,
    tolerance: f32,
    smooth: f32,
    snap: bool,
    feather: f32,
    line_width: f32,
    line_trees: bool,
    spacing: f32,
    contiguous: bool,
    // reference
    ref_opacity: f32,
    ref_locked: bool,
    ref_scale: f32,
    show_scale_bar: bool,
    show_grid: bool,
    // tuning panel
    defs: Vec<Def>,
    vals: Vec<f32>,
    query: String,
    fine_open: bool,
    dirty: bool,
    compare: bool,
    snapshot: Option<egui::TextureHandle>,
    snap_vals: Vec<f32>,
    split: f32,
    shot: Option<(std::path::PathBuf, u32, bool)>,
    zoom: f32,
}

fn sample_project() -> Project {
    let mut p = Project { map_w: 1600, map_h: 1000, ..Default::default() };
    let poly = |pts: &[(f32, f32)]| pts.iter().map(|&(x, y)| [x, y]).collect::<Vec<_>>();
    p.shapes.push(Shape::new(ShapeOp::Add, poly(&[(120.0, 180.0), (560.0, 120.0), (760.0, 300.0), (700.0, 560.0), (420.0, 640.0), (150.0, 520.0)])));
    p.shapes.push(Shape::new(ShapeOp::Add, poly(&[(880.0, 360.0), (1300.0, 280.0), (1480.0, 520.0), (1360.0, 800.0), (960.0, 840.0), (820.0, 620.0)])));
    p.shapes.push(Shape::new(ShapeOp::Add, poly(&[(260.0, 700.0), (520.0, 720.0), (560.0, 900.0), (300.0, 930.0)])));
    p.ensure_mushroom_species();
    if let Some(t) = themes().into_iter().find(|t| t.name == "Aspen Stand") {
        p.apply_theme(&t);
    }
    p.gen.scale = 0.9;
    p
}

fn render_forest(ctx: &egui::Context) -> egui::TextureHandle {
    render_project(ctx, sample_project())
}

/// The picture with the tuning panel's settings applied, through the real engine.
fn render_forest_with(ctx: &egui::Context, m: &Mock) -> egui::TextureHandle {
    let mut p = sample_project();
    let v = |id: &str| m.val(id);
    p.gen.canopy_cover = v("denseness").clamp(0.05, 1.0);
    p.gen.scale = v("tree_size").clamp(0.1, 4.0);
    p.gen.canopy_patch_size = 400.0 - 360.0 * v("clumpiness");
    p.gen.gaps = 0.1 + 0.6 * v("clumpiness");
    p.gen.depth.gain = v("brightness");
    p.gen.depth.gamma = 1.4 - v("contrast");
    p.gen.depth.crown_shade = v("crown_shade");
    p.gen.depth.weave = v("weave");
    p.gen.depth.height_var = v("height_var");
    p.gen.leaf.variety = v("colour_variety") * 2.0;
    p.light.shadow_length = v("shadow_length");
    p.light.shadows = v("shadows") > 0.5;
    let w = v("warmth");
    p.grading.global.tint = [1.0 + 0.18 * w, 1.0 + 0.03 * w, 1.0 - 0.2 * w];
    render_project(ctx, p)
}

fn render_project(ctx: &egui::Context, p: Project) -> egui::TextureHandle {
    let layers = generate(&p, 0.6, &|| false).expect("render");
    let rgba = compose(&layers, &p.grading, ComposeOptions::default());
    let img = egui::ColorImage::from_rgba_unmultiplied([layers.w, layers.h], &rgba);
    ctx.load_texture("forest", img, egui::TextureOptions::LINEAR)
}

/// A stand-in for the map being worked on: sandy ground, hills and a river.
fn reference_image(ctx: &egui::Context) -> egui::TextureHandle {
    let (w, h) = (480usize, 300usize);
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let hill = ((fx * 9.0).sin() * (fy * 7.0).cos() * 0.5 + 0.5) * 22.0;
            let river = ((fy - 0.55 - 0.12 * (fx * 8.0).sin()).abs() < 0.025) as u8 as f32;
            let mut c = [200.0 - hill, 188.0 - hill, 150.0 - hill];
            if river > 0.0 {
                c = [96.0, 140.0, 176.0];
            }
            let i = (y * w + x) * 4;
            px[i..i + 4].copy_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, 255]);
        }
    }
    ctx.load_texture("reference", egui::ColorImage::from_rgba_unmultiplied([w, h], &px), egui::TextureOptions::LINEAR)
}

impl Mock {
    fn new(ctx: &egui::Context) -> Mock {
        let theme_names: Vec<(String, [Color32; 4])> = themes()
            .into_iter()
            .map(|t| {
                let c = |i: usize| {
                    let r = t.species[0].ramp[i];
                    Color32::from_rgb((r[0] * 255.0) as u8, (r[1] * 255.0) as u8, (r[2] * 255.0) as u8)
                };
                (t.name.to_string(), [c(0), c(1), c(2), c(3)])
            })
            .collect();
        let theme = theme_names.iter().position(|(n, _)| n == "Aspen Stand").unwrap_or(0);
        let g = Color32::from_rgb(110, 190, 120);
        let mut rows = vec![
            row("Western woods", true, 0, 0, g),
            row("Aspen stand", false, 1, 4210, Color32::from_rgb(120, 170, 90)),
            row("North meadow edge", false, 1, 1380, Color32::from_rgb(150, 190, 90)),
            row("Riverside", true, 0, 0, g),
            row("Pine strip", false, 1, 2105, Color32::from_rgb(90, 150, 110)),
            row("Clearing, camp", false, 1, 0, Color32::from_rgb(210, 150, 90)),
            row("Eastern grove", false, 0, 3340, Color32::from_rgb(130, 180, 110)),
            row("No-trees: road and camp", false, 0, 0, Color32::from_rgb(220, 90, 90)),
        ];
        rows[7].mask = true;
        rows[1].selected = true;
        let shot = std::env::var("MOCK_SHOT").ok().map(|p| (std::path::PathBuf::from(p), 0u32, false));
        let mut m = Mock {
            tool: std::env::var("MOCK_TOOL").map(|s| Tool::from_name(&s)).unwrap_or(Tool::Wand),
            mode: Mode::Add,
            tab: match std::env::var("MOCK_TAB").unwrap_or_default().as_str() {
                "shape" => Tab::Shape,
                "trees" => Tab::Trees,
                "colour" => Tab::Colour,
                "ground" => Tab::Ground,
                "weather" => Tab::Weather,
                "map" => Tab::Map,
                "export" => Tab::Export,
                _ => Tab::Theme,
            },
            rows,
            renaming: None,
            species: 1,
            forest: Some(render_forest(ctx)),
            reference: Some(reference_image(ctx)),
            theme_names,
            theme,
            brush_size: 80.0,
            density: 0.7,
            tolerance: 32.0,
            smooth: 0.4,
            snap: true,
            feather: 0.3,
            line_width: 24.0,
            line_trees: false,
            spacing: 18.0,
            contiguous: true,
            ref_opacity: 0.85,
            ref_locked: true,
            ref_scale: 1.5,
            show_scale_bar: true,
            show_grid: false,
            vals: Vec::new(),
            defs: Vec::new(),
            query: std::env::var("MOCK_QUERY").unwrap_or_default(),
            fine_open: std::env::var("MOCK_FINE").is_ok(),
            dirty: false,
            compare: std::env::var("MOCK_COMPARE").is_ok(),
            snapshot: None,
            snap_vals: Vec::new(),
            split: 0.5,
            shot,
            zoom: 1.0,
        };
        m.defs = defs();
        m.vals = m.defs.iter().map(|d| d.def).collect();
        m.snap_vals = m.vals.clone();
        m.snapshot = m.forest.clone();
        if m.compare {
            // a demo state: a few settings changed after the snapshot
            for (id, v) in [("denseness", 0.42), ("tree_size", 1.35), ("brightness", 1.25), ("warmth", 0.5), ("clumpiness", 0.75)] {
                if let Some(i) = m.defs.iter().position(|d| d.id == id) {
                    m.vals[i] = v;
                }
            }
            m.dirty = true;
        }
        m
    }
}

// ------------------------------------------------------------------ icons

fn icon(painter: &egui::Painter, r: Rect, tool: Tool, col: Color32) {
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

fn eye(painter: &egui::Painter, r: Rect, on: bool, col: Color32) {
    let c = r.center();
    if on {
        painter.add(egui::Shape::line(vec![Pos2::new(c.x - 6.0, c.y), Pos2::new(c.x - 3.0, c.y - 3.5), Pos2::new(c.x + 3.0, c.y - 3.5), Pos2::new(c.x + 6.0, c.y), Pos2::new(c.x + 3.0, c.y + 3.5), Pos2::new(c.x - 3.0, c.y + 3.5), Pos2::new(c.x - 6.0, c.y)], Stroke::new(1.2, col)));
        painter.circle_filled(c, 2.0, col);
    } else {
        painter.line_segment([Pos2::new(c.x - 6.0, c.y), Pos2::new(c.x + 6.0, c.y)], Stroke::new(1.2, col.gamma_multiply(0.5)));
    }
}

fn lock(painter: &egui::Painter, r: Rect, on: bool, col: Color32) {
    let c = r.center();
    let col = if on { col } else { col.gamma_multiply(0.35) };
    painter.rect_filled(Rect::from_center_size(Pos2::new(c.x, c.y + 2.0), Vec2::new(9.0, 6.0)), 1.0, col);
    painter.circle_stroke(Pos2::new(c.x, c.y - 1.5), 3.0, Stroke::new(1.2, col));
}

// ------------------------------------------------------------------ the editor

impl Mock {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for t in ["New", "Open", "Save", "Save as"] {
                let _ = ui.button(t);
            }
            ui.separator();
            let _ = ui.add_enabled(false, egui::Button::new("Undo"));
            let _ = ui.add_enabled(false, egui::Button::new("Redo"));
            ui.separator();
            ui.label(egui::RichText::new("Avernus crossing.treeproj").weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let _ = ui.add(egui::Button::new(egui::RichText::new("Export PNG").strong()).fill(Color32::from_rgb(40, 100, 150)));
                if ui.selectable_label(self.compare, "Compare").on_hover_text("Split the map: snapshot on the left, what you are editing on the right").clicked() {
                    self.compare = !self.compare;
                    if self.compare {
                        self.snapshot = self.forest.clone();
                        self.snap_vals = self.vals.clone();
                    }
                }
                ui.label(format!("{:.0}%", self.zoom * 50.0));
                ui.label("Zoom");
            });
        });
    }

    /// The bar under the menu: modes on the left, then options of whichever tool is active.
    fn options_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(self.tool.name()).strong());
            ui.separator();
            if matches!(self.tool, Tool::Lasso | Tool::Polygon | Tool::Wand | Tool::Select) {
                for (m, l, tip) in [(Mode::Add, "Add", "Grow the forest (default)"), (Mode::Subtract, "Subtract", "Cut a clearing (hold Alt)"), (Mode::Intersect, "Intersect", "Keep only the overlap (hold Shift+Alt)")] {
                    ui.selectable_value(&mut self.mode, m, l).on_hover_text(tip);
                }
                ui.separator();
            }
            match self.tool {
                Tool::Select => {
                    for l in ["Union", "Subtract", "Intersect", "Split with line", "Smooth", "Simplify"] {
                        let _ = ui.small_button(l);
                    }
                    ui.separator();
                    ui.label("Grow / shrink");
                    let mut px = 20.0f32;
                    ui.add(egui::DragValue::new(&mut px).suffix(" px"));
                }
                Tool::Lasso | Tool::Polygon => {
                    ui.add(egui::Slider::new(&mut self.smooth, 0.0..=1.0).text("Smooth"));
                    ui.add(egui::Slider::new(&mut self.feather, 0.0..=1.0).text("Edge feather"));
                    ui.checkbox(&mut self.snap, "Snap to zone edges").on_hover_text("Points snap to other zones' edges and corners, so touching zones leave no gap");
                }
                Tool::Wand => {
                    ui.add(egui::Slider::new(&mut self.tolerance, 1.0..=100.0).text("Tolerance"));
                    ui.add(egui::Slider::new(&mut self.smooth, 0.0..=1.0).text("Smooth"));
                    ui.checkbox(&mut self.contiguous, "Contiguous");
                    egui::ComboBox::from_id_salt("wand_src").selected_text("Sample: reference map").show_ui(ui, |ui| {
                        let _ = ui.selectable_label(true, "Reference map");
                        let _ = ui.selectable_label(false, "Rendered forest");
                    });
                }
                Tool::Line => {
                    ui.add(egui::Slider::new(&mut self.line_width, 2.0..=120.0).text("Width"));
                    ui.selectable_value(&mut self.line_trees, false, "Cut a path");
                    ui.selectable_value(&mut self.line_trees, true, "Trees along it");
                    if self.line_trees {
                        ui.add(egui::Slider::new(&mut self.spacing, 4.0..=80.0).text("Spacing"));
                    }
                    ui.add(egui::Slider::new(&mut self.feather, 0.0..=1.0).text("Bank softness"));
                }
                Tool::Brush | Tool::Eraser | Tool::Single => {
                    ui.add(egui::Slider::new(&mut self.brush_size, 5.0..=400.0).text("Size"));
                    if self.tool == Tool::Brush {
                        ui.add(egui::Slider::new(&mut self.density, 0.0..=1.0).text("Density"));
                    }
                    ui.label(egui::RichText::new("Species 1 to 9 below the map").weak());
                }
                Tool::Mask => {
                    ui.add(egui::Slider::new(&mut self.brush_size, 5.0..=400.0).text("Brush"));
                    let _ = ui.small_button("Mask from selection");
                    let _ = ui.small_button("Invert");
                }
                Tool::Pan => {
                    ui.label(egui::RichText::new("Drag to move the map. Wheel zooms. Space + drag pans from any tool.").weak());
                }
            }
        });
    }

    fn tool_strip(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        for t in Tool::ALL {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 40.0), Sense::click());
            let sel = self.tool == t;
            let hov = resp.hovered();
            let bg = if sel { Color32::from_rgb(34, 92, 140) } else if hov { Color32::from_gray(52) } else { Color32::TRANSPARENT };
            ui.painter().rect_filled(rect, 5.0, bg);
            icon(ui.painter(), rect.shrink(6.0), t, if sel { Color32::WHITE } else { Color32::from_gray(205) });
            ui.painter().text(rect.right_bottom() + Vec2::new(-3.0, -2.0), Align2::RIGHT_BOTTOM, t.key(), FontId::proportional(9.0), Color32::from_gray(150));
            if resp.on_hover_text(format!("{}  ({})", t.name(), t.key())).clicked() {
                self.tool = t;
            }
            if t == Tool::Line || t == Tool::Wand || t == Tool::Select {
                ui.add_space(2.0);
            }
        }
    }

    fn zones_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Zones").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let _ = ui.small_button("+ Group").on_hover_text("New group");
                let _ = ui.small_button("Merge").on_hover_text("Merge the selected zones");
            });
        });
        ui.separator();
        let n = self.rows.len();
        let mut clicked: Option<(usize, bool)> = None;
        let mut menu_row: Option<usize> = None;
        let mut hidden_under: Option<u8> = None;
        for i in 0..n {
            if let Some(d) = hidden_under {
                if self.rows[i].indent > d {
                    continue;
                }
                hidden_under = None;
            }
            let rw = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(rw, 26.0), Sense::click_and_drag());
            let r = &mut self.rows[i];
            if r.selected {
                ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(34, 70, 104));
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 3.0, Color32::from_gray(44));
            }
            let x0 = rect.min.x + 4.0 + r.indent as f32 * 14.0;
            let cy = rect.center().y;
            // drag handle, then fold arrow or colour chip
            for k in 0..3 {
                ui.painter().circle_filled(Pos2::new(rect.min.x + 5.0, cy - 4.0 + k as f32 * 4.0), 0.9, Color32::from_gray(110));
            }
            if r.group {
                let tri = if r.collapsed { vec![Pos2::new(x0 + 6.0, cy - 4.0), Pos2::new(x0 + 6.0, cy + 4.0), Pos2::new(x0 + 11.0, cy)] } else { vec![Pos2::new(x0 + 5.0, cy - 2.0), Pos2::new(x0 + 12.0, cy - 2.0), Pos2::new(x0 + 8.5, cy + 3.0)] };
                ui.painter().add(egui::Shape::convex_polygon(tri, Color32::from_gray(180), Stroke::NONE));
            } else {
                let chip = Rect::from_min_size(Pos2::new(x0 + 4.0, cy - 7.0), Vec2::new(16.0, 14.0));
                if r.mask {
                    ui.painter().rect_stroke(chip, 2.0, Stroke::new(1.0, r.colour), egui::StrokeKind::Inside);
                    for k in 0..4 {
                        ui.painter().line_segment([Pos2::new(chip.min.x + k as f32 * 4.0, chip.max.y), Pos2::new(chip.min.x + 4.0 + k as f32 * 4.0, chip.min.y)], Stroke::new(1.0, r.colour));
                    }
                } else {
                    ui.painter().rect_filled(chip, 2.0, r.colour.gamma_multiply(if r.visible { 1.0 } else { 0.3 }));
                }
            }
            let tx = x0 + 26.0;
            let name_col = if r.visible { Color32::from_gray(225) } else { Color32::from_gray(110) };
            if self.renaming == Some(i) {
                let edit = Rect::from_min_size(Pos2::new(tx - 2.0, rect.min.y + 2.0), Vec2::new((rect.max.x - tx - 96.0).max(40.0), 22.0));
                let mut ui2 = ui.new_child(egui::UiBuilder::new().max_rect(edit));
                let te = ui2.add(egui::TextEdit::singleline(&mut self.rows[i].name));
                te.request_focus();
                if te.lost_focus() {
                    self.renaming = None;
                }
            } else {
                ui.painter().text(Pos2::new(tx, cy), Align2::LEFT_CENTER, &r.name, FontId::proportional(13.0), name_col);
                if !r.group && r.trees > 0 {
                    ui.painter().text(Pos2::new(rect.max.x - 94.0, cy), Align2::RIGHT_CENTER, format!("{}", r.trees), FontId::proportional(10.5), Color32::from_gray(120));
                }
            }
            // eye, lock, solo on the right
            let bx = |k: f32| Rect::from_center_size(Pos2::new(rect.max.x - 72.0 + k * 22.0, cy), Vec2::splat(20.0));
            let (eye_r, lock_r, solo_r) = (bx(0.0), bx(1.0), bx(2.0));
            let col = Color32::from_gray(200);
            let mut toggled = false;
            if ui.interact(eye_r, ui.id().with(("eye", i)), Sense::click()).on_hover_text("Show or hide").clicked() {
                self.rows[i].visible = !self.rows[i].visible;
                toggled = true;
            }
            if ui.interact(lock_r, ui.id().with(("lock", i)), Sense::click()).on_hover_text("Lock").clicked() {
                self.rows[i].locked = !self.rows[i].locked;
                toggled = true;
            }
            if ui.interact(solo_r, ui.id().with(("solo", i)), Sense::click()).on_hover_text("Solo: show only this").clicked() {
                self.rows[i].solo = !self.rows[i].solo;
                toggled = true;
            }
            let r = &self.rows[i];
            eye(ui.painter(), eye_r, r.visible, col);
            lock(ui.painter(), lock_r, r.locked, col);
            ui.painter().text(solo_r.center(), Align2::CENTER_CENTER, "S", FontId::proportional(11.0), if r.solo { Color32::from_rgb(240, 200, 80) } else { Color32::from_gray(90) });
            if resp.double_clicked() {
                self.renaming = Some(i);
            } else if resp.clicked() && !toggled {
                let multi = ui.input(|inp| inp.modifiers.command || inp.modifiers.shift);
                clicked = Some((i, multi));
                if self.rows[i].group {
                    self.rows[i].collapsed = !self.rows[i].collapsed;
                }
            }
            if resp.secondary_clicked() {
                menu_row = Some(i);
            }
            if self.rows[i].group && self.rows[i].collapsed {
                hidden_under = Some(self.rows[i].indent);
            }
        }
        if let Some((i, multi)) = clicked {
            for (k, r) in self.rows.iter_mut().enumerate() {
                if k == i {
                    r.selected = if multi { !r.selected } else { true };
                } else if !multi {
                    r.selected = false;
                }
            }
        }
        let _ = menu_row;
        ui.add_space(6.0);
        ui.separator();
        let sel: Vec<&Row> = self.rows.iter().filter(|r| r.selected).collect();
        ui.label(egui::RichText::new(format!("{} selected", sel.len())).weak());
        ui.horizontal_wrapped(|ui| {
            for l in ["Duplicate", "Group", "Fit view", "Copy settings", "Paste settings"] {
                let _ = ui.small_button(l);
            }
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Right click a row for the same actions. Drag the grip to reorder or move into a group.").small().weak());
    }

    fn species_strip(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Species").weak());
            let names = ["Aspen", "Fir", "Pine", "Oak", "Birch", "Willow", "Hazel", "Fern", "Snag"];
            let cols = [Color32::from_rgb(120, 168, 90), Color32::from_rgb(40, 90, 70), Color32::from_rgb(90, 130, 80), Color32::from_rgb(96, 120, 50), Color32::from_rgb(170, 190, 110), Color32::from_rgb(120, 160, 70), Color32::from_rgb(100, 120, 60), Color32::from_rgb(70, 130, 80), Color32::from_rgb(120, 100, 80)];
            for i in 0..9 {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(52.0, 44.0), Sense::click());
                let sel = self.species == i;
                ui.painter().rect_filled(rect, 5.0, if sel { Color32::from_rgb(34, 92, 140) } else { Color32::from_gray(38) });
                ui.painter().circle_filled(rect.center() + Vec2::new(0.0, -6.0), 11.0, cols[i]);
                ui.painter().circle_filled(rect.center() + Vec2::new(-3.0, -9.0), 4.0, cols[i].gamma_multiply(1.4));
                ui.painter().text(rect.center() + Vec2::new(0.0, 15.0), Align2::CENTER_CENTER, names[i], FontId::proportional(9.5), Color32::from_gray(210));
                ui.painter().text(rect.left_top() + Vec2::new(4.0, 2.0), Align2::LEFT_TOP, format!("{}", i + 1), FontId::proportional(9.0), Color32::from_gray(150));
                if resp.clicked() {
                    self.species = i;
                }
            }
            ui.label(egui::RichText::new("Hover a map spot to preview the tree").weak());
        });
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        self.search_box(ui);
        ui.add_space(2.0);
        if !self.query.trim().is_empty() {
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| self.search_results(ui));
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for t in Tab::ALL {
                if ui.selectable_label(self.tab == t, t.name()).clicked() {
                    self.tab = t;
                }
            }
        });
        ui.separator();
        let tab = self.tab;
        egui::ScrollArea::vertical().show(ui, |ui| match tab {
            Tab::Theme => self.theme_tab(ui),
            Tab::Map => self.reference_tab(ui),
            Tab::Export => self.export_tab(ui),
            t => self.tuning_tab(ui, t),
        });
    }

    fn theme_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Pick a theme, then fine tune. Tree types you picked stay.").small().weak());
        let cols = 3;
        let gap = 6.0;
        let w = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).floor();
        let h = (w * 0.62).floor();
        let n = self.theme_names.len().min(15);
        egui::Grid::new("themes").spacing([gap, gap]).show(ui, |ui| {
            for k in 0..n {
                let (name, cs) = self.theme_names[k].clone();
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h + 18.0), Sense::click());
                let img = Rect::from_min_size(rect.min, Vec2::new(w, h));
                let p = ui.painter();
                p.rect_filled(img, 3.0, cs[0].gamma_multiply(0.55));
                for (j, c) in cs.iter().enumerate() {
                    for q in 0..5 {
                        let ang = (j * 5 + q) as f32 * 1.7;
                        p.circle_filled(Pos2::new(img.min.x + w * (0.15 + 0.7 * ((ang * 0.37).sin() * 0.5 + 0.5)), img.min.y + h * (0.2 + 0.6 * ((ang * 0.53).cos() * 0.5 + 0.5))), w * 0.11, *c);
                    }
                }
                let stroke = if self.theme == k { Stroke::new(2.0, Color32::from_rgb(100, 180, 255)) } else { Stroke::new(1.0, Color32::from_gray(80)) };
                p.rect_stroke(img, 3.0, stroke, egui::StrokeKind::Inside);
                p.text(Pos2::new(rect.min.x + 2.0, img.max.y + 3.0), Align2::LEFT_TOP, &name, FontId::proportional(10.5), Color32::from_gray(215));
                if resp.clicked() {
                    self.theme = k;
                }
                if (k + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(6.0);
        let _ = ui.button("Show all themes (40)");
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Satellite themes carry the stand's own density, crown size, clumping and shading, measured from aerial lidar and hyperspectral imagery.").small().weak());
    }

    fn val(&self, id: &str) -> f32 {
        self.defs.iter().position(|d| d.id == id).map(|i| self.vals[i]).unwrap_or(0.0)
    }

    /// One setting as a control: a label with a dot when it differs from the theme, a slider or tick box, a good tooltip.
    fn setting_row(&mut self, ui: &mut egui::Ui, i: usize) {
        let (label, tip, lo, hi, kind, def) = {
            let df = &self.defs[i];
            (df.label, df.tip, df.lo, df.hi, df.kind, df.def)
        };
        let changed_from_theme = (self.vals[i] - def).abs() > 1e-4;
        let changed_from_snap = self.compare && self.snap_vals.get(i).is_some_and(|s| (s - self.vals[i]).abs() > 1e-4);
        ui.horizontal(|ui| {
            let mark = if changed_from_snap { "\u{25cf} " } else if changed_from_theme { "\u{2022} " } else { "  " };
            let colour = if changed_from_snap { Color32::from_rgb(255, 200, 90) } else { Color32::from_rgb(100, 180, 255) };
            let text = if changed_from_theme || changed_from_snap { egui::RichText::new(format!("{mark}{label}")).color(colour) } else { egui::RichText::new(format!("{mark}{label}")) };
            ui.add_sized([128.0, 18.0], egui::Label::new(text).truncate()).on_hover_text(tip);
            match kind {
                Kind::Check => {
                    let mut on = self.vals[i] > 0.5;
                    if ui.checkbox(&mut on, "").on_hover_text(tip).changed() {
                        self.vals[i] = on as u8 as f32;
                        self.dirty = true;
                    }
                }
                Kind::Slider | Kind::Percent => {
                    ui.spacing_mut().slider_width = (ui.available_width() - 92.0).clamp(50.0, 260.0);
                    let r = if kind == Kind::Percent {
                        let mut p = self.vals[i] * 100.0;
                        let r = ui.add(egui::Slider::new(&mut p, lo * 100.0..=hi * 100.0).suffix("%").max_decimals(0));
                        self.vals[i] = p / 100.0;
                        r
                    } else {
                        ui.add(egui::Slider::new(&mut self.vals[i], lo..=hi))
                    };
                    if r.on_hover_text(tip).changed() {
                        self.dirty = true;
                    }
                    if changed_from_theme && ui.small_button("\u{21ba}").on_hover_text("Reset to the theme's value").clicked() {
                        self.vals[i] = def;
                        self.dirty = true;
                    }
                }
            }
        });
    }

    fn tuning_tab(&mut self, ui: &mut egui::Ui, tab: Tab) {
        ui.label(egui::RichText::new(tab.blurb()).small().weak());
        let mixed: Vec<usize> = (0..self.defs.len()).filter(|&i| self.defs[i].tab == tab && self.defs[i].mixed).collect();
        let fine: Vec<usize> = (0..self.defs.len()).filter(|&i| self.defs[i].tab == tab && !self.defs[i].mixed).collect();
        let n_changed = |m: &Mock, v: &[usize]| v.iter().filter(|&&i| (m.vals[i] - m.defs[i].def).abs() > 1e-4).count();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Mixed Tuning").strong().size(14.0));
            ui.label(egui::RichText::new("one slider, several settings").small().weak());
        });
        ui.separator();
        for &i in &mixed {
            self.setting_row(ui, i);
        }
        ui.add_space(10.0);
        let fc = n_changed(self, &fine);
        let title = if fc > 0 { format!("Fine Tuning   {fc} changed") } else { "Fine Tuning".to_string() };
        let open = self.fine_open;
        let h = egui::CollapsingHeader::new(egui::RichText::new(title).strong().size(14.0)).id_salt(("fine", tab as u8)).open(Some(open)).show_background(false);
        let resp = h.show(ui, |ui| {
            ui.label(egui::RichText::new("Every individual setting. Hover a name for what it does.").small().weak());
            for &i in &fine {
                self.setting_row(ui, i);
            }
        });
        if resp.header_response.clicked() {
            self.fine_open = !self.fine_open;
        }
        ui.add_space(8.0);
        if ui.small_button("Reset this tab to the theme").clicked() {
            for &i in mixed.iter().chain(fine.iter()) {
                self.vals[i] = self.defs[i].def;
            }
            self.dirty = true;
        }
    }

    fn search_box(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Search settings: try \"bigger trees\", \"shadow\", \"denser\"").desired_width(ui.available_width() - 26.0));
            if ui.small_button("x").clicked() {
                self.query.clear();
            }
        });
    }

    fn search_results(&mut self, ui: &mut egui::Ui) {
        let hits = search(&self.defs, &self.query);
        if hits.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Nothing matches. Try another word, like \"size\", \"light\", \"edge\" or \"snow\".").weak());
            return;
        }
        ui.label(egui::RichText::new(format!("{} matches, best first. The controls below are live.", hits.len())).small().weak());
        for (i, _score, alias) in hits {
            ui.add_space(2.0);
            let (tab, mixed) = (self.defs[i].tab, self.defs[i].mixed);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{} \u{203a} {}", tab.name(), if mixed { "Mixed Tuning" } else { "Fine Tuning" })).small().weak());
                if let Some(a) = alias {
                    ui.label(egui::RichText::new(format!("matches \"{a}\"")).small().color(Color32::from_rgb(150, 200, 150)));
                }
                if ui.small_button("Go to").clicked() {
                    self.tab = tab;
                    self.query.clear();
                    self.fine_open = !mixed;
                }
            });
            self.setting_row(ui, i);
        }
    }

    fn render_live(&mut self, ctx: &egui::Context) {
        let t = render_forest_with(ctx, self);
        self.forest = Some(t);
        self.dirty = false;
    }

    fn compare_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Compare").strong());
            let changed: Vec<&str> = (0..self.defs.len()).filter(|&i| self.snap_vals.get(i).is_some_and(|s| (s - self.vals[i]).abs() > 1e-4)).map(|i| self.defs[i].label).collect();
            ui.label(egui::RichText::new(format!("{} settings changed since the snapshot", changed.len())).weak());
            if !changed.is_empty() {
                ui.label(egui::RichText::new(changed.join(", ")).small().color(Color32::from_rgb(255, 200, 90)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("Close compare")).clicked() {
                    self.compare = false;
                }
                if ui.add(egui::Button::new(egui::RichText::new("Keep changes").strong()).fill(Color32::from_rgb(40, 110, 70))).on_hover_text("Make what you see on the right the new snapshot").clicked() {
                    self.snapshot = self.forest.clone();
                    self.snap_vals = self.vals.clone();
                }
                if ui.add(egui::Button::new("Revert to snapshot").fill(Color32::from_rgb(120, 60, 50))).on_hover_text("Put every setting back to what the left side shows").clicked() {
                    self.vals = self.snap_vals.clone();
                    self.dirty = true;
                }
                if ui.button("New snapshot here").on_hover_text("Replace the left side with the picture as it is now").clicked() {
                    self.snapshot = self.forest.clone();
                    self.snap_vals = self.vals.clone();
                }
            });
        });
    }

    fn reference_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Map you are painting on. It is never exported.").small().weak());
        ui.horizontal(|ui| {
            let _ = ui.button("Import map image");
            ui.label(egui::RichText::new("avernus_overland.png").weak());
        });
        ui.horizontal(|ui| {
            ui.label("Opacity");
            ui.add(egui::Slider::new(&mut self.ref_opacity, 0.0..=1.0));
        });
        ui.checkbox(&mut self.ref_locked, "Lock position");
        ui.separator();
        ui.label(egui::RichText::new("Calibration").strong());
        ui.horizontal(|ui| {
            ui.label("Pixels per metre");
            ui.add(egui::DragValue::new(&mut self.ref_scale).speed(0.05).range(0.1..=20.0));
        });
        let _ = ui.button("Calibrate: click two points a known distance apart");
        ui.checkbox(&mut self.show_scale_bar, "Scale bar");
        ui.checkbox(&mut self.show_grid, "Grid every 100 m");
        ui.separator();
        ui.label(egui::RichText::new("Masks").strong());
        ui.label(egui::RichText::new("No-trees masks live in the Zones list and can be shown, locked and exported separately.").small().weak());
    }

    fn export_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Output scale");
        let mut s = 2.0f32;
        ui.add(egui::Slider::new(&mut s, 0.5..=8.0));
        ui.checkbox(&mut true, "Trees only (transparent)");
        ui.checkbox(&mut false, "Shadows on a separate layer");
        ui.checkbox(&mut false, "Mask as its own PNG");
        let _ = ui.button("Export PNG");
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(26));
        // the picture fits the area, 1600 x 1000 map
        let map = Vec2::new(1600.0, 1000.0);
        let z = ((rect.width() - 40.0) / map.x).min((rect.height() - 40.0) / map.y);
        self.zoom = z * 2.0;
        let origin = rect.center() - map * z * 0.5;
        let to = |x: f32, y: f32| origin + Vec2::new(x, y) * z;
        let map_rect = Rect::from_min_size(origin, map * z);
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        if let Some(t) = &self.reference {
            painter.image(t.id(), map_rect, uv, Color32::from_white_alpha((self.ref_opacity * 255.0) as u8));
        }
        if self.compare {
            // left: the snapshot, right: now, with a draggable divider
            let sx = map_rect.min.x + map_rect.width() * self.split;
            let left = Rect::from_min_max(map_rect.min, Pos2::new(sx, map_rect.max.y));
            let right = Rect::from_min_max(Pos2::new(sx, map_rect.min.y), map_rect.max);
            if let Some(t) = &self.snapshot {
                painter.image(t.id(), left, Rect::from_min_max(Pos2::ZERO, Pos2::new(self.split, 1.0)), Color32::WHITE);
            }
            if let Some(t) = &self.forest {
                painter.image(t.id(), right, Rect::from_min_max(Pos2::new(self.split, 0.0), Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            painter.line_segment([Pos2::new(sx, map_rect.min.y), Pos2::new(sx, map_rect.max.y)], Stroke::new(2.0, Color32::WHITE));
            let handle = Rect::from_center_size(Pos2::new(sx, map_rect.center().y), Vec2::new(18.0, 40.0));
            painter.rect_filled(handle, 9.0, Color32::WHITE);
            painter.text(handle.center(), Align2::CENTER_CENTER, "\u{2194}", FontId::proportional(14.0), Color32::BLACK);
            let r = ui.interact(handle.expand(6.0), ui.id().with("split"), Sense::drag());
            if r.dragged() {
                self.split = ((sx + r.drag_delta().x - map_rect.min.x) / map_rect.width()).clamp(0.05, 0.95);
            }
            for (pos, al, txt) in [(left.left_top() + Vec2::new(10.0, 8.0), Align2::LEFT_TOP, "Snapshot"), (right.right_top() + Vec2::new(-10.0, 8.0), Align2::RIGHT_TOP, "Now (editing)")] {
                let galley = painter.layout_no_wrap(txt.to_string(), FontId::proportional(12.0), Color32::WHITE);
                let r = al.anchor_size(pos, galley.size()).expand(4.0);
                painter.rect_filled(r, 4.0, Color32::from_black_alpha(170));
                painter.text(pos, al, txt, FontId::proportional(12.0), Color32::WHITE);
            }
        } else if let Some(t) = &self.forest {
            painter.image(t.id(), map_rect, uv, Color32::WHITE);
        }
        if self.show_grid {
            let step = 100.0 * self.ref_scale * 0.5 * z * 3.0;
            let mut x = map_rect.min.x;
            while x < map_rect.max.x {
                painter.line_segment([Pos2::new(x, map_rect.min.y), Pos2::new(x, map_rect.max.y)], Stroke::new(0.5, Color32::from_white_alpha(40)));
                x += step;
            }
            let mut y = map_rect.min.y;
            while y < map_rect.max.y {
                painter.line_segment([Pos2::new(map_rect.min.x, y), Pos2::new(map_rect.max.x, y)], Stroke::new(0.5, Color32::from_white_alpha(40)));
                y += step;
            }
        }
        // no-trees mask, hatched
        let mask: Vec<Pos2> = [(1010.0, 140.0), (1180.0, 120.0), (1240.0, 240.0), (1100.0, 260.0)].iter().map(|&(x, y)| to(x, y)).collect();
        if self.rows.iter().any(|r| r.mask && r.visible) {
            painter.add(egui::Shape::convex_polygon(mask.clone(), Color32::from_rgba_unmultiplied(220, 80, 80, 40), Stroke::new(1.2, Color32::from_rgb(220, 90, 90))));
            let (mnx, mny, mxx, mxy) = (mask.iter().map(|p| p.x).fold(f32::MAX, f32::min), mask.iter().map(|p| p.y).fold(f32::MAX, f32::min), mask.iter().map(|p| p.x).fold(0.0, f32::max), mask.iter().map(|p| p.y).fold(0.0, f32::max));
            let mut x = mnx - (mxy - mny);
            while x < mxx {
                let a = Pos2::new(x, mxy);
                let b = Pos2::new(x + (mxy - mny), mny);
                painter.line_segment([a, b], Stroke::new(0.8, Color32::from_rgba_unmultiplied(220, 90, 90, 110)));
                x += 9.0;
            }
        }
        // zone outlines
        let zones: [&[(f32, f32)]; 3] = [
            &[(120.0, 180.0), (560.0, 120.0), (760.0, 300.0), (700.0, 560.0), (420.0, 640.0), (150.0, 520.0)],
            &[(880.0, 360.0), (1300.0, 280.0), (1480.0, 520.0), (1360.0, 800.0), (960.0, 840.0), (820.0, 620.0)],
            &[(260.0, 700.0), (520.0, 720.0), (560.0, 900.0), (300.0, 930.0)],
        ];
        for (k, pts) in zones.iter().enumerate() {
            let selected = k == 0;
            let col = if selected { Color32::from_rgb(120, 210, 255) } else { Color32::from_rgb(130, 220, 130) };
            let sp: Vec<Pos2> = pts.iter().map(|&(x, y)| to(x, y)).collect();
            painter.add(egui::Shape::closed_line(sp.clone(), Stroke::new(if selected { 2.0 } else { 1.2 }, col)));
            if selected && matches!(self.tool, Tool::Select | Tool::Lasso | Tool::Polygon) {
                for p in &sp {
                    painter.rect_filled(Rect::from_center_size(*p, Vec2::splat(7.0)), 1.0, Color32::WHITE);
                    painter.rect_stroke(Rect::from_center_size(*p, Vec2::splat(7.0)), 1.0, Stroke::new(1.0, col), egui::StrokeKind::Inside);
                }
                // an edge midpoint, ready to add a corner
                let m = Pos2::new((sp[1].x + sp[2].x) * 0.5, (sp[1].y + sp[2].y) * 0.5);
                painter.circle_stroke(m, 4.0, Stroke::new(1.2, col));
            }
        }
        // tool specific overlays at the pointer
        let hover = resp.hover_pos().or_else(|| if self.shot.is_some() { Some(to(930.0, 560.0)) } else { None });
        if let Some(h) = hover {
            match self.tool {
                Tool::Wand => {
                    // the region the wand would take, and its tolerance readout
                    let blob: Vec<Pos2> = (0..28)
                        .map(|i| {
                            let a = i as f32 / 28.0 * std::f32::consts::TAU;
                            let r = 120.0 * z * (1.0 + 0.28 * (a * 3.0).sin() + 0.12 * (a * 7.0 + 1.0).cos());
                            h + Vec2::new(a.cos() * r * 1.3, a.sin() * r * 0.9)
                        })
                        .collect();
                    painter.add(egui::Shape::convex_polygon(blob.clone(), Color32::from_rgba_unmultiplied(255, 220, 90, 45), Stroke::new(1.5, Color32::from_rgb(255, 220, 90))));
                    painter.circle_stroke(h, 5.0, Stroke::new(1.5, Color32::WHITE));
                    painter.text(h + Vec2::new(14.0, 12.0), Align2::LEFT_TOP, format!("Tolerance {:.0}  |  click to add  |  Alt: subtract", self.tolerance), FontId::proportional(11.0), Color32::WHITE);
                }
                Tool::Brush | Tool::Eraser => {
                    let col = if self.tool == Tool::Eraser { Color32::from_rgb(255, 120, 120) } else { Color32::from_rgb(200, 255, 160) };
                    painter.circle_stroke(h, self.brush_size * z * 1.2, Stroke::new(1.5, col));
                    if self.tool == Tool::Brush {
                        for k in 0..9 {
                            let a = k as f32 * 2.4;
                            let p = h + Vec2::new(a.cos(), a.sin()) * self.brush_size * z * (0.2 + 0.06 * k as f32);
                            painter.circle_filled(p, 9.0 * z * 1.6, Color32::from_rgba_unmultiplied(120, 168, 90, 120));
                        }
                    }
                }
                Tool::Single => {
                    painter.circle_filled(h, 18.0, Color32::from_rgba_unmultiplied(120, 168, 90, 150));
                    painter.circle_stroke(h, 18.0, Stroke::new(1.5, Color32::WHITE));
                }
                Tool::Lasso => {
                    let pts: Vec<Pos2> = (0..30).map(|i| h + Vec2::new(-60.0 + i as f32 * 2.0, (i as f32 * 0.4).sin() * 20.0)).collect();
                    painter.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::WHITE)));
                    painter.circle_filled(h, 3.0, Color32::WHITE);
                }
                _ => {}
            }
        }
        // line tool preview: a path with a width band and trees along it
        if self.tool == Tool::Line {
            let pts: Vec<Pos2> = (0..=20).map(|i| to(240.0 + i as f32 * 36.0, 800.0 + (i as f32 * 0.55).sin() * 70.0)).collect();
            let w = self.line_width * z * 2.0;
            painter.add(egui::Shape::line(pts.clone(), Stroke::new(w, Color32::from_rgba_unmultiplied(255, 255, 255, 28))));
            painter.add(egui::Shape::line(pts.clone(), Stroke::new(1.6, Color32::from_rgb(255, 220, 90))));
            for p in &pts {
                painter.circle_filled(*p, 3.5, Color32::from_rgb(255, 220, 90));
            }
            if self.line_trees {
                for p in &pts {
                    painter.circle_filled(*p, 8.0, Color32::from_rgba_unmultiplied(120, 168, 90, 200));
                }
            }
        }
        // snap indicator
        if self.snap && matches!(self.tool, Tool::Lasso | Tool::Polygon | Tool::Select) {
            let s = to(760.0, 300.0);
            painter.add(egui::Shape::convex_polygon(vec![s + Vec2::new(0.0, -8.0), s + Vec2::new(8.0, 0.0), s + Vec2::new(0.0, 8.0), s + Vec2::new(-8.0, 0.0)], Color32::TRANSPARENT, Stroke::new(1.6, Color32::from_rgb(255, 120, 220))));
            painter.text(s + Vec2::new(12.0, -14.0), Align2::LEFT_BOTTOM, "Snap: zone corner", FontId::proportional(10.5), Color32::from_rgb(255, 150, 230));
        }
        // scale bar and reference labels
        if self.show_scale_bar {
            let len = 100.0 * self.ref_scale * 0.5 * z * 3.0;
            let a = map_rect.left_bottom() + Vec2::new(14.0, -16.0);
            painter.line_segment([a, a + Vec2::new(len, 0.0)], Stroke::new(2.0, Color32::WHITE));
            painter.line_segment([a + Vec2::new(0.0, -4.0), a + Vec2::new(0.0, 4.0)], Stroke::new(2.0, Color32::WHITE));
            painter.line_segment([a + Vec2::new(len, -4.0), a + Vec2::new(len, 4.0)], Stroke::new(2.0, Color32::WHITE));
            painter.text(a + Vec2::new(len + 8.0, 0.0), Align2::LEFT_CENTER, "100 m", FontId::proportional(11.0), Color32::WHITE);
        }
        if self.ref_locked && !self.compare {
            painter.text(map_rect.right_top() + Vec2::new(-10.0, 8.0), Align2::RIGHT_TOP, "Reference map locked", FontId::proportional(11.0), Color32::from_white_alpha(170));
        }
    }
}

impl eframe::App for Mock {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::top("options").show(ui, |ui| self.options_bar(ui));
        if self.compare {
            egui::Panel::top("compare").show(ui, |ui| self.compare_bar(ui));
        }
        if self.dirty && !ctx.input(|i| i.pointer.any_down()) {
            self.render_live(&ctx);
        }
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Wand: click a region of the map to trace it.");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label("12,314 trees | GPU 38 ms"));
            });
        });
        egui::Panel::left("tools").resizable(false).exact_size(52.0).show(ui, |ui| self.tool_strip(ui));
        egui::Panel::left("zones").resizable(true).default_size(250.0).size_range(210.0..=380.0).show(ui, |ui| self.zones_panel(ui));
        egui::Panel::right("settings").resizable(true).default_size(340.0).size_range(280.0..=560.0).show(ui, |ui| self.settings(ui));
        egui::Panel::bottom("species").show(ui, |ui| self.species_strip(ui));
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.canvas(ui));

        if let Some((path, frames, asked)) = &mut self.shot {
            ctx.request_repaint();
            if let Some(img) = ctx.input(|i| i.raw.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None })) {
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(out) = image::RgbaImage::from_raw(img.width() as u32, img.height() as u32, rgba) {
                    let _ = out.save(&*path);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            *frames += 1;
            if *frames > 20 && !*asked {
                *asked = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Tree-inator UI mock-up").with_inner_size([1500.0, 950.0]),
        ..Default::default()
    };
    eframe::run_native("Tree-inator UI mock-up", options, Box::new(|cc| Ok(Box::new(Mock::new(&cc.egui_ctx)))))
}
