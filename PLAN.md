# Tree-inator: Build Plan

> **Current state (2026-10-07).** The app now uses one tree system: generated Weber and Penn style trees (`crates/forest-core/src/wp.rs`). The CG2A texture bank, palette mode, dome crowns and leaf-dab texture described below were removed from the code after the generated trees replaced them. This document keeps the original analysis and plan for reference. The full earlier source is in `backups/tree-inator-before-cleanup-2026-10-07.zip`.


A desktop tool (Rust) that turns a drawn shape into a top-down forest canopy PNG in the style of CG2A forest assets, with layered colour grading, two edge styles, and a see-through overlay mode for tracing.

## 1. Reference analysis

Measured from the 15 supplied images (script: palette k-means, alpha stats, shadow cross-correlation).

### What the assets are

All files are RGBA with real transparency. The white seen in previews is empty alpha. There are two distinct families:

**A. Blurred-edge family (images 1, 2, 13, 14)**
- A dense, continuous canopy texture clipped by a shape mask.
- The mask edge is a wide alpha feather (roughly 5 to 10 percent of the shape width). The canopy colour stays the same while alpha drops; it does not fade to white or grey.
- No cast shadow outside the shape, no scattered outlier trees.
- Small dark clearings inside (bare ground, dark grey-brown, some tiny shrubs).
- Conclusion: this is "canopy texture + feathered mask". Easy to reproduce.

**B. Realistic-edge family (images 3 to 12, 15)**
- Looks like an orthographic top-down render of a 3D tree scatter (very likely Blender or similar).
- Single sun from the upper left. Shadows fall to the lower right, long and soft, and land on transparent ground as dark semi-transparent pixels.
- The edge breaks up: crowns get smaller and sparser, then single trees, then tiny "pebble" shrubs, each with its own drop shadow.
- Many have a ground haze/understory layer: grey-green (about #4a4d42 to #4c4f42) at roughly 25 to 35 percent alpha. It extends past the canopy in some (9, 10, 11, 15) and forms soft mist bands.
- Internal gaps show dark ground with shadow.
- Clumping: patches of a lighter sage/blue-green species sit inside darker olive canopy (clear in 1, 2, 10).

### Shadow measurements

Two tests were run on every image.

Test 1: outside shadow. For each of 8 directions, compare the alpha of semi-transparent pixels just past the canopy edge on one side versus the opposite side.
- Realistic family (3 to 13, 15): the dark spill is always toward the lower right (45 degrees in screen space, sun from the upper left). At 16 to 32 px from the edge the downwind side is 20 to 50 alpha units darker than the upwind side, so the shadow is long and soft.
- Blurred family (1, 2, 14): both sides equal within a few units. No cast shadow.

Test 2: inside shadow. Inside the opaque canopy, compare brightness at +14 px vs -14 px from the brightest crown pixels.
- Most realistic images (4, 6, 7, 8, 9, 11, 12, 13) show darkening to the lower right at cluster scale. Clusters cast shadows on whatever is below them.
- At leaf scale (6 px) the direction is weak and inconsistent (often lit from the right). The fine canopy texture has its own baked lighting that does not match the cluster shadows.

Close-up crops (images 7 and 10) show the structure directly:
1. A translucent grey ground haze.
2. A low undergrowth tier: small, fuzzy, dark clumps with soft alpha.
3. A tall crown tier: large lumpy clusters.
4. Each tier drops a soft offset shadow onto the tiers below it. The haze casts none.

Conclusion: the assets are built as stacked height tiers, each with its own drop shadow, rather than one physically lit 3D scene. This matches the observation that "each layer has shadows". The tool adopts this model (section 5.5).

### Colour data

Global canopy palette (dark to light, share of canopy pixels):

```
#101907 14%   deep shadow between crowns
#223010 19%   shaded crown
#324419 19%   mid crown
#415529 14%   lit crown, olive
#4c6416  5%   saturated yellow-green
#546c35 12%   lit crown, sage
#6a8441  9%   highlight
#85a256  3%   top highlight / lime
```

Notes for the renderer:
- About a third of canopy pixels are near-black green. Deep crevice darkening (ambient occlusion) is essential to the look.
- Highlights shift toward yellow-lime; shadows stay green, not blue-grey.
- Feature scale: luminance autocorrelation drops off at 10 to 18 px in the previews, so individual crown lumps are small relative to the map. Crowns are "cauliflower" clusters of many sub-blobs, not smooth domes.

### Full-resolution source files (`Base Assets/`)

Licence: free for personal use. Keep `Base Assets/` out of any public repo.

- `Tree Cluster 01` to `11`: realistic family. 580 px to 3894 px wide. 01 to 06 are the same files as the previews; 07 to 11 are larger (up to 3822 x 2665).
- `Zero Edge Fill 01` to `04`: blurred family. An 18 px empty margin, then a feather into an opaque canopy.
- Colour variants exist on Patreon but share these shapes, so colour is handled by grading, not more files.

Measured on Tree Cluster 11 by alpha band:
- alpha 255 (35 percent of pixels): canopy, mean #3e5426.
- alpha 160 to 224: haze and crown edges, mean #373d22.
- alpha 1 to 96: cast shadow, mean #1e180b. **Shadows are a dark warm brown, not black.**
- Scale at 1:1: single crowns are about 20 to 40 px across, clusters 100 to 150 px, edge "pebble" shrubs 15 to 25 px. The shadow offset is about 20 to 30 px.
- The canopy interior is photographic-quality aerial texture, far richer than the previews suggest.

### Prototype tests (`research/`)

The scripts are in `research/scripts`, and the results are in `research/results`.

1. **Strip and recast shadows on a CG2A file** (`01_shadow_strip_and_recast.png`). Removing baked shadows by alpha threshold works roughly, but leaves a dark rim on the lower right edges and drops the semi-transparent pebble trees. Recasting in a new direction looks fine. Verdict: do not reuse whole CG2A clusters. Use only their interior canopy texture.
2. **Image quilting** (`02_quilted_canopy.png`). Efros-Freeman quilting with min-error seams, 128 px patches and 32 px overlap, sampled from the opaque interior of Tree Cluster 11. It gives a seamless 1024 x 1024 canopy in 5.6 s in Python (Rust with rayon: well under 1 s). It looks like genuine CG2A canopy. Minor flaw: a faint grid rhythm in the bright patches. Fix it by jittering patch size and picking randomly among the top few matches instead of the single best.
3. **Procedural only** (`03_procedural_only.png`). Crowns as clusters of sub-domes, lit, AO, cast shadows, coloured by the measured palette. The shape, edge and shadows are right, but it reads as moss or broccoli: flat colour and no photographic leaf detail.
4. **Hybrid** (`04_hybrid.png`, `04_hybrid_crop.png`). Procedural crown heightfield for shape, edge, shading and shadows, multiplied over the quilted CG2A texture as albedo. **Clearly the best result, and close to the CG2A look.** Still missing in the prototype: pebble outliers, visible haze, tier separation. Those are already planned features.

5. **Layered ecology model** (`05_layered_model.png`, `05_layered_crop_gaps.png`, `05_layered_crop_fringe.png`, script `layers.py`). Haze, grass, shrubs, canopy and lone trees are each generated from their own noise against a shared "depth inside shape" field. Shadows cascade down through every layer. Result: distinct canopy patches with grass showing through, shrubs in the gaps, a soft grassy fringe, and lone trees with long shadows out in the open. What it taught:
   - Raw noise thresholds fail. Averaged noise has too small a range, so the canopy came out solid. Converting each noise field to a uniform rank (0 to 1) fixes this: "rank < coverage" gives an exact coverage percentage.
   - Crowns placed at a patch's edge spill over and fill the gaps. A crown is only placed where the blurred patch mask is above 0.8, so it fits inside its patch.
   - Grass by threshold gives a hard cut with a dark outline. Grass as thousands of tiny soft splats, with density fading at the fringe, gives the CG2A grassy edge.
   - Shadows cascade for free if each layer is shadowed by marching the combined top surface of all layers against that layer's own height.

Reference crop: `ref_cg2a_grass_and_lone_trees.png` (Tree Cluster 07 at 1:1) shows the target. From bottom to top: faint grainy haze, a speckled olive grass mat (#424d29, grain 2 to 9 px, mostly opaque with a ragged fringe), shrub clumps, canopy fragments, then lone trees and pebble shrubs out in the open.

### Patreon

Patreon blocks automated access (403, and the built-in browser refuses the domain), so the page was not inspected.

## 2. Approach

**Hybrid, decided by test 4 above.** It has two parts:

1. **Albedo from a texture bank.** On first run, the tool harvests the opaque interior of every file in `Base Assets/` (eroded away from edges, haze and shadows) into a texture bank. Image quilting then synthesizes seamless canopy of any size from the bank. This gives the photographic CG2A leaf detail for free.
2. **Shape, light and shadow procedurally.** Crowns are placed in stacked height tiers, and each tier's heightfield is stamped from sub-dome clusters. Shading, AO and tier drop shadows come from those heightfields and are multiplied over the albedo. The shape, edges, outliers, shadow direction and shadowless mode are all under control.

Fallbacks:
- **Palette mode**: no texture bank, albedo from the species gradient map (test 3). Useful for stylised looks or if no assets are present.
- **Texture fill** (blurred family): quilted canopy plus a feathered mask, no heightfield. Reproduces `Zero Edge Fill` exactly.

Why not reuse the CG2A clusters directly: their shadows and edges are baked in and cannot be cleanly removed (test 1), and their shapes are fixed.

## 3. Language and stack

**Rust.** Reasons: fast per-pixel CPU work with `rayon`, good immediate-mode GUI (`egui`), easy Win32 access for the overlay window, single static `.exe`.

Crates:
- `eframe` / `egui`: window, panels, canvas, widgets. Use the `glow` (OpenGL) backend; window transparency is more reliable on Windows than with wgpu.
- `rayon`: parallel pixel passes.
- `glam`: vector math.
- `fastnoise-lite`: fBm, cellular (Worley), domain warp. Seeded and deterministic.
- `tiny-skia`: anti-aliased polygon and stroke rasterization for masks.
- `image`: PNG/WebP read and PNG write.
- `serde`, `serde_json`: project files and presets.
- `rfd`: native file dialogs.
- `windows` (crate): Win32 calls for layered/click-through window.
- `global-hotkey`: hotkey to toggle click-through while the window is not focused.
- `rand`, `rand_pcg`: seeded RNG.
- Optional later: `wgpu` compute for the lighting passes if CPU becomes too slow at 8K.

## 4. Project layout

Cargo workspace:

```
tree-inator/
  Cargo.toml                 workspace
  crates/
    forest-core/             pure pipeline, no UI, no IO
      src/mask.rs            shapes to raster, density paint, SDF
      src/noise.rs           seeded noise helpers
      src/placement.rs       Poisson disk, species map, outliers
      src/texbank.rs         harvest canopy patches from Base Assets
      src/quilt.rs           image quilting synthesis
      src/crown.rs           crown height/albedo stamping
      src/brush.rs           dab brush engine, tree generator
      src/lighting.rs        normals, AO, shadows, shading
      src/layers.rs          layer buffers + colour grading
      src/compose.rs         blend layers to final RGBA
      src/pipeline.rs        staged, cached pipeline
      src/params.rs          all parameter structs (serde)
    forest-io/
      src/png.rs             PNG export, per-layer export
      src/psd.rs             minimal layered PSD writer
      src/project.rs         .treeproj (JSON + embedded density PNG)
    forest-cli/              headless render: project in, PNG out
    tree-inator/             the GUI app
      src/main.rs
      src/app.rs             state, undo stack, render thread
      src/canvas.rs          pan/zoom view, tool input
      src/tools/             lasso, polygon, brush, eraser, path
      src/panels/            shape, forest, edges, light, layers, export
      src/overlay.rs         Win32 transparency and click-through
  presets/                   JSON presets shipped with the app
  Base Assets/               CG2A source PNGs (personal licence, never committed)
  research/                  analysis scripts and prototype results
```

Keep `forest-core` free of UI so it can be tested with the CLI and golden images.

## 5. Generation pipeline

All stages are deterministic from `(project, seed)`. Each stage caches its output and a hash of its inputs, so changing a colour grade only reruns the last stage.

Units: the project has `px_per_cell` and `ft_per_cell` (default 5 ft). All tree sizes are entered in feet, so output matches battlemap grids.

### 5.1 Mask

- Shapes are vectors: closed polygons with modes `Add`, `Subtract`, `Clearing` (holes with ground), `Path` (open stroke with width, cuts a trail).
- Rasterize with `tiny-skia` into a float mask, then multiply by the density paint layer (a greyscale raster painted with a brush, 0 to 1).
- Compute a signed distance field (8SSEDT or two-pass Felzenszwalb, both O(n)). The SDF drives everything at the edge.
- Edge breakup: offset the SDF by fBm noise (amplitude and scale sliders). Gives lumpy, natural outlines instead of the smooth drawn curve.

### 5.2 Species map

- Each species has: crown diameter range (ft), height range, lumpiness, sub-blob count, a 3 to 5 stop colour gradient, weight, and a recolour (hue, saturation, value) applied to the texture bank albedo. Example: "Sage" is the bank texture shifted toward blue-green and lightened.
- Default species from the measured palette: "Olive broadleaf" (dark/mid), "Sage" (lighter blue-green, the clumps seen in 1, 2, 10), "Young growth" (small, lime-tinted), "Shrub" (tiny, for edges and pebbles).
- Assign species by Worley cells warped with noise, so species form clumps rather than salt and pepper. Slider: clump size.

### 5.3 Tree placement

- Bridson Poisson disk sampling with variable radius (radius comes from the species at that point).
- Acceptance probability = `density(sdf, paint)`:
  - Interior: near 1, reduced by a "gaps" noise threshold that creates small dark openings.
  - Edge band: falls off over `edge_width`.
- Size scaling near the edge: crowns shrink toward the outline (slider).
- Outliers (realistic mode only): beyond the outline, sample single trees with probability decaying with distance, plus a second pass of tiny shrubs ("pebbles"). Sliders: outlier count, max distance, pebble count.

### 5.3a Layer generation model (realistic layers)

Proven in test 5. This model replaces simple tier assignment, and it decides where every layer exists. It answers two needs: distinct patches where you see through to the layer below, and the grassy edges with lone trees.

**Shared field: depth `D`.** From the shape SDF: 0 at the drawn edge, 1 deep inside, negative outside, down to -1 at the outer limit. The edge is domain-warped by noise so outlines are lobed, not blob-like. The density paint brush adds to or subtracts from `D` directly.

**Per-layer coverage curves.** Each layer has:
- Its own noise field: domain-warped fBm with its own seed, scale and octaves. Optional Worley component for clumpy patches.
- The noise is converted to a **uniform rank** `u` (0 to 1) by histogram equalization. In Rust: sort a downsampled copy to build a CDF lookup, then apply it per pixel. This is what makes thresholds behave.
- A **coverage curve** `cover(D)`: an editable curve widget (like Photoshop Curves) mapping depth to coverage fraction.
- The layer is present where `u < cover(D)`. Coverage is therefore an exact percentage: canopy at 72 percent deep inside means 28 percent of the interior is guaranteed open, showing the layer below.

Default layers and curves, bottom to top:
1. `Haze`: cover rises from D -0.75 to -0.1. Alpha 0.26 with fine grain. Reaches farthest out.
2. `Grass`: cover rises from D -0.7 to 0. Rendered as splats (below).
3. `Shrub`: placed only on grass that is not under canopy (Poisson, 4 to 9 px radius at CG2A scale).
4. `Canopy`: cover 0 at the edge, up to 72 percent at D 0.45 and deeper. Large-scale noise (160 px base) so the openings are real patches, not speckle.
5. `Emergents`: a few tall crowns on canopy peaks (top 3 percent of the canopy noise), longest shadows.
6. `Lone trees` and `Pebbles` (below).

**Correlation slider** per layer pair: blends a shared noise into each layer's own noise before ranking. High correlation means shrubs hug canopy patch edges (forest margin). Low correlation means independent scatter.

**Patch-fitting rule.** A crown is only placed where the blurred patch mask (blur radius about one crown radius) is above 0.8. Without this, crowns at patch edges spill into and fill the gaps (seen in test 5, iteration 3).

**Clearing and gap types** (each optional, each with its own controls):
- Noise gaps: from the coverage curve, as above.
- Treefall gaps: random ellipses (1 to 3 crown diameters) cut from canopy only, so grass and shrubs show. Count slider.
- Drawn clearings: from the Clearing shape tool. They cut every layer above grass, or above haze if "bare ground" is ticked.

**Grassy edges.** Grass is not thresholded. Thresholding gave a hard cut with a dark outline. Instead:
- Density `g = smoothstep(cover_grass(D) - u_grass)` times a medium-scale noise.
- Scatter tiny dabs with probability proportional to `g` (2 to 5 px soft discs at CG2A scale). Accumulate them into a coverage buffer with a small blur, then `alpha = 1 - exp(-k * coverage)`. Dense inside, thinning to sparse specks at the fringe, exactly the stippled CG2A edge.
- Colour: base #424d29 with grain and light/dark speck jitter. Height about 0.4, so it receives shadows but barely casts any.
- Options: fringe width, speck size, density falloff, "tuft" mode (dabs elongated randomly for a rougher look).
- When `Base Assets` contains grass regions (the corner of Tree Cluster 07, the left of Tree Cluster 09), offer them as a second texture bank category `grass`. The user marks harvest rectangles on a source file with a harvest tool.

**Lone trees.** A Matern cluster process: parent points scattered in the fringe band (D -0.45 to 0.3, not under canopy), each with 1 to 3 children within about one crown diameter. This reproduces the small groups seen in CG2A, rather than evenly spaced singles.
- Sizes are bimodal: about half are full trees (8 to 15 px radius at preview scale), half are small shrubs (3 to 6 px).
- Height is taller than surrounding grass, so each casts a long distinct shadow (test 5).
- Sliders: groups per area, trees per group, spread, size mix, max distance from edge.

**Pebbles.** Tiny 15 to 25 px (native) round shrubs, scattered with density decaying away from the edge into the haze. Each casts a short tight shadow. They are the dots trailing off CG2A edges.

**Shadow cascade.** Each layer `k` is shadowed by marching the combined top surface of all layers toward the sun, against that layer's own height `H_k`. Any layer above automatically shadows every layer below: canopy on shrubs, shrubs on grass, lone trees on haze and ground. Each result is stored as that layer's shadow, so it can still be graded, hidden or exported per layer. This replaces the offset-alpha tier shadows as the default, because it handles any number of layers and any height mix correctly. Cost at 4K: one march per layer (6 marches of about 48 steps), about 2 to 4 s on CPU with rayon. The fallback is offset-alpha if preview speed needs it.

**Everything is editable after generation.** Every placed tree, shrub, lone tree and pebble is stored as a record (position, radius, height, layer, species, seed), not just pixels. The user can select, move, delete, resize or reroll individual trees with the Select tool, and the brush tools add records the same way. Rendering always rebuilds from the records.

### 5.4 Crown stamping

Buffers at working resolution: `height: f32`, `albedo_t: f32` (position on species gradient, palette mode only), `species: u8`, `tree_id: u32`.

Albedo in hybrid mode comes from one quilted canopy sheet per tier (section 5.6), recoloured per species and jittered per tree. Every crown samples the sheet at its own position, so neighbouring crowns never repeat the same pixels.

Per tree:
- One main ellipsoid dome plus N sub-blobs (6 to 20) placed on its upper surface with random offsets and radii. This makes the cauliflower shape.
- Add high-frequency leaf noise to height (two octaves, small amplitude).
- Write with max-blend on height, so neighbouring crowns interlock and form valleys.
- Per-tree random hue/value jitter stored with the tree.

Ground buffer: a base ground height of 0 under the mask, so clearings are flat and receive shadows.

### 5.5 Lighting and shadows (tier model)

Layers (tiers) come from section 5.3a: Haze, Grass, Shrub, Canopy, Emergents, Lone trees, Pebbles. Each is its own heightfield and its own layer.

Shading inside a tier:
- Normals from height (Sobel), "relief strength" slider.
- Diffuse: wrapped Lambert from the sun direction. Wrap slider softens it.
- Ambient occlusion: multi-scale, `ao = sum over radii of clamp(blur_r(height) - height)`, box blurs via summed-area table. Creates the near-black crevices that are a third of the reference palette.
- Colour, hybrid mode (default): `rgb = albedo * (base + diffuse_gain * diffuse) * (ao_floor + (1 - ao_floor) * ao)`. Prototype values: base 0.62, diffuse gain 0.55, AO floor 0.55.
- Colour, palette mode: `albedo = species_gradient(albedo_t + light_term * shift)`. Lit areas move toward lime, occluded areas toward deep green, so shadows stay green instead of grey.
- Default cast shadow colour: #1e180b (measured warm brown), not black.

Shadows between tiers: default is the cascade march in section 5.3a. Fast fallback for previews:
- Each tier's alpha is offset along the shadow direction by `length = tier_height * shadow_length_scale`, blurred by `softness` (blur grows with length), tinted, and multiplied onto every tier below it plus the ground.
- This is cheap (one offset plus one blur per tier) and matches the measured look exactly: long soft lower-right shadows, each layer shadowing what is under it.
- The shadow of each tier is stored as its own layer (`Canopy shadow`, `Undergrowth shadow`), so it can be graded, hidden or exported separately.

Interactive preview uses the fallback at 1/4 scale. Final renders use the cascade march.

### 5.5a Shadow controls

- **Direction**: a 360 degree dial on the Light panel plus quick buttons (upper left, upper right, lower left, lower right, top). Default 315 degrees (light from upper left), matching CG2A. Crown shading and cast shadows always use the same angle, so they never disagree.
- **Length, softness, opacity, colour** sliders, global and per tier.
- **Shadowless**: one toggle disables all cast shadows. A second option, "Flat light", also removes directional crown shading and keeps only AO, so the asset shows no light direction at all. Useful when the target map uses a different light angle or none.
- **Per-tier shadow on/off**: for example keep undergrowth shadows but drop canopy ones.
- **Export variants**: one click writes `name.png` (with shadows), `name_noshadow.png`, and `name_shadow.png` (shadow only), so the shadow can be placed on its own layer in Dungeondraft or Photoshop.
- **Match map light**: eyedropper-style tool. Drag an arrow over a reference image or the traced map in overlay mode to set the direction.

### 5.5b Baked light in the texture bank

The CG2A canopy carries weak baked light at leaf scale (test 2 in section 1). In the hybrid prototype it did not clash with the procedural light. If it does once the shadow direction is rotated far from 315 degrees, enable "neutralize texture light": flatten the bank with a large-radius high-pass (keep detail, drop broad shading) before quilting. The procedural shading then supplies all the directional light.

### 5.6 Texture bank and quilting

Harvest (runs once, cached to `%APPDATA%/tree-inator/bank/`):
- For each PNG in `Base Assets/`, take the pixels with alpha 255, eroded by 64 px so no edge, haze or cast shadow gets in.
- Store a list of valid patch corners per source. Tag each source by style: `cluster` (Tree Cluster files) or `fill` (Zero Edge Fill files, lighter and more sage).
- The user can add any other canopy PNG to the bank, with a per-source on/off switch and weight.

Synthesis (Efros-Freeman image quilting):
- Raster scan of patches (default 128 px, overlap 32 px, scaled with output resolution).
- For each slot, test about 400 random candidate patches by SSD over the overlap, then pick randomly among the best 5 (removes the grid rhythm seen in test 2).
- Jitter patch size by plus or minus 20 percent.
- Join with a minimum-error boundary cut (dynamic programming seam), as in the prototype.
- Run per tile in parallel with rayon. Cache the sheet by seed and bank settings, so moving a shape does not resynthesize.
- Optional: a guide channel so quilting prefers bright patches where the species map says "Sage" and darker ones elsewhere.

Recolour: per-species HSV shift plus optional Reinhard colour transfer (match mean and deviation in Lab) toward a target palette or a reference image. This gives the CG2A colour variants and autumn or dead looks from the same bank.

### 5.6a Brush engine and single-tree generator

There are no single-tree cutouts to harvest cleanly, so trees are built with Photoshop-style dab brushes that write into the tier buffers (height, albedo source, alpha), never into flat pixels. Everything painted gets lit and shadowed like generated trees.

Brush parameters (same set as a Photoshop brush tip):
- Tip: procedural dome, cauliflower (main dome plus sub-domes), or a harvested leaf dab (a small round patch cut from the texture bank).
- Size, size jitter, spacing, scatter, count per dab, angle jitter, roundness.
- Height (which tier, and how tall), opacity/flow, hue/value jitter.
- Pressure maps to size and height if a pen tablet is used (winit reports pen pressure on Windows).

Brushes shipped as presets:
- **Crown**: one click makes one full tree: a main dome plus 6 to 20 sub-domes, albedo sampled from the bank at that spot.
- **Canopy fill**: large scatter of crowns. Hand painting of what the generator does.
- **Undergrowth**: small soft dabs on the low tier.
- **Pebbles**: 15 to 25 px round shrubs with tight shadows, matching the CG2A edge shrubs.
- **Haze**: soft noise brush on the haze layer.
- **Eraser and Thin out**: remove dabs, or reduce density so ground shows through.

Single-tree generator (a panel):
- Builds one tree from the Crown brush recipe with sliders for diameter (ft), lumpiness, sub-dome count, height, species and seed.
- Preview with live shadow direction.
- "Generate sheet": N variations exported as individual transparent PNGs, with and without shadow, sized to the grid. These are the single-tree cutouts for other map tools.
- Generated trees can be saved as brush tips, so a hand-made tree becomes a stamp.

### 5.6b Imported crowns (optional)

Load a PNG of a single tree crown. Estimate height from luminance plus a dome prior, keep its colours as albedo. It then stamps like a procedural crown and gets relit.

### 5.7 Layers

Each is its own RGBA buffer, gradable and exportable. Bottom to top:

1. `Haze`: grey-green base (#373d22 measured at full resolution, #4b4e42 in the previews), noise-modulated alpha around 30 percent, mask dilated by a slider so it can spill past the canopy (references 9 to 11, 15).
2. `Ground shadow`: shadows on bare ground and haze.
3. `Clearings`: dark ground in drawn clearings and treefall gaps.
4. `Grass` and `Grass shadow`
5. `Shrub` and `Shrub shadow`
6. `Canopy` (one sub-layer per species, so "Sage" can be graded separately from "Olive") and `Canopy shadow`
7. `Emergents` and `Emergents shadow` (if enabled)
8. `Lone trees` and `Pebbles`, each with its shadow
9. `Highlights`: the top-lit component split out so it can be pushed or tamed.
10. `Mist`: optional wisps over the canopy (fBm, white-grey, low alpha), as seen in 3 and 4.

Each `X shadow` layer is the shadow falling onto layer X from everything above it (section 5.3a).

### 5.8 Edge modes

**Blurred:**
- Final alpha = canopy alpha times `smoothstep(-feather, 0, sdf)` with noise added to the SDF so the feather is uneven.
- Lone trees, pebbles, ground shadow and haze spill are disabled. Grass is optional.
- Optional inner darkening near the edge (slider) for blending into neighbouring patches.
- Designed so two overlapping blurred patches merge without a seam.

**Realistic:**
- Alpha from crowns only, plus outliers, pebbles, cast shadow and haze spill.
- Edge breakup noise on.

Both modes are presets over the same parameters, so a user can mix them (for example a realistic edge with a little feather).

### 5.9 Compose and export

- Composite layers bottom to top with per-layer blend mode (normal, multiply, screen, overlay, soft light) and opacity.
- Export: final PNG (straight alpha), each layer as separate PNG, or a layered PSD.
- Supersampling: render at 2x and downsample on export for crisp leaf detail.
- Crop to content with a margin, or keep full canvas size (needed for overlay alignment, section 7).

## 6. Colour grading (per layer and global)

Each layer has a grade stack applied in this order:
- Exposure, contrast, gamma
- Levels (black, white, mid)
- Hue shift, saturation, vibrance, lightness
- Temperature/tint
- Shadow/mid/highlight tint (three colour wheels, the main tool for "darker and moodier")
- Gradient map override (replace the species gradient with a custom one)
- Opacity, blend mode

Global extras: overall darkness, "AO strength", "shadow strength", "shadow length", "shadow softness", "highlight lime shift".

Presets: "CG2A default", "Dark forest", "Autumn", "Dead/blighted", "Swamp", "Snowy", "Night". Presets are just JSON files of parameters.

Eyedropper: sample colours from a loaded reference image straight into a species gradient. "Match reference": k-means the canopy pixels of a reference image (same method used in section 1) and fill the species gradients automatically.

## 7. Overlay mode (tracing over other software)

Goal: see Dungeondraft/Photoshop/Inkarnate through the window, draw the shape over it, and get an export that lines up.

Implementation on Windows:
- Window opacity: add `WS_EX_LAYERED` and call `SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA)`. Whole-window opacity is the most reliable option and still accepts mouse input. Slider 20 to 100 percent.
- Always on top: `ViewportCommand::WindowLevel(AlwaysOnTop)`.
- Click-through toggle: add `WS_EX_TRANSPARENT` so clicks reach the app underneath. Toggled by a global hotkey (default Ctrl+Shift+T) via `global-hotkey`, because the window cannot be clicked while it is click-through.
- Minimal chrome: in overlay mode hide panels, show only the canvas and a small floating toolbar.
- Get HWND from `eframe` via `raw-window-handle`.

Alignment:
- "Calibrate" tool: drag a rectangle over the target map's canvas in the other app and enter its pixel size (for example 4200 x 2800). The tool maps screen space to map pixels.
- Shapes are stored in map pixels, not screen pixels. Export at the calibrated size so the PNG drops in at 0,0 with no manual positioning.
- Option to export at full canvas size (transparent outside the forest) or cropped with the offset written to a sidecar `.json`.

## 8. GUI layout

- Centre: canvas with pan (middle mouse/space), zoom (wheel), grid overlay, checkerboard background, optional reference image underlay.
- Left toolbar: Lasso, Polygon, Smooth freehand, Density brush, Tree brushes (Crown, Canopy fill, Undergrowth, Pebbles, Haze, Thin out), Eraser, Clearing, Path cutter, Select/move shape, Calibrate.
- Right panel tabs: Shape, Forest (species, density, placement), Texture bank, Brushes, Tree generator, Edges, Light, Layers (list with visibility, solo, grade stack), Export.
- Bottom bar: seed (with reroll button), preview resolution, render time, progress.
- Shortcuts: Ctrl+Z / Ctrl+Y undo/redo, R reroll, O overlay, H hide UI, F frame all.

Rendering thread:
- UI sends a `RenderRequest { project_snapshot, generation }` over a channel.
- Worker renders at preview scale (1/4) first, then refines to 1/2 and full when idle. Stale generations are cancelled with an atomic flag checked between rows.
- Results come back as an `egui::TextureHandle`.

Undo: snapshot-based command stack of shape and parameter edits. Density paint uses tile-based diffs (64x64 tiles) to keep memory low.

## 9. Asset handling

- `Base Assets/` holds the 15 CG2A PNGs. Licence: free for personal use, which this tool is. Never commit them or ship them inside the `.exe`. The app reads them from a folder chosen in settings.
- Scale: CG2A crowns are 20 to 40 px at native resolution. Add a "bank scale" setting (default 1.0 = native). When the map's `px_per_cell` implies a different crown size in pixels, the bank is resampled before quilting, so leaf detail stays in proportion with the crown sizes in feet.
- Without the folder the app still works in palette mode (procedural albedo).

## 10. Milestones

Each milestone ends with something runnable.

**M0. Setup (half a day)**
- Workspace, crates, CI-free local build. `cargo run -p tree-inator` opens an empty egui window.
- `forest-cli` takes a JSON project and writes a PNG.

**M1. Mask, depth field and layer placement (3 to 4 days)**
- Polygon to mask, SDF, depth field `D` with warped edge.
- Noise to uniform rank, coverage curves, correlation, patch-fitting rule.
- Placement for every layer: Poisson for canopy and shrubs, Matern clusters for lone trees, decaying scatter for pebbles, splat density for grass.
- Tree records (position, radius, height, layer, species, seed).
- Debug output: dots coloured by layer over the mask, plus a coverage report per layer.

**M2. Texture bank and quilting (2 to 3 days)**
- Harvest from `Base Assets/`, cache, quilting with top-k pick and size jitter, per-species recolour.
- CLI command `forest-cli quilt` writes a sheet. Compare against `research/results/02_quilted_canopy.png`; the grid rhythm must be gone.

**M3. Crowns and lighting, look-dev (4 to 6 days, the core of the project)**
- Port `research/scripts/layers.py` to Rust as the starting point. It contains the full layered model: depth field, rank coverage, crown stamping, grass splats, lone tree clusters, shading, AO, cascade shadows and the hybrid compose. `proto.py` is the simpler single-layer version.
- Crown stamping per layer, normals, diffuse, AO, hybrid and palette colouring.
- Cascade shadows plus the fast preview fallback, with direction, length, softness, brown default tint, and the shadowless and flat-light switches.
- Side-by-side compare in the CLI: render next to a CG2A file, plus a palette histogram comparison against the numbers in section 1.
- Target: beat `research/results/05_layered_model.png`. Known gaps to close: the grass fringe should be wider and more stippled, the haze needs CG2A's fine grain, stray specks far from the shape need a distance cutoff, and crowns need more lumpiness.

**M4. GUI canvas and shape tools (3 to 4 days)**
- Pan/zoom, lasso, polygon, smoothing (Chaikin), add/subtract/clearing, density brush, undo.
- Background render thread with progressive preview.

**M5. Edge modes and extra layers (2 to 3 days)**
- Blurred and realistic modes, mist, drawn clearings, treefall gaps, path cutter.
- Coverage curve editor widget and correlation sliders in the GUI.
- Texture fill path for the blurred family.

**M6. Brush engine and tree generator (3 to 4 days)**
- Dab engine writing into tier buffers, brush presets, pen pressure.
- Single-tree generator panel and sprite sheet export.

**M7. Layers and grading (2 to 3 days)**
- Layer panel, per-layer grade stack, blend modes, presets, eyedropper, match-reference, Reinhard colour transfer.

**M8. Overlay mode (1 to 2 days)**
- Layered window opacity, always on top, click-through hotkey, calibrate tool, aligned export.

**M9. Export and project files (1 to 2 days)**
- PNG, shadow variants, per-layer PNG, PSD writer (uncompressed RGBA layers, about 300 lines following the Adobe PSD spec), `.treeproj` save/load. Brush strokes are saved as vector dab lists, so projects stay small and re-render at any resolution.

**M10. Polish and performance (ongoing)**
- Profile at 8K. If lighting or quilting is too slow, move them to `wgpu` compute shaders.
- Imported crowns, seasonal presets.

## 11. Performance targets

- Preview (1/4 scale of a 4K map): under 300 ms after an edit.
- Full 4096 x 4096 render: under 5 s on a modern 8-core CPU.
- Grade-only changes: under 50 ms (only the compose stage reruns).

Rough cost: preview shadows (offset-alpha) are well under 1 s at 4K. The cascade march for final renders is about 2 to 4 s at 4K. AO with summed-area tables is constant per pixel. Python prototype for reference: the whole layered model at 1280 x 900 in about 4 s, single-threaded. Crown stamping scales with tree count (tens of thousands), fine on CPU.

## 12. Testing

- Unit tests in `forest-core`: SDF correctness on simple shapes, Poisson minimum spacing, deterministic output for a fixed seed.
- Golden images: `forest-cli` renders a set of test projects; compare against stored PNGs with a tolerance. Catches accidental look changes.
- Palette check: script compares canopy k-means of the output with the reference palette from section 1.

## 13. Risks

- **Fidelity gap vs a true 3D render.** A heightfield cannot do overhanging foliage or true volumetric haze. Mitigation: the texture bank supplies the photographic detail, and AO does most of the rest (test 4). The mist layer covers haze.
- **Visible repetition from a small bank.** Only about 15 source images. Mitigation: top-k random quilting, patch size jitter, per-tree recolour, and allowing any extra canopy PNG into the bank.
- **Texture scale mismatch.** If map resolution differs a lot from CG2A native, resampled leaf detail gets blurry or too fine. Mitigation: bank scale setting, and palette mode detail noise blended in when upscaling past 1.5x.
- **Window transparency on Windows with GPU backends.** Mitigation: use whole-window `LWA_ALPHA` opacity rather than per-pixel transparency.
- **Large canvases.** 8K float buffers for several layers is several hundred MB. Mitigation: tile the pipeline (512 px tiles with an apron for shadows and AO) if memory becomes an issue.
