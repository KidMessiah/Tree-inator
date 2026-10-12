"""Hyperspectral true colour against engine renders, same window, same 1 m scale, with class colour strips."""
import json, os, sys
import numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import structure_lib as sl
import colour_tune as ct
import colour_tune2 as c2
import colour_tune3 as c3

UP = 4
def tile(a, label):
    im = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8)).resize((sl.WIN * UP, sl.WIN * UP), Image.LANCZOS)
    ImageDraw.Draw(im).text((8, 6), label, fill="white")
    return im
def strip(px, w, h=34, n=24):
    if px is None or len(px) < 20:
        return Image.new("RGB", (w, h))
    q = px[np.argsort(px.mean(1))][np.linspace(0, len(px) - 1, n).astype(int)]
    return Image.fromarray(np.repeat(q[None].astype(np.uint8), h, 0)).resize((w, h), Image.NEAREST)

for key in ("dfir", "pine", "aspen"):
    bands, sun = c3.site_data(key)
    k = c3.calibrate(key, bands, sun)
    img = c3.true_colour(bands, k)
    wins = sl.real_windows(key, 16, seed=5)
    reals = [img[y:y + sl.WIN, x:x + sl.WIN] for _, (y, x) in (wins[1], wins[5])]
    new = json.load(open(os.path.join(m.OUT, f"{key}_best_hyper2.json")))
    old = json.load(open(os.path.join(m.OUT, f"{key}_best_hyper.json")))
    def render(b):
        kk = b["knobs"]
        rgba, hm = ct.render_pair(key, kk, b["ramp"], 41)
        e = ct.composite(rgba, (kk["ground_r"], kk["ground_g"], kk["ground_b"]))
        return e.reshape(sl.WIN, 2, sl.WIN, 2, 3).mean((1, 3)), hm.reshape(sl.WIN, 2, sl.WIN, 2).max((1, 3))
    e_old, _ = render(old)
    e_new, h_new = render(new)
    w = sl.WIN * UP
    sheet = Image.new("RGB", (w * 2, w * 2 + 80), (20, 20, 20))
    for i, t in enumerate([tile(reals[0], "real: hyperspectral true colour (1 m)"), tile(reals[1], "real"), tile(e_old, "engine, previous hyperspectral round"), tile(e_new, "engine, now: pure-vegetation colour, capped weave")]):
        sheet.paste(t, ((i % 2) * w, (i // 2) * w))
    cl_r = c2.classes(np.kron(wins[1][0], np.ones((1, 1))), 1.0, sun)
    d = ImageDraw.Draw(sheet)
    scale = 1.0
    hpos = h_new[h_new > 0]
    hs = np.percentile(np.concatenate([a[a > 2] for a, _ in wins[:8]]), 90) / max(np.percentile(hpos, 90), 1e-6)
    cl_e = c2.classes(h_new * hs, 1.0, sun)
    y0 = 2 * w + 4
    for j, (lab, px) in enumerate((("lit canopy, real", reals[0][cl_r["canopy_lit"]]), ("lit canopy, engine", e_new[cl_e["canopy_lit"]]), ("shaded canopy, real", reals[0][cl_r["canopy_shade"]]), ("shaded canopy, engine", e_new[cl_e["canopy_shade"]]))):
        xx, yy = (j % 2) * w, y0 + (j // 2) * 38
        sheet.paste(strip(px, w - 8), (xx + 4, yy)); d.text((xx + 8, yy + 2), lab, fill="white")
    out = os.path.join(m.OUT, "sheets", f"hyper_{key}.png")
    sheet.save(out); print(out, "parts", new["parts"])
