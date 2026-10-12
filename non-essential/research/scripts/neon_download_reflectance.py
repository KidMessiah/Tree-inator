"""Download one 1 km hyperspectral reflectance tile from NEON for a site. Needs: pip install neonutilities
Usage: python neon_download_reflectance.py            (Douglas fir, WREF)
       python neon_download_reflectance.py STEI       (aspen)
       python neon_download_reflectance.py OSBS       (pine)"""
import sys
from neonutilities import by_tile_aop

TILES = {
    "WREF": (2023, 578000, 5072000),
    "STEI": (2022, 296000, 5044000),
    "OSBS": (2023, 409000, 3281000),
}
site = sys.argv[1] if len(sys.argv) > 1 else "WREF"
year, e, n = TILES[site]
by_tile_aop(dpid="DP3.30006.001", site=site, year=year, easting=e, northing=n, savepath=f"D:/neon_dl/{site}")
print("done, files are under D:/neon_dl/" + site)
