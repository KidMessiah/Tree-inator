# Tree-inator

Draw a shape, get a top-down forest PNG made of generated trees. Design notes are in [PLAN.md](PLAN.md).

## Run

```
cargo run --release -p tree-inator
```

The built app is `target/release/tree-inator.exe`. Close it before rebuilding, since Windows will not replace a running exe.

## Using it

- **Lasso: add** drags a loop and the forest grows when you let go. **Lasso: cut clearing** and the **Polygon** tools cut holes or make straight edges (Enter or right click closes).
- Middle drag or Space+drag pans, the wheel zooms, F fits the map, H hides the panels.
- Ctrl+Z / Ctrl+Y undo and redo shapes, painting and erasing. R rerolls the seed.
- **Map size** (left toolbar): the pixel size of the map you will place the forest on. Exports use map pixels, so a PNG drops in at 0,0.

The right panel has six tabs.

**Forest**
- **Patch shape**: *Generate patch* makes a random natural outline, *Fill map* covers the whole map. *Preset* sets a whole look at once (CG2A dense forest, Fragmented woodland, Riparian / branching, Open woodland, Savanna, Old growth with gaps). *Pattern* reshapes any outline: fragmented edge, branching fingers, clumps, lobed.
- **Patch shape, containment**: *Fit the whole forest inside the drawn area* (on by default) pulls the generation in so the fall-off, grass fringe and stray trees all end before the line. The pull-in is measured and varies along the outline. *Pull in* scales it (100% fits exactly, lower reaches closer to the line, higher leaves a margin) and the panel shows the pixels pulled in. *Trim anything still crossing the line* is a safety net and *Keep shadows inside too* can be turned off to let shadows fall outside. Painted trees are exempt.
- **Size**: *Forest scale* from 10% to 400% makes every tree, leaf, shrub and shadow bigger or smaller on the same drawn area, for different elevations.
- **Edges**: seed, realistic or blurred edges, feather, outline softness, warp and noise.
- **Canopy**: coverage, patch size, edge thinning, crown size, treefall gaps.
- **Undergrowth and fringe**: grass, shrubs, lone trees, pebbles, haze.

**Trees**
- **Forest type**: what the generated forest is made of. *Canopy and lone trees* sets the main species, *Shrubs and pebbles* the undergrowth, and *Mix a second species* blends another type in as clumps.
- **Brush**: paint, place or erase individual trees, by species and layer. Painted trees use the brush species.
- **Species editor**: each species has a branching form (oak, spruce, sage, bush, dead, birch, pine, willow, cypress, acacia), branchiness, clump size, leaf cover, height, leaf size and shape, and four leaf colours. Duplicate one to make your own.

**Look**
- **Leaf colour**: colour variety, mottling, sun-dappled tops, per-tree lean.
- **Canopy depth and clusters**: patch tone, crown shading, crown height variation, crown to crown difference, brightness, shadow lift. *CG2A match* sets values tuned to CG2A's brightness spread.
- **Layers and colour grading**: show or hide and grade each layer, plus a grade for the whole forest.

**Light**: shadow direction dial, sun height, length, softness, opacity and colour, shadows on or off, flat light.

**Addons**: extras laid over the forest. *Snow* has looks (dusting, frosted, blanket, outer edge, snow line) and controls for amount, where it lies, how much clings to crown tops, ground cover, patchiness and colour.

**Output**: export scale and variants (with shadows, `_noshadow`, `_shadow`), preview quality, **My defaults** (save your settings so the app starts with them), and **Overlay** mode for tracing over another app (Ctrl+Shift+T toggles click-through from anywhere). *Anchor to a canvas* picks another program's window, such as Photoshop, takes the canvas corners from Ctrl+Shift+1 (top left) and Ctrl+Shift+2 (bottom right), then keeps this window exactly over the canvas, following the program when it moves.

Projects save as `.treeproj` (JSON). Older project files still open; settings that no longer exist are ignored.

## Command line

```
cargo run --release -p forest-cli -- [project.treeproj] -out forest.png -scale 1.0 -bg
```

With no project it renders a test shape. `-bg` adds a tan background for viewing. `-strokes FILE` replays brush strokes and `-trees FILE` writes the final tree list.

## Layout

- `crates/forest-core`: the engine (depth field, layer coverage, placement, generated trees in `wp.rs`, light, cascade shadows, compositing).
- `crates/tree-inator`: the egui app, render worker thread and Win32 overlay.
- `crates/forest-cli`: headless renderer for testing.
- `research/`, `demo/`: analysis scripts and prototype work. Some were written for the older engine; the backup below matches them.

## Backup of the earlier version

`backups/tree-inator-before-cleanup-2026-10-07.zip` holds the full source from before the old tree generation, the CG2A texture bank and the leaf-dab texture were removed, with `RESTORE.txt` inside. `backups/forest-cli-before-cleanup.exe` is the matching command-line renderer, which still needs a `Base Assets` folder to use the CG2A bank. The `Base Assets` folder itself is untouched; the app no longer reads it.
