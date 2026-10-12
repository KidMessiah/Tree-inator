"""Fetches small NAIP aerial crops (about 0.6 m per pixel, public domain) from the Microsoft Planetary Computer.
Usage: fetch_naip.py name lat lon [size_m] [year_min]
Saves research/satellite/<name>.png and appends the source to research/satellite/sources.json."""
import json, math, os, sys
import requests

OUT = os.path.join(os.path.dirname(__file__), "..", "satellite")
STAC = "https://planetarycomputer.microsoft.com/api/stac/v1/search"
DATA = "https://planetarycomputer.microsoft.com/api/data/v1/item/bbox"


def fetch(name, lat, lon, size_m=360, year_min=2015):
    os.makedirs(OUT, exist_ok=True)
    body = {"collections": ["naip"], "intersects": {"type": "Point", "coordinates": [lon, lat]}, "limit": 20}
    items = requests.post(STAC, json=body, timeout=60).json().get("features", [])
    items = [i for i in items if int(i["properties"]["datetime"][:4]) >= year_min]
    if not items:
        return None
    # leaf-on summer scenes first, then the newest
    items.sort(key=lambda i: (6 <= int(i["properties"]["datetime"][5:7]) <= 8, i["properties"]["datetime"]), reverse=True)
    it = items[0]
    dlat = size_m / 2 / 111320.0
    dlon = size_m / 2 / (111320.0 * math.cos(math.radians(lat)))
    px = int(size_m / 0.6)
    bbox = f"{lon - dlon},{lat - dlat},{lon + dlon},{lat + dlat}"
    url = f"{DATA}/{bbox}/{px}x{px}.png"
    r = requests.get(url, params={"collection": "naip", "item": it["id"], "assets": "image", "asset_bidx": "image|1,2,3"}, timeout=120)
    if r.status_code != 200:
        print("failed", r.status_code, r.text[:200])
        return None
    path = os.path.join(OUT, name + ".png")
    open(path, "wb").write(r.content)
    meta_path = os.path.join(OUT, "sources.json")
    meta = json.load(open(meta_path)) if os.path.exists(meta_path) else {}
    meta[name] = {"lat": lat, "lon": lon, "size_m": size_m, "item": it["id"], "date": it["properties"]["datetime"][:10], "px": px}
    json.dump(meta, open(meta_path, "w"), indent=1)
    print(name, it["id"], it["properties"]["datetime"][:10], len(r.content) // 1024, "KB")
    return path


def fetch_all(name, lat, lon, size_m=360, year_min=2012):
    """Every leaf-on scene at a point, each saved with its shadow share (dark pixels), so high-sun scenes can be picked."""
    import io
    import numpy as np
    from PIL import Image
    os.makedirs(OUT, exist_ok=True)
    body = {"collections": ["naip"], "intersects": {"type": "Point", "coordinates": [lon, lat]}, "limit": 30}
    items = requests.post(STAC, json=body, timeout=60).json().get("features", [])
    items = [i for i in items if int(i["properties"]["datetime"][:4]) >= year_min and 5 <= int(i["properties"]["datetime"][5:7]) <= 9]
    dlat = size_m / 2 / 111320.0
    dlon = size_m / 2 / (111320.0 * math.cos(math.radians(lat)))
    px = int(size_m / 0.6)
    bbox = f"{lon - dlon},{lat - dlat},{lon + dlon},{lat + dlat}"
    meta_path = os.path.join(OUT, "sources.json")
    meta = json.load(open(meta_path)) if os.path.exists(meta_path) else {}
    seen = set()
    for it in items:
        date = it["properties"]["datetime"][:10]
        if date in seen:
            continue
        seen.add(date)
        r = requests.get(f"{DATA}/{bbox}/{px}x{px}.png", params={"collection": "naip", "item": it["id"], "assets": "image", "asset_bidx": "image|1,2,3"}, timeout=120)
        if r.status_code != 200:
            continue
        im = np.asarray(Image.open(io.BytesIO(r.content)).convert("RGB")).astype(float)
        luma = im.mean(axis=2)
        if (luma < 3).mean() > 0.2:
            continue  # off the edge of the scene
        dark = float((luma < 0.45 * np.median(luma)).mean())
        tag = f"{name}__{date}"
        open(os.path.join(OUT, tag + ".png"), "wb").write(r.content)
        meta[tag] = {"lat": lat, "lon": lon, "size_m": size_m, "item": it["id"], "date": date, "px": px, "shadow": round(dark, 3)}
        print(tag, f"shadow {dark:.3f}", len(r.content) // 1024, "KB")
    json.dump(meta, open(meta_path, "w"), indent=1)


if __name__ == "__main__":
    a = sys.argv[1:]
    (fetch_all if os.environ.get('ALL') else fetch)(a[0], float(a[1]), float(a[2]), float(a[3]) if len(a) > 3 else 360)
