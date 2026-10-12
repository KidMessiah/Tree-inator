"""Writes random but plausible projects for the CLI to render: odd settings, layers, painted trees, zones.

Usage: python fuzz_projects.py base.json out_dir count [seed]
"""
import json
import os
import random
import sys

base_path, out_dir, count = sys.argv[1], sys.argv[2], int(sys.argv[3])
rnd = random.Random(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
base = json.load(open(base_path))
os.makedirs(out_dir, exist_ok=True)

TYPES = ["quaking_aspen","black_oak","acer","weeping_willow","balsam_fir","palm","fan_palm","bamboo","bare_oak","dead_oak","stag_oak","dead_birch","dead_willow","dead_pine","snag","burnt_oak","burnt_pine","baobab","banyan","mangrove","dragon_blood","joshua_tree","krummholz","eucalyptus","ginkgo","fallen_log","stump","mushroom_redcap","mushroom_puffball"]
KEEP = {"seed", "main_species", "mix_species", "shrub_species", "levels"}


def jitter(gen):
    for k, v in list(gen.items()):
        if k in KEEP:
            continue
        if isinstance(v, bool):
            if rnd.random() < 0.15:
                gen[k] = not v
        elif isinstance(v, float):
            if v != 0 and rnd.random() < 0.35:
                gen[k] = v * rnd.choice([0.25, 0.5, 0.8, 1.25, 2.0, 3.0])
            elif v == 0 and rnd.random() < 0.05:
                gen[k] = rnd.random()
    gen["seed"] = rnd.randrange(1, 100000)
    gen["scale"] = rnd.choice([0.25, 0.5, 1.0, 1.0, 1.5, 2.5, 4.0])
    gen["canopy_cover"] = min(1.0, max(0.05, gen.get("canopy_cover", 0.7) * rnd.choice([0.5, 1.0, 1.3])))
    gen["crown_shyness"] = rnd.random() < 0.4
    gen["shyness_gap"] = rnd.choice([0.5, 1.6, 4.0, 8.0])
    gen["shyness_giants_only"] = rnd.random() < 0.3


def poly(w, h):
    n = rnd.randrange(3, 12)
    cx, cy = rnd.uniform(0.2, 0.8) * w, rnd.uniform(0.2, 0.8) * h
    r = rnd.uniform(0.1, 0.45) * min(w, h)
    import math
    return [[cx + math.cos(i / n * 6.283) * r * rnd.uniform(0.5, 1.3), cy + math.sin(i / n * 6.283) * r * rnd.uniform(0.5, 1.3)] for i in range(n)]


for i in range(count):
    p = json.loads(json.dumps(base))
    w, h = p["map_w"], p["map_h"]
    jitter(p["gen"])
    nsp = len(p["species"])
    p["gen"]["main_species"] = rnd.randrange(nsp)
    p["gen"]["shrub_species"] = rnd.randrange(nsp)
    p["gen"]["mix_species"] = rnd.randrange(nsp)
    for sp in p["species"]:
        if rnd.random() < 0.5:
            sp["preset"] = rnd.choice(TYPES)
            sp["procedural"] = True
    p["gen"]["procedural_forms"] = rnd.random() < 0.7
    # a random tree of groups and layers, each changing a few settings
    nodes = []
    for k in range(rnd.randrange(0, 6)):
        kind = rnd.choice(["Layer", "Layer", "Group"])
        groups = [n["id"] for n in nodes if n["kind"] == "Group"]
        over = {}
        if rnd.random() < 0.7:
            over["scale"] = rnd.choice([0.5, 1.0, 1.5, 2.5])
        if rnd.random() < 0.4:
            over["canopy_cover"] = rnd.choice([0.3, 0.6, 0.9])
        if rnd.random() < 0.3:
            over["depth"] = {"gain": rnd.choice([0.7, 1.0, 1.4])}
        if rnd.random() < 0.3:
            over["crown_shyness"] = True
        nodes.append(dict(id=k + 1, parent=rnd.choice([None] + groups), kind=kind, name=f"n{k}", visible=rnd.random() < 0.85, collapsed=False, overrides=over or None, species=None))
    p["groups"] = nodes
    p.pop("layers", None)
    leaf_ids = [n["id"] for n in nodes]
    for s in p["shapes"]:
        if rnd.random() < 0.3:
            s["points"] = poly(w, h)
        s["visible"] = rnd.random() < 0.9
        s.pop("layer", None)
        if leaf_ids and rnd.random() < 0.6:
            s["group"] = rnd.choice(leaf_ids)
    for _ in range(rnd.randrange(0, 3)):
        p["shapes"].append(dict(op=rnd.choice(["Add", "Subtract", "Add"]), points=poly(w, h), name="", visible=True, group=rnd.choice([None] + leaf_ids)))
    p["painted"] = [dict(x=rnd.uniform(0, w), y=rnd.uniform(0, h), r=rnd.uniform(4, 40), h=rnd.uniform(6, 60), tone=rnd.uniform(-0.3, 0.3), layer=rnd.choice(["Canopy", "Shrub", "Lone", "Pebble"]), species=rnd.randrange(nsp), seed=rnd.randrange(1000)) for _ in range(rnd.randrange(0, 40))]
    p["erased"] = [[rnd.uniform(0, w), rnd.uniform(0, h), rnd.uniform(5, 80)] for _ in range(rnd.randrange(0, 5))]
    p["light"]["shadow_angle"] = rnd.uniform(0, 360)
    json.dump(p, open(os.path.join(out_dir, f"case{i:03d}.json"), "w"))
print("wrote", count)
