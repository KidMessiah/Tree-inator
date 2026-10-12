"""Try tree types from a trial table without rebuilding: single trees, and a forest of each, in one sheet.
Usage: python lab_sheet.py OUT.png TABLE.json name:RRGGBB[:height[:barkRRGGBB]] [name:...]   (TABLE.json is read through TREE_INATOR_EXTRAS)"""
import json, math, os, subprocess, sys
from PIL import Image, ImageDraw
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, ".."))
CLI = os.environ.get("CLI") or os.path.join(ROOT, "target", "release", "forest-cli.exe")
TMP = os.environ.get("TEMP", ".")


def ramp(rgb):
    return [[c / 255.0 * m for c in rgb] for m in (0.30, 0.55, 0.85, 1.2)]


def run(proj, env, args=()):
    path, png = os.path.join(TMP, "lab.json"), os.path.join(TMP, "lab.png")
    json.dump(proj, open(path, "w"))
    e = dict(os.environ); e.update(env)
    o = subprocess.run([CLI, path, "-bg", "-out", png, *args], capture_output=True, text=True, env=e)
    if o.returncode:
        print(o.stderr[-300:]); return None
    return Image.open(png).convert("RGB")


def main():
    out, table, specs = sys.argv[1], sys.argv[2], sys.argv[3:]
    env = {"TREE_INATOR_EXTRAS": table} if table != "-" else {}
    pts = [[600 + 430 * math.cos(t / 40 * 6.283) * (1 + 0.15 * math.sin(3 * t / 40 * 6.283)), 450 + 300 * math.sin(t / 40 * 6.283)] for t in range(40)]
    rows = []
    for sp in specs:
        p = sp.split(":")
        name, col = p[0], tuple(int(p[1][i:i + 2], 16) for i in (0, 2, 4))
        h = float(p[2]) if len(p) > 2 else 1.0
        s = dict(name=name, procedural=True, preset=name, ramp=ramp(col), height=h, bark=([int(p[3][i:i + 2], 16) / 255.0 for i in (0, 2, 4)] if len(p) > 3 else [0.38, 0.30, 0.22]))
        trees = [dict(x=150 + k * 300, y=150, r=110, h=(110 * 1.3 + 18) * h, tone=0.0, layer="Canopy", species=0, seed=17 + k * 5) for k in range(2)]
        a = run(dict(map_w=600, map_h=300, species=[s], painted=trees, gen=dict(procedural_trees=False, painted_trees=True, seed=3)), env)
        b = run(dict(map_w=1200, map_h=900, species=[s], shapes=[dict(op="Add", points=pts)], gen=dict(seed=9, main_species=0, shrub_species=0, mix_amount=0.0, canopy_cover=float(os.environ.get("COVER", "0.6")), lone_groups=0.0)), env)
        if a is None or b is None:
            continue
        b = b.crop((300, 250, 900, 560))
        row = Image.new("RGB", (1200, 310), (214, 202, 176)); row.paste(a, (0, 0)); row.paste(b, (600, 0))
        d = ImageDraw.Draw(row); d.rectangle((4, 4, 12 + 7 * len(name), 20), fill=(255, 255, 255)); d.text((8, 6), name, fill=(0, 0, 0))
        rows.append(row)
    sheet = Image.new("RGB", (1200, 310 * len(rows)))
    for i, r in enumerate(rows):
        sheet.paste(r, (0, i * 310))
    sheet.save(out); print(out, sheet.size)


main()
