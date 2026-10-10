# Renders every feature through forest-cli for the demo page.
import json, math, random, subprocess, os
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.abspath(__file__))
CLI = os.path.join(ROOT, "..", "target", "release", "forest-cli.exe")
J, I = os.path.join(ROOT, "json"), os.path.join(ROOT, "img")
os.makedirs(J, exist_ok=True)
os.makedirs(I, exist_ok=True)
MW, MH = 2048, 1536


def blob(seed, scale=1.0, cx=0.5, cy=0.5):
    """Same outline generator as the app's Generate patch button."""
    rnd = random.Random(seed)
    r = min(MW, MH) * 0.3 * scale
    st = rnd.uniform(1.2, 1.8)
    rot = rnd.uniform(0, math.pi)
    hs = [(rnd.uniform(.1, .3) / (k * .6), rnd.uniform(0, 6.28)) for k in range(2, 7)]
    pts = []
    for i in range(128):
        t = i / 128 * 6.283
        rr = 1 + sum(a * math.cos((k + 2) * t + ph) for k, (a, ph) in enumerate(hs))
        x, y = math.cos(t) * r * rr * st, math.sin(t) * r * rr
        pts.append([x * math.cos(rot) - y * math.sin(rot), x * math.sin(rot) + y * math.cos(rot)])
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    k = min(0.8 * MW * scale / (max(xs) - min(xs)), 0.8 * MH * scale / (max(ys) - min(ys)), 1)
    bx, by = (max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2
    return [[MW * cx + (p[0] - bx) * k, MH * cy + (p[1] - by) * k] for p in pts]


BASE_SHAPE = blob(11)


def proj(gen=None, light=None, grading=None, shapes=None):
    p = {"shapes": shapes if shapes is not None else [{"op": "Add", "points": BASE_SHAPE}],
         "gen": {"seed": 7, **(gen or {})}}
    if light:
        p["light"] = light
    if grading:
        p["grading"] = grading
    return p


def grade(layers=None, glob=None, **kw):
    g = {"layers": [dict(l) for l in (layers or [{}] * 7)], **kw}
    if glob:
        g["global"] = glob
    return g


def solo(i):
    return grade([{"visible": j == i} for j in range(7)])


renders = {}


def add(name, p, scale=0.4, bg=True):
    renders[name] = (p, scale, bg)


# Drawing
add("draw_plain", proj(gen={"pattern": "Plain"}))
hole = [[MW * .47 + math.cos(t / 20 * 6.283) * 170, MH * .5 + math.sin(t / 20 * 6.283) * 120] for t in range(20)]
add("draw_cut", proj(gen={"pattern": "Plain"},
                     shapes=[{"op": "Add", "points": BASE_SHAPE}, {"op": "Subtract", "points": hole}]))
for s in (21, 22, 23):
    add(f"gen_{s}", proj(shapes=[{"op": "Add", "points": blob(s)}]))
# Patterns
for pat in ["Plain", "Fragmented", "Branching", "Clumps", "Lobed"]:
    add(f"pat_{pat}", proj(gen={"pattern": pat, "pattern_strength": 0.9 if pat == "Branching" else 0.6}))
add("pat_gaps", proj(gen={"pattern": "Plain", "gaps": 1.0, "gap_size": 60, "canopy_cover": 0.95}))
# Presets
PRESETS = {
    "CG2A dense forest": dict(pattern="Fragmented", pattern_strength=.5, pattern_scale=300, canopy_cover=.88, gaps=.2),
    "Fragmented woodland": dict(pattern="Fragmented", pattern_strength=.9, pattern_scale=260, canopy_cover=.75, gaps=.3, interior_lone=.1),
    "Riparian / branching": dict(pattern="Branching", pattern_strength=.9, pattern_scale=350, branch_width=.35, canopy_cover=.85, interior_lone=.15),
    "Open woodland": dict(pattern="Clumps", pattern_strength=.8, pattern_scale=260, canopy_cover=.6, shrub_density=1.5, interior_lone=.4),
    "Savanna": dict(pattern="Clumps", pattern_strength=1, pattern_scale=120, canopy_cover=.22, interior_lone=1, lone_groups=3, grass_reach=1),
    "Old growth with gaps": dict(pattern="Lobed", pattern_strength=.5, pattern_scale=400, canopy_cover=.92, gaps=.8, gap_size=40),
}
for i, (k, g) in enumerate(PRESETS.items()):
    add(f"preset_{i}", proj(gen=g))
# Edge modes
add("edge_realistic", proj())
add("edge_blurred", proj(gen={"edge_mode": "Blurred"}))
add("edge_blurred_soft", proj(gen={"edge_mode": "Blurred", "feather": 0.9}))
two = [{"op": "Add", "points": blob(31, 0.6, 0.35, 0.45)}, {"op": "Add", "points": blob(32, 0.6, 0.65, 0.55)}]
add("edge_blurred_merge", proj(gen={"edge_mode": "Blurred"}, shapes=two))
# Layers
NAMES = ["Haze", "Ground shadow", "Grass", "Shrubs", "Canopy", "Lone trees", "Pebbles"]
for i in range(7):
    add(f"layer_{i}", proj(grading=solo(i)))
add("layer_all", proj())
# Light and shadow
for a in (45, 135, 225, 315):
    add(f"light_dir_{a}", proj(light={"shadow_angle": a}))
add("light_len_short", proj(light={"shadow_length": 0.4}))
add("light_len_long", proj(light={"shadow_length": 2.5}))
add("light_soft_0", proj(light={"shadow_softness": 0.0}))
add("light_soft_3", proj(light={"shadow_softness": 3.0}))
add("light_none", proj(light={"shadows": False}))
add("light_flat", proj(light={"shadows": False, "flat": True}))
add("light_shadow_only", proj(grading=solo(1)))
add("light_elev_low", proj(light={"elevation": 15}))
# Colour grading
add("grade_default", proj())
add("grade_autumn", proj(grading=grade([{}, {}, {"hue": 15, "saturation": 0.8}, {"hue": 45, "saturation": 1.3},
                                        {"hue": 55, "saturation": 1.4, "brightness": 1.1}, {"hue": 40, "saturation": 1.3}, {}])))
add("grade_dark", proj(grading=grade(glob={"brightness": 0.7, "contrast": 1.25, "saturation": 0.85})))
add("grade_blight", proj(grading=grade(glob={"saturation": 0.25, "tint": [1.05, 0.95, 0.8], "brightness": 0.9})))
add("grade_sage", proj(grading=grade([{}, {}, {}, {}, {"tint": [0.85, 1.0, 1.05], "brightness": 1.15, "saturation": 0.75}, {}, {}])))
add("grade_night", proj(grading=grade(glob={"tint": [0.6, 0.75, 1.0], "brightness": 0.55, "saturation": 0.6}, shadow_opacity=0.85)))
# Canopy controls
add("can_cover_50", proj(gen={"canopy_cover": 0.5}))
add("can_cover_95", proj(gen={"canopy_cover": 0.97}))
add("can_crown_small", proj(gen={"crown_min": 5, "crown_max": 9}))
add("can_crown_big", proj(gen={"crown_min": 18, "crown_max": 30}))
add("can_lumps_low", proj(gen={"crown_lumps": 0.3}), scale=1.0)
add("can_lumps_high", proj(gen={"crown_lumps": 2.5}), scale=1.0)
# Undergrowth
add("under_grass_0", proj(gen={"grass_reach": 0.0, "shrub_density": 0}))
add("under_grass_1", proj(gen={"grass_reach": 1.0, "grass_density": 1.5}))
add("under_lone_0", proj(gen={"lone_groups": 0, "pebbles": 0}))
add("under_lone_4", proj(gen={"lone_groups": 4, "pebbles": 4, "lone_reach": 0.9}))
add("under_haze_0", proj(gen={"haze_opacity": 0}))
add("under_haze_6", proj(gen={"haze_opacity": 0.6, "haze_reach": 1.0}))
# Texture bank vs palette, 1:1 detail
add("tex_bank", proj(), scale=1.0)
add("tex_palette", proj(gen={"use_texture_bank": False}), scale=1.0)
# Transparent export at full resolution
add("export_full", proj(), scale=1.0, bg=False)


def run(name):
    p, scale, bg = renders[name]
    jp = os.path.join(J, name + ".json")
    json.dump(p, open(jp, "w"))
    out = os.path.join(J, name + ".png")
    args = [CLI, jp, "-scale", str(scale), "-out", out] + (["-bg"] if bg else [])
    subprocess.run(args, check=True, capture_output=True, cwd=os.path.join(ROOT, ".."))
    return out


def save_jpg(img, name, q=84):
    img.convert("RGB").save(os.path.join(I, name + ".jpg"), quality=q, optimize=True)


xs = [x for x, _ in BASE_SHAPE]
ys = [y for _, y in BASE_SHAPE]
for name, (p, scale, bg) in renders.items():
    im = Image.open(run(name))
    if name.startswith("draw_") or name.startswith("gen_"):
        d = ImageDraw.Draw(im)
        for sh in p["shapes"]:
            pts = [(x * scale, y * scale) for x, y in sh["points"]]
            col = (40, 140, 40) if sh["op"] == "Add" else (200, 50, 50)
            d.line(pts + [pts[0]], fill=col, width=2)
    if name == "export_full":
        sm = im.resize((im.width * 4 // 10, im.height * 4 // 10), Image.LANCZOS)
        chk = Image.new("RGBA", sm.size)
        cd = ImageDraw.Draw(chk)
        for y in range(0, sm.height, 16):
            for x in range(0, sm.width, 16):
                c = (205, 205, 205, 255) if (x // 16 + y // 16) % 2 else (245, 245, 245, 255)
                cd.rectangle([x, y, x + 15, y + 15], fill=c)
        chk.alpha_composite(sm)
        save_jpg(chk, "export_checker")
        nz = Image.effect_noise(sm.size, 24).convert("L")
        ground = Image.composite(Image.new("RGBA", sm.size, (92, 84, 62, 255)),
                                 Image.new("RGBA", sm.size, (110, 100, 74, 255)), nz)
        ground.alpha_composite(sm)
        save_jpg(ground, "export_ground")
        continue
    if name.startswith("under_"):
        # Edge detail: lower half of the patch, where grass, lone trees and haze sit.
        w, h = im.size
        im = im.crop((int(w * 0.18), int(h * 0.45), int(w * 0.82), int(h * 0.98)))
    if scale == 1.0:
        # 1:1 detail crop at the patch edge so leaf-level texture is visible.
        cx, cy = int(min(xs) + 160), int((min(ys) + max(ys)) / 2)
        im = im.crop((cx - 250, cy - 230, cx + 450, cy + 230))
    save_jpg(im, name)
    print(name)
json.dump({"presets": list(PRESETS), "layers": NAMES}, open(os.path.join(J, "names.json"), "w"))
