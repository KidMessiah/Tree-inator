"""High resolution comparison sheets: real patches on top, engine renders below (default preset, tuned, tuned with weave)."""
import json, os, sys
import numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import ba_changes as bc

UP = 3
OUTD = os.path.join(m.OUT, "sheets")
os.makedirs(OUTD, exist_ok=True)
W2 = {"Z_BASE": "0.75", "Z_JITTER": "0.3"}


def tile(a, label):
    im = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8)).resize((256 * UP, 256 * UP), Image.LANCZOS)
    ImageDraw.Draw(im).text((8, 6), label, fill="white")
    return im


for key in ("aspen", "pine", "dfir"):
    real = m.real_patches(key)
    v3 = json.load(open(os.path.join(m.OUT, f"{key}_best_v3.json")))
    w2 = json.load(open(os.path.join(m.OUT, f"{key}_best_w2.json")))
    gc3 = m.ground_colour(real, v3["knobs"]["ground_dark"])
    gcw = m.ground_colour(real, w2["knobs"]["ground_dark"])
    base_best = dict(knobs=m.start_knobs(), ramp=None)
    gc0 = m.ground_colour(real, m.start_knobs()["ground_dark"])
    m.SUPER = 2
    default = m.composite(m.render(m.build(key, m.start_knobs(), seed=41), "hs_def"), gc0)
    tuned = m.composite(m.render(m.build(key, v3["knobs"], seed=41, ramp0=v3["ramp"]), "hs_v3"), gc3)
    weave = m.composite(bc.render_variant(key, w2, W2, 41), gcw)
    weave2 = m.composite(bc.render_variant(key, w2, W2, 42), gcw)
    tiles = [tile(real[0], "real"), tile(real[3], "real"), tile(real[5] if len(real) > 5 else real[1], "real"),
             tile(default, "default preset (engine)"), tile(tuned, "tuned (engine)"), tile(weave, "tuned + canopy weave (engine)"), tile(weave2, "tuned + canopy weave, other seed")]
    w, h = tiles[0].size
    sheet = Image.new("RGB", (w * 3, h * 3), (20, 20, 20))
    for i, t in enumerate(tiles[:6]):
        sheet.paste(t, ((i % 3) * w, (i // 3) * h))
    sheet.paste(tiles[6], (0, 2 * h))
    sheet.paste(tile(real[1], "real"), (w, 2 * h))
    sheet.paste(tile(real[2], "real"), (2 * w, 2 * h))
    out = os.path.join(OUTD, f"{key}_sheet.png")
    sheet.save(out)
    print(out, sheet.size)
