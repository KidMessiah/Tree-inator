"""Real NEON canopy height windows, and the same measurements on the engine's own height map."""
import json, os, subprocess, sys, threading
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(__file__))
import canopy_metrics as cm
import chm_metrics as cmm
import match_species as m

ROOT = os.path.join(os.path.dirname(__file__), "..")
NEON = os.path.join(ROOT, "neon")
WIN = 128
SITES = {"aspen": ("STEI", 2022, 296000, 5044000), "pine": ("OSBS", 2023, 409000, 3281000), "dfir": ("WREF", 2023, 578000, 5072000)}
MIN_COVER = {"aspen": 0.85, "pine": 0.5, "dfir": 0.75}
MIN_MEDIAN = {"pinyon": 2.5, "boreal": 3.0}  # stands of low trees: the median canopy height a window must have
WINDOW_OK = lambda key, y, x: True  # replaced by biomes.py for strips with missing or shaded parts
MAX_COVER = {"aspen": 1.01, "pine": 0.99, "dfir": 1.01}


def chm_tile(key):
    site, y, e, n = SITES[key]
    a = np.asarray(Image.open(os.path.join(NEON, site, f"{y}_{e}_{n}_CHM.tif"))).astype(np.float32)
    a[a < -50] = 0
    return a


def real_windows(key, n=24, seed=5):
    a = chm_tile(key)
    rng = np.random.default_rng(seed)
    out, tries = [], 0
    while len(out) < n and tries < 4000:
        tries += 1
        y, x = rng.integers(0, a.shape[0] - WIN), rng.integers(0, a.shape[1] - WIN)
        w = a[y : y + WIN, x : x + WIN]
        if not WINDOW_OK(key, y, x):
            continue
        c = (w > 2).mean()
        if MIN_COVER[key] <= c <= MAX_COVER[key] and np.percentile(w[w > 2], 50) > MIN_MEDIAN.get(key, 4):
            out.append((w, (y, x)))
    return out


def real_metrics(key, n=24):
    ms = [cmm.metrics(w, 2.0, 3.0) for w, _ in real_windows(key, n)]
    return {k: np.stack([x[k] for x in ms]) for k in ms[0]}


def render_chm(key, knobs, seed=1, scale=2, extra_env=None, cli=None):
    p = m.build(key, knobs, seed=seed)
    side = int(round(WIN * m.K))
    p["map_w"] = p["map_h"] = side
    p["shapes"][0]["points"] = [[-100, -100], [side + 100, -100], [side + 100, side + 100], [-100, side + 100]]
    pj = os.path.join(m.OUT, f"chm_{key}_{threading.get_ident()}.json")
    json.dump(p, open(pj, "w"))
    out = os.path.join(m.OUT, f"chm_{key}_{threading.get_ident()}.png")
    env = dict(os.environ)
    env.update(extra_env or {})
    subprocess.run([cli or m.CLI, pj, "-chm", out, "-scale", str(scale * WIN / side)], check=True, capture_output=True, env=env)
    a = np.asarray(Image.open(out)).astype(np.float32) / 256.0
    s = scale
    h = a.reshape(a.shape[0] // s, s, a.shape[1] // s, s).max((1, 3)) / float(s)  # back to the 1 m grid, in the render's own height units
    return h


def engine_metrics(key, knobs, seeds=(1, 2, 3), **kw):
    ms = [cmm.metrics(render_chm(key, knobs, s, **kw), 0.3, 0.8) for s in seeds]
    return {k: np.mean([x[k] for x in ms], 0) for k in ms[0]}


def distance(feat, ref, groups=None):
    rows = {}
    for k, v in feat.items():
        r = ref[k]
        mu, sd = r.mean(0), r.std(0)
        floor = 0.08 * np.abs(mu).mean() + 1e-3
        rows[k] = float(np.mean(np.abs(v - mu) / np.maximum(sd, floor)))
    return float(np.mean(list(rows.values()))), rows
