"""Where do tuned renders still differ from aerial patches? Structural statistics, real against ours.
Usage: diagnose.py species"""
import json, os, sys
import numpy as np
sys.path.insert(0, os.path.dirname(__file__))
import match_species as m
import canopy_metrics as cm

key = sys.argv[1]
best = json.load(open(os.path.join(m.OUT, f"{key}_best.json")))
real = m.real_patches(key)
k = best["knobs"]
gc = m.ground_colour(real, k["ground_dark"])
ours = [m.composite(m.render(m.build(key, k, seed=s, ramp0=best["ramp"]), f"diag{s}"), gc) for s in (11, 12, 13, 14)]


def lum(img):
    return cm.to_lab(img)[..., 0]


def spectrum(img):
    L = lum(img)
    L = L - L.mean()
    win = np.outer(np.hanning(L.shape[0]), np.hanning(L.shape[1]))
    P = np.abs(np.fft.fft2(L * win)) ** 2
    h, w = P.shape
    fy = np.fft.fftfreq(h)[:, None]
    fx = np.fft.fftfreq(w)[None, :]
    r = np.hypot(fx, fy)
    bands = [(0.004, 0.012), (0.012, 0.03), (0.03, 0.07), (0.07, 0.15), (0.15, 0.3), (0.3, 0.5)]
    e = np.array([P[(r >= a) & (r < b)].mean() for a, b in bands])
    return e / e.sum()


def slope(img):
    L = lum(img)
    L = L - L.mean()
    P = np.abs(np.fft.fft2(L * np.outer(np.hanning(256), np.hanning(256)))) ** 2
    fy = np.fft.fftfreq(256)[:, None]
    fx = np.fft.fftfreq(256)[None, :]
    r = np.hypot(fx, fy)
    rs = np.exp(np.linspace(np.log(0.01), np.log(0.4), 14))
    pw = np.array([P[(r >= a) & (r < b)].mean() for a, b in zip(rs[:-1], rs[1:])])
    x = np.log(np.sqrt(rs[:-1] * rs[1:]))
    return float(np.polyfit(x, np.log(pw), 1)[0])


def gap_exponent(img):
    L = lum(img)
    mask = L < 0.6 * np.median(L)
    s = cm.label_sizes(mask)
    if len(s) < 8:
        return float("nan"), len(s)
    s = s[s >= 2]
    xs = np.unique(np.round(np.logspace(np.log10(2), np.log10(max(s.max(), 3)), 8)))
    ccdf = np.array([(s >= x).mean() for x in xs])
    ok = ccdf > 0
    return float(-np.polyfit(np.log(xs[ok]), np.log(ccdf[ok]), 1)[0]), len(s)


def lacunarity(img):
    L = lum(img)
    mask = (L > np.median(L)).astype(float)
    out = []
    for b in (4, 8, 16, 32):
        n = 256 // b
        mass = mask[: n * b, : n * b].reshape(n, b, n, b).sum((1, 3)).ravel()
        out.append(mass.var() / max(mass.mean() ** 2, 1e-9) + 1)
    return np.array(out)


def chroma_vs_l(img):
    lab = cm.to_lab(img)
    c = np.hypot(lab[..., 1], lab[..., 2]).ravel()
    L = lab[..., 0].ravel()
    return float(np.corrcoef(L, c)[0, 1])


def sun_asymmetry(img):
    """Mean brightness gradient along each of 8 directions, relative to its overall size."""
    L = cm.blur(lum(img), 1.2)
    gy, gx = np.gradient(L)
    mag = np.hypot(gx, gy).mean()
    res = []
    for a in np.arange(0, 360, 45):
        d = np.array([np.cos(np.radians(a)), np.sin(np.radians(a))])
        res.append((gx * d[0] + gy * d[1]).mean() / mag)
    return np.array(res)


def crown_tone_scales(img):
    """Share of brightness variance at fine, crown and stand scale."""
    L = lum(img)
    a, b, c = cm.blur(L, 1.0), cm.blur(L, 4.0), cm.blur(L, 16.0)
    f, cr, st = (L - a).var(), (a - b).var(), (b - c).var()
    t = f + cr + st
    return np.array([f, cr, st]) / t, np.array([f, cr, st])


def table(name, fn, fmt="{:7.3f}"):
    r = np.mean([np.atleast_1d(fn(p)) for p in real], 0)
    o = np.mean([np.atleast_1d(fn(p)) for p in ours], 0)
    sd = np.std([np.atleast_1d(fn(p)) for p in real], 0)
    print(f"{name:22s} real " + " ".join(fmt.format(v) for v in r) + "   ours " + " ".join(fmt.format(v) for v in o) + "   real spread " + " ".join(fmt.format(v) for v in sd))


print(f"== {key} ==")
table("spectrum bands (lo>hi)", spectrum)
table("spectral slope", slope)
table("gap exponent / count", gap_exponent)
table("lacunarity 4/8/16/32", lacunarity)
table("chroma~L correlation", chroma_vs_l)
table("gradient by direction", sun_asymmetry, "{:7.4f}")
table("variance share f/c/s", lambda im: crown_tone_scales(im)[0])
table("variance abs f/c/s", lambda im: crown_tone_scales(im)[1], "{:7.1f}")


def hue_stats(img):
    """Spread of hue at fixed brightness: std of the Lab hue angle within each brightness quartile, and the
    mean a*, b* of the darkest and lightest quartile."""
    lab = cm.to_lab(img).reshape(-1, 3)
    q = np.quantile(lab[:, 0], [0.25, 0.5, 0.75])
    out = []
    for lo, hi in ((-1, q[0]), (q[0], q[1]), (q[1], q[2]), (q[2], 1e9)):
        sel = lab[(lab[:, 0] > lo) & (lab[:, 0] <= hi)]
        ang = np.degrees(np.arctan2(sel[:, 2], sel[:, 1]))
        out.append(ang.std())
    d, l = lab[lab[:, 0] <= q[0]], lab[lab[:, 0] > q[2]]
    out += [d[:, 1].mean(), d[:, 2].mean(), l[:, 1].mean(), l[:, 2].mean()]
    return np.array(out)


table("hue std per L quartile", lambda im: hue_stats(im)[:4], "{:7.2f}")
table("a,b dark | a,b light", lambda im: hue_stats(im)[4:], "{:7.2f}")


def peaks(img, sigma=2.6, win=3):
    L = cm.blur(lum(img), sigma)
    h, w = L.shape
    pad = np.pad(L, win, mode="edge")
    ismax = np.ones_like(L, bool)
    for dy in range(-win, win + 1):
        for dx in range(-win, win + 1):
            if dx or dy:
                ismax &= L >= pad[win + dy : win + dy + h, win + dx : win + dx + w]
    ismax &= L > cm.blur(L, 10)
    ys, xs = np.nonzero(ismax[8:-8, 8:-8])
    return np.stack([ys + 8, xs + 8], 1).astype(float), L


def point_pattern(img):
    p, L = peaks(img)
    n = len(p)
    if n < 10:
        return np.array([n, 0, 0, 0])
    d = np.sqrt(((p[:, None] - p[None]) ** 2).sum(-1))
    np.fill_diagonal(d, 1e9)
    nn = d.min(1)
    # Clark-Evans ratio: observed mean NN distance over that of a random pattern of the same density
    dens = n / (240 * 240)
    ce = nn.mean() / (0.5 / np.sqrt(dens))
    return np.array([n, ce, nn.std() / nn.mean(), np.percentile(nn, 10) / nn.mean()])


def blob_shapes(img):
    """Area quantiles and compactness of the bright blobs (brighter than their 8 px neighbourhood)."""
    L = cm.blur(lum(img), 1.0)
    mask = (L - cm.blur(L, 6)) > 0
    h, w = mask.shape
    sizes = cm.label_sizes(mask)
    # perimeter by edge count of the whole mask divided by blob count gives a mean compactness proxy
    edge = (mask[1:, :] != mask[:-1, :]).sum() + (mask[:, 1:] != mask[:, :-1]).sum()
    area = mask.sum()
    return np.array([np.percentile(sizes, 25), np.percentile(sizes, 50), np.percentile(sizes, 90), 4 * np.pi * area / max(edge**2, 1) * 1000])


table("peaks n/ClarkEvans/CVnn/p10", point_pattern, "{:7.2f}")
table("blobs a25/a50/a90/compact", blob_shapes, "{:7.1f}")
