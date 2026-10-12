"""Bake the 3D mushroom generator into engine sprites: five seeds for each of five kinds, seen from above.

Each sprite stores what the engine's tree sprites store: height, surface tilt, a tone, a position along the species
colour ramp (0 shadow end, 1 light end) and a class (1 cap, 2 solid parts such as warts, drawn in the species' bark
colour). Colours come from the species at render time, so themes and grading still apply.

Output: crates/forest-core/assets/mushrooms.bin, a list of records, each a name, a variant, a size, zmax and two
PNG images: image A is RGBA (tilt x + 128, tilt y + 128, tone, ramp position) and image B is 16 bit gray with the
height, plus 0x8000 for class 2 and 0 for empty.
"""
import io
import math
import os
import struct
import sys

import numpy as np
from PIL import Image, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from mushroom_round3 import vnoise  # noqa: E402
from mushroom_round4 import PROFILES, catmull, rot, unit  # noqa: E402

K = int(os.environ.get("MUSH_K", "1"))  # 2 makes the double resolution set the engine uses for big crowns
R0 = 96.0 * K                  # crown radius the engine builds sprites for
SIZE = int(2 * R0 * 1.45) + 4  # same sprite size rule as the tree sprites
SS = 3
CAP_R = 0.95 * R0              # the cap radius inside the sprite

PROFILES = dict(PROFILES)
# a shallow funnel: its middle stays near the height of the rim, so it never reads as a hole
PROFILES["shallow"] = [(1.0, 0.0), (0.96, 0.12), (0.72, 0.22), (0.4, 0.2), (0.18, 0.17), (0.0, 0.16)]

KINDS = {
    "mushroom_redcap": dict(profile="convex", warts=70, wart_size=0.11, blot=0.2, ramp=(0.0, 0.62, 0.95)),
    "mushroom_brown": dict(profile="umbonate", warts=0, blot=0.28, ramp=(0.1, 0.55, 0.95)),
    "mushroom_violet": dict(profile="campanulate", warts=28, wart_size=0.11, blot=0.22, ramp=(0.05, 0.55, 0.95)),
    "mushroom_funnel": dict(profile="shallow", warts=0, blot=0.22, ramp=(0.05, 0.6, 1.0)),
    "mushroom_puffball": dict(profile="convex", warts=0, spines=700, blot=0.15, ramp=(0.25, 0.7, 0.95)),
}


class Cap:
    def __init__(self, kind, seed):
        self.k = KINDS[kind]
        self.seed = seed
        rng = self.rng = np.random.default_rng(seed)
        self.ph = rng.random(8) * 6.283
        self.ex = 1 + 0.06 * rng.standard_normal()
        self.height = rng.uniform(0.8, 1.2)
        self.curve = catmull(PROFILES[self.k["profile"]], 120)
        self.build()

    def outline(self, th):
        ph = self.ph
        return 1 + 0.04 * np.sin(2 * th + ph[0]) + 0.03 * np.sin(3 * th + ph[1]) + 0.012 * np.sin(5 * th + ph[2]) + 0.006 * np.sin(9 * th + ph[3])

    def prof(self, s):
        c = self.curve
        t = np.clip(s, 0, 1) * (len(c) - 1)
        i = np.clip(np.floor(t).astype(int), 0, len(c) - 2)
        f = (t - i)[..., None]
        p = c[i] * (1 - f) + c[i + 1] * f
        return p[..., 0], p[..., 1]

    def cap_point(self, th, s):
        r, z = self.prof(s)
        rho = r * self.outline(th) * (1 + 0.012 * np.sin(11 * th + self.ph[4]) * (1 - s) ** 2)
        z = z * self.height + 0.015 * np.sin(7 * th + self.ph[5]) * (1 - s) ** 3
        return np.stack([rho * np.cos(th) * self.ex, rho * np.sin(th), z], -1)

    def normals(self, th, s, P):
        e = 1e-3
        dth = (self.cap_point(th + e, s) - P) / e
        ds = (self.cap_point(th, np.clip(s + e, 0, 1)) - P) / e
        N = unit(np.cross(dth, ds))
        N = np.where(N[:, 2:3] < 0, -N, N)
        # the very middle of a cap has no defined tilt, so it faces up
        return np.where(N[:, 2:3] < 0.05, np.array([0.0, 0.0, 1.0]), N)

    def build(self):
        k, rng = self.k, self.rng
        n = 2200 * 640 * K * K
        th = rng.random(n) * 2 * np.pi
        # half uniform in the profile parameter (fills the middle), half by area (fills the rim)
        u = rng.random(n)
        s = 1 - np.where(rng.random(n) < 0.5, u, np.sqrt(u))
        P = self.cap_point(th, s)
        N = self.normals(th, s, P)
        u = (th / (2 * np.pi)) * 64
        blot = vnoise(u, s * 9 + 1, self.seed + 5, period=64) * 0.6 + vnoise(u * 2, s * 18 + 1, self.seed + 6, period=128) * 0.4
        streak = vnoise(u * 3, s * 4 + 1, self.seed + 7, period=192)
        t = 1 - s
        # ramp position, following the 3D generator's colouring: top 0, middle 1/3, margin 2/3, lip 1
        w_top = np.clip(1 - t * 2.2, 0, 1)
        w_mid = np.clip(1 - np.abs(t - 0.45) / 0.45, 0, 1)
        w_rim = np.clip((t - 0.5) / 0.4, 0, 1)
        pos = (0.0 * w_top + 0.333 * w_mid + 0.667 * w_rim) / (w_top + w_mid + w_rim + 1e-6)
        lip = np.clip((t - 0.9) / 0.1, 0, 1) * 0.6
        pos = pos * (1 - lip) + lip
        # blotches and fibres move the colour a little darker or lighter along the ramp
        pos = pos + (blot - 0.5) * k["blot"] * 0.9 + (streak - 0.5) * 0.3
        tone = 0.5 + (blot - 0.5) * 0.5
        parts = [(P, N, np.clip(pos, 0, 1), tone, np.ones(n, np.uint8))]
        # warts exactly as in the 3D generator: real cones on the cap, a big set and a small set
        self.flakes = []
        if k["warts"]:
            parts.append(self.warts(k["warts"], 0.11))
            parts.append(self.warts(k["warts"] // 2, 0.06))
        if k.get("spines"):
            parts.append(self.warts(k["spines"], 0.05))
        self.P = np.vstack([p[0] for p in parts])
        self.N = np.vstack([p[1] for p in parts])
        self.T = np.concatenate([p[2] for p in parts])
        self.TN = np.concatenate([p[3] for p in parts])
        self.C = np.concatenate([p[4] for p in parts])

    def flake_set(self, count, size):
        """Warts spread evenly over the cap: best candidate sampling in the disc, so no two sit close together
        and none clump, with sizes that differ only a little (a touch bigger toward the middle)."""
        rng = self.rng
        pts = []
        for _ in range(count):
            best, best_d = None, -1.0
            for _ in range(24):
                a = rng.random() * 6.283
                r = 0.93 * np.sqrt(rng.random())
                q = (r * math.cos(a), r * math.sin(a))
                dmin = min([math.hypot(q[0] - o[0], q[1] - o[1]) for o in pts] + [9.0])
                if dmin > best_d:
                    best, best_d = q, dmin
            pts.append(best)
        pts = np.array(pts)
        r = np.hypot(pts[:, 0], pts[:, 1])
        th = np.arctan2(pts[:, 1], pts[:, 0])
        # radius in the cap plane to the profile parameter
        sg = np.linspace(0, 1, 600)
        rg, _ = self.prof(sg)
        s = np.interp(r, rg[::-1], sg[::-1])
        rad = size * (0.85 + 0.3 * rng.random(len(th))) * (1.15 - 0.3 * r)
        return th, s, rad, rng.random((len(th), 2)) * 6.283

    def warts(self, count, size):
        rng = self.rng
        th = rng.random(count * 3) * 2 * np.pi
        s = 1 - 0.93 * np.sqrt(rng.random(count * 3))
        keep = rng.random(count * 3) < (0.55 + 0.45 * s)
        th, s = th[keep][:count], s[keep][:count]
        base = self.cap_point(th, s)
        n = self.normals(th, s, base)
        a = unit(np.cross(n, np.array([0.3, 0.9, 0.2])))
        b = np.cross(n, a)
        # enough samples that a wart is solid in the middle and only its fringe is speckled
        m = int(np.clip(1400 * size / 0.11, 400, 1400)) * K * K
        phi = rng.random((len(th), m)) * 2 * np.pi
        uu = rng.random((len(th), m))
        sz = (size * (0.6 + 0.9 * rng.random(len(th))) * (0.5 + 0.7 * s))[:, None]
        rad = sz * (1 - uu)
        pts = base[:, None, :] + a[:, None, :] * (rad * np.cos(phi))[..., None] + b[:, None, :] * (rad * np.sin(phi))[..., None] + n[:, None, :] * (sz * 0.9 * uu)[..., None]
        nn = unit(n[:, None, :] * 0.8 + a[:, None, :] * np.cos(phi)[..., None] + b[:, None, :] * np.sin(phi)[..., None])
        cnt = len(th) * m
        return pts.reshape(-1, 3), nn.reshape(-1, 3), np.full(cnt, 0.5), (0.35 + 0.4 * rng.random((len(th), m))).reshape(-1), np.full(cnt, 2, np.uint8)


def render(cap, tilt, yaw):
    """Splat the surface into a supersampled orthographic height buffer and class and attribute images."""
    S = SIZE * SS
    P, M = rot(cap.P, tilt, 0.0, yaw)
    N = cap.N @ M.T
    scale = CAP_R * SS
    ix = np.round(S / 2 + P[:, 0] * scale).astype(int)
    iy = np.round(S / 2 + P[:, 1] * scale).astype(int)
    ok = (ix >= 0) & (ix < S) & (iy >= 0) & (iy < S) 
    idx = (iy * S + ix)[ok]
    z, Nn, T, TN, C = P[ok, 2], N[ok], cap.T[ok], cap.TN[ok], cap.C[ok]
    o = np.lexsort((z, idx))
    idx_s = idx[o]
    last = np.r_[idx_s[1:] != idx_s[:-1], True]
    sel = o[last]
    pix = idx[sel]
    buf = {k: np.zeros(S * S, np.float32) for k in ("z", "nx", "ny", "t", "tn", "c", "has", "fr")}
    buf["z"][pix] = z[sel]
    buf["nx"][pix] = np.clip(Nn[sel, 0] / np.maximum(Nn[sel, 2], 0.2), -1.5, 1.5)
    buf["ny"][pix] = np.clip(Nn[sel, 1] / np.maximum(Nn[sel, 2], 0.2), -1.5, 1.5)
    buf["t"][pix] = T[sel]
    buf["tn"][pix] = TN[sel]
    buf["c"][pix] = C[sel]
    buf["has"][pix] = 1
    out = {k: v.reshape(S, S) for k, v in buf.items()}
    return out, (P, M, scale)


def fill_holes(b):
    """Close single pixel gaps between splatted samples by taking the nearest filled neighbour's values."""
    has = b["has"] > 0
    closed = np.asarray(Image.fromarray((has * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.MinFilter(3))) > 127
    miss = closed & ~has
    if miss.any():
        for k in ("z", "nx", "ny", "t", "tn", "c", "fr"):
            v = b[k].copy()
            # neighbour average of filled pixels
            w = has.astype(np.float32)
            num = np.zeros_like(v)
            den = np.zeros_like(v)
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    if dx or dy:
                        num += np.roll(np.roll(v * w, dy, 0), dx, 1)
                        den += np.roll(np.roll(w, dy, 0), dx, 1)
            avg = num / np.maximum(den, 1e-6)
            if k == "c":
                avg = np.round(avg)
            b[k] = np.where(miss, avg, v)
        b["has"] = np.where(miss, 1.0, b["has"])
    return b


def downsample(b):
    S = SIZE * SS
    has = b["has"].reshape(SIZE, SS, SIZE, SS)
    cnt = has.sum((1, 3))
    solid = cnt * 2 >= SS * SS
    out = {}
    w = has
    for k in ("z", "nx", "ny", "t", "tn", "fr"):
        out[k] = (b[k].reshape(SIZE, SS, SIZE, SS) * w).sum((1, 3)) / np.maximum(cnt, 1)
    # a class is wart if any covered sample is a wart in the majority
    c2 = ((b["c"].reshape(SIZE, SS, SIZE, SS) == 2) * w).sum((1, 3))
    out["c"] = np.where(solid, np.where(c2 * 2 > cnt, 2, 1), 0)
    # where wart samples only partly cover a pixel, the cap shows them as a white dusting, the speckle of the 3D look
    out["fr"] = np.clip(c2 / np.maximum(cnt, 1), 0, 1)
    out["solid"] = solid
    return out


def pack(d):
    solid = d["solid"]
    zmax = float(d["z"][solid].max()) if solid.any() else 1.0
    zpx = np.where(solid, np.clip(d["z"] / zmax * 0.97 + 0.03, 0.01, 1.0), 0.0)
    z16 = (zpx * 32767).astype(np.uint16)
    z16 = np.where(d["c"] == 2, z16 | 0x8000, z16).astype(np.uint16)
    z16 = np.where(solid, z16, 0).astype(np.uint16)
    nx = np.clip(np.round(d["nx"] * 80 + 128), 1, 255).astype(np.uint8)
    ny = np.clip(np.round(d["ny"] * 80 + 128), 1, 255).astype(np.uint8)
    # warts keep their grain in the tone; on the cap the tone is how much frost dusts it
    tone = np.clip(np.where(d["c"] == 2, d["tn"], d["fr"]) * 255, 0, 255).astype(np.uint8)
    depth = np.clip(d["t"] * 255, 0, 255).astype(np.uint8)
    rgba = np.stack([nx, ny, tone, depth], -1)
    rgba = np.where(solid[..., None], rgba, 0).astype(np.uint8)
    a = io.BytesIO()
    Image.fromarray(rgba, "RGBA").save(a, "PNG", optimize=True)
    b = io.BytesIO()
    Image.fromarray(z16.astype(np.uint16), "I;16").save(b, "PNG", optimize=True)
    return a.getvalue(), b.getvalue(), zmax


def main():
    out = bytearray()
    records = 0
    previews = []
    for name in KINDS:
        for v in range(5):
            cap = Cap(name, 100 + v * 17 + len(name))
            rng = np.random.default_rng(v + 3)
            tilt, yaw = math.radians(rng.uniform(0, 10)), rng.random() * 6.283
            b, info = render(cap, tilt, yaw)
            d = downsample(paint_flakes(fill_holes(b), cap, tilt, yaw, info))
            a, z, zmax = pack(d)
            nb = name.encode()
            out += struct.pack("<B", len(nb)) + nb + struct.pack("<BHf", v, SIZE, zmax) + struct.pack("<II", len(a), len(z)) + a + z
            records += 1
            print(name, v, len(a) + len(z), flush=True)
            previews.append(Image.fromarray((np.stack([d["t"] * 255] * 3, -1) * d["solid"][..., None]).astype(np.uint8)))
    dest = os.path.join(HERE, "..", "..", "crates", "forest-core", "assets")
    os.makedirs(dest, exist_ok=True)
    with open(os.path.join(dest, "mushrooms.bin" if K == 1 else f"mushrooms{K}x.bin"), "wb") as f:
        f.write(struct.pack("<I", records))
        f.write(out)
    print("wrote", records, "sprites,", (len(out) + 4) // 1024, "KB")



def paint_flakes(b, cap, tilt, yaw, info):
    """Draw each wart as a rough, granular cone with a few satellite grains around it, plus fine frost between."""
    if not getattr(cap, "flakes", None):
        return b
    S = SIZE * SS
    th, s, rad, ph = cap.flakes
    rng = np.random.default_rng(cap.seed + 99)
    scale = CAP_R * SS
    P, M = rot(cap.cap_point(th, s), tilt, 0.0, yaw)
    items = []
    for i in range(len(th)):
        cx, cy, r = S / 2 + P[i, 0] * scale, S / 2 + P[i, 1] * scale, rad[i] * scale
        items.append((cx, cy, r, 1.0))
        # satellite grains stuck to the rim of the wart
        for _ in range(int(rng.integers(1, 4))):
            a = rng.random() * 6.283
            dd = r * rng.uniform(0.75, 1.35)
            items.append((cx + math.cos(a) * dd, cy + math.sin(a) * dd, r * rng.uniform(0.18, 0.34), 0.7))
    # small spiky veil remnants along the rim, poking out past the edge
    spikes = []
    if cap.k.get("warts"):
        ns = 26
        st = np.linspace(0, 2 * np.pi, ns, endpoint=False) + rng.random(ns) * 0.15
        Ps, _ = rot(cap.cap_point(st, np.full(ns, 0.015)), tilt, 0.0, yaw)
        for i in range(ns):
            if rng.random() < 0.8:
                spikes.append((S / 2 + Ps[i, 0] * scale, S / 2 + Ps[i, 1] * scale, scale * rng.uniform(0.028, 0.045), 0.8, st[i]))
    # frost: clouds of fine grains around each wart and a light dusting elsewhere
    for i in range(len(th)):
        cx, cy, r = S / 2 + P[i, 0] * scale, S / 2 + P[i, 1] * scale, rad[i] * scale
        for _ in range(int(rng.integers(10, 22))):
            a = rng.random() * 6.283
            dd = abs(rng.normal(0, 1.4)) * r + r * 0.6
            items.append((cx + math.cos(a) * dd, cy + math.sin(a) * dd, scale * rng.uniform(0.006, 0.013), 0.5))
    fr = int(len(th) * 6)
    ft = rng.random(fr) * 6.283
    fs = 1 - rng.random(fr) * 0.95
    Pf, _ = rot(cap.cap_point(ft, fs), tilt, 0.0, yaw)
    for i in range(fr):
        items.append((S / 2 + Pf[i, 0] * scale, S / 2 + Pf[i, 1] * scale, scale * rng.uniform(0.005, 0.01), 0.5))
    # a granular noise field, at the size of grains on a wart
    yy_all, xx_all = np.mgrid[0:S, 0:S]
    n_fine = vnoise(xx_all / 2.2, yy_all / 2.2, cap.seed + 41)
    n_mid = vnoise(xx_all / 5.0, yy_all / 5.0, cap.seed + 42)
    grain = 0.6 * n_fine + 0.4 * n_mid
    for (cx, cy, r, amp, a0) in spikes:
        x0, x1 = int(max(cx - r * 2.5, 0)), int(min(cx + r * 2.5 + 1, S))
        y0, y1 = int(max(cy - r * 2.5, 0)), int(min(cy + r * 2.5 + 1, S))
        yy, xx = np.mgrid[y0:y1, x0:x1]
        dx, dy = xx - cx, yy - cy
        ca, sa = math.cos(a0 + yaw), math.sin(a0 + yaw)
        along, across = dx * ca + dy * sa, -dx * sa + dy * ca
        # a triangle pointing outward from the rim
        m = (along > -r) & (along < r * 1.8) & (np.abs(across) < r * (1.8 - along / r) / 2.8)
        sl = (slice(y0, y1), slice(x0, x1))
        zc = b["z"][sl]
        base_z = float(np.max(np.where(b["has"][sl] > 0, zc, 0)))
        b["z"][sl] = np.where(m, np.maximum(zc, base_z * 0.6 + 0.01), zc)
        b["c"][sl] = np.where(m, 2, b["c"][sl])
        b["tn"][sl] = np.where(m, 0.45, b["tn"][sl])
        b["nx"][sl] = np.where(m, ca * 0.6, b["nx"][sl])
        b["ny"][sl] = np.where(m, sa * 0.6, b["ny"][sl])
        b["has"][sl] = np.where(m, 1.0, b["has"][sl])
    for k, (cx, cy, r, amp) in enumerate(items):
        if amp == 0.5:
            # a frost grain: dusts the cap, partly covering a pixel once the sprite is shrunk
            x0, x1 = int(max(cx - r - 1, 0)), int(min(cx + r + 2, S))
            y0, y1 = int(max(cy - r - 1, 0)), int(min(cy + r + 2, S))
            if x1 > x0 and y1 > y0:
                yy, xx = np.mgrid[y0:y1, x0:x1]
                cov = np.clip(r + 0.5 - np.hypot(xx - cx, yy - cy), 0, 1)
                b["fr"][y0:y1, x0:x1] = np.maximum(b["fr"][y0:y1, x0:x1], cov)
            continue
        if r < 0.6:
            continue
        x0, x1 = int(max(cx - r * 1.4, 0)), int(min(cx + r * 1.4 + 1, S))
        y0, y1 = int(max(cy - r * 1.4, 0)), int(min(cy + r * 1.4 + 1, S))
        if x1 <= x0 or y1 <= y0:
            continue
        yy, xx = np.mgrid[y0:y1, x0:x1]
        dx, dy = (xx - cx) / r, (yy - cy) / r
        gn = grain[y0:y1, x0:x1]
        ang = np.arctan2(dy, dx)
        # a rough, slightly ragged outline
        d = np.hypot(dx, dy) / (1 + 0.16 * (gn - 0.5) * 2 + 0.05 * np.sin(3 * ang + k))
        m = (d < 1) & (b["has"][y0:y1, x0:x1] > 0)
        if not m.any():
            continue
        sl = (slice(y0, y1), slice(x0, x1))
        z = b["z"][sl]
        # a cone with a bumpy surface
        hgt = np.clip(1 - d * d, 0, 1) ** 0.55 * (0.85 + 0.3 * gn)
        b["z"][sl] = np.where(m, z + 0.012 + 0.9 * r / scale * hgt * amp, z)
        b["c"][sl] = np.where(m, 2, b["c"][sl])
        # grains vary in brightness, which reads as a rough, dusty surface
        b["tn"][sl] = np.where(m, 0.2 + 0.7 * gn, b["tn"][sl])
        ux, uy = dx / np.maximum(np.hypot(dx, dy), 1e-3), dy / np.maximum(np.hypot(dx, dy), 1e-3)
        bump = (gn - 0.5) * 0.6
        b["nx"][sl] = np.where(m, 0.2 * b["nx"][sl] + 0.8 * ux * np.clip(d * 1.1, 0, 1) + bump, b["nx"][sl])
        b["ny"][sl] = np.where(m, 0.2 * b["ny"][sl] + 0.8 * uy * np.clip(d * 1.1, 0, 1) + (n_mid[y0:y1, x0:x1] - 0.5) * 0.6, b["ny"][sl])
    return b


if __name__ == "__main__":
    main()
