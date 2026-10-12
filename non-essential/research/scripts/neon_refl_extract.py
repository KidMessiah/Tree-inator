"""Pull red, green, blue and near-infrared reflectance and the flight sun angles out of the big NEON reflectance
files into small npz files next to the canopy height model. Usage: neon_refl_extract.py"""
import glob, json, os
import h5py
import numpy as np

SRC = {"WREF": "D:/neon_dl/WREF", "STEI": "D:/neon_dl/STEI", "OSBS": "D:/neon_dl/OSBS"}
OUT = os.path.join(os.path.dirname(__file__), "..", "neon")
for site, root in SRC.items():
    fs = glob.glob(os.path.join(root, "**", "*bidirectional_reflectance.h5"), recursive=True)
    if not fs:
        continue
    f = h5py.File(fs[0], "r")
    top = list(f.keys())[0]
    R = f[top]["Reflectance"]
    M = R["Metadata"]
    wl = M["Spectral_Data/Wavelength"][:]
    refl = R["Reflectance_Data"]
    pick = {c: int(np.abs(wl - nm).argmin()) for c, nm in (("R", 640), ("G", 550), ("B", 460), ("NIR", 860))}
    d = {c: np.where(refl[:, :, i] >= 0, refl[:, :, i].astype(np.float32) / 10000.0, np.nan) for c, i in pick.items()}
    az = [float(M["Logs"][g]["Solar_Azimuth_Angle"][()]) for g in M["Logs"]]
    zen = [float(M["Logs"][g]["Solar_Zenith_Angle"][()]) for g in M["Logs"]]
    # the flight lines of the afternoon are close together; take the median of the lines that agree
    az_m, zen_m = float(np.median(az)), float(np.median(zen))
    tag = os.path.basename(fs[0]).split("_DP3_")[1].split("_bidirectional")[0]
    np.savez_compressed(os.path.join(OUT, site, f"refl_{tag}.npz"), **d)
    json.dump({"site": site, "sun_azimuth": az_m, "sun_elevation": 90 - zen_m, "lines": len(az)}, open(os.path.join(OUT, site, "sun.json"), "w"))
    print(site, tag, "sun az %.0f el %.0f" % (az_m, 90 - zen_m), "lines", len(az))
