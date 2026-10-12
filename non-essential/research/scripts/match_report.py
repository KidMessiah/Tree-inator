"""Held-out check and picture for tuned species. Usage: match_report.py species"""
import json, os, sys
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import canopy_metrics as cm

key = sys.argv[1]
HELD = {
    "aspen": [("aspenCO_4__2023-09-25", 1), ("aspenB_41__2023-09-25", 1), ("aspenB_42__2019-09-14", 1)],
    "pine": [("pineSD_13__2018-07-31", 1), ("pineOR_16__2014-06-30", 1), ("pineAZ_12__2019-06-24", 1)],
    "dfir": [("dfirWA_24__2017-08-22", 1), ("dfirOR_19__2012-08-02", 1), ("dfirOR_20__2014-07-06", 1)],
}[key]
tr = m.real_patches(key)
refs = cm.stack([cm.features(p) for p in tr])
rng = np.random.default_rng(99)
held = []
for tag, n in HELD:
    try:
        img = np.asarray(Image.open(m.scene_path(tag)).convert("RGB"))
    except FileNotFoundError:
        continue
    held += [p.astype(float) for p in cm.crop_patches(img, m.PATCH, n, rng)]
best = json.load(open(os.path.join(m.OUT, f"{key}_best.json")))
k0, k1 = m.start_knobs(), best["knobs"]
gc = m.ground_colour(tr, k0["ground_dark"])
gc1 = m.ground_colour(tr, k1["ground_dark"])
def sc(k, g, pats, ramp=None):
    img = m.composite(m.render(m.build(key, k, seed=5, ramp0=ramp), "rep"), g)
    f = cm.features(img)
    rs = cm.stack([cm.features(p) for p in pats])
    return cm.distance(f, rs)[0], img
# a held-out patch against the held-out set itself shows how far real patches differ from each other
real_vs_real = np.mean([cm.distance(cm.features(p), refs)[0] for p in held])
s0, i0 = sc(k0, gc, held)
s1, i1 = sc(k1, gc1, held, best["ramp"])
print(f"{key}: held-out real patches vs training set {real_vs_real:.2f}; default preset {s0:.2f}; tuned {s1:.2f}; tuned on training {best['score']:.2f}")
rows = [np.concatenate([tr[0], tr[len(tr) // 2], held[0], i0, i1], 1)]
Image.fromarray(np.concatenate(rows, 0).astype(np.uint8)).save(os.path.join(m.OUT, f"report_{key}.png"))
