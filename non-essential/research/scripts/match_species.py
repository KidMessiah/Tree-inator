"""Tunes a species preset against aerial patches. Usage:
  match_species.py baseline|tune species_key
Reads patches from research/satellite, renders with forest-cli, and writes results to research/satellite/match/."""
import json, os, subprocess, sys, copy
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(__file__))
import canopy_metrics as cm

ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "..")
SAT = os.path.join(ROOT, "non-essential", "research", "satellite")
OUT = os.path.join(SAT, "match")
CLI = os.environ.get("CLI") or os.path.join(ROOT, "target", "release", "forest-cli.exe")
TAG = os.environ.get("TAG", "")
os.makedirs(OUT, exist_ok=True)
PATCH = 256
# One map pixel is this many metres. 100% forest size (scale 1) is the base size: a temperate broadleaf crown of about
# 6 m radius is 13.5 px (the middle of the default 10 to 17), so every theme is tuned with this fixed scale.
MPP = float(os.environ.get("MPP", 0.45))
K = 1.0 / MPP
TEMPLATE = os.environ.get("TEMPLATE")  # a project json to borrow the field layout from

# preset, conifer?, reference scenes (site__date) with a crop count each
SPECIES = {
    "bamboo": dict(preset="bamboo", needles=False, scenes=[("bambooJP_nagA", 1), ("bambooJP_nagB", 1), ("bambooJP_araA", 1), ("bambooJP_araB", 1)]),
    "aspen": dict(preset="quaking_aspen", needles=False, scenes=[("aspenCO_4__2019-09-13", 2), ("aspenCO_4__2021-09-06", 1), ("aspenCO_4__2015-09-11", 1), ("aspenB_41__2015-09-10", 1), ("aspenB_41__2019-09-13", 1), ("aspenB_42__2021-09-06", 2), ("aspenB_42__2013-09-24", 1)]),
    "pine": dict(preset="small_pine", needles=True, scenes=[("pineOR_17__2014-06-30", 2), ("pineOR_17__2020-06-25", 1), ("pineAZ_12__2013-06-19", 1), ("pineSD_14__2016-09-11", 1), ("pineSD_15__2018-07-31", 1)]),
    "dfir": dict(preset="douglas_fir", needles=True, scenes=[("dfirOR_19__2016-07-26", 2), ("dfirOR_19__2020-07-25", 1), ("dfirOR_20__2020-07-20", 1), ("dfirWA_22__2015-08-18", 1), ("dfirWA_22__2017-08-22", 1)]),
}


def scene_path(tag):
    for d in (SAT,):
        for root, _, files in os.walk(d):
            if tag + ".png" in files:
                return os.path.join(root, tag + ".png")
    raise FileNotFoundError(tag)


def real_patches(key):
    rng = np.random.default_rng(7)
    out = []
    for tag, n in SPECIES[key]["scenes"]:
        img = np.asarray(Image.open(scene_path(tag)).convert("RGB"))
        for p in cm.crop_patches(img, PATCH, n, rng):
            out.append(p.astype(np.float64))
    return out


def base_project():
    p = json.load(open(TEMPLATE))
    p["map_w"] = p["map_h"] = PATCH
    p["shapes"] = [dict(p["shapes"][0], points=[[-200, -200], [PATCH + 200, -200], [PATCH + 200, PATCH + 200], [-200, PATCH + 200]], op="Add")]
    p["painted"], p["erased"], p["background_image"] = [], [], None
    return p


# the knobs the search moves: name -> (low, high, start)
KNOBS = {
    "scale": (0.1, 0.6, 0.2),
    "crown_max": (6, 32, 17),
    "canopy_cover": (0.15, 1.0, 0.7),
    "gaps": (0.0, 1.0, 0.2),
    "gap_size": (5, 60, 20),
    "shrub_density": (0.0, 2.0, 0.5),
    "grass_density": (0.0, 2.0, 0.5),
    "haze_opacity": (0.0, 0.4, 0.1),
    "gain": (0.6, 1.8, 1.0),
    "gamma": (0.3, 1.6, 1.0),
    "crown_tone": (0.0, 0.5, 0.1),
    "crown_shade": (0.0, 1.0, 0.2),
    "height_var": (0.0, 2.0, 1.0),
    "mottling": (0.0, 3.0, 1.0),
    "tint_r": (0.9, 1.1, 1.0),
    "tint_g": (0.9, 1.1, 1.0),
    "tint_b": (0.9, 1.1, 1.0),
    "shadow_length": (0.1, 2.0, 0.8),
    "elevation": (20, 80, 50),
    "shadow_softness": (0.3, 2.5, 1.0),
    "ground_dark": (0.2, 1.0, 0.6),
    "smooth": (0.0, 0.0, 0.0),
    "weave": (0.0, 1.0, 0.6),
    "crown_tone_size": (10, 200, 28),
    "cluster_tone": (0.0, 3.0, 1.0),
    "crown_var": (0.0, 1.5, 0.35),
    "depth_strength": (0.0, 2.0, 1.0),
    "mottle_size": (4, 80, 14),
    "tree_lean": (0.0, 2.0, 0.5),
    "dappled": (0.0, 1.5, 0.4),
    "variety": (0.0, 4.0, 2.0),
    "canopy_patch_size": (8, 600, 120),
    "shade_radius": (4, 40, 14),
}


def ramp_from_real(patches):
    """Four ramp colours (dark to light) from the real patches' own colour clusters, leaving out the darkest cluster
    (shadow and ground), as 0..1 RGB."""
    px = np.concatenate([p.reshape(-1, 3) for p in patches])
    green = px[:, 1] / np.maximum(px.sum(1), 1)
    leafy = px[green > 0.352]  # leave bare soil out of the crown colours
    if len(leafy) > 5000:
        px = leafy
    px = px[:: max(1, len(px) // 30000)]
    x = np.concatenate([px.mean(1, keepdims=True), px], 1)
    c, _ = cm.kmeans(x, 5)
    return [list(map(float, col[1:] / 255.0)) for col in c[1:]]


def build(key, k, seed=1, ramp0=None):
    sp = SPECIES[key]
    p = base_project()
    g = p["gen"]
    g.update(crown_max=k["crown_max"], crown_min=0.6 * k["crown_max"], contain=False, pattern="Plain", edge_warp=0.0, edge_noise=0.0, seed=seed, scale=1.0, canopy_patch_size=k["canopy_patch_size"], canopy_cover=k["canopy_cover"],
             gaps=k["gaps"], gap_size=k["gap_size"], shrub_density=k["shrub_density"], grass_density=k["grass_density"],
             haze_opacity=k["haze_opacity"], mix_amount=0.0, lone_groups=0.0, pebbles=0.0)
    g["depth"].update(smooth=0.0, weave=k["weave"], crown_tone_size=k["crown_tone_size"], cluster_tone=k["cluster_tone"], crown_var=k["crown_var"], depth_strength=k["depth_strength"], shade_radius=k["shade_radius"], gain=k["gain"], gamma=k["gamma"], crown_tone=k["crown_tone"], crown_shade=k["crown_shade"], height_var=k["height_var"])
    g["leaf"].update(mottling=k["mottling"], mottle_size=k["mottle_size"], tree_lean=k["tree_lean"], dappled=k["dappled"], variety=k["variety"])
    p["light"].update(shadow_length=k["shadow_length"], elevation=k["elevation"], shadow_softness=k["shadow_softness"])
    if "_shadow_angle" in k:
        p["light"].update(shadow_angle=k["_shadow_angle"], elevation=k["_elevation"])
    main = copy.deepcopy(p["species"][0])
    main.update(procedural=True, preset=sp["preset"], needles=sp["needles"], name=key)
    base = ramp0 or main["ramp"]
    main["ramp"] = [[min(1.0, c * t) for c, t in zip(col, (k["tint_r"], k["tint_g"], k["tint_b"]))] for col in base]
    bush = copy.deepcopy(p["species"][0])
    bush.update(procedural=True, preset=os.environ.get("BUSH_PRESET", "black_oak"), name="shrub")  # irregular, not a perfect disc
    bush["ramp"] = main["ramp"]
    p["species"] = [main, bush]
    g.update(main_species=0, mix_species=0, shrub_species=1)
    return p


SUPER = int(os.environ.get("SUPER", 2))
SENSOR_BLUR = float(os.environ.get("SENSOR_BLUR", 0.3))  # aerial imagery is soft; renders get the same softness before they are compared


def render(p, name, bg=None):
    pj = os.path.join(OUT, name + ".json")
    json.dump(p, open(pj, "w"))
    png = os.path.join(OUT, name + ".png")
    env = dict(os.environ)
    if bg is not None and "FLAT_R" in env:
        # the experiment build filters the canopy after compositing, so it needs the ground colour itself
        env["BGCOLOR"] = ",".join(str(int(round(float(v)))) for v in bg)
    subprocess.run([CLI, pj, "-out", png, "-scale", str(SUPER)], check=True, capture_output=True, env=env)
    a = np.asarray(Image.open(png).convert("RGBA")).astype(np.float64)
    a[..., :3] *= a[..., 3:4] / 255.0  # premultiply so the average does not bleed colour out of empty pixels
    h, w = a.shape[0] // SUPER, a.shape[1] // SUPER
    a = a[: h * SUPER, : w * SUPER].reshape(h, SUPER, w, SUPER, 4).mean((1, 3))
    a[..., :3] = a[..., :3] / np.maximum(a[..., 3:4], 1e-6) * 255.0
    return a


def composite(rgba, ground):
    a = rgba[..., 3:4] / 255.0
    out = rgba[..., :3] * a + np.array(ground) * (1 - a)
    return np.stack([cm.blur(out[..., i], SENSOR_BLUR) for i in range(3)], -1)


def ground_colour(patches, dark):
    allpx = np.concatenate([p.reshape(-1, 3) for p in patches])
    lum = allpx.mean(1)
    green = allpx[:, 1] / np.maximum(allpx.sum(1), 1)
    soil = (green < 0.352) & (lum > np.median(lum))
    if soil.mean() > 0.06:
        # open ground shows: use its own colour, dark scales it around 1
        return allpx[soil].mean(0) * (0.5 + dark)
    lo = allpx[lum <= np.quantile(lum, 0.06)]
    return lo.mean(0) * (0.4 + dark) / 1.0


RAMP = {}


def score(key, k, refs_stack, patches, seeds=(1, 2)):
    gc = ground_colour(patches, k["ground_dark"])
    fs = []
    for s in seeds:
        img = composite(render(build(key, k, seed=s, ramp0=RAMP.get(key)), f"{key}{TAG}_cur{s}", bg=gc), gc)
        fs.append(cm.features(img))
    avg = {n: np.mean([f[n] for f in fs], 0) for n in fs[0]}
    return cm.distance(avg, refs_stack), gc


def start_knobs():
    return {n: v[2] for n, v in KNOBS.items()}


def main():
    mode, key = sys.argv[1], sys.argv[2]
    patches = real_patches(key)
    RAMP[key] = ramp_from_real(patches)
    refs = cm.stack([cm.features(p) for p in patches])
    k = start_knobs()
    if os.environ.get("START"):
        b0 = json.load(open(os.environ["START"]))
        k.update(b0["knobs"])
        RAMP[key] = b0["ramp"]
    if mode == "baseline":
        (tot, rows), gc = score(key, k, refs, patches)
        print(key, f"{len(patches)} real patches, baseline score {tot:.2f}")
        for n, v in sorted(rows.items(), key=lambda x: -x[1]):
            print(f"  {n:18s} {v:5.2f}")
        return
    rng = np.random.default_rng(int(os.environ.get("SEED", 3)))
    best = k
    (best_score, rows), gc = score(key, best, refs, patches)
    log = [(0, best_score)]
    n_iter = int(os.environ.get("ITERS", 160))
    for it in range(1, n_iter + 1):
        cand = dict(best)
        step = 0.35 * (1 - it / (n_iter + 1)) + 0.05
        for n in rng.choice(list(KNOBS), size=rng.integers(1, 4), replace=False):
            lo, hi, _ = KNOBS[n]
            cand[n] = float(np.clip(cand[n] + rng.normal(0, step * (hi - lo)), lo, hi))
        (s, r), gc2 = score(key, cand, refs, patches)
        if s < best_score:
            best, best_score, rows = cand, s, r
            print(f"{it:4d} score {s:.3f}", flush=True)
        log.append((it, best_score))
    json.dump(dict(ramp=RAMP[key], knobs=best, score=best_score, metrics=rows, ground=list(map(float, ground_colour(patches, best["ground_dark"])))), open(os.path.join(OUT, f"{key}_best{TAG}.json"), "w"), indent=1)
    print("best", best_score)


if __name__ == "__main__":
    main()
