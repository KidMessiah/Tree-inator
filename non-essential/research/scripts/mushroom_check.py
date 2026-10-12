"""Render the baked mushrooms through the real engine, large, so the sprites can be judged."""
import json, os, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rust_trees_sheet as rt
from PIL import Image
S = [("mushroom_redcap", [[150,24,22],[204,42,30],[228,100,40],[238,176,76]], [246,238,218]),
     ("mushroom_brown", [[88,52,28],[140,90,48],[190,144,92],[214,182,132]], [214,190,150]),
     ("mushroom_violet", [[66,40,90],[128,86,150],[176,140,190],[206,180,214]], [226,210,232]),
     ("mushroom_funnel", [[168,112,26],[224,170,52],[240,206,100],[246,228,150]], [250,238,190]),
     ("mushroom_puffball", [[196,184,156],[228,220,198],[238,232,212],[240,234,216]], [168,154,124])]
species = [dict(name=n, procedural=True, preset=n, canopy_fill=0.0, ramp=[[c/255 for c in r] for r in ramp], bark=[c/255 for c in w], height=1.0) for n, ramp, w in S]
trees = []
for k in range(5):
    for v in range(5):
        trees.append(dict(x=70 + v*135, y=70 + k*135, r=55, h=26, tone=0.0, layer="Canopy", species=k, seed=v))
proj = dict(map_w=700, map_h=700, species=species, painted=trees, gen=dict(procedural_trees=False, painted_trees=True, seed=3))
jp = os.path.join(rt.TMP, "mc.json"); json.dump(proj, open(jp, "w"))
subprocess.run([rt.CLI, jp, "-bg", "-scale", "1.5", "-out", os.path.join(rt.TMP, "mc.png")], check=True)
Image.open(os.path.join(rt.TMP, "mc.png")).save(os.path.join(rt.OUT, "mushroom_check.png"))
