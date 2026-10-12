"""Image statistics for comparing aerial forest patches with our renders. numpy and PIL only.
features(rgb) takes an HxWx3 float array in 0..255 and returns a dict of named vectors."""
import numpy as np


def to_lab(rgb):
    c = rgb / 255.0
    c = np.where(c > 0.04045, ((c + 0.055) / 1.055) ** 2.4, c / 12.92)
    m = np.array([[0.4124, 0.3576, 0.1805], [0.2126, 0.7152, 0.0722], [0.0193, 0.1192, 0.9505]])
    xyz = c @ m.T / np.array([0.95047, 1.0, 1.08883])
    f = np.where(xyz > 0.008856, np.cbrt(xyz), 7.787 * xyz + 16 / 116)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], -1)


def blur(img, sigma):
    h, w = img.shape
    fy = np.fft.fftfreq(h)[:, None]
    fx = np.fft.fftfreq(w)[None, :]
    k = np.exp(-2 * (np.pi * sigma) ** 2 * (fx**2 + fy**2))
    return np.fft.ifft2(np.fft.fft2(img) * k).real


def label_sizes(mask):
    """Areas of the 4-connected components of a boolean mask (iterative union-find on rows)."""
    h, w = mask.shape
    parent = np.arange(h * w)

    def find(a):
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    flat = mask.ravel()
    idx = np.nonzero(flat)[0]
    for i in idx:
        if (i % w) and flat[i - 1]:
            ra, rb = find(i), find(i - 1)
            if ra != rb:
                parent[ra] = rb
        if i >= w and flat[i - w]:
            ra, rb = find(i), find(i - w)
            if ra != rb:
                parent[ra] = rb
    roots = np.array([find(i) for i in idx])
    _, counts = np.unique(roots, return_counts=True)
    return counts


def kmeans(x, k, iters=12):
    qs = np.quantile(x[:, 0], (np.arange(k) + 0.5) / k)
    c = np.stack([x[np.argmin(np.abs(x[:, 0] - q))] for q in qs])
    for _ in range(iters):
        d = ((x[:, None, :] - c[None]) ** 2).sum(-1)
        a = d.argmin(1)
        for j in range(k):
            if (a == j).any():
                c[j] = x[a == j].mean(0)
    w = np.bincount(a, minlength=k) / len(a)
    o = np.argsort(c[:, 0])
    return c[o], w[o]


QS = np.linspace(0.05, 0.95, 10)
RADII = [1, 2, 3, 4, 6, 8, 12, 16]


def features(rgb):
    rgb = rgb.astype(np.float64)
    lab = to_lab(rgb)
    L = lab[..., 0]
    med = np.median(L)
    f = {}
    for i, n in enumerate("Lab"):
        f["q_" + n] = np.quantile(lab[..., i], QS)
    sub = lab.reshape(-1, 3)[:: max(1, lab.shape[0] * lab.shape[1] // 20000)]
    c, w = kmeans(sub, 6)
    f["cluster_centres"] = c.ravel()
    f["cluster_weights"] = w
    s = rgb.sum(-1) + 1e-6
    green = rgb[..., 1] / s
    dark = L < 0.55 * med
    open_ = (green < 0.355) & (L > 0.7 * med)
    f["dark_share"] = np.array([dark.mean()])
    f["open_share"] = np.array([open_.mean()])
    f["canopy_share"] = np.array([1 - (dark | open_).mean()])
    for name, m in (("dark", dark), ("open", open_)):
        sizes = label_sizes(m)
        edges = [0, 4, 16, 64, 256, 1e9]
        tot = max(m.sum(), 1)
        f[name + "_gap_sizes"] = np.array([sizes[(sizes > a) & (sizes <= b)].sum() / tot for a, b in zip(edges[:-1], edges[1:])])
    # structure scale: autocorrelation of the band-passed brightness
    hp = L - blur(L, 24)
    hp = hp - hp.mean()
    ac = np.fft.ifft2(np.abs(np.fft.fft2(hp)) ** 2).real
    ac = ac / ac[0, 0]
    h, w_ = ac.shape
    yy, xx = np.indices(ac.shape)
    r = np.hypot(np.minimum(yy, h - yy), np.minimum(xx, w_ - xx))
    f["autocorr"] = np.array([ac[(r >= q - 0.5) & (r < q + 0.5)].mean() for q in RADII])
    # texture energy at several scales
    f["texture"] = np.array([np.std(blur(L, a) - blur(L, 2 * a)) for a in (0.6, 1.0, 1.6, 2.5, 4.0, 6.5)])
    f["contrast"] = np.array([np.percentile(L, 5) / max(med, 1), np.percentile(L, 95) / max(med, 1), L.std() / 100])
    gy, gx = np.gradient(blur(L, 0.8))
    f["edges"] = np.array([np.hypot(gx, gy).mean()])
    # band energies of the brightness spectrum, where crown-scale and stand-scale structure show up
    P = np.abs(np.fft.fft2((L - L.mean()) * np.outer(np.hanning(L.shape[0]), np.hanning(L.shape[1])))) ** 2
    fy, fx = np.fft.fftfreq(L.shape[0])[:, None], np.fft.fftfreq(L.shape[1])[None, :]
    rr = np.hypot(fx, fy)
    e = np.array([P[(rr >= lo) & (rr < hi)].mean() for lo, hi in ((0.004, 0.03), (0.03, 0.07), (0.07, 0.15), (0.15, 0.5))])
    f["bands"] = e / e.sum()
    l2 = blur(L, 1.0)
    bm = (l2 - blur(l2, 6)) > 0
    bs = label_sizes(bm)
    edge = (bm[1:] != bm[:-1]).sum() + (bm[:, 1:] != bm[:, :-1]).sum()
    f["blobs"] = np.array([np.log(np.median(bs)), np.log(np.percentile(bs, 90)), 4 * np.pi * bm.sum() / max(edge**2, 1) * 1000])
    chroma = np.hypot(lab[..., 1], lab[..., 2])
    f["chroma_link"] = np.array([np.corrcoef(L.ravel(), chroma.ravel())[0, 1], lab[..., 2][L <= np.quantile(L, 0.25)].mean() / 10])
    # how cleanly the picture splits into lit canopy and dark ground or gap: Otsu split of brightness
    hist, edges_ = np.histogram(L, bins=64)
    ph = hist / hist.sum()
    centres = (edges_[:-1] + edges_[1:]) / 2
    best_t, best_v = 0, 0
    for t in range(1, 63):
        w0, w1 = ph[:t].sum(), ph[t:].sum()
        if w0 > 0 and w1 > 0:
            m0, m1 = (ph[:t] * centres[:t]).sum() / w0, (ph[t:] * centres[t:]).sum() / w1
            v = w0 * w1 * (m0 - m1) ** 2
            if v > best_v:
                best_t, best_v = t, v
    thr = centres[best_t]
    hi, lo = L[L >= thr], L[L < thr]
    sepm = blur(L, 0.7)
    gy2, gx2 = np.gradient(sepm)
    gm = np.hypot(gx2, gy2)
    maskb = L >= thr
    bnd = (maskb[1:, :-1] != maskb[:-1, :-1]) | (maskb[:-1, 1:] != maskb[:-1, :-1])
    bnd = np.pad(bnd, ((0, 1), (0, 1)))
    contrast_ = (hi.mean() - lo.mean()) if len(hi) and len(lo) else 1.0
    f["separation"] = np.array([
        best_v / max(L.var(), 1e-6),  # share of brightness variance explained by the two-class split
        (hi.mean() - lo.mean()) / 100.0 if len(hi) and len(lo) else 0.0,  # gap between lit and dark class means
        gm[bnd].mean() / max(contrast_, 1e-6) if bnd.any() else 0.0,  # edge steepness at the boundary, per unit contrast
        maskb.mean(),  # share lit
        lo.std() / 20.0 if len(lo) else 0.0,  # how much texture the dark class has
    ])
    f["greenness"] = np.array([green.mean(), green.std()])
    return f


def crop_patches(img, size, n, rng):
    h, w = img.shape[:2]
    out = []
    for _ in range(n):
        y, x = rng.integers(0, h - size + 1), rng.integers(0, w - size + 1)
        out.append(img[y : y + size, x : x + size])
    return out


def stack(fs):
    return {k: np.stack([f[k] for f in fs]) for k in fs[0]}


WEIGHTS = {'separation': 2.0, 'bands': 2.0, 'blobs': 2.0, 'chroma_link': 1.5, 'texture': 2.0, 'autocorr': 2.0, 'edges': 1.5, 'dark_share': 1.5, 'canopy_share': 1.5, 'cluster_centres': 1.5, 'dark_gap_sizes': 1.5}


def distance(feat, ref_stack, weights=None):
    """Per-metric z-distance of one feature dict from the real patches: mean absolute difference from the real
    mean, in units of the real patches' own spread (never below a floor), so a metric is judged against how much
    real patches of the species vary."""
    rows = {}
    for k, v in feat.items():
        r = ref_stack[k]
        mu, sd = r.mean(0), r.std(0)
        floor = 0.08 * np.abs(mu).mean() + 1e-3
        rows[k] = float(np.mean(np.abs(v - mu) / np.maximum(sd, floor)))
    w = dict(WEIGHTS)
    w.update(weights or {})
    tot = sum(rows[k] * w.get(k, 1.0) for k in rows) / sum(w.get(k, 1.0) for k in rows)
    return tot, rows
