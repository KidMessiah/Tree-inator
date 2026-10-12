# Forest pattern and growth papers, and what they mean for the generator

Collected 2026-10-11 while matching NAIP aerial imagery.

## Stand structure from competition

- Strigul, Pristinski, Purves, Dushoff, Pacala 2008, "Scaling from trees to forests: tractable macroscopic equations for forest dynamics" (Ecological Monographs 78(4)). The perfect plasticity approximation: crowns deform to fill the space they are given, so a closed canopy is a tessellation of crown cells at a shared height, with smaller trees in layers below. Use: closed canopies should show crowns meeting along edges, not round crowns with dark triangular gaps at triple junctions.
- Bohlman and Pacala 2012, a forest structure model that assigns trees to crown layers. Use: a tree's brightness and visibility depend on how many crowns sit above it, so suppressed trees belong under the top layer, partly hidden.
- Deussen, Hanrahan, Lintermann, Mech, Pharr, Prusinkiewicz 1998, "Realistic modeling and rendering of plant ecosystems" (SIGGRAPH). Placement by ecosystem simulation (competition, self-thinning), then instancing.
- Makowski, Hadrich, Scheffczyk, Michels, Pirk, Palubicki 2019, "Synthetic silviculture: multi-scale modeling of plant ecosystems" (ACM TOG 38(4)). Individual plants compete for light and space and the stand structure emerges. Use: crown size should follow from available space, not be drawn independently of neighbours.

## Spatial point patterns

- Kokkila, Makela, Nikinmaa 2002, marked Gibbs process for tree positions. Trees repel like charges, so competition shapes positions.
- Grabarnik and Sarkka 2009, hierarchical multivariate Gibbs models: large trees are affected only by their own size class, small trees by the larger ones, so small trees fill the gaps between large crowns.
- Picard et al. 2009 (J. Ecology), tropical forest: regularity at short range (about 6 m), clustering at long range (about 30 m). Matches what the aerial patches show (spacing as regular as ours, clumping beyond).

## Canopy texture and gaps

- Couteron et al., FOTO (Fourier textural ordination): canopy grain from the 2D Fourier spectrum gives apparent crown size to about 5 percent. Used here as band energies in canopy_metrics.py.
- Gap size frequency follows a power law (boreal ALS studies, UAV beech study). Our gap exponent already matches (0.85 to 0.94 against 0.92 to 1.02).
- Li and Strahler geometric-optical model: a scene is sunlit crown, shaded crown, sunlit ground and shaded ground. Aerial imagery of closed forest is close to two classes (lit crown, dark gap).

## Rendering

- Bruneton and Neyret 2012, "Real-time realistic rendering and lighting of forests" (CGF): in-crown shadowing and view-light masking correlations.
