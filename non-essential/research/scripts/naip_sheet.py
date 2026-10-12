"""Scores each saved NAIP scene for shadow contrast (dark tail against median brightness; lower means higher sun)
and writes contact sheets per site. Usage: naip_sheet.py prefix"""
import glob, json, os, sys
import numpy as np
from PIL import Image, ImageDraw

D = os.path.join(os.path.dirname(__file__), "..", "satellite")
prefix = sys.argv[1]
files = sorted(glob.glob(os.path.join(D, prefix + "*__*.png")))
meta_path = os.path.join(D, "sources.json")
meta = json.load(open(meta_path))
rows = []
for f in files:
    im = Image.open(f).convert("RGB")
    luma = np.asarray(im).astype(float).mean(axis=2)
    ratio = float(np.percentile(luma, 8) / max(np.median(luma), 1))
    tag = os.path.basename(f)[:-4]
    meta[tag]["contrast"] = round(ratio, 3)
    rows.append((tag, ratio, im))
json.dump(meta, open(meta_path, "w"), indent=1)
cols = 4
t = 200
n = len(rows)
sheet = Image.new("RGB", (cols * t, ((n + cols - 1) // cols) * (t + 14)), "white")
for i, (tag, ratio, im) in enumerate(rows):
    x, y = (i % cols) * t, (i // cols) * (t + 14)
    sheet.paste(im.resize((t, t)), (x, y + 14))
    ImageDraw.Draw(sheet).text((x + 2, y + 1), f"{tag.split('__')[0][-8:]} {tag.split('__')[1][2:]} c{ratio:.2f}", fill="black")
out = os.path.join(D, f"sheet_{prefix}.png")
sheet.save(out)
print(out, n)
