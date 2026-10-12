"""Cut a 1 km square out of the G-LiHT Puerto Rico canopy height strip and its aerial photo strip, in the same layout the NEON
tiles use (2017_E_N_CHM.tif at 1 m, 2017_E_N_image.tif at 10 cm), under research/neon/PRYV/. Usage: prep_gliht.py [first_row]"""
import os, sys
import numpy as np
import rasterio
from rasterio.windows import from_bounds
from PIL import Image
Image.MAX_IMAGE_PIXELS = None
HERE = os.path.dirname(os.path.abspath(__file__))
G = os.path.join(HERE, "..", "gliht")
OUT = os.path.join(HERE, "..", "neon", "PRYV")
os.makedirs(OUT, exist_ok=True)
r0 = int(sys.argv[1]) if len(sys.argv) > 1 else 900
with rasterio.open(os.path.join(G, "GLCHMT_PR_15March2017_63_64_70_l0s9_CHM.tif")) as d:
    left, top = d.bounds.left, d.bounds.top
    a = d.read(1, window=((r0, r0 + 1000), (0, 1000)))
e, n = int(left), int(top - r0 - 1000)
a = np.where(np.isfinite(a) & (a > -50), a, -9999).astype("float32")
Image.fromarray(a, mode="F").save(os.path.join(OUT, f"2017_{e}_{n}_CHM.tif"))
with rasterio.open(os.path.join(G, "GLORTHO_PR_15March2017_63_64_70_l0s9_ortho.tif")) as d:
    win = from_bounds(e, n, e + 1000, n + 1000, d.transform)
    rgb = d.read([1, 2, 3], window=win, out_shape=(3, 10000, 10000), resampling=rasterio.enums.Resampling.average)
Image.fromarray(np.moveaxis(rgb, 0, -1)).save(os.path.join(OUT, f"2017_{e}_{n}_image.tif"), compression="tiff_lzw")
print("PRYV tile", e, n)
