"""Before and after: the same tuned settings drawn with 1, 2 and 4 samples per pixel axis, beside real patches."""
import json, os, sys
import numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m

rows = []
for key in ("aspen", "pine", "dfir"):
    best = json.load(open(os.path.join(m.OUT, f"{key}_best.json")))
    real = m.real_patches(key)
    gc = m.ground_colour(real, best["knobs"]["ground_dark"])
    cells = [real[0], real[3]]
    for S in (1, 2, 4):
        m.SUPER = S
        cells.append(m.composite(m.render(m.build(key, best["knobs"], seed=21, ramp0=best["ramp"]), f"ba{S}"), gc))
    rows.append(np.concatenate(cells, 1))
img = Image.fromarray(np.concatenate(rows, 0).astype(np.uint8))
d = ImageDraw.Draw(img)
for i, t in enumerate(("real", "real", "1 sample", "2x2 samples", "4x4 samples")):
    d.text((i * 256 + 4, 2), t, fill="white")
img.save(os.path.join(m.OUT, "ba_super.png"))
