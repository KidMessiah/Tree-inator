"""Real patches beside renders from successive tuning rounds, with structure numbers.
Usage: ba_versions.py out_tag ver1 ver2 ...   (versions are the suffixes of *_best<suffix>.json, '' for the first round)"""
import json, os, sys, subprocess
import numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import ba_changes as bc

# engine settings each version was tuned under (experiment build); the first rounds used the plain build
ENVS = {"_w1": {"Z_BASE": "0.6", "Z_JITTER": "0.15"}, "_w2": {"Z_BASE": "0.75", "Z_JITTER": "0.3"}}
EXPCLI = os.environ.get("EXPCLI", "")

vers = sys.argv[2:]
rows, table = [], {}
cols = ["low", "mid", "crown", "fine", "chroma_corr", "dark_b", "blob50", "compact"]
for key in ("aspen", "pine", "dfir"):
    real = m.real_patches(key)
    rs = [bc.stats(p) for p in real]
    table[(key, "real")] = {k: float(np.mean([r[k] for r in rs])) for k in rs[0]}
    cells = [real[0], real[3]]
    for v in vers:
        tag = "" if v == "first" else v
        best = json.load(open(os.path.join(m.OUT, f"{key}_best{tag}.json")))
        gc = m.ground_colour(real, best["knobs"]["ground_dark"])
        if v in ENVS:
            imgs = [m.composite(bc.render_variant(key, best, ENVS[v], s), gc) for s in (31, 32, 33)]
        else:
            imgs = [m.composite(m.render(m.build(key, best["knobs"], seed=s, ramp0=best["ramp"]), f"bv_{key}"), gc) for s in (31, 32, 33)]
        cells.append(imgs[0])
        st = [bc.stats(i) for i in imgs]
        table[(key, v)] = {k: float(np.mean([r[k] for r in st])) for k in st[0]}
    rows.append(np.concatenate(cells, 1))
img = Image.fromarray(np.concatenate(rows, 0).astype(np.uint8))
d = ImageDraw.Draw(img)
for i, t in enumerate(["real", "real"] + vers):
    d.text((i * 256 + 4, 2), t, fill="white")
out = os.path.join(m.OUT, f"ba_versions_{sys.argv[1]}.png")
img.save(out)
print(out)
print(f"{'':16s}" + "".join(f"{c:>12s}" for c in cols))
for (key, name), v in table.items():
    print(f"{key + ' ' + name:16s}" + "".join(f"{v[c]:12.3f}" for c in cols))
