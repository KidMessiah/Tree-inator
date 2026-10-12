"""One species, several engine variants at high resolution beside real patches.
Usage: variants_hires.py species version name=ENV1:val,ENV2:val ..."""
import json, os, sys
import numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import ba_changes as bc

key, ver = sys.argv[1], sys.argv[2]
variants = []
for a in sys.argv[3:]:
    name, _, envs = a.partition("=")
    env = dict(kv.split(":") for kv in envs.split(",") if kv)
    variants.append((name, env))
best = json.load(open(os.path.join(m.OUT, f"{key}_best{ver}.json")))
real = m.real_patches(key)
gc = m.ground_colour(real, best["knobs"]["ground_dark"])
UP = 3
def tile(a, label):
    im = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8)).resize((256 * UP, 256 * UP), Image.LANCZOS)
    ImageDraw.Draw(im).text((8, 6), label, fill="white")
    return im
tiles = [tile(real[0], "real"), tile(real[3], "real")]
for name, env in variants:
    if "FLAT_R" in env:
        env["BGCOLOR"] = ",".join(str(int(v)) for v in gc)
    tiles.append(tile(m.composite(bc.render_variant(key, best, env, 41), gc), name))
cols = 3
rows = (len(tiles) + cols - 1) // cols
w = 256 * UP
sheet = Image.new("RGB", (cols * w, rows * w), (20, 20, 20))
for i, t in enumerate(tiles):
    sheet.paste(t, ((i % cols) * w, (i // cols) * w))
out = os.path.join(m.OUT, "sheets", f"variants_{key}.png")
sheet.save(out)
print(out)
