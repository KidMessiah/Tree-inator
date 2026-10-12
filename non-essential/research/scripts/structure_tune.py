"""Tune the engine's structure settings (crown size, density, gaps, height variation, weave) against real canopy
height models, so structure is matched on measured height and not on colour. Usage: structure_tune.py species"""
import json, os, sys
import numpy as np
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import structure_lib as sl

KEYS = {
    "crown_max": (8, 40),
    "canopy_cover": (0.15, 1.0),
    "gaps": (0.0, 1.0),
    "gap_size": (4, 80),
    "canopy_patch_size": (8, 600),
    "height_var": (0.4, 2.5),
    "weave": (0.0, 0.45),
    "depth_strength": (0.0, 2.0),
}


def main():
    key = sys.argv[1]
    iters = int(os.environ.get("ITERS", 200))
    ref = sl.real_metrics(key)
    base = json.load(open(os.path.join(m.OUT, f"{key}_best_s1.json")))
    knobs = dict(base["knobs"])
    import biomes
    # crown radius is held near the stand's real size: the mean radius is 0.8 of crown_max (crown_min is 0.6 of it)
    mid = biomes.CROWN_R[key] / m.MPP / 0.8
    KEYS["crown_max"] = (mid * 0.88, mid * 1.12)
    knobs["crown_max"] = mid
    knobs["scale"] = 1.0
    knobs["weave"] = min(knobs["weave"], 0.45)
    knobs["height_var"] = max(knobs["height_var"], 0.4)
    rng = np.random.default_rng(11)

    def score(k, seeds=(1, 2, 3)):
        f = sl.engine_metrics(key, k, seeds=seeds)
        return sl.distance(f, ref)

    best, (bs, rows) = knobs, score(knobs)
    print(key, "start", round(bs, 3), flush=True)
    names, bounds = list(KEYS), KEYS
    K = int(os.environ.get("BATCH", 4))
    from concurrent.futures import ThreadPoolExecutor
    pool = ThreadPoolExecutor(K)
    since, patience = 0, max(2, int(os.environ.get("PATIENCE", 45)) // K)
    for it in range(1, iters + 1, K):
        step = 0.3 * (1 - it / (iters + 1)) + 0.04
        cands = []
        for _ in range(K):
            c = dict(best)
            for n in rng.choice(names, size=rng.integers(1, 4), replace=False):
                lo, hi = bounds[n]
                c[n] = float(np.clip(c[n] + rng.normal(0, step * (hi - lo)), lo, hi))
            cands.append(c)
        # the candidates are tried at the same time, one engine run each
        res = list(pool.map(score, cands))
        j = min(range(K), key=lambda i: res[i][0])
        if res[j][0] < bs:
            since = 0
            best, (bs, rows) = cands[j], res[j]
            print(f"{it:4d} {bs:.3f}", flush=True)
        else:
            since += 1
            if since >= patience:
                break
    json.dump(dict(ramp=base["ramp"], knobs=best, structure_score=bs, rows=rows), open(os.path.join(m.OUT, f"{key}_best_struct2{os.environ.get('OUT_TAG', '')}.json"), "w"), indent=1)
    print("best", bs)


if __name__ == "__main__":
    main()
