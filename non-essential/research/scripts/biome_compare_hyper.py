"""Real hyperspectral true colour next to engine renders of the tuned stand, same 128 m windows.
Usage: biome_compare_hyper.py OUT.png key [key ...]"""
import json, os, sys
import numpy as np
from PIL import Image, ImageDraw
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.setdefault("TEMPLATE", os.path.join(HERE, "..", "ui_mock", "sheet_project.json"))
import biomes
import match_species as m
import structure_lib as sl
import colour_tune as ct
import colour_tune3 as t3

out, keys = sys.argv[1], sys.argv[2:]
T = 256
rows = []
for key in keys:
    d = json.load(open(os.path.join(m.OUT, f"{key}_best_hyper2.json")))
    kn = d["knobs"]
    bands, sun = t3.site_data(key)
    img = t3.true_colour(bands, d["scale"])
    real = []
    for w, (y, x) in sl.real_windows(key, 24, seed=5)[:4]:
        tile = np.clip(img[y : y + sl.WIN, x : x + sl.WIN], 0, 255).astype(np.uint8)
        real.append(Image.fromarray(tile).resize((T, T), Image.BICUBIC))
    eng = []
    for s in (1, 2, 3, 4):
        rgba, _ = ct.render_pair(key, kn, d["ramp"], s)
        c = ct.composite(rgba, (kn["ground_r"], kn["ground_g"], kn["ground_b"]))
        eng.append(Image.fromarray(np.clip(c, 0, 255).astype(np.uint8)).resize((T, T)))
    rows.append((key, real, eng))
sheet = Image.new("RGB", (8 * T + 20, len(rows) * (T + 4)), (24, 24, 24))
dr = ImageDraw.Draw(sheet)
for r, (key, real, eng) in enumerate(rows):
    for i, im in enumerate(real):
        sheet.paste(im, (i * T, r * (T + 4)))
    for i, im in enumerate(eng):
        sheet.paste(im, (4 * T + 20 + i * T, r * (T + 4)))
    dr.rectangle((0, r * (T + 4), 110, r * (T + 4) + 14), fill=(255, 255, 255))
    dr.text((3, r * (T + 4) + 1), key + " real (hyper)", fill=(0, 0, 0))
    dr.rectangle((4 * T + 20, r * (T + 4), 4 * T + 110, r * (T + 4) + 14), fill=(255, 255, 255))
    dr.text((4 * T + 23, r * (T + 4) + 1), key + " engine", fill=(0, 0, 0))
sheet.save(out)
print(out, sheet.size)
