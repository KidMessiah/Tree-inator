"""NEON airborne data for forest patches: a 1 km canopy height model (1 m) and the matching 10 cm camera mosaic,
read straight from NEON's public AOP bucket (no account needed for the objects themselves).

Usage: neon_fetch.py SITE YEAR_MONTH keyword [--rgb]
  Finds the 1 km tile near the site whose LANDFIRE class group contains `keyword` the most, then saves the
  canopy height model (and with --rgb the 339 MB camera mosaic) under research/neon/SITE/."""
import io, json, math, os, sys
import numpy as np
import requests
from PIL import Image

HERE = os.path.dirname(__file__)
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, ".."))
OUT = os.path.join(HERE, "..", "neon")
API = "https://data.neonscience.org/api/v0"
BASE = "https://storage.googleapis.com/neon-aop-products/"
LF = "https://lfps.usgs.gov/arcgis/rest/services/Landfire_LF2023/LF2023_EVT_CONUS/ImageServer"


def to_utm(lat, lon):
    a = 6378137.0
    f = 1 / 298.257223563
    k0 = 0.9996
    e2 = f * (2 - f)
    ep2 = e2 / (1 - e2)
    zone = int((lon + 180) // 6) + 1
    lon0 = math.radians((zone - 1) * 6 - 180 + 3)
    phi, lam = math.radians(lat), math.radians(lon)
    N = a / math.sqrt(1 - e2 * math.sin(phi) ** 2)
    T, C, A = math.tan(phi) ** 2, ep2 * math.cos(phi) ** 2, math.cos(phi) * (lam - lon0)
    M = a * ((1 - e2 / 4 - 3 * e2**2 / 64 - 5 * e2**3 / 256) * phi - (3 * e2 / 8 + 3 * e2**2 / 32 + 45 * e2**3 / 1024) * math.sin(2 * phi) + (15 * e2**2 / 256 + 45 * e2**3 / 1024) * math.sin(4 * phi) - (35 * e2**3 / 3072) * math.sin(6 * phi))
    E = 500000 + k0 * N * (A + (1 - T + C) * A**3 / 6 + (5 - 18 * T + T**2 + 72 * C - 58 * ep2) * A**5 / 120)
    Nn = k0 * (M + N * math.tan(phi) * (A**2 / 2 + (5 - T + 9 * C + 4 * C**2) * A**4 / 24 + (61 - 58 * T + T**2 + 600 * C - 330 * ep2) * A**6 / 720))
    return zone, E, Nn


def legend_colours(keyword):
    import base64
    L = requests.get(LF + "/legend?f=json", timeout=60).json()["layers"][0]["legend"]
    groups = {}
    for l in L:
        im = Image.open(io.BytesIO(base64.b64decode(l["imageData"]))).convert("RGBA")
        c = tuple(int(v) for v in np.asarray(im)[10, 10][:3])
        groups.setdefault(c, []).append(l["label"])
    return {c for c, names in groups.items() if any(keyword.lower() in n.lower() for n in names)}


def tile_scores(zone, e0, n0, keyword, span_km=7):
    """LANDFIRE share of the keyword class in every 1 km tile around (e0, n0)."""
    cols = legend_colours(keyword)
    x0, y0 = e0 - span_km * 1000, n0 - span_km * 1000
    size = 2 * span_km * 1000
    px = size // 30
    r = requests.get(LF + "/exportImage", params={"bbox": f"{x0},{y0},{x0 + size},{y0 + size}", "bboxSR": 32600 + zone, "imageSR": 32600 + zone, "size": f"{px},{px}", "format": "png", "interpolation": "RSP_NearestNeighbor", "f": "image"}, timeout=120)
    im = np.asarray(Image.open(io.BytesIO(r.content)).convert("RGBA"))
    mask = np.zeros(im.shape[:2], bool)
    for c in cols:
        mask |= (im[..., 0] == c[0]) & (im[..., 1] == c[1]) & (im[..., 2] == c[2]) & (im[..., 3] > 0)
    t = 1000 // 30
    out = []
    for j in range(2 * span_km):
        for i in range(2 * span_km):
            sub = mask[(2 * span_km - 1 - j) * t : (2 * span_km - j) * t, i * t : (i + 1) * t]
            out.append((float(sub.mean()), x0 + i * 1000, y0 + j * 1000))
    return sorted(out, reverse=True)


def find_release(site, domain, year_month, e, n):
    year = year_month[:4]
    for v in range(1, 10):
        u = f"{year}/FullSite/{domain}/{year}_{site}_{v}/L3/DiscreteLidar/CanopyHeightModelGtif/NEON_{domain}_{site}_DP3_{e}_{n}_CHM.tif"
        h = requests.head(BASE + u, timeout=30)
        if h.status_code == 200:
            return v
    return None


def main():
    site, ym, keyword = sys.argv[1], sys.argv[2], sys.argv[3]
    want_rgb = "--rgb" in sys.argv
    info = requests.get(f"{API}/sites/{site}", timeout=60).json()["data"]
    dom = info["domainCode"]
    zone, E, N = to_utm(info["siteLatitude"], info["siteLongitude"])
    year = ym[:4]
    scores = tile_scores(zone, int(E // 1000 * 1000), int(N // 1000 * 1000), keyword)
    os.makedirs(os.path.join(OUT, site), exist_ok=True)
    chosen = None
    for share, e, n in scores[:25]:
        v = find_release(site, dom, ym, e, n)
        if v:
            chosen = (share, e, n, v)
            break
    if not chosen:
        print("no tile with data")
        return
    share, e, n, v = chosen
    print(f"{site}: tile {e} {n}, {share:.0%} of it is '{keyword}', visit {v}")
    chm_u = f"{year}/FullSite/{dom}/{year}_{site}_{v}/L3/DiscreteLidar/CanopyHeightModelGtif/NEON_{dom}_{site}_DP3_{e}_{n}_CHM.tif"
    path = os.path.join(OUT, site, f"{year}_{e}_{n}_CHM.tif")
    open(path, "wb").write(requests.get(BASE + chm_u, timeout=300).content)
    meta = {"site": site, "year": year, "e": e, "n": n, "zone": zone, "visit": v, "keyword": keyword, "landfire_share": share}
    if want_rgb:
        rgb_u = f"{year}/FullSite/{dom}/{year}_{site}_{v}/L3/Camera/Mosaic/{year}_{site}_{v}_{e}_{n}_image.tif"
        rp = os.path.join(OUT, site, f"{year}_{e}_{n}_image.tif")
        with requests.get(BASE + rgb_u, stream=True, timeout=600) as r:
            r.raise_for_status()
            with open(rp, "wb") as f:
                for chunk in r.iter_content(1 << 20):
                    f.write(chunk)
        print("saved", rp, os.path.getsize(rp) // 1_000_000, "MB")
    json.dump(meta, open(os.path.join(OUT, site, f"{year}_{e}_{n}.json"), "w"), indent=1)
    print("saved", path)


if __name__ == "__main__":
    main()
