//! The tuning panel: tabs by what you are trying to do, each with Mixed Tuning (a few plain-language sliders that move
//! several settings together) and Fine Tuning (every individual setting, with a tooltip), and a search over all of them.

use super::simple::Slot;
use super::*;
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tab {
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
    pub(super) const ALL: [Tab; 8] = [Tab::Theme, Tab::Shape, Tab::Trees, Tab::Colour, Tab::Ground, Tab::Weather, Tab::Map, Tab::Export];

    /// The tab named by TREE_INATOR_TAB, for self tests.
    pub(super) fn from_env() -> Tab {
        let name = std::env::var("TREE_INATOR_TAB").unwrap_or_default().to_lowercase();
        Tab::ALL.into_iter().find(|t| t.name().to_lowercase().starts_with(&name) && !name.is_empty()).unwrap_or(Tab::Theme)
    }

    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

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
            Tab::Trees => "The trees themselves: which kinds, how big, how crowns overlap.",
            Tab::Colour => "Colour, brightness, shading and shadows.",
            Tab::Ground => "Grass, undergrowth, haze and stones between the trees.",
            Tab::Weather => "Snow and fire laid over the forest.",
            _ => "",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Gen,
    Light,
    Grading,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Slider,
    Percent,
    Check,
    Rgb,
    /// Drawn by its own function: pickers, dials and the like.
    Custom,
}

/// One individual setting, found by its path in the settings object it belongs to.
struct Row {
    path: &'static str,
    scope: Scope,
    label: &'static str,
    tab: Tab,
    group: &'static str,
    kind: Kind,
    lo: f32,
    hi: f32,
    tip: &'static str,
    aliases: &'static [&'static str],
    show: Option<fn(&GenParams) -> bool>,
}

#[allow(clippy::too_many_arguments)]
fn g(path: &'static str, label: &'static str, tab: Tab, group: &'static str, kind: Kind, lo: f32, hi: f32, tip: &'static str, aliases: &'static [&'static str]) -> Row {
    Row { path, scope: Scope::Gen, label, tab, group, kind, lo, hi, tip, aliases, show: None }
}

#[allow(clippy::too_many_arguments)]
fn l(path: &'static str, label: &'static str, tab: Tab, group: &'static str, kind: Kind, lo: f32, hi: f32, tip: &'static str, aliases: &'static [&'static str]) -> Row {
    Row { path, scope: Scope::Light, label, tab, group, kind, lo, hi, tip, aliases, show: None }
}

#[allow(clippy::too_many_arguments)]
fn gr(path: &'static str, label: &'static str, tab: Tab, group: &'static str, kind: Kind, lo: f32, hi: f32, tip: &'static str, aliases: &'static [&'static str]) -> Row {
    Row { path, scope: Scope::Grading, label, tab, group, kind, lo, hi, tip, aliases, show: None }
}

fn custom(id: &'static str, label: &'static str, tab: Tab, group: &'static str, tip: &'static str, aliases: &'static [&'static str]) -> Row {
    Row { path: id, scope: Scope::Gen, label, tab, group, kind: Kind::Custom, lo: 0.0, hi: 0.0, tip, aliases, show: None }
}

impl Row {
    fn when(mut self, f: fn(&GenParams) -> bool) -> Row {
        self.show = Some(f);
        self
    }
}

fn rows() -> Vec<Row> {
    use Kind::*;
    use Tab::*;
    vec![
        // ------------------------------------------------------------------ Shape
        custom("edge_style", "Edge style", Shape, "Outline", "Starting points for the edge and pattern settings. Pick one, then fine tune below.", &["preset", "look", "style", "natural edge", "ragged"]),
        custom("pattern", "Pattern", Shape, "Outline", "The overall shape of the forest edge: plain, fragmented islands, branching ribbons, clumps or lobes.", &["shape", "islands", "branching", "lobed", "clumps", "fragmented"]),
        g("pattern_strength", "Pattern strength", Shape, "Outline", Slider, 0.0, 1.0, "How strongly the pattern reshapes the drawn outline. 0 keeps your outline exactly as drawn.", &["shape strength", "amount of pattern"]),
        g("pattern_scale", "Pattern size", Shape, "Outline", Slider, 50.0, 1000.0, "Feature size of the pattern, in generated map pixels. Small gives fine detail, large gives big bays and islands.", &["feature size", "scale of pattern"]),
        g("branch_width", "Branch width", Shape, "Outline", Slider, 0.05, 1.0, "For the Branching pattern: how wide the forest ribbons are. Narrow gives thin river-like fingers, wide gives thick arms.", &["fingers", "ribbons", "arms"]).when(|g| g.pattern == Pattern::Branching),
        g("shape_softness", "Outline softness", Shape, "Outline", Slider, 5.0, 200.0, "How gradually the forest thins at its edge, in map pixels. Small values give a sharp wall of trees, large values a long soft fade.", &["feather", "fade", "blur edge", "transition", "soft edge"]),
        g("edge_warp", "Outline wobble", Shape, "Outline", Slider, 0.0, 300.0, "How far noise pushes the outline in and out from the line you drew. 0 follows the line exactly.", &["warp", "wander", "irregular", "wavy edge", "jagged"]),
        g("edge_noise", "Outline noise", Shape, "Outline", Slider, 0.0, 0.6, "Fine roughness along the edge, on top of the wobble.", &["rough", "jitter", "bumpy"]),
        custom("edge_mode", "Edge mode", Shape, "Outline", "Realistic draws individual trees all the way to the edge. Blurred fades a solid canopy out instead.", &["blurred", "realistic", "soft canopy"]),
        g("feather", "Blur feather", Shape, "Outline", Slider, 0.05, 1.0, "For Blurred edge mode: how wide the fade is.", &["blur width"]).when(|g| g.edge_mode == EdgeMode::Blurred),
        g("canopy_edge_depth", "Edge thinning", Shape, "Outline", Slider, 0.0, 1.0, "How far in from the edge the canopy takes to reach full cover. Larger means a wider thinning band of sparse trees.", &["fringe width", "thin edge", "sparse border"]),
        g("contain", "Keep everything inside my line", Shape, "Keep inside the line", Check, 0.0, 1.0, "Pulls the generation in so its fall-off, grass fringe and stray trees all end before the line you drew. Painted trees are exempt.", &["clip", "constrain", "boundary", "spill", "overflow", "inside", "contain", "fit"]),
        g("contain_fit", "Margin from the line", Shape, "Keep inside the line", Percent, 0.0, 2.0, "100% pulls in exactly as far as needed so nothing is cut by the line. More leaves an empty margin, less lets the fringe reach the line.", &["pull in", "inset", "padding", "gap to line", "fit"]).when(|g| g.contain),
        g("contain_clip", "Trim anything still crossing", Shape, "Keep inside the line", Check, 0.0, 1.0, "A safety net that cuts whatever still crosses the line. Turn off to keep the pulled-in edge fully natural.", &["clip", "cut"]).when(|g| g.contain),
        g("contain_shadows", "Keep shadows inside too", Shape, "Keep inside the line", Check, 0.0, 1.0, "Off lets shadows fall outside the area, the way a real forest's would.", &["shadow spill", "outside shadows"]).when(|g| g.contain),
        g("gaps", "Treefall gaps", Shape, "Clearings", Slider, 0.0, 1.0, "How many clearings appear inside the forest, where trees have fallen.", &["clearings", "glades", "holes", "openings", "meadows"]),
        g("gap_size", "Gap size", Shape, "Clearings", Slider, 10.0, 120.0, "Typical width of a treefall gap, in generated map pixels.", &["clearing size", "glade size", "hole size"]),
        // ------------------------------------------------------------------ Trees
        custom("seed", "Variation", Trees, "Which trees", "The random seed. The same settings with a different seed grow different trees. R rerolls it.", &["seed", "reroll", "random", "new variation", "shuffle"]),
        custom("species", "Tree types", Trees, "Which trees", "The kind of tree for the main canopy, the second kind mixed in, and the shrubs.", &["species", "main trees", "mixed in", "shrubs", "tree type", "pine", "oak", "birch", "palm"]),
        g("mix_amount", "Mix amount", Trees, "Which trees", Percent, 0.0, 1.0, "How much of the second tree type is mixed into the canopy, in clumps.", &["second species", "mix", "mixed forest", "diversity"]),
        g("mix_scale", "Mix clump size", Trees, "Which trees", Slider, 40.0, 800.0, "Size of the clumps the second tree type grows in, in generated map pixels.", &["patch size of mix"]),
        g("scale", "Tree size", Trees, "Size", Percent, 0.1, 4.0, "Makes every tree, leaf, shrub and shadow bigger or smaller on the same drawn area. Use it to match your map's scale.", &["scale", "bigger trees", "smaller trees", "crown size", "zoom", "larger", "tiny", "huge", "forest scale"]),
        g("age", "Stand age (years)", Trees, "Size", Slider, 0.0, 400.0, "How old the forest is. 0 shows it as drawn, mature. Older ages grow the crowns along each tree type's own curve, up to the largest size that type really reaches. Young stands are open with many shrubs, mature ones are closed, very old ones open up again with gaps and a wider spread of heights.", &["growth", "young forest", "old growth", "mature", "saplings", "years", "ageing", "aging", "succession"]),
        g("crown_min", "Smallest crown", Trees, "Size", Slider, 3.0, 40.0, "Radius of the smallest canopy tree, in generated map pixels.", &["minimum crown", "small trees", "crown min"]),
        g("crown_max", "Largest crown", Trees, "Size", Slider, 4.0, 60.0, "Radius of the largest canopy tree, in generated map pixels.", &["maximum crown", "big trees", "crown max"]),
        g("canopy_cover", "Coverage", Trees, "Density", Percent, 0.05, 1.0, "How much of the ground the crowns cover. Low is sparse woodland with the ground showing, high is a closed canopy.", &["density", "thick", "thin", "sparse", "crowded", "cover", "fuller", "denser", "fewer trees", "more trees", "canopy cover"]),
        g("canopy_patch_size", "Cover patch size", Trees, "Density", Slider, 30.0, 500.0, "Size of the patches the cover varies in, in generated map pixels. Small patches give fine clumping, large ones broad groves.", &["clump size", "grove size", "clustering"]),
        g("shrub_density", "Shrubs", Trees, "Density", Slider, 0.0, 3.0, "How many shrubs grow under and around the trees.", &["bushes", "undergrowth", "understory"]),
        g("lone_groups", "Lone tree groups", Trees, "Density", Slider, 0.0, 5.0, "Small groups of trees standing outside the forest edge.", &["outliers", "strays", "scattered trees", "isolated"]),
        g("lone_reach", "Lone tree reach", Trees, "Density", Slider, 0.0, 1.0, "How far outside the edge the lone trees stray.", &["stray distance"]),
        g("interior_lone", "Lone trees inside", Trees, "Density", Slider, 0.0, 1.0, "Lone trees scattered inside the patch too, for savanna and open woodland.", &["savanna", "open woodland", "scattered inside"]),
        g("depth.height_var", "Crown height variation", Trees, "Crowns", Percent, 0.0, 1.5, "Random crown heights, so taller crowns shade the ones beside them. Low is flat and uniform.", &["tall", "short", "emergent", "relief", "height"]),
        g("depth.crown_var", "Neighbour difference", Trees, "Crowns", Percent, 0.0, 1.5, "How different neighbouring crowns look in brightness and colour, so clusters follow crown outlines.", &["variation between trees", "individual tone"]),
        g("depth.weave", "Canopy weave", Trees, "Crowns", Percent, 0.0, 1.0, "Neighbouring crowns of similar height interleave at their edges instead of one dome hiding the next, as in aerial photos. 0 is off.", &["interleave", "overlap", "merge", "blend crowns", "overlapping", "woven"]),
        g("crown_lean", "Crown lean", Trees, "Crowns", Percent, 0.0, 1.0, "Crowns grow away from crowded neighbours toward open space, so a stand is not a lattice of identical discs. 0 is off.", &["competition", "asymmetric crowns", "light competition", "lean", "grow toward light"]),
        g("clump", "Tree clusters", Trees, "Density", Percent, 0.0, 1.0, "Trees gather in clusters with open ground between them, like groves and thickets. 0 is an even scatter.", &["groves", "thickets", "clumping", "clustered", "patchy trees", "gather"]),
        g("clump_size", "Cluster size", Trees, "Density", Slider, 20.0, 250.0, "Typical radius of one cluster of trees, in generated map pixels.", &["grove size", "thicket size"]).when(|g| g.clump > 0.0),
        g("crown_shyness", "Crown shyness", Trees, "Crowns", Check, 0.0, 1.0, "Thin gaps where neighbouring crowns of similar height meet, as in real canopies. Applies to trees and mushrooms.", &["gap between trees", "crown gap", "separation", "spacing", "shy"]),
        g("shyness_gap", "Gap width", Trees, "Crowns", Slider, 0.5, 6.0, "Width of the gaps between crowns, in map pixels. Small crowns get thinner gaps.", &["crown gap width"]).when(|g| g.crown_shyness),
        g("shyness_giants_only", "Only giant mushrooms", Trees, "Crowns", Check, 0.0, 1.0, "Only the giant mushrooms keep gaps; trees and small mushrooms overlap as before.", &["giants only"]).when(|g| g.crown_shyness),
        g("mushrooms", "Mushrooms", Trees, "Mushrooms", Slider, 0.0, 4.0, "How many mushroom clusters grow on open ground and in clearings. 0 is none.", &["fungus", "toadstool", "fungi"]),
        g("giant_mushrooms", "Giant mushrooms", Trees, "Mushrooms", Percent, 0.0, 0.6, "Share of the big trees that are mushrooms as tall as trees.", &["fungus", "toadstool", "fantasy", "huge mushrooms"]),
        custom("mushroom_kinds", "Mushroom kinds", Trees, "Mushrooms", "Which kinds of mushroom grow.", &["redcap", "puffball", "violet cap", "funnel"]),
        custom("species_editor", "Species editor", Trees, "Species", "Edit the colours, height and tree type of each species by hand.", &["leaf colour", "ramp", "bark", "height", "edit species", "tree colours"]),
        // ------------------------------------------------------------------ Colour and light
        g("leaf.variety", "Colour variety", Colour, "Leaves", Percent, 0.0, 3.0, "Each leaf a little lighter, darker, yellower or bluer.", &["colour noise", "leaf variation", "lushness", "vibrant"]),
        g("leaf.mottling", "Mottling", Colour, "Leaves", Percent, 0.0, 3.0, "Broad lighter and darker patches across the leaves.", &["blotches", "patches of tone"]),
        g("leaf.mottle_size", "Mottling size", Colour, "Leaves", Slider, 4.0, 40.0, "Size of the mottled patches, in pixels.", &["patch size of tone"]),
        g("leaf.dappled", "Sun-dappled tops", Colour, "Leaves", Percent, 0.0, 1.5, "Sun-facing clumps warm up toward yellow.", &["sunny", "warm tops", "highlights"]),
        g("leaf.tree_lean", "Per-tree colour lean", Colour, "Leaves", Percent, 0.0, 1.5, "Some whole trees lean lime, some lean blue-green.", &["tree to tree", "individual colour"]),
        g("depth.crown_tone", "Patch tone", Colour, "Crowns", Percent, 0.0, 2.0, "Broad lighter and darker patches across crowns.", &["blotches", "tone"]),
        g("depth.crown_tone_size", "Tone patch size", Colour, "Crowns", Slider, 12.0, 120.0, "Size of those lighter and darker patches, in pixels.", &["blotch size"]),
        g("depth.canopy_ao", "Occlusion", Colour, "Crowns", Percent, 0.0, 1.5, "Darkens the crevices between and below crowns from the shape of the canopy surface, which adds depth. 0 is off.", &["ambient occlusion", "ao", "crevices", "contact shadow", "depth", "dark between crowns"]),
        g("depth.translucency", "Leaf glow", Colour, "Crowns", Percent, 0.0, 2.0, "Light shining through thin leaves on the side of a crown away from the sun. 0 is off.", &["translucency", "backlight", "glow", "subsurface", "see through leaves"]),
        g("depth.lumps", "Crown lumps", Colour, "Crowns", Percent, 0.0, 2.0, "Lets the clumps of leaves and branches inside a crown catch the light, not only its overall dome. 0 is off.", &["bumpy", "normals", "relief", "texture of crowns", "sculpted"]),
        g("depth.crown_shade", "Crown shading", Colour, "Crowns", Percent, 0.0, 1.5, "Crown tops lit, gaps between crowns dark. Higher gives more depth between neighbouring crowns.", &["depth", "ambient occlusion", "dark gaps", "shading"]),
        g("depth.shade_radius", "Shading reach", Colour, "Crowns", Slider, 4.0, 40.0, "How far away a crown is compared with to decide if it is high or low, in map pixels.", &["occlusion radius"]),
        g("depth.cluster_tone", "Clump colour", Colour, "Crowns", Percent, 0.0, 2.0, "How much whole leaf clumps share a colour.", &["cluster", "group colour"]),
        g("depth.depth_strength", "Depth darkening", Colour, "Crowns", Percent, 0.0, 2.0, "How much low and deep parts of a crown darken.", &["inner shadow", "depth"]),
        g("depth.gain", "Crown brightness", Colour, "Crowns", Slider, 0.5, 2.0, "Overall brightness of the canopy. Raises or lowers all crown colours together.", &["lighter", "darker", "exposure", "dim", "bright", "gain"]),
        g("depth.gamma", "Shadow lift", Colour, "Crowns", Slider, 0.4, 1.4, "Below 1 lifts shadows and midtones and flattens the look. Above 1 deepens the shadows.", &["gamma", "contrast", "midtones", "flat", "punchy"]),
        g("depth.smooth", "Canopy smoothing", Colour, "Crowns", Percent, 0.0, 1.5, "Flattens each crown into a lobe of one tone with crisp edges and clean gaps, as in aerial photos. 0 is off. Large crowns up close are limited so it stays fast.", &["flatten", "simplify", "kuwahara", "clean", "lobes"]),
        l("shadows", "Cast shadows", Colour, "Shadows", Check, 0.0, 1.0, "Show shadows on trees and ground. Off is useful when you add shadows on your own map.", &["shadow", "no shadows", "drop shadow"]),
        l("flat", "Flat light", Colour, "Shadows", Check, 0.0, 1.0, "Removes the light direction from crown shading, leaving only the occlusion.", &["no direction", "ambient only", "overcast", "even light"]),
        custom("shadow_dir", "Shadow direction", Colour, "Shadows", "The way shadows fall. Drag the dial or use a corner.", &["sun direction", "light direction", "angle", "up left", "down right", "sun position"]),
        l("elevation", "Sun elevation", Colour, "Shadows", Slider, 10.0, 90.0, "How high the sun is, in degrees. Low light shades crowns more strongly.", &["sun height", "sun angle", "noon", "evening", "morning"]),
        l("shadow_length", "Shadow length", Colour, "Shadows", Slider, 0.2, 4.0, "How far cast shadows reach. Long shadows suggest a low sun.", &["long shadows", "short shadows", "evening"]),
        l("shadow_softness", "Shadow softness", Colour, "Shadows", Slider, 0.0, 4.0, "How blurred shadow edges are. Hard edges look like clear midday sun.", &["blur", "diffuse", "overcast", "penumbra", "hard shadows"]),
        gr("shadow_opacity", "Shadow opacity", Colour, "Shadows", Slider, 0.0, 1.0, "How dark the cast shadows are.", &["shadow strength", "shadow darkness", "faint shadows"]),
        gr("shadow_color", "Shadow colour", Colour, "Shadows", Rgb, 0.0, 1.0, "The colour shadows are tinted with.", &["shadow tint", "shadow hue"]),
        custom("grading", "Layers and colour grading", Colour, "Grading", "Hue, saturation, brightness, contrast and tint for each layer or the whole forest.", &["saturation", "hue", "tint", "levels", "grade", "layers", "warmth", "colour temperature", "contrast"]),
        // ------------------------------------------------------------------ Ground
        g("grass_reach", "Grass reach", Ground, "Ground cover", Slider, 0.0, 1.0, "How far from the forest edge the grass spreads outward.", &["spread", "fringe", "grass distance"]),
        g("grass_density", "Grass density", Ground, "Ground cover", Slider, 0.0, 3.0, "Amount of grass and ground cover between and around the trees.", &["lawn", "turf", "undergrowth", "forest floor", "bare ground"]),
        g("pebbles", "Pebble shrubs", Ground, "Ground cover", Slider, 0.0, 5.0, "Small round shrubs and stones scattered on the ground.", &["rocks", "stones", "boulders"]),
        g("grass_color", "Ground colour", Ground, "Colours", Rgb, 0.0, 1.0, "The main colour of the ground cover.", &["grass colour", "soil", "floor colour", "sand"]),
        g("grass_accent", "Ground accent", Ground, "Colours", Rgb, 0.0, 1.0, "Patches and flecks of this colour break up the ground cover: leaf litter, embers, snow.", &["flecks", "litter", "dirt"]),
        g("haze_reach", "Haze reach", Ground, "Haze", Slider, 0.1, 1.0, "How far the haze reaches outward from the forest.", &["mist reach", "fog spread"]),
        g("haze_opacity", "Haze opacity", Ground, "Haze", Slider, 0.0, 0.8, "Strength of the soft darkening around and under the forest, like moss and shade.", &["mist", "fog", "smoke", "atmosphere", "shade"]),
        g("haze_color", "Haze colour", Ground, "Haze", Rgb, 0.0, 1.0, "The colour of the haze.", &["mist colour", "fog colour"]),
        // ------------------------------------------------------------------ Weather
        custom("snow_look", "Snow looks", Weather, "Snow", "Ready-made snow looks. Pick one, then fine tune.", &["winter", "frost", "white"]),
        g("snow.amount", "Snow amount", Weather, "Snow", Percent, 0.0, 1.0, "How much snow there is overall. 0 turns snow off.", &["winter", "frost", "white", "cold", "ice", "snowfall"]),
        custom("snow_mode", "Snow where", Weather, "Snow", "Everywhere, only near the outer edge, or rising across the map toward one side.", &["snow line", "gradient", "altitude", "edge snow"]),
        g("snow.reach", "Snow edge width", Weather, "Snow", Slider, 20.0, 600.0, "How far in from the outer edge the snow reaches.", &["snowline reach"]).when(|g| g.snow.mode == SnowMode::Edge),
        custom("snow_angle", "Snow rises toward", Weather, "Snow", "The side of the map the snow gets heavier toward.", &["snow direction"]),
        g("snow.tops", "Clings to tops", Weather, "Snow", Percent, 0.0, 1.0, "100% keeps snow on the upward facing tops of crowns, 0% covers crowns evenly.", &["canopy snow", "crown snow"]),
        g("snow.ground", "Ground cover", Weather, "Snow", Percent, 0.0, 1.0, "How much of the grass and ground turns white.", &["snow on ground"]),
        g("snow.patchiness", "Snow patchiness", Weather, "Snow", Percent, 0.0, 1.0, "Breaks the snow into patches.", &["patchy snow"]),
        g("snow.tint", "Snow colour", Weather, "Snow", Rgb, 0.0, 1.0, "The colour of the snow.", &["snow tint"]),
        custom("fire_look", "Fire looks", Weather, "Fire", "Ready-made burnt looks. Pick one, then fine tune.", &["burnt", "wildfire", "ash"]),
        g("fire.amount", "Burn amount", Weather, "Fire", Percent, 0.0, 1.0, "How much of the forest has burnt. 0 turns fire off.", &["burnt", "wildfire", "ash", "flames", "burn", "fire amount"]),
        custom("fire_mode", "Fire where", Weather, "Fire", "Everywhere, near the outer edge, or sweeping across the map as a front.", &["fire front", "edge fire"]),
        g("fire.reach", "Fire edge width", Weather, "Fire", Slider, 20.0, 600.0, "How far in from the outer edge the fire reaches.", &["burn reach"]).when(|g| g.fire.mode == FireMode::Edge),
        custom("fire_angle", "Fire comes from", Weather, "Fire", "The side of the map the fire front comes from.", &["fire direction"]),
        g("fire.flames", "Flames", Weather, "Fire", Percent, 0.0, 1.0, "How much of the burnt area is still alight.", &["burning", "alight"]),
        g("fire.char", "Charring", Weather, "Fire", Percent, 0.0, 1.0, "How black the burnt crowns and ground get.", &["black", "scorch", "soot"]),
        g("fire.embers", "Embers", Weather, "Fire", Percent, 0.0, 1.0, "Glowing embers and beds on the ground.", &["glowing", "coals"]),
        g("fire.glow", "Glow", Weather, "Fire", Percent, 0.0, 1.0, "Warm light the flames throw on nearby trees.", &["firelight", "orange light", "heat"]),
        g("fire.patchiness", "Fire patchiness", Weather, "Fire", Percent, 0.0, 1.0, "Breaks the fire into patches.", &["patchy fire"]),
    ]
}

/// A plain-language slider that moves several settings together. `get` reads it back from the settings, so the
/// slider always shows where the settings actually are.
struct Intent {
    label: &'static str,
    tab: Tab,
    percent: bool,
    lo: f32,
    hi: f32,
    tip: &'static str,
    aliases: &'static [&'static str],
    get: fn(&Project) -> f32,
    set: fn(&mut Project, f32),
}

fn intents() -> Vec<Intent> {
    use Tab::*;
    vec![
        Intent {
            label: "Denseness",
            tab: Shape,
            percent: true,
            lo: 0.05,
            hi: 1.0,
            tip: "How much of the ground the crowns cover. Low is sparse woodland with the ground showing, high is a closed canopy. Sets Coverage.",
            aliases: &["density", "thick", "thin", "sparse", "crowded", "fuller", "denser", "fewer trees", "more trees"],
            get: |p| p.gen.canopy_cover,
            set: |p, v| p.gen.canopy_cover = v,
        },
        Intent {
            label: "Clumpiness",
            tab: Shape,
            percent: true,
            lo: 0.0,
            hi: 1.0,
            tip: "Smooth and even at 0, groups of trees with gaps between them at 1. Moves Patch size and Treefall gaps together.",
            aliases: &["patchy", "clusters", "groves", "uneven", "natural", "random", "grouping", "clustering"],
            get: |p| ((400.0 - p.gen.canopy_patch_size) / 360.0).clamp(0.0, 1.0),
            set: |p, v| {
                p.gen.canopy_patch_size = 400.0 - 360.0 * v;
                p.gen.gaps = 0.1 + 0.6 * v;
            },
        },
        Intent {
            label: "Edge raggedness",
            tab: Shape,
            percent: true,
            lo: 0.0,
            hi: 1.0,
            tip: "How rough the forest's outer edge is. Low gives a tidy fringe, high gives a broken, wandering edge. Moves Outline wobble and Outline noise.",
            aliases: &["edge", "border", "fringe", "outline", "rough", "wobble", "messy", "boundary", "jagged"],
            get: |p| (p.gen.edge_warp / 200.0).clamp(0.0, 1.0),
            set: |p, v| {
                p.gen.edge_warp = 200.0 * v;
                p.gen.edge_noise = 0.4 * v;
            },
        },
        Intent {
            label: "Openness",
            tab: Shape,
            percent: true,
            lo: 0.0,
            hi: 1.0,
            tip: "Clearings inside the forest where trees have fallen or never grew. More openness means more glades. Sets Treefall gaps.",
            aliases: &["clearings", "gaps", "glades", "holes", "meadows", "open", "treefall"],
            get: |p| p.gen.gaps,
            set: |p, v| p.gen.gaps = v,
        },
        Intent {
            label: "Tree size",
            tab: Trees,
            percent: true,
            lo: 0.1,
            hi: 4.0,
            tip: "Makes every tree, leaf, shrub and shadow bigger or smaller on the same drawn area. Use it to match your map's scale.",
            aliases: &["scale", "bigger trees", "smaller trees", "crown size", "zoom", "size", "larger", "tiny", "huge"],
            get: |p| p.gen.scale,
            set: |p, v| p.gen.scale = v,
        },
        Intent {
            label: "Species mix",
            tab: Trees,
            percent: true,
            lo: 0.0,
            hi: 1.0,
            tip: "How much of the second tree type is mixed into the canopy, in clumps. Sets Mix amount.",
            aliases: &["mix", "second species", "diversity", "mixed forest", "different trees"],
            get: |p| p.gen.mix_amount,
            set: |p, v| p.gen.mix_amount = v,
        },
        Intent {
            label: "Maturity",
            tab: Trees,
            percent: true,
            lo: 0.0,
            hi: 1.5,
            tip: "Young, even stands at 0 to old, uneven ones at 1 and above: taller emergents and more difference between neighbouring crowns. Moves Crown height variation and Crown to crown difference.",
            aliases: &["age", "old growth", "young", "height variation", "tall trees", "uneven"],
            get: |p| p.gen.depth.height_var / 1.2,
            set: |p, v| {
                p.gen.depth.height_var = 1.2 * v;
                p.gen.depth.crown_var = (0.1 + 0.3 * v).min(1.0);
            },
        },
        Intent {
            label: "Wildness",
            tab: Trees,
            percent: true,
            lo: 0.0,
            hi: 1.0,
            tip: "Orderly woodland at 0 to untidy, wild forest at 1: more lone trees, scattered inside the patch too. Moves Lone tree groups and Lone trees inside.",
            aliases: &["messy", "natural", "tidy", "untamed", "random", "stray trees"],
            get: |p| (p.gen.lone_groups / 2.5).clamp(0.0, 1.0),
            set: |p, v| {
                p.gen.lone_groups = 2.5 * v;
                p.gen.interior_lone = 0.3 * v;
            },
        },
        Intent {
            label: "Brightness",
            tab: Colour,
            percent: false,
            lo: 0.5,
            hi: 2.0,
            tip: "Overall brightness of the canopy. Raises or lowers all crown colours together.",
            aliases: &["lighter", "darker", "exposure", "light", "dark", "dim", "bright"],
            get: |p| p.gen.depth.gain,
            set: |p, v| p.gen.depth.gain = v,
        },
        Intent {
            label: "Contrast",
            tab: Colour,
            percent: false,
            lo: 0.4,
            hi: 1.4,
            tip: "Below 1 lifts the shadows and flattens the look, above 1 deepens shadows and sharpens crown shapes. The reverse of Shadow lift.",
            aliases: &["gamma", "shadow lift", "punchy", "flat", "harsh", "soft"],
            get: |p| 1.8 - p.gen.depth.gamma,
            set: |p, v| p.gen.depth.gamma = 1.8 - v,
        },
        Intent {
            label: "Warmth",
            tab: Colour,
            percent: false,
            lo: -1.0,
            hi: 1.0,
            tip: "Shifts all colours toward yellow-orange (warm, summer) or blue-green (cool, damp). Sets the tint of the whole forest grading.",
            aliases: &["colour temperature", "color temperature", "yellow", "blue", "cool", "warm", "autumn", "tint", "cold"],
            get: |p| (p.grading.global.tint[0] - p.grading.global.tint[2]) / 0.38,
            set: |p, v| p.grading.global.tint = [1.0 + 0.18 * v, 1.0 + 0.03 * v, 1.0 - 0.2 * v],
        },
        Intent {
            label: "Lushness",
            tab: Colour,
            percent: true,
            lo: 0.0,
            hi: 1.5,
            tip: "How varied and rich the leaf colours are. Low is uniform and dull, high is lively with mottled patches. Moves Colour variety and Mottling.",
            aliases: &["saturation", "vibrant", "colourful", "colorful", "variety", "dull", "vivid"],
            get: |p| p.gen.leaf.variety / 2.0,
            set: |p, v| {
                p.gen.leaf.variety = 2.0 * v;
                p.gen.leaf.mottling = 1.2 * v;
            },
        },
        Intent {
            label: "Undergrowth",
            tab: Ground,
            percent: true,
            lo: 0.0,
            hi: 2.0,
            tip: "How much grass, shrubs and stones fill the ground under and around the trees. Moves Grass density, Shrubs and Pebble shrubs.",
            aliases: &["grass", "shrubs", "forest floor", "bushes", "bare", "ground cover"],
            get: |p| p.gen.grass_density,
            set: |p, v| {
                p.gen.grass_density = v;
                p.gen.shrub_density = v;
                p.gen.pebbles = v;
            },
        },
        Intent {
            label: "Haze",
            tab: Ground,
            percent: true,
            lo: 0.0,
            hi: 0.8,
            tip: "A soft darkening of the ground and canopy edge, like moss and shade, around the forest. Sets Haze opacity.",
            aliases: &["mist", "fog", "smoke", "atmosphere", "shade"],
            get: |p| p.gen.haze_opacity,
            set: |p, v| p.gen.haze_opacity = v,
        },
        Intent {
            label: "Snow",
            tab: Weather,
            percent: true,
            lo: 0.0,
            hi: 1.0,
            tip: "How much snow lies on the forest overall. Sets Snow amount.",
            aliases: &["winter", "frost", "white", "cold", "ice"],
            get: |p| p.gen.snow.amount,
            set: |p, v| p.gen.snow.amount = v,
        },
        Intent {
            label: "Burn",
            tab: Weather,
            percent: true,
            lo: 0.0,
            hi: 1.0,
            tip: "How much of the forest has burnt: charred ground, embers and flames. Sets Burn amount.",
            aliases: &["burnt", "wildfire", "ash", "flames", "fire"],
            get: |p| p.gen.fire.amount,
            set: |p, v| p.gen.fire.amount = v,
        },
    ]
}

// ---------------------------------------------------------------------- search

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

/// How well one query word matches a field: a whole word, the start of a word, inside a word, or a typo away.
fn word_score(q: &str, field: &str) -> f32 {
    let mut best = 0.0f32;
    for w in field.split(|c: char| !c.is_alphanumeric()) {
        if w.is_empty() {
            continue;
        }
        let w = w.to_lowercase();
        let s = if w == q {
            1.0
        } else if q.len() >= 2 && w.starts_with(q) {
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

#[derive(Clone, Copy)]
enum Item {
    Intent(usize),
    Row(usize),
}

struct Hit {
    item: Item,
    alias: Option<&'static str>,
}

/// Settings ranked for a query. Every word must match somewhere, in the label, the aliases or the tooltip; labels
/// count most. A whole alias phrase typed out ("bigger trees") beats word by word matches. Typos within a letter or
/// two still match.
fn search(rows: &[Row], intents: &[Intent], query: &str) -> Vec<Hit> {
    let q = query.to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(f32, Hit)> = Vec::new();
    let fields: Vec<(Item, &str, &[&'static str], &str, bool)> = intents
        .iter()
        .enumerate()
        .map(|(i, x)| (Item::Intent(i), x.label, x.aliases, x.tip, true))
        .chain(rows.iter().enumerate().map(|(i, x)| (Item::Row(i), x.label, x.aliases, x.tip, false)))
        .collect();
    for (item, label, aliases, tip, mixed) in fields {
        let (mut total, mut hit_alias, mut ok) = (0.0f32, None, true);
        for w in &words {
            let lbl = word_score(w, label);
            let (mut a, mut which) = (0.0f32, None);
            for al in aliases {
                let s = word_score(w, al);
                if s > a {
                    a = s;
                    which = Some(*al);
                }
            }
            let t = word_score(w, tip);
            let best = lbl.max(a * 0.95).max(t * 0.4);
            if best <= 0.0 {
                ok = false;
                break;
            }
            if a * 0.95 > lbl && which.is_some() {
                hit_alias = which;
            }
            total += best;
        }
        if !ok {
            continue;
        }
        for al in aliases {
            if al.to_lowercase() == q {
                total += words.len() as f32;
                hit_alias = Some(*al);
            }
        }
        let mut score = total / words.len() as f32;
        if !mixed {
            score *= 0.97;
        }
        scored.push((score, Hit { item, alias: hit_alias }));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(10).map(|(_, h)| h).collect()
}

// ---------------------------------------------------------------------- the settings, as editable values

/// The three settings objects as JSON values, so rows can be drawn from a table; changed ones go back after.
struct Vals {
    gen: Value,
    light: Value,
    grading: Value,
    base_gen: Value,
    base_light: Value,
    base_grading: Value,
    touched: [bool; 3],
    /// The generation settings as typed values, for the rows' show conditions.
    typed: GenParams,
    base_project: Project,
}

impl Vals {
    fn load(app: &TreeApp) -> Vals {
        let base_gen_typed = app.gen_base.clone().unwrap_or_default();
        let base_project = Project { gen: base_gen_typed.clone(), ..Default::default() };
        Vals {
            gen: serde_json::to_value(&app.project.gen).unwrap_or_default(),
            light: serde_json::to_value(&app.project.light).unwrap_or_default(),
            grading: serde_json::to_value(&app.project.grading).unwrap_or_default(),
            base_gen: serde_json::to_value(&base_gen_typed).unwrap_or_default(),
            base_light: serde_json::to_value(Light::default()).unwrap_or_default(),
            base_grading: serde_json::to_value(Grading::default()).unwrap_or_default(),
            touched: [false; 3],
            typed: app.project.gen.clone(),
            base_project,
        }
    }

    /// Write changed values back into the project.
    fn flush(&mut self, app: &mut TreeApp) {
        if self.touched[0] {
            if let Ok(g) = serde_json::from_value::<GenParams>(self.gen.clone()) {
                app.project.gen = g;
            }
        }
        if self.touched[1] {
            if let Ok(x) = serde_json::from_value::<Light>(self.light.clone()) {
                app.project.light = x;
            }
        }
        if self.touched[2] {
            if let Ok(x) = serde_json::from_value::<Grading>(self.grading.clone()) {
                app.project.grading = x;
            }
        }
        self.touched = [false; 3];
    }

    /// Read the project again, after something else changed it.
    fn reload(&mut self, app: &TreeApp) {
        self.gen = serde_json::to_value(&app.project.gen).unwrap_or_default();
        self.light = serde_json::to_value(&app.project.light).unwrap_or_default();
        self.grading = serde_json::to_value(&app.project.grading).unwrap_or_default();
        self.typed = app.project.gen.clone();
    }

    fn cur(&self, s: Scope) -> &Value {
        match s {
            Scope::Gen => &self.gen,
            Scope::Light => &self.light,
            Scope::Grading => &self.grading,
        }
    }

    fn base(&self, s: Scope) -> &Value {
        match s {
            Scope::Gen => &self.base_gen,
            Scope::Light => &self.base_light,
            Scope::Grading => &self.base_grading,
        }
    }

    fn set(&mut self, s: Scope, path: &str, v: Value) {
        let idx = match s {
            Scope::Gen => 0,
            Scope::Light => 1,
            Scope::Grading => 2,
        };
        let root = match s {
            Scope::Gen => &mut self.gen,
            Scope::Light => &mut self.light,
            Scope::Grading => &mut self.grading,
        };
        let mut cur = root;
        let mut it = path.split('.').peekable();
        while let Some(k) = it.next() {
            if it.peek().is_none() {
                if let Some(o) = cur.as_object_mut() {
                    o.insert(k.to_string(), v);
                }
                break;
            }
            match cur.get_mut(k) {
                Some(n) => cur = n,
                None => return,
            }
        }
        self.touched[idx] = true;
    }
}

fn pointer<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur.get(k)?;
    }
    Some(cur)
}

pub(super) const LABEL_W: f32 = 140.0;
const RESET_W: f32 = 20.0;
const VALUE_W: f32 = 66.0;

fn same(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => {
            if let (Some(p), Some(q)) = (x.as_f64(), y.as_f64()) {
                (p - q).abs() < 1e-4
            } else {
                x == y
            }
        }
        _ => true,
    }
}

/// The label column, with a dot when the setting differs from what it is measured against.
fn label_cell(ui: &mut egui::Ui, text: &str, changed: bool, tip: &str) {
    let rich = if changed { egui::RichText::new(format!("\u{2022} {text}")).color(ui.visuals().hyperlink_color) } else { egui::RichText::new(format!("  {text}")) };
    ui.allocate_ui_with_layout(Vec2::new(LABEL_W, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(Vec2::new(LABEL_W, 20.0));
        ui.add(egui::Label::new(rich).truncate()).on_hover_text(tip);
    });
}

/// The reset column: a button when changed, empty space of the same width otherwise, so the controls line up.
fn reset_cell(ui: &mut egui::Ui, changed: bool, tip: &str) -> bool {
    if changed {
        ui.add_sized([RESET_W, 20.0], egui::Button::new("\u{21ba}").small().frame(false)).on_hover_text(tip).clicked()
    } else {
        ui.allocate_space(Vec2::new(RESET_W, 20.0));
        false
    }
}

/// A slider followed by a fixed width value box, so every row lines up whatever the numbers look like.
fn slider_box(ui: &mut egui::Ui, value: &mut f32, lo: f32, hi: f32, pct: bool, log: bool, tip: &str) -> bool {
    let width = slider_width(ui);
    ui.spacing_mut().slider_width = width;
    let mut shown = if pct { *value * 100.0 } else { *value };
    let (lo, hi) = if pct { (lo * 100.0, hi * 100.0) } else { (lo, hi) };
    let mut changed = ui.add(egui::Slider::new(&mut shown, lo..=hi).show_value(false).logarithmic(log)).on_hover_text(tip).changed();
    let decimals = if pct || hi - lo > 50.0 { 0 } else { 2 };
    let mut dv = egui::DragValue::new(&mut shown).range(lo..=hi).max_decimals(decimals).min_decimals(decimals).speed(((hi - lo) / 300.0) as f64);
    if pct {
        dv = dv.suffix("%");
    }
    changed |= ui.add_sized([VALUE_W, 20.0], dv).on_hover_text(tip).changed();
    if changed {
        *value = if pct { shown / 100.0 } else { shown };
    }
    changed
}

fn slider_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() - VALUE_W - RESET_W - 3.0 * ui.spacing().item_spacing.x).clamp(50.0, 400.0)
}

/// The name of a setting from its place in the generation, light or grading settings, for lists of changes.
pub(super) fn setting_label(scope: &str, path: &str) -> String {
    let want = match scope {
        "light" => Scope::Light,
        "grading" => Scope::Grading,
        _ => Scope::Gen,
    };
    if let Some(r) = rows().iter().find(|r| r.scope == want && r.path == path) {
        return r.label.to_string();
    }
    let leaf = path.rsplit('.').next().unwrap_or(path).replace('_', " ");
    let mut c = leaf.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => leaf,
    }
}

impl TreeApp {
    fn reset_tip(&self) -> &'static str {
        if self.gen_base.is_some() {
            "Back to what this group inherits"
        } else {
            "Back to the default"
        }
    }

    /// One scalar setting from the table.
    fn scalar_row(&mut self, ui: &mut egui::Ui, v: &mut Vals, row: &Row) {
        let cur = pointer(v.cur(row.scope), row.path).cloned();
        let base = pointer(v.base(row.scope), row.path).cloned();
        let Some(cur) = cur else { return };
        let changed = !same(Some(&cur), base.as_ref());
        let mut new: Option<Value> = None;
        let mut reset = false;
        let tip = row.tip;
        ui.horizontal(|ui| {
            label_cell(ui, row.label, changed, tip);
            match row.kind {
                Kind::Check => {
                    let mut b = cur.as_bool().unwrap_or(false);
                    if ui.checkbox(&mut b, "").on_hover_text(tip).changed() {
                        new = Some(Value::Bool(b));
                    }
                }
                Kind::Slider | Kind::Percent => {
                    let mut x = cur.as_f64().unwrap_or(0.0) as f32;
                    if slider_box(ui, &mut x, row.lo, row.hi, row.kind == Kind::Percent, row.path == "scale", tip) {
                        new = Some(serde_json::json!(x));
                    }
                }
                Kind::Rgb => {
                    let mut c = [0.0f32; 3];
                    if let Some(a) = cur.as_array() {
                        for (i, n) in a.iter().take(3).enumerate() {
                            c[i] = n.as_f64().unwrap_or(0.0) as f32;
                        }
                    }
                    if ui.color_edit_button_rgb(&mut c).on_hover_text(tip).changed() {
                        new = Some(serde_json::json!([c[0], c[1], c[2]]));
                    }
                }
                Kind::Custom => {}
            }
            reset = reset_cell(ui, changed, self.reset_tip());
        });
        if reset {
            if let Some(b) = base {
                new = Some(b);
                if row.scope == Scope::Gen && !row.path.contains('.') && self.gen_base.is_some() {
                    self.pending_reset.push(row.path.to_string());
                }
            }
        }
        if let Some(n) = new {
            v.set(row.scope, row.path, n);
        }
    }

    fn intent_row(&mut self, ui: &mut egui::Ui, v: &mut Vals, it: &Intent) {
        v.flush(self);
        let now = (it.get)(&self.project);
        let base = (it.get)(&v.base_project);
        let changed = (now - base).abs() > 1e-3;
        let mut value = now;
        let mut reset = false;
        let mut moved = false;
        ui.horizontal(|ui| {
            label_cell(ui, it.label, changed, it.tip);
            if slider_box(ui, &mut value, it.lo, it.hi, it.percent, it.label == "Tree size", it.tip) {
                moved = true;
            }
            reset = reset_cell(ui, changed, self.reset_tip());
        });
        if reset {
            value = base;
            moved = true;
        }
        if moved {
            (it.set)(&mut self.project, value);
            v.reload(self);
        }
    }

    fn draw_item(&mut self, ui: &mut egui::Ui, v: &mut Vals, rows: &[Row], intents: &[Intent], item: Item) {
        match item {
            Item::Intent(i) => self.intent_row(ui, v, &intents[i]),
            Item::Row(i) => self.draw_row(ui, v, &rows[i]),
        }
    }

    fn draw_row(&mut self, ui: &mut egui::Ui, v: &mut Vals, row: &Row) {
        if let Some(f) = row.show {
            if !f(&v.typed) {
                return;
            }
        }
        if row.kind != Kind::Custom {
            self.scalar_row(ui, v, row);
            return;
        }
        // custom rows work on the project directly, so hand over what is pending first and read it back after
        v.flush(self);
        self.custom_row(ui, row);
        v.reload(self);
    }

    fn custom_row(&mut self, ui: &mut egui::Ui, row: &Row) {
        let tip = row.tip;
        match row.path {
            "seed" => {
                ui.horizontal(|ui| {
                    label_cell(ui, "Variation", false, tip);
                    let g = &mut self.project.gen;
                    ui.add(egui::DragValue::new(&mut g.seed)).on_hover_text("The random seed");
                    if ui.button("New variation (R)").on_hover_text(tip).clicked() {
                        g.seed = g.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) % 1_000_000;
                    }
                });
            }
            "edge_style" => {
                ui.horizontal(|ui| {
                    label_cell(ui, row.label, false, tip);
                    let g = &mut self.project.gen;
                    egui::ComboBox::from_id_salt("edge_style").selected_text("Apply...").show_ui(ui, |ui| {
                        for (name, f) in PRESETS {
                            if ui.selectable_label(false, *name).clicked() {
                                f(g);
                            }
                        }
                    });
                });
            }
            "pattern" => {
                ui.horizontal(|ui| {
                    label_cell(ui, row.label, self.project.gen.pattern != self.gen_base.clone().unwrap_or_default().pattern, tip);
                    let g = &mut self.project.gen;
                    egui::ComboBox::from_id_salt("pattern").selected_text(g.pattern.name()).show_ui(ui, |ui| {
                        for p in Pattern::ALL {
                            ui.selectable_value(&mut g.pattern, p, p.name());
                        }
                    });
                });
            }
            "edge_mode" => {
                ui.horizontal(|ui| {
                    label_cell(ui, row.label, false, tip);
                    let g = &mut self.project.gen;
                    ui.radio_value(&mut g.edge_mode, EdgeMode::Realistic, "Realistic");
                    ui.radio_value(&mut g.edge_mode, EdgeMode::Blurred, "Blurred");
                });
            }
            "species" => {
                self.tree_type_picker(ui, Slot::Main, "Main trees");
                self.tree_type_picker(ui, Slot::Mix, "Mixed in");
                self.tree_type_picker(ui, Slot::Shrub, "Shrubs");
            }
            "mushroom_kinds" => {
                let g = &mut self.project.gen;
                if g.mushrooms > 0.0 || g.giant_mushrooms > 0.0 {
                    ui.horizontal_wrapped(|ui| {
                        for (k, (_, name)) in forest_core::mushrooms::KINDS.iter().enumerate() {
                            let mut on = g.mushroom_mix[k] > 0.0;
                            if ui.checkbox(&mut on, *name).changed() {
                                g.mushroom_mix[k] = if on { 0.5 } else { 0.0 };
                            }
                        }
                    });
                    if g.mushroom_mix.iter().sum::<f32>() <= 0.0 {
                        g.mushroom_mix = [0.5; 5];
                    }
                    self.project.ensure_mushroom_species();
                }
            }
            "species_editor" => self.sec_species_editor(ui),
            "grading" => self.sec_layers_and_colour_grading(ui),
            "shadow_dir" => {
                let light = &mut self.project.light;
                ui.horizontal(|ui| {
                    label_cell(ui, row.label, (light.shadow_angle - Light::default().shadow_angle).abs() > 0.5, tip);
                    angle_dial(ui, &mut light.shadow_angle);
                    ui.vertical(|ui| {
                        ui.add(egui::DragValue::new(&mut light.shadow_angle).range(0.0..=360.0).suffix(" deg"));
                        ui.horizontal(|ui| {
                            for (lbl, a) in [("UL", 225.0), ("UR", 315.0), ("DL", 135.0), ("DR", 45.0)] {
                                if ui.small_button(lbl).on_hover_text("Shadows fall this way").clicked() {
                                    light.shadow_angle = a;
                                }
                            }
                        });
                    });
                });
            }
            "snow_look" => {
                ui.horizontal_wrapped(|ui| {
                    label_cell(ui, row.label, false, tip);
                    for (name, look) in snow_looks() {
                        if ui.button(name).clicked() {
                            self.project.gen.snow = look;
                        }
                    }
                });
            }
            "fire_look" => {
                ui.horizontal_wrapped(|ui| {
                    label_cell(ui, row.label, false, tip);
                    for (name, look) in fire_looks() {
                        if ui.button(name).clicked() {
                            self.project.gen.fire = look;
                        }
                    }
                });
            }
            "snow_mode" => {
                ui.horizontal(|ui| {
                    label_cell(ui, row.label, false, tip);
                    let sn = &mut self.project.gen.snow;
                    egui::ComboBox::from_id_salt("snow_mode").selected_text(sn.mode.name()).show_ui(ui, |ui| {
                        for m in SnowMode::ALL {
                            ui.selectable_value(&mut sn.mode, m, m.name());
                        }
                    });
                });
            }
            "fire_mode" => {
                ui.horizontal(|ui| {
                    label_cell(ui, row.label, false, tip);
                    let fi = &mut self.project.gen.fire;
                    egui::ComboBox::from_id_salt("fire_mode").selected_text(fi.mode.name()).show_ui(ui, |ui| {
                        for m in FireMode::ALL {
                            ui.selectable_value(&mut fi.mode, m, m.name());
                        }
                    });
                });
            }
            "snow_angle" | "fire_angle" => {
                let snow = row.path == "snow_angle";
                let (show, angle): (bool, &mut f32) = if snow { (self.project.gen.snow.mode == SnowMode::Gradient, &mut self.project.gen.snow.angle) } else { (self.project.gen.fire.mode == FireMode::Front, &mut self.project.gen.fire.angle) };
                if show {
                    ui.horizontal(|ui| {
                        label_cell(ui, row.label, false, tip);
                        angle_dial(ui, angle);
                        ui.vertical(|ui| {
                            ui.add(egui::DragValue::new(angle).range(0.0..=360.0).suffix(" deg"));
                            ui.horizontal(|ui| {
                                for (lbl, a) in [("Top", 270.0), ("Right", 0.0), ("Bottom", 90.0), ("Left", 180.0)] {
                                    if ui.small_button(lbl).clicked() {
                                        *angle = a;
                                    }
                                }
                            });
                        });
                    });
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ the panel

    pub(super) fn tuning_panel(&mut self, ui: &mut egui::Ui) {
        let rows = rows();
        let intents = intents();
        let mut v = Vals::load(self);

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search settings, e.g. \"bigger trees\"")
                    .desired_width(ui.available_width() - 28.0),
            );
            if ui.small_button("x").on_hover_text("Clear the search").clicked() {
                self.search.clear();
            }
        });
        ui.add_space(2.0);

        if !self.search.trim().is_empty() {
            ui.separator();
            let hits = search(&rows, &intents, &self.search);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if hits.is_empty() {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Nothing matches. Try another word, like \"size\", \"light\", \"edge\" or \"snow\".").weak());
                    return;
                }
                ui.label(egui::RichText::new(format!("{} matches, best first. The controls are live.", hits.len())).small().weak());
                for h in hits {
                    ui.add_space(3.0);
                    let (tab, mixed) = match h.item {
                        Item::Intent(i) => (intents[i].tab, true),
                        Item::Row(i) => (rows[i].tab, false),
                    };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{} \u{203a} {}", tab.name(), if mixed { "Mixed Tuning" } else { "Fine Tuning" })).small().weak());
                        if let Some(a) = h.alias {
                            ui.label(egui::RichText::new(format!("matches \"{a}\"")).small().color(Color32::from_rgb(150, 200, 150)));
                        }
                        if ui.small_button("Go to").clicked() {
                            self.tab = tab;
                            self.search.clear();
                            self.fine_open[tab.index()] = !mixed;
                        }
                    });
                    self.draw_item(ui, &mut v, &rows, &intents, h.item);
                }
            });
            v.flush(self);
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
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match tab {
            Tab::Theme => {
                self.theme_cards(ui);
                ui.add_space(8.0);
                self.sec_my_defaults(ui);
            }
            Tab::Map => self.map_tab(ui),
            Tab::Export => {
                self.sec_export(ui);
                ui.add_space(6.0);
                self.sec_preview(ui);
            }
            t => self.tuning_tab(ui, &mut v, &rows, &intents, t),
        });
        v.flush(self);
    }

    fn tuning_tab(&mut self, ui: &mut egui::Ui, v: &mut Vals, rows: &[Row], intents: &[Intent], tab: Tab) {
        ui.label(egui::RichText::new(tab.blurb()).small().weak());
        if tab == Tab::Shape {
            ui.horizontal(|ui| {
                if ui.button("Generate patch").on_hover_text("Random natural outline in the middle of the map").clicked() {
                    self.generate_patch();
                }
                if ui.button("Fill map").clicked() {
                    self.fill_map();
                }
            });
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Mixed Tuning").strong().size(14.0));
            ui.label(egui::RichText::new("one slider, several settings").small().weak());
        });
        ui.separator();
        for (i, it) in intents.iter().enumerate() {
            if it.tab == tab {
                self.intent_row(ui, v, &intents[i]);
            }
        }
        ui.add_space(10.0);
        // how many fine settings differ, for the header
        let changed = rows
            .iter()
            .filter(|r| r.tab == tab && r.kind != Kind::Custom)
            .filter(|r| !same(pointer(v.cur(r.scope), r.path), pointer(v.base(r.scope), r.path)))
            .count();
        let title = if changed > 0 { format!("Fine Tuning   {changed} changed") } else { "Fine Tuning".to_string() };
        let open = self.fine_open[tab.index()];
        let head = ui.horizontal(|ui| {
            let (_rect, icon) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
            egui::collapsing_header::paint_default_icon(ui, if open { 1.0 } else { 0.0 }, &icon);
            let r = ui.add(egui::Button::new(egui::RichText::new(title).strong().size(14.0)).frame(false));
            r.on_hover_text("Show or hide every individual setting").clicked() || icon.clicked()
        });
        if head.inner {
            self.fine_open[tab.index()] = !open;
        }
        if open {
            ui.separator();
            ui.label(egui::RichText::new("Every individual setting. Hover a name for what it does.").small().weak());
            let mut last_group = "";
            for (i, r) in rows.iter().enumerate() {
                if r.tab != tab {
                    continue;
                }
                if r.show.is_some_and(|f| !f(&v.typed)) {
                    continue;
                }
                if r.group != last_group {
                    last_group = r.group;
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(r.group.to_uppercase()).small().weak());
                }
                self.draw_row(ui, v, &rows[i]);
            }
        }
        ui.add_space(8.0);
        if ui.small_button("Reset this tab").on_hover_text(self.reset_tip()).clicked() {
            v.flush(self);
            for r in rows.iter().filter(|r| r.tab == tab && r.kind != Kind::Custom) {
                if let Some(b) = pointer(v.base(r.scope), r.path).cloned() {
                    v.set(r.scope, r.path, b);
                    if r.scope == Scope::Gen && !r.path.contains('.') && self.gen_base.is_some() {
                        self.pending_reset.push(r.path.to_string());
                    }
                }
            }
            v.flush(self);
            v.reload(self);
        }
    }

    fn map_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("The map you are painting on. A background picture is never exported.").small().weak());
        ui.horizontal(|ui| {
            ui.label("Map size");
            ui.add(egui::DragValue::new(&mut self.map_size_edit[0]).range(64..=16384));
            ui.label("x");
            ui.add(egui::DragValue::new(&mut self.map_size_edit[1]).range(64..=16384));
            if ui.button("Apply size").on_hover_text("Use the pixel size of the map you will place the forest on").clicked() {
                self.project.map_w = self.map_size_edit[0];
                self.project.map_h = self.map_size_edit[1];
                self.fit_pending = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Background");
            ui.color_edit_button_rgb(&mut self.background);
        });
        ui.checkbox(&mut self.show_outlines, "Show outlines");
        ui.separator();
        self.background_ui(ui);
        self.reference_options(ui);
    }

    /// Scale, grid and calibration for the map picture.
    fn reference_options(&mut self, ui: &mut egui::Ui) {
        let Some(b) = self.project.background_image.as_mut() else { return };
        ui.checkbox(&mut b.locked, "Lock position").on_hover_text("The picture cannot be moved or clicked by accident");
        ui.separator();
        ui.label(egui::RichText::new("Scale").strong());
        let known = b.px_per_m > 0.0;
        if known {
            ui.label(format!("{:.2} map pixels per metre", b.px_per_m));
        } else {
            ui.label(egui::RichText::new("Not set. Calibrate to show real distances.").weak());
        }
        ui.checkbox(&mut b.scale_bar, "Scale bar").on_hover_text("A 100 m bar over the map in the editor");
        ui.checkbox(&mut b.grid, "Grid every 100 m");
        let calibrating = self.calibrating;
        if ui.button(if calibrating { "Stop calibrating" } else { "Calibrate: click two points a known distance apart" }).clicked() {
            self.calibrating = !calibrating;
            self.calib.clear();
        }
        if self.calibrating && self.calib.len() == 2 {
            let (a, c) = (self.calib[0], self.calib[1]);
            let px = ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2)).sqrt();
            ui.horizontal(|ui| {
                ui.label(format!("{px:.0} px apart. Distance in metres"));
                ui.add(egui::DragValue::new(&mut self.calib_metres).range(1.0..=100000.0));
                if ui.button("Set scale").clicked() && self.calib_metres > 0.0 {
                    if let Some(b) = self.project.background_image.as_mut() {
                        b.px_per_m = px / self.calib_metres;
                        b.scale_bar = true;
                    }
                    self.calibrating = false;
                    self.calib.clear();
                }
            });
        }
    }
}
