#!/bin/bash
# retune each species with the canopy flattening experiment on (experiment build of the engine)
cd "$(dirname "$0")/../.."
export TEMPLATE=${TEMPLATE:?set TEMPLATE to a project json}
export CLI=${EXPCLI:?set EXPCLI to the experiment forest-cli}
export ITERS=${ITERS:-250}
export FLAT_R=${FLAT_R:-4}
for sp in aspen pine dfir; do
  ( START=research/satellite/match/${sp}_best_v3.json TAG=_f4 python research/scripts/match_species.py tune $sp | tail -1 | sed "s/^/$sp /" ) &
done
wait
