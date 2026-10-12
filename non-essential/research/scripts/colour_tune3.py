"""Colour and shadow tuning against hyperspectral reflectance. The reference image is true colour built from the
reflectance bands (no camera tone curve, haze or white balance), scaled once per site so lit canopy has the camera's
brightness. Classes (sunlit canopy, shaded canopy, ground, shaded ground) come from lidar height plus the sun angle
in the flight logs. Usage: colour_tune3.py species"""
import json, os, sys
import numpy as np
from PIL import Image
Image.MAX_IMAGE_PIXELS = None
sys.path.insert(0, os.path.dirname(__file__))
import canopy_metrics as cm
import match_species as m
import structure_lib as sl
import class_colour_lib as cc
import colour_tune as ct
import colour_tune2 as c2
import sun_shadow as ss

NEON = sl.NEON
W_CLASS = c2.W_CLASS


def site_data(key):
    site, y, e, n = sl.SITES[key]
    d = np.load(os.path.join(NEON, site, f"refl_{e}_{n}.npz"))
    sun = json.load(open(os.path.join(NEON, site, "sun.json")))
    return {c: d[c] for c in ("R", "G", "B", "NIR")}, (sun["sun_azimuth"], sun["sun_elevation"])


def true_colour(bands, k):
    lin = np.stack([bands["R"], bands["G"], bands["B"]], -1)
    return np.clip(np.nan_to_num(lin, nan=0.0) * k, 0, 1) ** (1 / 2.2) * 255.0


def calibrate(key, bands, sun):
    """One scale so sunlit canopy has the photo's brightness, found on the whole tile."""
    chm = sl.chm_tile(key)
    cam = np.asarray(cc.rgb_tile(key).resize((1000, 1000), Image.BOX)).astype(float)
    sh = ss.cast_shadow(chm, sun[0], sun[1], px_m=1.0, steps=40, max_m=60)
    can = (chm > 2) & ~sh & (cm.blur((chm > 2).astype(float), 1.0) > 0.9)
    g_cam = np.median(cam[..., 1][can])
    g_ref = np.nanmedian(bands["G"][can])
    k = (g_cam / 255.0) ** 2.2 / g_ref
    return k


def real_stats(key, bands, sun, k):
    img = true_colour(bands, k)
    ndvi = np.nan_to_num((bands["NIR"] - bands["R"]) / (bands["NIR"] + bands["R"] + 1e-6), nan=0.0)
    per = {c: [] for c in W_CLASS}
    pix = {c: [] for c in W_CLASS}
    chm_t = sl.chm_tile(key)
    for w, (y, x) in sl.real_windows(key, 40, seed=5):
        # windows that touch missing data (the edge of a flight line) are left out
        if np.isnan(bands["R"][y : y + sl.WIN, x : x + sl.WIN]).mean() > 0.002:
            continue
        rgb = img[y : y + sl.WIN, x : x + sl.WIN]
        cl = c2.classes(w, 1.0, sun)
        nd = ndvi[y : y + sl.WIN, x : x + sl.WIN]
        for c, mask in cl.items():
            mask = mask & ((nd > 0.55) if c.startswith("canopy") else (nd < 0.35))
            f = c2.feats_of(rgb, mask)
            if f:
                per[c].append(f)
                pix[c].append(rgb[mask][:: max(1, mask.sum() // 2500)])
    stack = lambda fs: {n: np.stack([f[n] for f in fs]) for n in fs[0]} if fs else None
    return {c: stack(v) for c, v in per.items()}, {c: (np.concatenate(v) if v else None) for c, v in pix.items()}


def engine_feats(key, kk, ramp, sun, h90, seeds=(1, 2)):
    acc = {c: [] for c in W_CLASS}
    for s in seeds:
        rgba, hm = ct.render_pair(key, kk, ramp, s)
        img = ct.composite(rgba, (kk["ground_r"], kk["ground_g"], kk["ground_b"]))
        # to the 1 m grid the reference lives on
        img = img.reshape(sl.WIN, 2, sl.WIN, 2, 3).mean((1, 3))
        hm = hm.reshape(sl.WIN, 2, sl.WIN, 2).max((1, 3))
        hpos = hm[hm > 0]
        scale = h90 / max(np.percentile(hpos, 90), 1e-6) if len(hpos) else 1.0
        cl = c2.classes(hm * scale, 1.0, sun)
        for c, mask in cl.items():
            f = c2.feats_of(img, mask)
            if f:
                acc[c].append(f)
    return {c: ({n: np.mean([f[n] for f in v], 0) for n in v[0]} if v else None) for c, v in acc.items()}


def main():
    key = sys.argv[1]
    iters = int(os.environ.get("ITERS", 250))
    bands, sun = site_data(key)
    k = float(os.environ["K_OVERRIDE"]) if os.environ.get("K_OVERRIDE") else calibrate(key, bands, sun)
    if os.environ.get("K_OVERRIDE"):
        # one exposure for all sites: a camera's own exposure (and glare off wet leaves) must not decide the brightness
        k = float(os.environ["K_OVERRIDE"])
    print(key, "sun", sun, "scale", round(float(k), 2), flush=True)
    ref, pix = real_stats(key, bands, sun, k)
    chm_all = np.concatenate([w[w > 2] for w, _ in sl.real_windows(key, 8, seed=5)])
    h90 = float(np.percentile(chm_all, 90))
    struct = json.load(open(os.path.join(m.OUT, f"{key}_best_struct2.json")))
    kn = dict(struct["knobs"])
    lit = pix["canopy_lit"]
    if os.environ.get("LIT_CAP"):
        # over-bright leaves (glare, a hot spot in the flight) are left out of the palette
        lum = lit.mean(1)
        lit = lit[lum <= np.percentile(lum, float(os.environ["LIT_CAP"]))]
    ramp = ct.ramp_from_pixels(lit)
    gp = pix["ground_lit"] if pix["ground_lit"] is not None and len(pix["ground_lit"]) > 100 else pix["ground_shade"]
    g = gp.mean(0) if gp is not None and len(gp) > 20 else np.array([34.0, 40.0, 28.0])  # dense forest: almost no bare ground, so a deep shadow colour
    kn.update(ground_r=float(g[0]), ground_g=float(g[1]), ground_b=float(g[2]), shadow_opacity=0.6, shadow_r=0.1, shadow_g=0.12, shadow_b=0.1)
    kn["_shadow_angle"], kn["_elevation"] = (sun[0] + 180.0) % 360 - 90.0 + 90.0, sun[1]
    # the engine measures shadow angle as 0 = right, 90 = down; sun az is clockwise from north
    kn["_shadow_angle"] = (np.degrees(np.arctan2(np.cos(np.radians(sun[0])), -np.sin(np.radians(sun[0])))) + 360) % 360
    for n in ("tint_r", "tint_g", "tint_b"):
        kn[n] = 1.0
    bounds = {n: m.KNOBS[n][:2] for n in ct.COLOUR_KNOBS if n in m.KNOBS}
    bounds.update({n: v[:2] for n, v in ct.EXTRA.items() if n not in ("ground_r", "ground_g", "ground_b")})
    bounds.update(ground_r=(10, 240), ground_g=(10, 240), ground_b=(10, 240))
    # a ground colour that is not the stand's own (the deep blue of a shaded gap is real, but looks wrong on a map)
    if os.environ.get("GROUND_FIX"):
        for n, v in zip(("ground_r", "ground_g", "ground_b"), [float(x) for x in os.environ["GROUND_FIX"].split(",")]):
            kn[n] = v
            bounds[n] = (v - 1.0, v + 1.0)
    for n, hi in (("crown_var", 0.12), ("tree_lean", 0.15), ("variety", 0.8), ("mottling", 0.4), ("cluster_tone", 0.8), ("dappled", 0.3), ("tint_r", 1.08), ("tint_g", 1.08), ("tint_b", 1.08)):
        if n in bounds:
            bounds[n] = (0.92 if n[:4] == "tint" else bounds[n][0], hi)
            kn[n] = min(kn.get(n, 1.0), hi)
    # no deep, heavy shadows: the cast shadow stays light enough to read as shade, not as a hole
    bounds["shadow_opacity"] = (0.1, float(os.environ.get("SHADOW_MAX", 1.0)))
    kn["shadow_opacity"] = min(kn["shadow_opacity"], bounds["shadow_opacity"][1])
    bounds.pop("elevation", None)
    rng = np.random.default_rng(23)

    def score(c, seeds=(1, 2)):
        f = engine_feats(key, c, ramp, sun, h90, seeds=seeds)
        parts = {cl: c2.dist(f[cl], ref[cl]) for cl in W_CLASS}
        return sum(W_CLASS[cl] * parts[cl] for cl in W_CLASS), parts

    best, (bs, parts) = kn, score(kn)
    print(key, "start", round(bs, 3), {c: round(v, 2) for c, v in parts.items()}, flush=True)
    names = list(bounds)
    K = int(os.environ.get("BATCH", 4))
    from concurrent.futures import ThreadPoolExecutor
    pool = ThreadPoolExecutor(K)
    since, patience = 0, max(2, int(os.environ.get("PATIENCE", 45)) // K)
    for it in range(1, iters + 1, K):
        step = 0.3 * (1 - it / (iters + 1)) + 0.04
        cands = []
        for _ in range(K):
            c = dict(best)
            for n in rng.choice(names, size=rng.integers(1, 4), replace=False):
                lo, hi = bounds[n]
                c[n] = float(np.clip(c[n] + rng.normal(0, step * (hi - lo)), lo, hi))
            cands.append(c)
        # the candidates are tried at the same time, one engine run each
        res = list(pool.map(score, cands))
        j = min(range(K), key=lambda i: res[i][0])
        if res[j][0] < bs:
            since = 0
            best, (bs, parts) = cands[j], res[j]
            print(f"{it:4d} {bs:.3f} {({c_: round(v, 2) for c_, v in parts.items()})}", flush=True)
        else:
            since += 1
            if since >= patience:
                break
    json.dump(dict(ramp=ramp, knobs=best, score=bs, parts=parts, sun=sun, scale=float(k)), open(os.path.join(m.OUT, f"{key}_best_hyper2{os.environ.get('OUT_TAG', '')}.json"), "w"), indent=1)
    print("best", bs)


if __name__ == "__main__":
    main()
