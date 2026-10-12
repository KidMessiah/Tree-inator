"""Run one stage of the tuning for a biome from biomes.py.
Usage: biome_run.py KEY structure|colour   (ITERS=n for the number of tries)
structure: matches crown size, cover, gaps and height variation on the lidar canopy height model.
colour: matches sunlit and shaded canopy and ground colour on the camera mosaic, structure locked."""
import json, os, runpy, shutil, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.setdefault("TEMPLATE", os.path.join(HERE, "..", "ui_mock", "sheet_project.json"))
import biomes
import match_species as m

key, stage = sys.argv[1], sys.argv[2]
import structure_lib as _sl
if key not in _sl.SITES:
    sys.exit("unknown biome " + key)
s1 = os.path.join(m.OUT, f"{key}_best_s1.json")
if not os.path.exists(s1):
    json.dump(dict(ramp=[[0.2, 0.3, 0.1], [0.3, 0.4, 0.2], [0.4, 0.5, 0.25], [0.5, 0.6, 0.3]], knobs=m.start_knobs()), open(s1, "w"))
script = {"structure": "structure_tune.py", "colour": "colour_tune2.py", "hyper": "colour_tune3.py"}[stage]
if stage == "colour":
    # the colour tuner reads the structure result under the older name
    src = os.path.join(m.OUT, f"{key}_best_struct2.json")
    shutil.copy(src, os.path.join(m.OUT, f"{key}_best_struct.json"))
sys.argv = [script, key]
runpy.run_path(os.path.join(HERE, script), run_name="__main__")
