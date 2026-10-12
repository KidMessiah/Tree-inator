"""Download the 10 cm camera mosaic for tiles already fetched by neon_fetch.py. Usage: neon_rgb.py SITE ..."""
import glob, json, os, sys
import requests
sys.path.insert(0, os.path.dirname(__file__))
import neon_fetch as nf

for site in sys.argv[1:]:
    for jp in glob.glob(os.path.join(nf.OUT, site, "*.json")):
        m = json.load(open(jp))
        if "e" not in m:
            continue
        info = requests.get(f"{nf.API}/sites/{site}", timeout=60).json()["data"]
        dom = info["domainCode"]
        y, e, n, v = m["year"], m["e"], m["n"], m["visit"]
        u = f"{y}/FullSite/{dom}/{y}_{site}_{v}/L3/Camera/Mosaic/{y}_{site}_{v}_{e}_{n}_image.tif"
        rp = os.path.join(nf.OUT, site, f"{y}_{e}_{n}_image.tif")
        if os.path.exists(rp) and os.path.getsize(rp) > 100_000_000:
            print("have", rp)
            continue
        with requests.get(nf.BASE + u, stream=True, timeout=900) as r:
            r.raise_for_status()
            with open(rp, "wb") as f:
                for chunk in r.iter_content(1 << 20):
                    f.write(chunk)
        print("saved", rp, os.path.getsize(rp) // 1_000_000, "MB", flush=True)
