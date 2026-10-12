"""Canopy-to-ground separation numbers, real patches against engine renders of tuned settings.
Usage: separation_report.py version_suffix ..."""
import json, os, sys
import numpy as np
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import canopy_metrics as cm
import ba_changes as bc

NAMES = ["split explains", "class gap", "edge steepness", "share lit", "dark texture"]
print(f"{'':16s}" + "".join(f"{n:>16s}" for n in NAMES))
for key in ("aspen", "pine", "dfir"):
    real = m.real_patches(key)
    rv = np.mean([cm.features(p)["separation"] for p in real], 0)
    print(f"{key + ' real':16s}" + "".join(f"{v:16.3f}" for v in rv))
    for v in sys.argv[1:]:
        best = json.load(open(os.path.join(m.OUT, f"{key}_best{v}.json")))
        gc = m.ground_colour(real, best["knobs"]["ground_dark"])
        env = bc.ENVS_FOR.get(v, {}) if hasattr(bc, "ENVS_FOR") else {}
        vals = []
        for s in (31, 32, 33):
            img = m.composite(bc.render_variant(key, best, env, s) if env else m.render(m.build(key, best["knobs"], seed=s, ramp0=best["ramp"]), "sep_" + key), gc)
            vals.append(cm.features(img)["separation"])
        print(f"{key + ' ' + v:16s}" + "".join(f"{x:16.3f}" for x in np.mean(vals, 0)))
