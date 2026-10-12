"""One theme, many tree types: the theme as it ships, then the same forest with the canopy, mix and shrub slots
switched to procedural tree types. The theme supplies all colours.

Usage: python theme_swap_sheet.py [THEME] [type ...]
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

DEFAULT_TYPES = ["quaking_aspen", "black_oak", "weeping_willow", "douglas_fir", "european_larch", "hill_cherry", "palm", "sassafras"]


def run(proj, theme):
    path = os.path.join(TMP, "ts.json")
    png = os.path.join(TMP, "ts.png")
    json.dump(proj, open(path, "w"))
    o = subprocess.run([CLI, path, "-bg", "-theme", theme, "-out", png], capture_output=True, text=True)
    if o.returncode:
        print(o.stderr[-400:])
        return None
    return Image.open(png).convert("RGB")


def label(im, txt):
    d = ImageDraw.Draw(im)
    d.rectangle((4, 4, 12 + 7 * len(txt), 20), fill=(255, 255, 255))
    d.text((8, 6), txt, fill=(0, 0, 0))


def main(argv):
    theme = argv[0] if argv else "Hellscape"
    types = argv[1:] or DEFAULT_TYPES
    pts = [[600 + 430 * math.cos(t / 40 * 6.283) * (1 + 0.15 * math.sin(3 * t / 40 * 6.283)),
            450 + 300 * math.sin(t / 40 * 6.283)] for t in range(40)]
    tiles = []
    for ty in [None] + types:
        sp = dict(name="Slot")
        if ty:
            sp.update(procedural=True, preset=ty)
        proj = dict(map_w=1200, map_h=900, species=[sp], shapes=[dict(op="Add", points=pts)],
                    gen=dict(seed=9, main_species=0, mix_species=0, shrub_species=0, canopy_cover=0.8))
        im = run(proj, theme)
        if im is None:
            continue
        im = im.crop((250, 170, 950, 590)).resize((525, 315), Image.LANCZOS)
        label(im, f"{theme}: {ty or 'as shipped'}")
        tiles.append(im)
        print(ty, flush=True)
    W, H = tiles[0].size
    cols = 3
    rows = (len(tiles) + cols - 1) // cols
    s = Image.new("RGB", (W * cols, H * rows), (214, 202, 176))
    for k, t in enumerate(tiles):
        s.paste(t, ((k % cols) * W, (k // cols) * H))
    name = f"theme_swap_{theme.lower().replace(' ', '_')}.png"
    s.save(os.path.join(OUT, name))
    print("wrote", name, s.size)


if __name__ == "__main__":
    main(sys.argv[1:])
