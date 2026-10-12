"""Real camera windows next to engine renders of the tuned biome, same 128 m window size.
Usage: biome_compare.py OUT.png key [key ...]"""
import json, os, sys
import numpy as np
from PIL import Image, ImageDraw
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.setdefault("TEMPLATE", os.path.join(HERE, "..", "ui_mock", "sheet_project.json"))
import biomes
import match_species as m
import structure_lib as sl
import class_colour_lib as cc
import colour_tune as ct

out, keys = sys.argv[1], sys.argv[2:]
T = 256
rows = []
for key in keys:
    d = json.load(open(os.path.join(m.OUT, f"{key}_best_lit2.json")))
    kn = d["knobs"]
    real = []
    for w, (y, x) in sl.real_windows(key, 24, seed=5)[:4]:
        rgb, _, _ = cc.real_window_rgb(key, y, x, out_px=T)
        real.append(Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8)))
    eng = []
    for s in (1, 2, 3, 4):
        rgba, _ = ct.render_pair(key, kn, d["ramp"], s)
        img = ct.composite(rgba, (kn["ground_r"], kn["ground_g"], kn["ground_b"]))
        eng.append(Image.fromarray(np.clip(img, 0, 255).astype(np.uint8)).resize((T, T)))
    rows.append((key, real, eng))
sheet = Image.new("RGB", (8 * T + 20, len(rows) * (T + 4)), (24, 24, 24))
for r, (key, real, eng) in enumerate(rows):
    for i, im in enumerate(real):
        sheet.paste(im, (i * T, r * (T + 4)))
    for i, im in enumerate(eng):
        sheet.paste(im, (4 * T + 20 + i * T, r * (T + 4)))
    dr = ImageDraw.Draw(sheet)
    dr.rectangle((0, r * (T + 4), 90, r * (T + 4) + 14), fill=(255, 255, 255))
    dr.text((3, r * (T + 4) + 1), key + " real", fill=(0, 0, 0))
    dr.rectangle((4 * T + 20, r * (T + 4), 4 * T + 110, r * (T + 4) + 14), fill=(255, 255, 255))
    dr.text((4 * T + 23, r * (T + 4) + 1), key + " engine", fill=(0, 0, 0))
sheet.save(out)
print(out, sheet.size)
