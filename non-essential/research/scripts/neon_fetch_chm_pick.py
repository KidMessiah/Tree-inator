"""For sites outside the LANDFIRE map (Puerto Rico, Hawaii): look at the canopy height model of the 1 km tiles around the
site and keep the tallest, densest one, with its camera mosaic.
Usage: neon_fetch_chm_pick.py SITE YEAR_MONTH [radius_km]"""
import io, json, os, sys
import numpy as np
import requests
from PIL import Image
sys.path.insert(0, os.path.dirname(__file__))
import neon_fetch as nf

site, ym = sys.argv[1], sys.argv[2]
rad = int(sys.argv[3]) if len(sys.argv) > 3 else 3
info = requests.get(f"{nf.API}/sites/{site}", timeout=60).json()["data"]
dom = info["domainCode"]
zone, E, N = nf.to_utm(info["siteLatitude"], info["siteLongitude"])
e0, n0 = int(E // 1000 * 1000), int(N // 1000 * 1000)
year = ym[:4]
best = None
for de in range(-rad, rad + 1):
    for dn in range(-rad, rad + 1):
        e, n = e0 + de * 1000, n0 + dn * 1000
        v = nf.find_release(site, dom, ym, e, n)
        if not v:
            continue
        u = f"{year}/FullSite/{dom}/{year}_{site}_{v}/L3/DiscreteLidar/CanopyHeightModelGtif/NEON_{dom}_{site}_DP3_{e}_{n}_CHM.tif"
        a = np.asarray(Image.open(io.BytesIO(requests.get(nf.BASE + u, timeout=300).content))).astype(np.float32)
        a[a < -50] = 0
        cover = float((a > 2).mean())
        med = float(np.median(a[a > 2])) if (a > 2).any() else 0
        score = cover * min(med, 40)
        print(e, n, "cover %.2f median %.1f" % (cover, med), flush=True)
        if best is None or score > best[0]:
            best = (score, e, n, v, cover, med, u)
if not best:
    sys.exit("no tile")
_, e, n, v, cover, med, chm_u = best
print("chosen", e, n, v, cover, med)
os.makedirs(os.path.join(nf.OUT, site), exist_ok=True)
open(os.path.join(nf.OUT, site, f"{year}_{e}_{n}_CHM.tif"), "wb").write(requests.get(nf.BASE + chm_u, timeout=300).content)
rgb_u = f"{year}/FullSite/{dom}/{year}_{site}_{v}/L3/Camera/Mosaic/{year}_{site}_{v}_{e}_{n}_image.tif"
rp = os.path.join(nf.OUT, site, f"{year}_{e}_{n}_image.tif")
try:
    with requests.get(nf.BASE + rgb_u, stream=True, timeout=600) as r:
        r.raise_for_status()
        with open(rp, "wb") as f:
            for chunk in r.iter_content(1 << 20):
                f.write(chunk)
except Exception as ex:
    print("no camera mosaic:", str(ex)[:80])
json.dump({"site": site, "year": year, "e": e, "n": n, "zone": zone, "visit": v, "keyword": "picked by canopy", "cover": cover, "median": med}, open(os.path.join(nf.OUT, site, f"{year}_{e}_{n}.json"), "w"), indent=1)
print("saved", rp)
