"""Pull red, green, blue and near-infrared reflectance and the flight sun angles out of the big NEON reflectance files of
the biomes into small npz files next to the canopy height model. Usage: neon_refl_extract_biomes.py [key ...]"""
import glob, json, os, sys
import h5py
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import biomes
import structure_lib as sl

OUT = os.path.join(HERE, "..", "neon")
for key in sys.argv[1:] or list(biomes.BIOMES):
    if key not in sl.SITES:
        continue
    site, year, e, n = sl.SITES[key]
    fs = glob.glob(f"D:/neon_dl/{site}/**/*DP3_{e}_{n}*reflectance.h5", recursive=True)
    if not fs:
        print(key, "no reflectance file yet")
        continue
    f = h5py.File(fs[0], "r")
    top = list(f.keys())[0]
    R = f[top]["Reflectance"]
    M = R["Metadata"]
    wl = M["Spectral_Data/Wavelength"][:]
    refl = R["Reflectance_Data"]
    pick = {c: int(np.abs(wl - nm).argmin()) for c, nm in (("R", 640), ("G", 550), ("B", 460), ("NIR", 860))}
    d = {c: np.where(refl[:, :, i] >= 0, refl[:, :, i].astype(np.float32) / 10000.0, np.nan) for c, i in pick.items()}
    logs = M["Logs"]
    az = [float(logs[g]["Solar_Azimuth_Angle"][()]) for g in logs]
    zen = [float(logs[g]["Solar_Zenith_Angle"][()]) for g in logs]
    az_m, zen_m = float(np.median(az)), float(np.median(zen))
    np.savez_compressed(os.path.join(OUT, site, f"refl_{e}_{n}.npz"), **d)
    json.dump({"site": site, "sun_azimuth": az_m, "sun_elevation": 90 - zen_m, "lines": len(az)}, open(os.path.join(OUT, site, "sun.json"), "w"))
    print(key, site, "shape", d["R"].shape, "sun az %.0f el %.0f" % (az_m, 90 - zen_m), "lines", len(az), flush=True)
