"""The nine new non-dead tree types: hero renders and forests through the Rust engine.

Usage: python new_trees_sheet.py [--names a,b,c] [--suffix S] [--r R] [--scale S]
"""
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import rust_trees_sheet as rt  # noqa: E402

# name: (leaf colour, bark colour, height)
TYPES = {
    "baobab": ((92, 128, 52), [0.52, 0.44, 0.38], 1.0),
    "banyan": ((52, 96, 40), [0.42, 0.36, 0.3], 1.0),
    "mangrove": ((60, 92, 48), [0.3, 0.25, 0.2], 0.8),
    "dragon_blood": ((52, 112, 104), [0.46, 0.42, 0.4], 0.9),
    "joshua_tree": ((112, 128, 70), [0.5, 0.4, 0.3], 0.8),
    "krummholz": ((40, 78, 52), [0.32, 0.26, 0.2], 0.5),
    "eucalyptus": ((104, 138, 108), [0.5, 0.44, 0.38], 1.2),
    "ginkgo": ((214, 176, 40), [0.4, 0.33, 0.27], 1.2),
    "fallen_log": ((100, 140, 50), [0.34, 0.27, 0.2], 0.4),
    "stump": ((100, 140, 50), [0.5, 0.38, 0.26], 0.3),
}


def species(name):
    leaf, bark, height = TYPES[name]
    return dict(name=name, procedural=True, preset=name, ramp=rt.ramp_for(leaf), height=height, bark=bark)


def main(argv):
    names, suffix, r, args = list(TYPES), "", 110, ()
    for i, a in enumerate(argv):
        if a == "--names":
            names = argv[i + 1].split(",")
        elif a == "--suffix":
            suffix = argv[i + 1]
        elif a == "--r":
            r = float(argv[i + 1])
        elif a == "--scale":
            args = ("-scale", argv[i + 1])
    pts = [[600 + 430 * math.cos(t / 40 * 6.283) * (1 + 0.15 * math.sin(3 * t / 40 * 6.283)),
            450 + 300 * math.sin(t / 40 * 6.283)] for t in range(40)]
    singles, forests = [], []
    for nm in names:
        sp = species(nm)
        h = TYPES[nm][2]
        trees = [dict(x=150 + k * 300, y=150, r=r, h=(r * 1.3 + 18) * h, tone=0.0, layer="Canopy", species=0, seed=17 + k * 5) for k in range(2)]
        proj = dict(map_w=600, map_h=300, species=[sp], painted=trees, gen=dict(procedural_trees=False, painted_trees=True, seed=3))
        im = rt.run(proj)
        if im is not None:
            rt.label(im, nm)
            singles.append(im)
        small = nm in ("fallen_log", "stump")
        if small:
            # scattered over open ground, at the sizes of small crowns
            import random
            rnd = random.Random(5)
            bits = [dict(x=rnd.uniform(40, 560), y=rnd.uniform(40, 260), r=rnd.uniform(14, 30), h=10, tone=0.0, layer="Lone", species=0, seed=rnd.randrange(1000)) for _ in range(22)]
            proj = dict(map_w=600, map_h=300, species=[sp], painted=bits, gen=dict(procedural_trees=False, painted_trees=True, seed=3))
        else:
            proj = dict(map_w=1200, map_h=900, species=[sp], shapes=[dict(op="Add", points=pts)],
                        gen=dict(seed=9, main_species=0, shrub_species=0, mix_amount=0.0, canopy_cover=0.5, lone_groups=0.0))
        im = rt.run(proj, args)
        if im is not None:
            c = float(args[1]) if args else 1.0
            if not small:
                im = im.crop((int(300 * c), int(250 * c), int(900 * c), int(560 * c)))
            rt.label(im, nm)
            forests.append(im)
        print(nm, flush=True)
    rt.sheet(singles, 3, f"new_singles{suffix}.png")
    rt.sheet(forests, 3, f"new_forests{suffix}.png")


if __name__ == "__main__":
    main(sys.argv[1:])
