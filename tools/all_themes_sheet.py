"""Every theme on the same forest, in one labelled sheet.
Usage: python all_themes_sheet.py OUT.png [project.json] [scale] [theme ...]"""
import json, os, re, subprocess, sys
from PIL import Image, ImageDraw
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, ".."))
CLI = os.environ.get("CLI") or os.path.join(ROOT, "target", "release", "forest-cli.exe")
TMP = os.environ.get("TEMP", ".")
out = sys.argv[1]
proj = sys.argv[2] if len(sys.argv) > 2 else os.path.join(HERE, "sample_projects", "sheet_project.json")
scale = sys.argv[3] if len(sys.argv) > 3 else "0.3"
names = sys.argv[4:]
if not names:
    src = open(os.path.join(ROOT, "crates", "forest-core", "src", "params.rs"), encoding="utf-8").read()
    names = re.findall(r'^        Theme \{\n            name: "([^"]+)"', src, re.M)
    names += list(json.load(open(os.path.join(ROOT, "crates", "forest-core", "assets", "satellite_themes.json"))).keys())
tiles = []
for n in names:
    png = os.path.join(TMP, "ats.png")
    o = subprocess.run([CLI, proj, "-bg", "-theme", n, "-scale", scale, "-out", png], capture_output=True, text=True)
    if o.returncode:
        print(n, o.stderr[-200:]); continue
    im = Image.open(png).convert("RGB")
    d = ImageDraw.Draw(im); d.rectangle((0, 0, 8 + 7 * len(n), 16), fill=(255, 255, 255)); d.text((4, 2), n, fill=(0, 0, 0))
    tiles.append(im)
w, h = tiles[0].size
cols = 4
rows = (len(tiles) + cols - 1) // cols
sheet = Image.new("RGB", (cols * w, rows * h), (30, 30, 30))
for i, t in enumerate(tiles):
    sheet.paste(t, ((i % cols) * w, (i // cols) * h))
sheet.save(out)
print(out, sheet.size, len(tiles))
