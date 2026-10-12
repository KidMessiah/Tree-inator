"""Colour statistics of canopy pixels and ground pixels separately, using a lidar canopy height model to decide
which pixel is which, on the real 10 cm camera mosaic and on engine renders."""
import os, sys
import numpy as np
from PIL import Image
Image.MAX_IMAGE_PIXELS = None
sys.path.insert(0, os.path.dirname(__file__))
import canopy_metrics as cm
import structure_lib as sl

_rgb_cache = {}


def rgb_tile(key):
    if key not in _rgb_cache:
        site, y, e, n = sl.SITES[key]
        _rgb_cache[key] = Image.open(os.path.join(sl.NEON, site, f"{y}_{e}_{n}_image.tif")).convert("RGB")
    return _rgb_cache[key]


def real_window_rgb(key, y, x, out_px=256):
    """The 128 m window at row y, column x (1 m units) as RGB at 0.5 m, with canopy and ground masks at the same size."""
    t = rgb_tile(key)
    rgb = np.asarray(t.crop((x * 10, y * 10, (x + sl.WIN) * 10, (y + sl.WIN) * 10)).resize((out_px, out_px), Image.LANCZOS)).astype(np.float64)
    chm = sl.chm_tile(key)[y : y + sl.WIN, x : x + sl.WIN]
    up = out_px // sl.WIN
    chm2 = np.kron(chm, np.ones((up, up)))
    canopy = chm2 > 2.0
    ground = chm2 <= 0.5
    # leave a one pixel border out, where the two blend
    k = cm.blur(canopy.astype(float), 1.0)
    return rgb, canopy & (k > 0.9), ground & (k < 0.05)


def class_features(rgb, mask):
    px = rgb[mask]
    if len(px) < 200:
        return None
    lab = cm.to_lab(px.reshape(-1, 1, 3)).reshape(-1, 3)
    f = {}
    for i, n in enumerate("Lab"):
        f["q_" + n] = np.quantile(lab[:, i], np.linspace(0.05, 0.95, 10))
    sub = lab[:: max(1, len(lab) // 8000)]
    c, w = cm.kmeans(sub, 4)
    f["clusters"] = np.concatenate([c.ravel(), w])
    chroma = np.hypot(lab[:, 1], lab[:, 2])
    f["link"] = np.array([np.corrcoef(lab[:, 0], chroma)[0, 1] if lab[:, 0].std() > 0 else 0.0, chroma.mean() / 20.0, lab[:, 0].std() / 20.0])
    return f
