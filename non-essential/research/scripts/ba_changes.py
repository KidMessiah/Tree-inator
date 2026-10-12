"""Before and after for engine experiments, with the structure numbers that matter.
Usage: ba_changes.py TAG   (uses *_best_v3.json knobs; variants are env settings for the experiment build)"""
import json, os, subprocess, sys
import numpy as np
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import canopy_metrics as cm

EXP = os.environ.get("EXPCLI", "")
VARIANTS = {
    "weave w2": {"Z_BASE": "0.75", "Z_JITTER": "0.3"},
    "w2 + soft 0.1": {"Z_BASE": "0.75", "Z_JITTER": "0.3", "Z_SOFT": "0.1"},
    "w2 + soft 0.25": {"Z_BASE": "0.75", "Z_JITTER": "0.3", "Z_SOFT": "0.25"},
    "soft 0.25 only": {"Z_BASE": "0.75", "Z_SOFT": "0.25"},
}


def lum(img):
    return cm.to_lab(img)[..., 0]


def stats(img):
    L = lum(img)
    P = np.abs(np.fft.fft2((L - L.mean()) * np.outer(np.hanning(256), np.hanning(256)))) ** 2
    fy, fx = np.fft.fftfreq(256)[:, None], np.fft.fftfreq(256)[None, :]
    r = np.hypot(fx, fy)
    bands = [(0.004, 0.03), (0.03, 0.07), (0.07, 0.15), (0.15, 0.5)]
    e = np.array([P[(r >= a) & (r < b)].mean() for a, b in bands])
    e = e / e.sum()
    lab = cm.to_lab(img)
    chroma = np.hypot(lab[..., 1], lab[..., 2])
    corr = np.corrcoef(L.ravel(), chroma.ravel())[0, 1]
    q = np.quantile(L, 0.25)
    dark_b = lab[..., 2][L <= q].mean()
    l2 = cm.blur(L, 1.0)
    mask = (l2 - cm.blur(l2, 6)) > 0
    sizes = cm.label_sizes(mask)
    edge = (mask[1:] != mask[:-1]).sum() + (mask[:, 1:] != mask[:, :-1]).sum()
    comp = 4 * np.pi * mask.sum() / max(edge**2, 1) * 1000
    return dict(low=e[0], mid=e[1], crown=e[2], fine=e[3], chroma_corr=corr, dark_b=dark_b, blob50=np.median(sizes), compact=comp)


def render_variant(key, best, env, seed):
    e = dict(os.environ)
    e.update(env)
    e["CLI"] = EXP
    k = best["knobs"]
    p = m.build(key, k, seed=seed, ramp0=best["ramp"])
    pj = os.path.join(m.OUT, f"bac_{key}.json")
    json.dump(p, open(pj, "w"))
    png = os.path.join(m.OUT, f"bac_{key}.png")
    subprocess.run([EXP, pj, "-out", png, "-scale", str(m.SUPER)], check=True, capture_output=True, env=e)
    a = np.asarray(Image.open(png).convert("RGBA")).astype(np.float64)
    a[..., :3] *= a[..., 3:4] / 255.0
    h, w = a.shape[0] // m.SUPER, a.shape[1] // m.SUPER
    a = a.reshape(h, m.SUPER, w, m.SUPER, 4).mean((1, 3))
    a[..., :3] = a[..., :3] / np.maximum(a[..., 3:4], 1e-6) * 255.0
    return a


def main():
    rows, table = [], {}
    for key in ("aspen", "pine", "dfir"):
        best = json.load(open(os.path.join(m.OUT, f"{key}_best{os.environ.get('VER', '_v3')}.json")))
        real = m.real_patches(key)
        gc = m.ground_colour(real, best["knobs"]["ground_dark"])
        rs = [stats(p) for p in real]
        table[(key, "real")] = {k: float(np.mean([r[k] for r in rs])) for k in rs[0]}
        cells = [real[0], real[3]]
        for name, env in VARIANTS.items():
            imgs = [m.composite(render_variant(key, best, env, s), gc) for s in (31, 32, 33)]
            cells.append(imgs[0])
            st = [stats(i) for i in imgs]
            table[(key, name)] = {k: float(np.mean([r[k] for r in st])) for k in st[0]}
        rows.append(np.concatenate(cells, 1))
    img = Image.fromarray(np.concatenate(rows, 0).astype(np.uint8))
    d = ImageDraw.Draw(img)
    for i, t in enumerate(["real", "real"] + list(VARIANTS)):
        d.text((i * 256 + 4, 2), t, fill="white")
    out = os.path.join(m.OUT, f"ba_changes{sys.argv[1]}.png")
    img.save(out)
    print(out)
    cols = ["low", "mid", "crown", "fine", "chroma_corr", "dark_b", "blob50", "compact"]
    print(f"{'':22s}" + "".join(f"{c:>12s}" for c in cols))
    for (key, name), v in table.items():
        print(f"{key + ' ' + name:22s}" + "".join(f"{v[c]:12.3f}" for c in cols))


if __name__ == "__main__":
    main()
