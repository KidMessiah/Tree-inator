"""Write the tuned stands into crates/forest-core/assets/satellite_themes.json from research/satellite/match/KEY_best_hyper2.json.
Themes never set the forest size (scale): the stand's own crown sizes carry the proportions, so 100% forest size is right
for all of them. Usage: package_biome.py [key ...]"""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
MATCH = os.path.join(ROOT, "non-essential", "research", "satellite", "match")
DEST = os.path.join(ROOT, "crates", "forest-core", "assets", "satellite_themes.json")


def mul(ramp, f):
    return [[min(255, int(c * f[i])) for i, c in enumerate(col)] for col in ramp]


# the standard ground: used wherever bare ground was not actually measured, so gaps never turn dark or odd
STD_GRASS = ((70, 86, 40), (96, 110, 50), (55, 61, 34))

# name, order, note, mix amount, species (name, preset, height, ramp multipliers, bark), ground override for dense stands
SPECS = {
    "aspen": dict(name="Aspen Stand", order=0, keep_species=True),
    "pine": dict(name="Longleaf Pine", order=1, keep_species=True),
    "dfir": dict(name="Douglas-fir Forest", order=2, keep_species=True, palette=STD_GRASS),
    "hardwood": dict(name="Eastern Hardwood", order=3, note="Oak and maple woodland, measured from Massachusetts aerial lidar and hyperspectral imagery", mix=0.15, palette=STD_GRASS,
        species=[("Red Oak", "black_oak", 1.0, (1, 1, 1), [92, 80, 66]), ("Eastern Hemlock", "balsam_fir", 1.2, (0.7, 0.85, 0.8), [84, 70, 58]), ("Witch Hazel", "acer", 0.5, (1.05, 1.0, 0.8), [110, 90, 66])]),
    "spruce": dict(name="Spruce-Fir Forest", order=4, note="Red spruce and balsam fir, measured from New Hampshire aerial lidar and hyperspectral imagery", mix=0.1, palette=STD_GRASS,
        species=[("Red Spruce", "balsam_fir", 1.3, (1, 1, 1), [80, 66, 56]), ("Paper Birch", "silver_birch", 1.0, (1.1, 1.1, 0.9), [214, 208, 196]), ("Mountain Ash", "sphere_tree", 0.5, (1.0, 1.0, 0.85), [96, 74, 60])]),
    "subalpine": dict(name="Subalpine Forest", order=5, note="Engelmann spruce and subalpine fir with open patches, measured from Colorado aerial lidar and hyperspectral imagery", mix=0.12,
        species=[("Engelmann Spruce", "balsam_fir", 1.3, (1, 1, 1), [76, 64, 54]), ("Lodgepole Pine", "small_pine", 1.2, (1.1, 1.1, 0.9), [96, 78, 62]), ("Willow Shrub", "black_oak", 0.4, (1.1, 1.1, 0.9), [110, 90, 70])]),
    "oak": dict(name="Oak Savanna", order=6, note="Blue oak and gray pine over dry grass, measured from California aerial lidar and hyperspectral imagery", mix=0.12,
        species=[("Blue Oak", "cambridge_oak", 1.0, (1, 1, 1), [96, 84, 70]), ("Gray Pine", "small_pine", 1.4, (0.8, 0.9, 0.9), [92, 80, 66]), ("Manzanita", "cambridge_oak", 0.5, (0.95, 1.0, 0.9), [110, 70, 54])]),
    "autumn": dict(name="Autumn Hardwood", order=9, note="Hardwood forest in October, measured from Great Smoky Mountains aerial lidar and hyperspectral imagery", mix=0.45, palette=STD_GRASS,
        species=[("Hickory", "black_oak", 1.0, "warm", [92, 80, 66]), ("Red Oak", "cambridge_oak", 1.0, (1, 1, 1), [84, 70, 58]), ("Sumac", "acer", 0.5, "warm", [110, 90, 66])]),
    "boreal": dict(name="Boreal Forest", order=10, note="Spruce forest of interior Alaska, measured from aerial lidar and hyperspectral imagery", mix=0.1, palette=STD_GRASS,
        species=[("Black Spruce", "balsam_fir", 1.2, (1, 1, 1), [70, 60, 50]), ("Paper Birch", "silver_birch", 1.0, (1.15, 1.15, 0.95), [214, 208, 196]), ("Dwarf Birch", "sphere_tree", 0.4, (1.05, 1.05, 0.9), [96, 74, 60])]),
    "pinyon": dict(name="Pinyon-Juniper", order=11, note="Pinyon pine and juniper over desert soil, measured from Utah aerial lidar and hyperspectral imagery", mix=0.2,
        species=[("Pinyon Pine", "small_pine", 1.0, (1, 1, 1), [96, 78, 62]), ("Utah Juniper", "sphere_tree", 0.9, (0.9, 1.0, 0.95), [110, 82, 62]), ("Sagebrush", "black_oak", 0.4, (1.15, 1.15, 1.0), [120, 100, 80])]),
    "sierra": dict(name="Sierra Conifer", order=12, note="Mixed conifer forest with open ground, measured from California aerial lidar and hyperspectral imagery", mix=0.2, palette=STD_GRASS,
        species=[("White Fir", "douglas_fir", 1.5, (1, 1, 1), [80, 66, 56]), ("Jeffrey Pine", "small_pine", 1.4, (1.1, 1.05, 0.9), [96, 78, 62]), ("Manzanita", "black_oak", 0.5, (0.95, 1.0, 0.85), [110, 70, 54])]),
    "tropdry": dict(name="Tropical Dry Forest", order=13, note="Dry tropical forest, measured from Puerto Rico aerial lidar and hyperspectral imagery", mix=0.15, palette=STD_GRASS,
        species=[("Gumbo Limbo", "black_tupelo", 1.0, (1, 1, 1), [110, 90, 70]), ("Ceiba", "cambridge_oak", 1.3, (1.0, 1.05, 0.9), [150, 140, 126]), ("Acacia Scrub", "acer", 0.5, (1.05, 1.0, 0.8), [90, 76, 60])]),
    "jungle": dict(name="Caribbean Rainforest", order=14, file="lit2", note="Montane rainforest, measured from NASA G-LiHT aerial lidar and photos of El Yunque, Puerto Rico", mix=0.15, palette=STD_GRASS,
        species=[("Tabonuco", "black_tupelo", 1.0, (1, 1, 1), [92, 78, 64]), ("Sierra Palm", "palm", 1.1, (0.9, 1.05, 0.9), [120, 104, 80]), ("Tree Fern", "fan_palm", 0.5, (0.95, 1.1, 0.9), [90, 76, 60])]),
    "bamboo": dict(name="Bamboo Grove", order=15, file="k4", note="Moso bamboo groves, measured from Japanese aerial photos of the Kyoto hills", mix=0.1, palette=STD_GRASS,
        species=[("Moso Bamboo", "bamboo", 1.0, (0.92, 1.0, 0.8), [150, 140, 90]), ("Hinoki Cypress", "balsam_fir", 1.2, (0.7, 0.85, 0.75), [90, 70, 56]), ("Bamboo Grass", "fan_palm", 0.4, (1.05, 1.1, 0.85), [140, 130, 90])]),
    "swamp": dict(name="Bottomland Swamp", order=7, note="Water tupelo and cypress swamp forest, measured from Alabama aerial lidar and hyperspectral imagery", mix=0.2, palette=STD_GRASS,
        species=[("Water Tupelo", "black_tupelo", 1.0, (1, 1, 1), [78, 70, 60]), ("Bald Cypress", "weeping_willow", 1.1, (0.95, 1.0, 0.9), [100, 80, 64]), ("Swamp Privet", "black_oak", 0.5, (1.05, 1.05, 0.9), [90, 76, 60])]),
    "rainforest": dict(name="Hawaiian Rainforest", order=8, note="Wet ohia forest, measured from Hawaii aerial lidar and hyperspectral imagery", mix=0.15, palette=STD_GRASS,
        species=[("Ohia", "black_tupelo", 1.0, (1, 1, 1), [92, 78, 64]), ("Koa", "acer", 1.1, (1.0, 1.05, 0.9), [100, 80, 64]), ("Tree Fern", "fan_palm", 0.5, (0.9, 1.1, 0.85), [90, 76, 60])]),
}


def main():
    data = json.load(open(DEST))
    for key in sys.argv[1:] or list(SPECS):
        sp = SPECS[key]
        d = json.load(open(os.path.join(MATCH, f"{key}_best_{sp.get('file', 'hyper2')}.json")))
        k = d["knobs"]
        ramp = sorted([[int(round(c * 255)) for c in col] for col in d["ramp"]], key=lambda c: sum(c))
        gen = {
            "crown_max": k["crown_max"], "crown_min": 0.6 * k["crown_max"], "canopy_cover": k["canopy_cover"], "gaps": k["gaps"], "gap_size": k["gap_size"],
            "canopy_patch_size": k["canopy_patch_size"], "shrub_density": k["shrub_density"], "grass_density": k["grass_density"], "haze_opacity": k["haze_opacity"],
            "depth": {x: k[x] for x in ("gain", "gamma", "crown_tone", "crown_shade", "height_var", "cluster_tone", "crown_var", "depth_strength", "shade_radius", "crown_tone_size", "weave")},
            "leaf": {x: k[x] for x in ("mottling", "mottle_size", "tree_lean", "dappled", "variety")},
        }
        gen["depth"]["smooth"] = 0.0
        gen["depth"].update({"canopy_ao": 0.6, "translucency": 0.5, "lumps": 0.35})
        gen["crown_lean"] = 0.5
        if sp.get("keep_species"):
            e = data[sp["name"]]
            e["species"][0]["ramp"] = ramp
            e["gen"] = gen
            if sp.get("palette"):
                e["grass"], e["accent"], e["haze"] = [list(c) for c in sp["palette"]]
            print("updated", sp["name"])
            continue
        ground = list(sp.get("ground") or ([int(k[x]) for x in ("ground_r", "ground_g", "ground_b")] if "ground_r" in k else [70, 86, 40]))
        grass, accent, haze = sp.get("palette") or (ground, [min(255, int(v * 1.08)) for v in ground], [int(v * 0.9) for v in ground])
        data[sp["name"]] = dict(note=sp["note"], mix_amount=sp["mix"], order=sp["order"],
                                species=[dict(name=n, preset=p, height=h, ramp=(json.load(open(os.path.join(MATCH, "autumn_warm_ramp.json"))) if f == "warm" else mul(ramp, f)), bark=b) for n, p, h, f, b in sp["species"]],
                                grass=list(grass), accent=list(accent), haze=list(haze), gen=gen)
        print("wrote", sp["name"])
    json.dump(data, open(DEST, "w"), indent=1)


main()
