"""Match colour and shadow with structure locked, canopy pixels against canopy pixels and ground pixels against
ground pixels, where a lidar canopy height model says which is which. Usage: colour_tune.py species"""
import json, os, subprocess, sys, threading
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(__file__))
import canopy_metrics as cm
import match_species as m
import structure_lib as sl
import class_colour_lib as cc

SS = 4  # render scale: 4 samples per metre along an axis, averaged to 0.5 m like the real mosaic


def real_stats(key, n=16):
    wins = sl.real_windows(key, n, seed=5)
    can, gnd, canpx, gndpx = [], [], [], []
    for w, (y, x) in wins:
        rgb, cm_, gm = cc.real_window_rgb(key, y, x)
        a, b = cc.class_features(rgb, cm_), cc.class_features(rgb, gm)
        if a:
            can.append(a)
            canpx.append(rgb[cm_][:: max(1, cm_.sum() // 3000)])
        if b:
            gnd.append(b)
            gndpx.append(rgb[gm][:: max(1, gm.sum() // 1500)])
    stack = lambda fs: {k: np.stack([f[k] for f in fs]) for k in fs[0]} if fs else None
    return stack(can), stack(gnd), np.concatenate(canpx) if canpx else None, np.concatenate(gndpx) if gndpx else None


def ramp_from_pixels(px):
    x = np.concatenate([px.mean(1, keepdims=True), px], 1)
    sub = x[:: max(1, len(x) // 20000)]
    c, _ = cm.kmeans(sub, 4)
    return [list(map(float, col[1:] / 255.0)) for col in c]


EXTRA = {
    "ground_r": (20, 230, None), "ground_g": (20, 230, None), "ground_b": (20, 230, None),
    "shadow_opacity": (0.1, 1.0, 0.6), "shadow_r": (0.0, 0.3, 0.12), "shadow_g": (0.0, 0.3, 0.1), "shadow_b": (0.0, 0.35, 0.05),
    "shadow_length": (0.1, 2.0, None),
}
COLOUR_KNOBS = ["gain", "gamma", "crown_tone", "crown_shade", "crown_var", "cluster_tone", "mottling", "mottle_size", "variety", "dappled", "tree_lean",
                "depth_strength", "tint_r", "tint_g", "tint_b", "shadow_length", "shadow_softness", "elevation", "shade_radius", "crown_tone_size"]


def render_pair(key, k, ramp, seed):
    p = m.build(key, k, seed=seed, ramp0=ramp)
    side = int(round(sl.WIN * m.K))
    p["map_w"] = p["map_h"] = side
    p["shapes"][0]["points"] = [[-100, -100], [side + 100, -100], [side + 100, side + 100], [-100, side + 100]]
    # bare ground is drawn from the grass layer and haze, so they carry the measured ground colour
    gc = [k["ground_r"] / 255.0, k["ground_g"] / 255.0, k["ground_b"] / 255.0]
    p["gen"]["grass_color"] = gc
    p["gen"]["grass_accent"] = [min(1.0, v * 1.08) for v in gc]
    p["gen"]["haze_color"] = [v * 0.9 for v in gc]
    p["grading"]["shadow_opacity"] = k["shadow_opacity"]
    p["grading"]["shadow_color"] = [k["shadow_r"], k["shadow_g"], k["shadow_b"]]
    tag = f"col_{key}_{threading.get_ident()}"
    pj = os.path.join(m.OUT, tag + ".json")
    json.dump(p, open(pj, "w"))
    rgba_p, chm_p = os.path.join(m.OUT, tag + ".png"), os.path.join(m.OUT, tag + "_h.png")
    subprocess.run([m.CLI, pj, "-out", rgba_p, "-chm", chm_p, "-scale", str(SS * sl.WIN / side)], check=True, capture_output=True)
    a = np.asarray(Image.open(rgba_p).convert("RGBA")).astype(np.float64)
    h = np.asarray(Image.open(chm_p)).astype(np.float32)
    f = SS // 2
    a[..., :3] *= a[..., 3:4] / 255.0
    a = a.reshape(a.shape[0] // f, f, a.shape[1] // f, f, 4).mean((1, 3))
    a[..., :3] = a[..., :3] / np.maximum(a[..., 3:4], 1e-6) * 255.0
    hm = h.reshape(h.shape[0] // f, f, h.shape[1] // f, f).max((1, 3))
    return a, hm


def composite(rgba, g0):
    al = rgba[..., 3:4] / 255.0
    return rgba[..., :3] * al + np.array(g0) * (1 - al)


def feats(key, k, ramp, seeds=(1, 2)):
    can, gnd = [], []
    for s in seeds:
        rgba, hm = render_pair(key, k, ramp, s)
        img = composite(rgba, (k["ground_r"], k["ground_g"], k["ground_b"]))
        canopy = hm > 0
        k_ = cm.blur(canopy.astype(float), 1.0)
        a, b = cc.class_features(img, canopy & (k_ > 0.9)), cc.class_features(img, (~canopy) & (k_ < 0.05))
        if a:
            can.append(a)
        if b:
            gnd.append(b)
    avg = lambda fs: {n: np.mean([f[n] for f in fs], 0) for n in fs[0]} if fs else None
    return avg(can), avg(gnd)


def dist(f, ref, w=None):
    if f is None or ref is None:
        return 5.0, {}
    rows = {}
    for n, v in f.items():
        r = ref[n]
        mu, sd = r.mean(0), r.std(0)
        floor = 0.08 * np.abs(mu).mean() + 1e-3
        rows[n] = float(np.mean(np.abs(v - mu) / np.maximum(sd, floor)))
    return float(np.mean(list(rows.values()))), rows


def main():
    key = sys.argv[1]
    iters = int(os.environ.get("ITERS", 150))
    ref_can, ref_gnd, canpx, gndpx = real_stats(key)
    struct = json.load(open(os.path.join(m.OUT, f"{key}_best_struct.json")))
    k = dict(struct["knobs"])
    ramp = ramp_from_pixels(canpx)
    # ground colour: where the ground shows, its own typical colour
    g = gndpx.mean(0) if gndpx is not None else np.array([60, 60, 50])
    k.update(ground_r=float(g[0]), ground_g=float(g[1]), ground_b=float(g[2]), shadow_opacity=0.6, shadow_r=0.12, shadow_g=0.1, shadow_b=0.05)
    for n in ("tint_r", "tint_g", "tint_b"):
        k[n] = 1.0
    bounds = {n: m.KNOBS[n][:2] for n in COLOUR_KNOBS if n in m.KNOBS}
    bounds.update({n: v[:2] for n, v in EXTRA.items()})
    for n in ("shadow_length",):
        bounds[n] = m.KNOBS[n][:2]
    rng = np.random.default_rng(13)

    def score(kk):
        a, b = feats(key, kk, ramp)
        sa, ra = dist(a, ref_can)
        sb, rb = dist(b, ref_gnd)
        return 0.6 * sa + 0.4 * sb, (sa, sb, ra, rb)

    best, (bs, info) = k, score(k)
    print(key, "start", round(bs, 3), "canopy", round(info[0], 2), "ground", round(info[1], 2), flush=True)
    names = list(bounds)
    for it in range(1, iters + 1):
        c = dict(best)
        step = 0.3 * (1 - it / (iters + 1)) + 0.04
        for n in rng.choice(names, size=rng.integers(1, 4), replace=False):
            lo, hi = bounds[n]
            c[n] = float(np.clip(c[n] + rng.normal(0, step * (hi - lo)), lo, hi))
        s, i2 = score(c)
        if s < bs:
            best, bs, info = c, s, i2
            print(f"{it:4d} {bs:.3f} canopy {info[0]:.2f} ground {info[1]:.2f}", flush=True)
    json.dump(dict(ramp=ramp, knobs=best, score=bs, canopy=info[0], ground=info[1]), open(os.path.join(m.OUT, f"{key}_best_colour.json"), "w"), indent=1)
    print("best", bs)


if __name__ == "__main__":
    main()
