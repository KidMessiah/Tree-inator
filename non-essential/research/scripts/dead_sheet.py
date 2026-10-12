"""Dead trees: hero renders of each type, then forests of each, through the Rust engine.

Usage: python dead_sheet.py [--names a,b,c] [--suffix S] [--r R]
"""
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import rust_trees_sheet as rt  # noqa: E402

LEAF = [[0.12, 0.2, 0.08], [0.22, 0.36, 0.12], [0.36, 0.52, 0.18], [0.5, 0.66, 0.26]]
# bark colour per type: weathered grey, silver, charred
BARK = {
    "bare_oak": [0.30, 0.24, 0.19], "dead_oak": [0.58, 0.54, 0.48], "stag_oak": [0.46, 0.40, 0.33],
    "dead_birch": [0.6, 0.58, 0.53], "dead_willow": [0.46, 0.42, 0.36], "dead_pine": [0.5, 0.42, 0.36],
    "snag": [0.6, 0.57, 0.52], "burnt_oak": [0.22, 0.21, 0.2], "burnt_pine": [0.22, 0.21, 0.2],
}
NAMES = list(BARK)


def species(name, extra=None):
    if name == 'old_dead':
        # the species the app had before: the old non-procedural bare tree
        return dict(name='Dead', bare=True, form='Dead', procedural=False, ramp=[[0.157, 0.133, 0.11], [0.29, 0.25, 0.2], [0.44, 0.38, 0.31], [0.59, 0.53, 0.44]], bark=[0.36, 0.27, 0.2])
    d = dict(name=name, procedural=True, preset=name, ramp=LEAF, height=rt.HEIGHT.get(name, 1.0), bark=BARK.get(name, [0.4, 0.33, 0.27]))
    if extra:
        d.update(extra)
    return d


def main(argv):
    names, suffix, r, args = NAMES, "", 110, ()
    for i, a in enumerate(argv):
        if a == "--names":
            names = argv[i + 1].split(",")
        elif a == "--suffix":
            suffix = argv[i + 1]
        elif a == "--scale":
            args = ("-scale", argv[i + 1])
        elif a == "--r":
            r = float(argv[i + 1])
    pts = [[600 + 430 * math.cos(t / 40 * 6.283) * (1 + 0.15 * math.sin(3 * t / 40 * 6.283)),
            450 + 300 * math.sin(t / 40 * 6.283)] for t in range(40)]
    singles, forests = [], []
    for nm in names:
        sp = species(nm)
        trees = [dict(x=150 + k * 300, y=150, r=r, h=r * 1.3 + 18, tone=0.0, layer="Canopy", species=0, seed=17 + k * 5) for k in range(2)]
        proj = dict(map_w=600, map_h=300, species=[sp], painted=trees, gen=dict(procedural_trees=False, painted_trees=True, seed=3))
        im = rt.run(proj)
        if im is not None:
            rt.label(im, nm)
            singles.append(im)
        proj = dict(map_w=1200, map_h=900, species=[sp], shapes=[dict(op="Add", points=pts)],
                    gen=dict(seed=9, main_species=0, shrub_species=0, mix_amount=0.0, canopy_cover=0.5, lone_groups=0.0, grass_color=[0.2, 0.19, 0.14], grass_accent=[0.28, 0.25, 0.18]))
        im = rt.run(proj, args)
        if im is not None:
            c = float(args[1]) if args else 1.0
            im = im.crop((int(300 * c), int(250 * c), int(900 * c), int(560 * c)))
            rt.label(im, nm)
            forests.append(im)
        print(nm, flush=True)
    rt.sheet(singles, 3, f"dead_singles{suffix}.png")
    rt.sheet(forests, 3, f"dead_forests{suffix}.png")


if __name__ == "__main__":
    main(sys.argv[1:])
