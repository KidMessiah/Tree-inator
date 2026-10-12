"""Stand structure from a canopy height model, measured the same way on a lidar model and on our engine's height
map, so neither colour nor shadow can be mistaken for ground. All lengths are in the model's own pixels; both
sides are brought to 1 m before measuring. numpy and PIL only."""
import numpy as np
import canopy_metrics as cm

RADIUS_BINS = np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.5, 8.0, 10.0])


def local_maxima(h, sigma=1.0, win=2, min_h=0.0):
    s = cm.blur(h, sigma)
    H, W = s.shape
    pad = np.pad(s, win, mode="edge")
    ok = np.ones_like(s, bool)
    for dy in range(-win, win + 1):
        for dx in range(-win, win + 1):
            if dx or dy:
                ok &= s >= pad[win + dy : win + dy + H, win + dx : win + dx + W]
    ok &= s > min_h
    ys, xs = np.nonzero(ok)
    return np.stack([ys, xs], 1).astype(float), s


def metrics(h, cover_h, tops_min_h):
    """h: height model at 1 m pixels (N x N). cover_h: height above which a pixel is canopy."""
    n = h.shape[0]
    mask = h > cover_h
    out = {}
    out["cover"] = np.array([mask.mean()])
    gaps = cm.label_sizes(~mask)
    edges = [0, 4, 16, 64, 256, 1e9]
    tot = max((~mask).sum(), 1)
    out["gap_sizes"] = np.array([gaps[(gaps > a) & (gaps <= b)].sum() / tot for a, b in zip(edges[:-1], edges[1:])]) if len(gaps) else np.zeros(5)
    out["gap_count"] = np.array([len(gaps) / (n * n) * 1e4])
    tops, s = local_maxima(h, 1.0, 2, tops_min_h)
    tops = tops[mask[tops[:, 0].astype(int), tops[:, 1].astype(int)]] if len(tops) else tops
    area_ha = n * n / 1e4
    out["top_density"] = np.array([len(tops) / area_ha])
    if len(tops) >= 6:
        d = np.sqrt(((tops[:, None] - tops[None]) ** 2).sum(-1))
        np.fill_diagonal(d, 1e9)
        nn = d.min(1)
        dens = len(tops) / (n * n)
        out["spacing"] = np.array([nn.mean(), nn.std() / nn.mean(), nn.mean() / (0.5 / np.sqrt(dens))])
        # crown radius: canopy area closest to each top, within 14 m
        yy, xx = np.nonzero(mask)
        pts = np.stack([yy, xx], 1).astype(float)
        if len(pts) > 20000:
            sel = np.random.default_rng(0).choice(len(pts), 20000, replace=False)
            scale_up = len(pts) / 20000
            pts = pts[sel]
        else:
            scale_up = 1.0
        dist = np.sqrt(((pts[:, None] - tops[None]) ** 2).sum(-1))
        owner = dist.argmin(1)
        near = dist.min(1) < 14
        counts = np.bincount(owner[near], minlength=len(tops)) * scale_up
        rad = np.sqrt(counts / np.pi)
        rad = rad[counts > 0]
        out["crown_radius"] = np.array([np.percentile(rad, 25), np.median(rad), np.percentile(rad, 75), np.percentile(rad, 90)])
    else:
        out["spacing"] = np.zeros(3)
        out["crown_radius"] = np.zeros(4)
    hv = h[mask]
    out["height"] = np.array([hv.std() / max(hv.mean(), 1e-6) if len(hv) else 0.0, (np.percentile(hv, 90) - np.percentile(hv, 10)) / max(hv.mean(), 1e-6) if len(hv) else 0.0])
    # roughness of the canopy surface: relief at 1 and 3 m, against the height
    r1 = np.abs(h - cm.blur(h, 1.0))[mask].mean() / max(hv.mean(), 1e-6) if mask.any() else 0.0
    r3 = np.abs(cm.blur(h, 1.0) - cm.blur(h, 3.0))[mask].mean() / max(hv.mean(), 1e-6) if mask.any() else 0.0
    out["relief"] = np.array([r1, r3])
    P = np.abs(np.fft.fft2((h - h.mean()) * np.outer(np.hanning(n), np.hanning(n)))) ** 2
    fy, fx = np.fft.fftfreq(n)[:, None], np.fft.fftfreq(n)[None, :]
    rr = np.hypot(fx, fy)
    e = np.array([P[(rr >= a) & (rr < b)].mean() for a, b in ((0.01, 0.04), (0.04, 0.1), (0.1, 0.2), (0.2, 0.5))])
    out["bands"] = e / e.sum()
    return out
