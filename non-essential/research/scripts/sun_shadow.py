"""Cast shadows from a lidar canopy height model for a sun direction, and a fit of the sun direction to the photo."""
import numpy as np
import canopy_metrics as cm


def cast_shadow(h, az_deg, el_deg, px_m=0.5, steps=60, max_m=60.0):
    """1 where a pixel lies in shadow. h in metres on a grid of px_m metres; az is where the sun is, clockwise from
    north (up the image); the shadow falls the other way."""
    az, el = np.radians(az_deg), np.radians(el_deg)
    dx, dy = -np.sin(az), np.cos(az)  # toward the sun, in image x (right) and y (down = south)
    dy = -dy * -1
    sh = np.zeros_like(h, bool)
    t = np.tan(el)
    H, W = h.shape
    yy, xx = np.mgrid[0:H, 0:W]
    for k in range(1, steps + 1):
        d = max_m * k / steps
        sx = np.clip((xx + dx * d / px_m).round().astype(int), 0, W - 1)
        sy = np.clip((yy - np.cos(az) * d / px_m * -1 * -1).round().astype(int), 0, H - 1)
        sx = np.clip((xx + (np.sin(az)) * d / px_m).round().astype(int), 0, W - 1)
        sy = np.clip((yy - (np.cos(az)) * d / px_m).round().astype(int), 0, H - 1)
        sh |= h[sy, sx] > h + d * t
    return sh


def fit_sun(h, lum, px_m=0.5):
    """Sun azimuth and elevation whose cast shadows best explain where the photo is dark."""
    best = (-2, 0, 0)
    dark = (lum < np.percentile(lum, 35)).astype(float)
    for az in range(90, 271, 20):
        for el in (35, 45, 55, 65):
            s = cast_shadow(h, az, el, px_m, steps=30).astype(float)
            c = np.corrcoef(s.ravel(), dark.ravel())[0, 1]
            if c > best[0]:
                best = (c, az, el)
    return best
