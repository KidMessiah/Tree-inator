"""Download the 1 km hyperspectral reflectance tile of each biome from NEON. The token comes from the NEON_TOKEN
environment variable and is not stored anywhere.
Usage: neon_download_biomes.py [key ...]"""
import os, sys
from neonutilities import by_tile_aop

TILES = {  # key: (site, year, easting, northing)
    "rainforest": ("PUUM", 2019, 255000, 2165000),
    "subalpine": ("NIWO", 2023, 453000, 4436000),
    "hardwood": ("HARV", 2022, 737000, 4709000),
    "spruce": ("BART", 2022, 320000, 4879000),
    "swamp": ("DELA", 2023, 417000, 3606000),
    "oak": ("SJER", 2013, 255000, 4112000),
    "autumn": ("GRSM", 2017, 278000, 3948000),
    "boreal": ("DEJU", 2023, 559000, 7085000),
    "pinyon": ("MOAB", 2019, 644000, 4240000),
    "sierra": ("TEAK", 2023, 314000, 4091000),
    "tropdry": ("GUAN", 2022, 726000, 1989000),
}
for key in sys.argv[1:] or TILES:
    site, year, e, n = TILES[key]
    # the older flights are in product .001, the newer ones (provisional) in .002
    for dpid in (os.environ.get("DPID_FIRST", "DP3.30006.002"), "DP3.30006.001"):
        try:
            by_tile_aop(dpid=dpid, site=site, year=year, easting=e, northing=n, savepath=f"D:/neon_dl/{site}", token=os.environ["NEON_TOKEN"], include_provisional=True, check_size=False)
            print("done", key, dpid, flush=True)
            break
        except Exception as ex:
            print("failed", key, dpid, str(ex)[:200], flush=True)
