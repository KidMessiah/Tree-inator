# Renders the brush, species and fallback sections of the tour. Strokes go through the real brush code (forest-cli -strokes).
import json, math, os, subprocess, random
from PIL import Image

ROOT = os.path.dirname(os.path.abspath(__file__))
CLI = os.path.join(ROOT, "..", "target", "release", "forest-cli.exe")
J, I = os.path.join(ROOT, "json"), os.path.join(ROOT, "img")
BASE = json.load(open(os.path.join(J, "edge_realistic.json")))  # same forest as the rest of the tour


def brush(**kw):
    b = dict(mode="Paint", layer="Canopy", species=0, radius=60, size_min=10, size_max=17, density=0.7,
             tone_jitter=0.1, erase_generated=True, erase_layer_only=False)
    b.update(kw)
    return b


def curve(pts, n=40):
    """Smooth path through control points (Catmull-Rom), like a mouse stroke."""
    out = []
    P = [pts[0]] + pts + [pts[-1]]
    for i in range(1, len(P) - 2):
        p0, p1, p2, p3 = P[i - 1], P[i], P[i + 1], P[i + 2]
        for k in range(n):
            t = k / n
            out.append([0.5 * ((2 * p1[j]) + (-p0[j] + p2[j]) * t + (2 * p0[j] - 5 * p1[j] + 4 * p2[j] - p3[j]) * t * t
                               + (-p0[j] + 3 * p1[j] - 3 * p2[j] + p3[j]) * t ** 3) for j in range(2)])
    out.append(pts[-1])
    return out


def render(name, project, strokes=None, scale=0.4, crop=None):
    jp = os.path.join(J, name + ".json")
    json.dump(project, open(jp, "w"))
    out = os.path.join(J, name + ".png")
    args = [CLI, jp, "-bg", "-scale", str(scale), "-out", out]
    if strokes:
        sp = os.path.join(J, name + "_strokes.json")
        json.dump(strokes, open(sp, "w"))
        args += ["-strokes", sp]
    r = subprocess.run(args, check=True, capture_output=True, text=True, cwd=os.path.join(ROOT, ".."))
    im = Image.open(out)
    if crop:
        w, h = im.size
        im = im.crop((int(w * crop[0]), int(h * crop[1]), int(w * crop[2]), int(h * crop[3])))
    im.convert("RGB").save(os.path.join(I, name + ".jpg"), quality=84, optimize=True)
    print(name, [l for l in r.stdout.splitlines() if "strokes" in l or "generate" in l])


# 1. Species line-up: one specimen and one painted grove per species.
species = ["Broadleaf", "Conifer", "Sage", "Autumn", "Golden", "Dead", "Bush"]
strokes = []
for i in range(len(species)):
    cx = 90 + i * 150
    strokes.append({"brush": brush(mode="Single", species=i, size_min=34, size_max=34), "points": [[cx, 85]]})
    strokes.append({"brush": brush(species=i, radius=48, density=0.6, size_min=9, size_max=15), "points": [[cx, 230]]})
render("sp_lineup", {"map_w": 1060, "map_h": 310, "shapes": []}, strokes, scale=1.0)

# 2. Painting on an empty map: strokes of different species and layers.
strokes = [
    {"brush": brush(species=1, radius=55, density=0.75), "points": curve([[200, 300], [420, 200], [650, 330], [880, 220], [1050, 320]])},
    {"brush": brush(species=0, radius=80, density=0.85), "points": curve([[260, 700], [480, 620], [700, 760], [900, 640]])},
    {"brush": brush(species=3, radius=45, density=0.6), "points": curve([[1200, 520], [1400, 640], [1600, 560], [1800, 700]])},
    {"brush": brush(species=6, layer="Shrub", radius=70, density=0.5, size_min=4, size_max=9), "points": curve([[300, 1000], [700, 1100], [1100, 980], [1500, 1100]])},
    {"brush": brush(mode="Single", species=0, layer="Lone", size_min=12, size_max=16),
     "points": [[1300, 250], [1420, 300], [1550, 230], [1700, 330], [1850, 260]]},
    {"brush": brush(species=5, radius=40, density=0.4, size_min=14, size_max=20), "points": curve([[1450, 1250], [1700, 1300], [1900, 1220]])},
]
render("brush_strokes", {"map_w": 2048, "map_h": 1536, "shapes": []}, strokes)

# 3. Editing a generated forest: conifer stand painted in, a trail erased, autumn trees at the edge.
edit = [
    {"brush": brush(species=1, radius=70, density=0.8), "points": curve([[780, 520], [900, 600], [860, 760], [740, 700]])},
    {"brush": brush(mode="Erase", radius=38), "points": curve([[300, 1000], [700, 900], [1000, 820], [1300, 900], [1750, 800]])},
    {"brush": brush(species=3, radius=60, density=0.7), "points": curve([[1250, 420], [1380, 520], [1330, 640]])},
]
render("edit_before", BASE)
render("edit_after", BASE, edit)

# Close-ups at full resolution: painted conifer stand next to the erased trail.
render("edit_after_crop", BASE, edit, scale=1.0, crop=(0.3, 0.28, 0.62, 0.62))

# 4. Fallback switches on the edited forest.
for n, (gen_on, paint_on) in {"fb_generated": (True, False), "fb_painted": (False, True), "fb_both": (True, True)}.items():
    p = json.loads(json.dumps(BASE))
    p["gen"].update(procedural_trees=gen_on, painted_trees=paint_on)
    render(n, p, edit)

# 5. Species mix inside generated canopy.
for n, g in {"mix_none": {}, "mix_sage": dict(mix_species=2, mix_amount=0.5),
             "mix_autumn": dict(mix_species=3, mix_amount=0.45, mix_scale=300),
             "mix_conifer": dict(mix_species=1, mix_amount=0.55, mix_scale=160)}.items():
    p = json.loads(json.dumps(BASE))
    p["gen"].update(g)
    render(n, p)
p = json.loads(json.dumps(BASE))
p["gen"].update(mix_species=1, mix_amount=0.55, mix_scale=160)
render("mix_conifer_crop", p, scale=1.0, crop=(0.35, 0.3, 0.65, 0.6))

# 6. Leaf texture prototype sheet, copied from research.
src = os.path.join(ROOT, "..", "research", "results", "08_leaf_texture.png")
im = Image.open(src).convert("RGB")
w, h = im.size
for i, n in enumerate(["leaf_broadleaf", "leaf_conifer", "leaf_sage", "leaf_autumn", "leaf_dead"]):
    tile = im.crop(((i % 3) * w // 3, (i // 3) * h // 2, (i % 3 + 1) * w // 3, (i // 3 + 1) * h // 2))
    tile.save(os.path.join(I, n + ".jpg"), quality=88)
# CG2A bank at the same zoom, for comparison.
render("leaf_cg2a", {"map_w": 520, "map_h": 300, "shapes": [],
                     "gen": {"seed": 5}},
       [{"brush": brush(mode="Single", species=0, size_min=s, size_max=s), "points": [[x, y]]}
        for x, y, s in [(130, 150, 70), (230, 110, 42), (250, 200, 40), (330, 160, 50), (400, 100, 34), (420, 210, 44)]], scale=1.0)
