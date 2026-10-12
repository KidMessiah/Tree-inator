"""The same forest rendered with different settings, side by side with labels.
Usage: python variants_sheet.py OUT.png THEME SCALE 'label={"gen":{...}}' ['label2={...}' ...]
Each variant is a JSON patch merged into the base project (tools/sample_projects/sheet_project.json)."""
import copy, json, os, subprocess, sys
from PIL import Image, ImageDraw
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, ".."))
CLI = os.environ.get("CLI") or os.path.join(ROOT, "target", "release", "forest-cli.exe")
BASE = os.environ.get("BASE") or os.path.join(HERE, "sample_projects", "sheet_project.json")
TMP = os.environ.get("TEMP", ".")
CROP = os.environ.get("CROP")  # x0,y0,x1,y1 as fractions


def merge(a, b):
    for k, v in b.items():
        if isinstance(v, dict) and isinstance(a.get(k), dict):
            merge(a[k], v)
        else:
            a[k] = v


def main():
    out, theme, scale, variants = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
    base = json.load(open(BASE))
    tiles = []
    for v in variants:
        label, patch = v.split("=", 1)
        p = copy.deepcopy(base)
        path, png = os.path.join(TMP, "var.json"), os.path.join(TMP, "var.png")
        extra = []
        if os.environ.get("AFTER"):
            # the patch is applied after the theme, so it wins over the theme's own settings
            pf = os.path.join(TMP, "var_patch.json")
            json.dump(json.loads(patch), open(pf, "w"))
            extra = ["-over", pf]
        else:
            merge(p, json.loads(patch))
        json.dump(p, open(path, "w"))
        args = [CLI, path, "-bg", "-scale", scale, "-out", png] + (["-theme", theme] if theme != "-" else []) + extra
        o = subprocess.run(args, capture_output=True, text=True)
        if o.returncode:
            print(label, o.stderr[-300:]); continue
        im = Image.open(png).convert("RGB")
        if CROP:
            x0, y0, x1, y1 = [float(c) for c in CROP.split(",")]
            w, h = im.size
            im = im.crop((int(x0 * w), int(y0 * h), int(x1 * w), int(y1 * h)))
        d = ImageDraw.Draw(im); d.rectangle((0, 0, 10 + 7 * len(label), 16), fill=(255, 255, 255)); d.text((4, 2), label, fill=(0, 0, 0))
        tiles.append(im)
    w, h = tiles[0].size
    sheet = Image.new("RGB", (w * len(tiles), h))
    for i, t in enumerate(tiles):
        sheet.paste(t, (i * w, 0))
    sheet.save(out); print(out, sheet.size)


main()
