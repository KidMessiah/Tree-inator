"""Aerial photo crops of Japan from the GSI (Geospatial Information Authority of Japan) seamless orthophoto tiles, about 0.5 m per
pixel. Free to use with the credit 'GSI Japan' (https://maps.gsi.go.jp/development/ichiran.html).
Usage: fetch_gsi.py name lat lon [size_m]   ->  research/satellite/jp/<name>.png"""
import io, math, os, sys
import requests
from PIL import Image

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "satellite", "jp")
URL = "https://cyberjapandata.gsi.go.jp/xyz/seamlessphoto/{z}/{x}/{y}.jpg"


def tile_of(lat, lon, z):
    n = 2 ** z
    x = (lon + 180) / 360 * n
    y = (1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n
    return x, y


def fetch(name, lat, lon, size_m=500, z=18):
    os.makedirs(OUT, exist_ok=True)
    mpp = 156543.03 * math.cos(math.radians(lat)) / 2 ** z
    half = size_m / 2 / mpp / 256
    x, y = tile_of(lat, lon, z)
    x0, x1, y0, y1 = int(x - half), int(x + half), int(y - half), int(y + half)
    img = Image.new("RGB", ((x1 - x0 + 1) * 256, (y1 - y0 + 1) * 256))
    ok = 0
    for tx in range(x0, x1 + 1):
        for ty in range(y0, y1 + 1):
            r = requests.get(URL.format(z=z, x=tx, y=ty), timeout=30, headers={"User-Agent": "tree-inator-research"})
            if r.status_code == 200:
                img.paste(Image.open(io.BytesIO(r.content)).convert("RGB"), ((tx - x0) * 256, (ty - y0) * 256))
                ok += 1
    if not ok:
        return None
    # crop to the requested square around the point
    cx, cy = (x - x0) * 256, (y - y0) * 256
    r = size_m / mpp / 2
    img = img.crop((int(cx - r), int(cy - r), int(cx + r), int(cy + r)))
    path = os.path.join(OUT, name + ".png")
    img.save(path)
    print(name, img.size, f"{mpp:.2f} m/px")
    return path


if __name__ == "__main__":
    fetch(sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4]) if len(sys.argv) > 4 else 500)
