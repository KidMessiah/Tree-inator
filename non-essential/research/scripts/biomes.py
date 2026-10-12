"""More forest types for the structure and colour tuners, measured the same way as aspen, pine and Douglas fir: a NEON
canopy height model, the 10 cm camera mosaic, and the engine's own render compared window by window.
Import this before structure_lib or match_species are used: it adds the sites and species to them.

Each biome: site, year, tile, the tree type that grows as its main canopy, and what share of a window may be canopy."""
import glob, os, sys
import numpy as np
from PIL import Image
Image.MAX_IMAGE_PIXELS = None
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import structure_lib as sl
import class_colour_lib as cc

HERE = os.path.dirname(os.path.abspath(__file__))
NEON = os.path.join(HERE, "..", "neon")

# key: (site, year, preset, needles, min cover, max cover)
BIOMES = {
    "hardwood": ("HARV", 2022, "black_oak", False, 0.9, 1.01),
    "spruce": ("BART", 2022, "balsam_fir", True, 0.9, 1.01),
    "subalpine": ("NIWO", 2023, "balsam_fir", True, 0.45, 0.98),
    "oak": ("SJER", 2013, "cambridge_oak", False, 0.2, 0.7),
    "swamp": ("DELA", 2023, "black_tupelo", False, 0.9, 1.01),
    "rainforest": ("PUUM", 2019, "black_tupelo", False, 0.97, 1.01),
    # the fancier stands
    "autumn": ("GRSM", 2017, "black_oak", False, 0.9, 1.01),
    "boreal": ("DEJU", 2023, "balsam_fir", True, 0.3, 0.9),
    "pinyon": ("MOAB", 2019, "small_pine", True, 0.2, 0.7),
    "sierra": ("TEAK", 2023, "douglas_fir", True, 0.7, 0.99),
    "tropdry": ("GUAN", 2022, "black_tupelo", False, 0.95, 1.01),
    # NASA G-LiHT, El Yunque rainforest, Puerto Rico (March 2017)
    "jungle": ("PRYV", 2017, "black_tupelo", False, 0.7, 1.01),
}


# mean crown radius in metres each stand is held to (what the trees of that type really are), from field and lidar
# knowledge. Crown size is a fixed target in the tuning, not something it may trade away for a better score.
CROWN_R = {"aspen": 4.0, "pine": 5.0, "dfir": 4.5, "hardwood": 6.0, "spruce": 2.5, "subalpine": 1.8, "oak": 5.5, "swamp": 6.5, "rainforest": 4.5, "autumn": 6.0, "boreal": 1.8, "pinyon": 2.5, "sierra": 4.5, "tropdry": 3.5, "jungle": 5.0}


def register():
    for key, (site, year, preset, needles, lo, hi) in BIOMES.items():
        fs = glob.glob(os.path.join(NEON, site, f"{year}_*_CHM.tif"))
        if not fs:
            continue
        e, n = os.path.basename(fs[0]).split("_")[1:3]
        sl.SITES[key] = (site, year, int(e), int(n))
        sl.MIN_COVER[key] = lo
        sl.MAX_COVER[key] = hi
        m.SPECIES[key] = dict(preset=preset, needles=needles, scenes=[])


register()

_orig_window = cc.real_window_rgb


def real_window_rgb(key, y, x, out_px=256):
    """As the original, but for a camera mosaic that is not 10 cm (it is read as pixels per metre from its own size)."""
    t = cc.rgb_tile(key)
    ppm = t.size[0] / 1000.0
    rgb = np.asarray(t.crop((int(x * ppm), int(y * ppm), int((x + sl.WIN) * ppm), int((y + sl.WIN) * ppm))).resize((out_px, out_px), Image.LANCZOS)).astype(np.float64)
    chm = sl.chm_tile(key)[y : y + sl.WIN, x : x + sl.WIN]
    up = out_px // sl.WIN
    chm2 = np.kron(chm, np.ones((up, up)))
    import canopy_metrics as cm
    canopy = chm2 > 2.0
    ground = chm2 <= 0.5
    k = cm.blur(canopy.astype(float), 1.0)
    return rgb, canopy & (k > 0.9), ground & (k < 0.05)


cc.real_window_rgb = real_window_rgb


# windows of the G-LiHT strip must be where the photo exists and the sun was on the canopy (the strip has cloud shadows and edges)
def _bright_grid(key):
    site, year, e, n = sl.SITES[key]
    im = cc.rgb_tile(key).resize((1000, 1000), Image.BOX)
    a = np.asarray(im).astype(np.float32).mean(2)
    return a


_grids = {}


def window_ok(key, y, x):
    if key != "jungle":
        return True
    if key not in _grids:
        _grids[key] = _bright_grid(key)
    w = _grids[key][y : y + sl.WIN, x : x + sl.WIN]
    return bool((w > 5).all() and w.mean() >= 85)


sl.WINDOW_OK = window_ok
