"""Real 10 cm photo windows beside engine renders: tuned on photos before, and now tuned on lidar structure and per-class colour.
Usage: compare_final.py [stage]    (stage is the suffix of the *_best_<stage>.json to show as 'now', default colour)"""
import json, os, sys
import numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import structure_lib as sl
import class_colour_lib as cc
import colour_tune as ct

stage = sys.argv[1] if len(sys.argv) > 1 else "lit2"
UP = 3


def tile(a, label):
    im = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8)).resize((256 * UP // 2 * 2, 256 * UP // 2 * 2), Image.LANCZOS)
    ImageDraw.Draw(im).text((8, 6), label, fill="white")
    return im


def strip(px, h=40, w=768, n=24):
    """A row of colours: the pixels' brightness quantiles, to read a colour distribution at a glance."""
    if px is None or len(px) < 10:
        return Image.new("RGB", (w, h))
    order = np.argsort(px.mean(1))
    q = px[order][np.linspace(0, len(px) - 1, n).astype(int)]
    return Image.fromarray(np.repeat(q[None].astype(np.uint8), h, 0)).resize((w, h), Image.NEAREST)


for key in ("dfir", "pine", "aspen"):
    wins = sl.real_windows(key, 16, seed=5)
    picks = [wins[1], wins[5]]
    real = []
    for w, (y, x) in picks:
        rgb, cmask, gmask = cc.real_window_rgb(key, y, x)
        real.append((rgb, cmask, gmask))
    now = json.load(open(os.path.join(m.OUT, f"{key}_best_{stage}.json")))
    before = json.load(open(os.path.join(m.OUT, f"{key}_best_colour.json")))
    kn = now["knobs"]
    g0 = (kn["ground_r"], kn["ground_g"], kn["ground_b"])
    rgba_now, h_now = ct.render_pair(key, kn, now["ramp"], 41)
    img_now = ct.composite(rgba_now, g0)
    kb = dict(before["knobs"])
    for n in ("ground_r", "ground_g", "ground_b", "shadow_opacity", "shadow_r", "shadow_g", "shadow_b"):
        kb[n] = kn[n]
    rgba_b, h_b = ct.render_pair(key, kb, before["ramp"], 41)
    img_b = ct.composite(rgba_b, g0)
    tiles = [tile(real[0][0], "real (10 cm photo)"), tile(real[1][0], "real"), tile(img_b, "engine, previous round (all canopy)"), tile(img_now, "engine, tuned on lit/shaded classes")]
    w = tiles[0].size[0]
    sheet = Image.new("RGB", (w * 2, w * 2 + 90), (20, 20, 20))
    for i, t in enumerate(tiles):
        sheet.paste(t, ((i % 2) * w, (i // 2) * w))
    # class colour strips: canopy pixels and ground pixels, real against ours
    canreal = np.concatenate([r[0][r[1]] for r in real])
    gndreal = np.concatenate([r[0][r[2]] for r in real]) if any(r[2].any() for r in real) else None
    cn = img_now[h_now > 0]
    gn = img_now[h_now == 0]
    y0 = 2 * w + 4
    d = ImageDraw.Draw(sheet)
    for j, (lab, px) in enumerate((("canopy colours, real", canreal), ("canopy colours, engine", cn), ("ground colours, real", gndreal), ("ground colours, engine", gn))):
        xx, yy = (j % 2) * w, y0 + (j // 2) * 44
        sheet.paste(strip(px, 40, w - 8), (xx + 4, yy))
        d.text((xx + 8, yy + 2), lab, fill="white")
    out = os.path.join(m.OUT, "sheets", f"final_{key}_{stage}.png")
    sheet.save(out)
    print(out)
