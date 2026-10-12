"""Tune the bamboo type on the Japanese aerial patches (imagery only: no lidar or hyperspectral is open for these stands).
Usage: bamboo_run.py   (env TAG, SEED, ITERS)"""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.setdefault("TEMPLATE", os.path.join(HERE, "..", "ui_mock", "sheet_project.json"))
import match_species as m
# bamboo clumps are small: crown radius of a few pixels, and cover is nearly total
m.KNOBS["crown_max"] = (3.0, 10.0, 5.0)
m.KNOBS["canopy_cover"] = (0.97, 1.0, 0.99)
m.KNOBS["gaps"] = (0.0, 0.05, 0.0)
m.KNOBS["haze_opacity"] = (0.0, 0.06, 0.02)
m.KNOBS["gap_size"] = (4, 12, 6)
# only the sunlit hillside patches: the grove at Arashiyama is in deep shade and would pull the colours blue
m.SPECIES["bamboo"]["scenes"] = [("bambooJP_nagA", 1), ("bambooJP_nagB", 1)]
sys.argv = ["match_species.py", "tune", "bamboo"]
m.main()
