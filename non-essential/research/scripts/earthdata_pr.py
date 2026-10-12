"""Pick overlapping G-LiHT canopy height and aerial photo granules over the El Yunque rainforest, Puerto Rico, from NASA's catalogue
(public), and optionally download them. The token comes from EARTHDATA_TOKEN and is not stored.
Usage: earthdata_pr.py list | get"""
import json, os, sys
import requests
CMR = "https://cmr.earthdata.nasa.gov/search/granules.json"
BB = "-65.85,18.28,-65.75,18.34"


def granules(sn):
    r = requests.get(CMR, params={"short_name": sn, "bounding_box": BB, "page_size": 200}, timeout=60).json()["feed"]["entry"]
    out = []
    for g in r:
        box = None
        if g.get("polygons"):
            v = [float(x) for x in g["polygons"][0][0].split()]
            lats, lons = v[0::2], v[1::2]
            box = [min(lats), min(lons), max(lats), max(lons)]
        url = next((l["href"] for l in g["links"] if l["href"].startswith("https") and l["href"].endswith(".tif")), None)
        out.append(dict(title=g["title"], box=box, url=url, size=float(g.get("granule_size", 0))))
    return out


def overlap(a, b):
    # boxes are S W N E
    s, w, n, e = a
    s2, w2, n2, e2 = b
    return max(0, min(n, n2) - max(s, s2)) * max(0, min(e, e2) - max(w, w2))


if __name__ == "__main__":
    chm, ortho = granules("GLCHMT"), granules("GLORTHO")
    best = None
    for c in chm:
        if not c["box"] or not c["url"]:
            continue
        for o in ortho:
            if o["box"] and o["url"] and c["title"].split("_")[2] == o["title"].split("_")[2]:
                ov = overlap(c["box"], o["box"])
                if best is None or ov > best[0]:
                    best = (ov, c, o)
    print(best[1]["title"], best[1]["size"], "MB |", best[2]["title"], best[2]["size"], "MB | overlap", round(best[0], 6))
    json.dump(best[1:], open(os.path.join(os.path.dirname(__file__), "..", "neon", "pr_pick.json"), "w"), indent=1)
    if sys.argv[1:] == ["get"]:
        tok = os.environ["EARTHDATA_TOKEN"]
        os.makedirs(os.path.join(os.path.dirname(__file__), "..", "gliht"), exist_ok=True)
        for g in best[1:]:
            path = os.path.join(os.path.dirname(__file__), "..", "gliht", os.path.basename(g["url"]))
            with requests.get(g["url"], headers={"Authorization": "Bearer " + tok}, stream=True, timeout=600) as r:
                print(r.status_code, g["title"])
                r.raise_for_status()
                with open(path, "wb") as f:
                    for ch in r.iter_content(1 << 20):
                        f.write(ch)
            print("saved", path, os.path.getsize(path) // 1_000_000, "MB")
