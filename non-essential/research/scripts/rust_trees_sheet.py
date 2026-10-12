"""Render every tree-gen preset through the Rust port: a sheet of single trees, and a sheet of forests where
the same tree fills the canopy, shrub, lone-tree and pebble layers.

Usage: python rust_trees_sheet.py [--fill F] [--suffix S]
"""
import json
import math
import os
import subprocess
import sys

from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
CLI = os.path.join(ROOT, "target", "release", "forest-cli.exe")
OUT = os.path.join(ROOT, "non-essential", "research", "results", "compare")
TMP = os.environ.get("TEMP", ".")
sys.path.insert(0, HERE)
import wp_treegen as wt  # noqa: E402

HEIGHT = {"balsam_fir": 1.4, "douglas_fir": 1.5, "small_pine": 1.3, "european_larch": 1.4, "palm": 1.4,
          "fan_palm": 0.9, "lombardy_poplar": 1.5, "bamboo": 1.3, "weeping_willow": 1.1, "weeping_willow_o": 1.1,
          "apple": 0.7, "hill_cherry": 0.8}


def ramp_for(rgb, blossom=None):
    r = [[c / 255.0 * m for c in rgb] for m in (0.30, 0.55, 0.85, 1.2)]
    if blossom:
        r[3] = [c / 255.0 for c in blossom]
    return r


PINK = [[0.55, 0.28, 0.38], [0.82, 0.45, 0.58], [0.95, 0.62, 0.75], [1.0, 0.82, 0.88]]


def species(name, fill):
    leaf, blossom = wt.COLORS.get(name, ((100, 140, 50), None))
    ramp = PINK if name == "hill_cherry" else ramp_for(leaf, blossom)
    d = dict(name=name, procedural=True, preset=name, ramp=ramp, height=HEIGHT.get(name, 1.0), bark=[0.36, 0.27, 0.20])
    if fill is not None:
        d["canopy_fill"] = fill
    return d


def run(proj, args=()):
    path = os.path.join(TMP, "rt.json")
    png = os.path.join(TMP, "rt.png")
    json.dump(proj, open(path, "w"))
    o = subprocess.run([CLI, path, "-bg", "-out", png, *args], capture_output=True, text=True)
    if o.returncode:
        print(o.stderr[-400:])
        return None
    return Image.open(png).convert("RGB")


def label(im, txt):
    d = ImageDraw.Draw(im)
    d.rectangle((4, 4, 12 + 7 * len(txt), 20), fill=(255, 255, 255))
    d.text((8, 6), txt, fill=(0, 0, 0))


def sheet(tiles, cols, name):
    W, H = tiles[0].size
    rows = (len(tiles) + cols - 1) // cols
    s = Image.new("RGB", (W * cols, H * rows), (214, 202, 176))
    for k, t in enumerate(tiles):
        s.paste(t, ((k % cols) * W, (k // cols) * H))
    s.save(os.path.join(OUT, name))
    print("wrote", name, s.size, flush=True)


def main(argv):
    fill, suffix = None, ""
    i = 0
    while i < len(argv):
        if argv[i] == "--fill":
            i += 1
            fill = float(argv[i])
        elif argv[i] == "--suffix":
            i += 1
            suffix = argv[i]
        i += 1
    names = wt.PRESETS
    singles, forests = [], []
    pts = [[600 + 430 * math.cos(t / 40 * 6.283) * (1 + 0.15 * math.sin(3 * t / 40 * 6.283)),
            450 + 300 * math.sin(t / 40 * 6.283)] for t in range(40)]
    for nm in names:
        sp = species(nm, fill)
        trees = []
        for k in range(3):
            r = 74
            trees.append(dict(x=130 + k * 250, y=130, r=r, h=r * 1.3 + 18, tone=0.0, layer="Canopy", species=0, seed=17 + k * 5 + nm.__hash__() % 7))
        proj = dict(map_w=760, map_h=260, species=[sp], painted=trees,
                    gen=dict(procedural_trees=False, painted_trees=True, seed=3))
        im = run(proj)
        if im is not None:
            im = im.resize((570, 195), Image.LANCZOS)
            label(im, nm)
            singles.append(im)
        # forest: same tree in every layer
        proj = dict(map_w=1200, map_h=900, species=[sp],
                    shapes=[dict(op="Add", points=pts)],
                    gen=dict(seed=9, main_species=0, shrub_species=0, mix_amount=0.0, canopy_cover=0.8))
        im = run(proj)
        if im is not None:
            im = im.crop((300, 200, 900, 560)).resize((450, 270), Image.LANCZOS)
            label(im, nm)
            forests.append(im)
        print(nm, flush=True)
    sheet(singles, 3, f"rust_singles{suffix}.png")
    sheet(forests, 3, f"rust_forests{suffix}.png")


if __name__ == "__main__":
    main(sys.argv[1:])
