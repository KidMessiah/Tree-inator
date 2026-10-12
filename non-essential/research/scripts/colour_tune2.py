"""Colour and shadow tuning with structure locked. Real and engine pixels are split into sunlit canopy, shaded
canopy, sunlit ground and shaded ground (lidar height plus the fitted sun), and each class is matched on its own,
so a shadow can never be taken for foliage or ground. Usage: colour_tune2.py species"""
import json, os, sys
import numpy as np
sys.path.insert(0, os.path.dirname(__file__))
import canopy_metrics as cm
import match_species as m
import structure_lib as sl
import class_colour_lib as cc
import colour_tune as ct
import sun_shadow as ss

W_CLASS = {"canopy_lit": 0.35, "canopy_shade": 0.25, "ground_lit": 0.2, "ground_shade": 0.2}
HUE_KEYS = ("hue",)


def classes(chm_m, px_m, sun):
    can = chm_m > 2.0
    if sun is None:
        return {"canopy_lit": can, "canopy_shade": np.zeros_like(can), "ground_lit": ~can, "ground_shade": np.zeros_like(can)}
    sh = ss.cast_shadow(chm_m, sun[0], sun[1], px_m, steps=30)
    k = cm.blur(can.astype(float), 1.0)
    c_in, g_in = can & (k > 0.9), (~can) & (k < 0.05)
    return {"canopy_lit": c_in & ~sh, "canopy_shade": c_in & sh, "ground_lit": g_in & ~sh, "ground_shade": g_in & sh}


def feats_of(rgb, mask):
    f = cc.class_features(rgb, mask)
    if f is None:
        return None
    lab = cm.to_lab(rgb[mask].reshape(-1, 1, 3)).reshape(-1, 3)
    ang = np.degrees(np.arctan2(lab[:, 2], lab[:, 1]))
    f["hue"] = np.array([ang.std() / 4.0, ang.std() / 4.0, np.hypot(lab[:, 1], lab[:, 2]).std() / 4.0])
    return f


def fit_sun_key(key):
    res = []
    for w, (y, x) in sl.real_windows(key, 16, seed=5)[:5]:
        rgb, _, _ = cc.real_window_rgb(key, y, x)
        res.append(ss.fit_sun(np.kron(w, np.ones((2, 2))), rgb.mean(2)))
    res.sort(reverse=True)
    c, az, el = res[0]
    return (az, el) if c > 0.2 else None


def real_stats(key, sun):
    per = {k: [] for k in W_CLASS}
    pix = {k: [] for k in W_CLASS}
    for w, (y, x) in sl.real_windows(key, 16, seed=5):
        rgb, _, _ = cc.real_window_rgb(key, y, x)
        cl = classes(np.kron(w, np.ones((2, 2))), 0.5, sun)
        for k, mask in cl.items():
            f = feats_of(rgb, mask)
            if f:
                per[k].append(f)
                pix[k].append(rgb[mask][:: max(1, mask.sum() // 2500)])
    stack = lambda fs: {n: np.stack([f[n] for f in fs]) for n in fs[0]} if fs else None
    return {k: stack(v) for k, v in per.items()}, {k: (np.concatenate(v) if v else None) for k, v in pix.items()}


def engine_feats(key, k, ramp, sun, real_h90, seeds=(1, 2)):
    acc = {c: [] for c in W_CLASS}
    for s in seeds:
        rgba, hm = ct.render_pair(key, k, ramp, s)
        img = ct.composite(rgba, (k["ground_r"], k["ground_g"], k["ground_b"]))
        hpos = hm[hm > 0]
        scale = real_h90 / max(np.percentile(hpos, 90), 1e-6) if len(hpos) else 1.0
        cl = classes(hm * scale, 0.5, sun)
        for c, mask in cl.items():
            f = feats_of(img, mask)
            if f:
                acc[c].append(f)
    return {c: ({n: np.mean([f[n] for f in v], 0) for n in v[0]} if v else None) for c, v in acc.items()}


def dist(f, ref):
    if f is None or ref is None:
        return 3.0
    rows = []
    for n, v in f.items():
        r = ref[n]
        mu, sd = r.mean(0), r.std(0)
        rows.append(np.mean(np.abs(v - mu) / np.maximum(sd, 0.08 * np.abs(mu).mean() + 1e-3)))
    return float(np.mean(rows))


def main():
    key = sys.argv[1]
    iters = int(os.environ.get("ITERS", 200))
    sun = fit_sun_key(key)
    print(key, "sun", sun, flush=True)
    ref, pix = real_stats(key, sun)
    chm_all = np.concatenate([w[w > 2] for w, _ in sl.real_windows(key, 8, seed=5)])
    h90 = float(np.percentile(chm_all, 90))
    struct = json.load(open(os.path.join(m.OUT, f"{key}_best_struct.json")))
    k = dict(struct["knobs"])
    lit = pix["canopy_lit"] if pix["canopy_lit"] is not None else pix["canopy_shade"]
    if os.environ.get("LIT_CAP"):
        # glare off wet leaves is not leaf colour: leave the brightest share out of the palette
        lum = lit.mean(1)
        lit = lit[lum <= np.percentile(lum, float(os.environ["LIT_CAP"]))]
    ramp = ct.ramp_from_pixels(lit)
    g = (pix["ground_lit"] if pix["ground_lit"] is not None else np.array([[90, 80, 60.0]])).mean(0)
    k.update(ground_r=float(g[0]), ground_g=float(g[1]), ground_b=float(g[2]), shadow_opacity=0.6, shadow_r=0.1, shadow_g=0.12, shadow_b=0.2)
    if sun:
        k["_shadow_angle"], k["_elevation"] = (sun[0] + 90.0) % 360, sun[1]
    for n in ("tint_r", "tint_g", "tint_b"):
        k[n] = 1.0
    bounds = {n: m.KNOBS[n][:2] for n in ct.COLOUR_KNOBS if n in m.KNOBS}
    bounds.update({n: v[:2] for n, v in ct.EXTRA.items() if n not in ("ground_r", "ground_g", "ground_b")})
    bounds.update(ground_r=(20, 240), ground_g=(20, 240), ground_b=(20, 240))
    # dense stands show no ground to measure: the ground colour is fixed, not left to wander
    if os.environ.get("GROUND_FIX"):
        gf = [float(v) for v in os.environ["GROUND_FIX"].split(",")]
        for n, v in zip(("ground_r", "ground_g", "ground_b"), gf):
            k[n] = v
            bounds[n] = (v - 1.0, v + 1.0)
    if os.environ.get("SHADOW_MAX"):
        bounds["shadow_opacity"] = (0.1, float(os.environ["SHADOW_MAX"]))
        k["shadow_opacity"] = min(k["shadow_opacity"], bounds["shadow_opacity"][1])
    # per-crown hue scatter is what made crowns look like confetti: keep every source of it small
    for n, hi in (("crown_var", 0.12), ("tree_lean", 0.15), ("variety", 0.8), ("mottling", 0.4), ("cluster_tone", 0.8), ("dappled", 0.3), ("tint_r", 1.08), ("tint_g", 1.08), ("tint_b", 1.08)):
        if n in bounds:
            bounds[n] = (bounds[n][0] if n[:4] != "tint" else 0.92, hi)
            k[n] = min(k.get(n, 1.0), hi)
    if sun:
        bounds.pop("elevation", None)
    bounds.pop("shadow_length", None) if False else None
    rng = np.random.default_rng(17)

    def score(kk, seeds=(1, 2)):
        f = engine_feats(key, kk, ramp, sun, h90, seeds=seeds)
        parts = {c: dist(f[c], ref[c]) for c in W_CLASS}
        return sum(W_CLASS[c] * parts[c] for c in W_CLASS), parts

    best, (bs, parts) = k, score(k)
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
    json.dump(dict(ramp=ramp, knobs=best, score=bs, parts=parts, sun=sun), open(os.path.join(m.OUT, f"{key}_best_lit2{os.environ.get('OUT_TAG', '')}.json"), "w"), indent=1)
    print("best", bs)


if __name__ == "__main__":
    main()
