---
navigation:
  title: "Reticles"
  parent: production/index.md
  position: 60
  icon: fcw:reticle_cpu
item_ids:
  - fcw:reticle_cpu
  - fcw:reticle_memory
  - fcw:reticle_gpu
  - fcw:reticle_controller
---

# Reticles

A reticle (photomask) sets which chips the scanner prints on a wafer. It sits in the reticle slot and is not consumed, but loses 1 durability per wafer. Durability: 64 exposures.

| Reticle | Sample in the recipe | Wafer | Made from it |
|---|---|---|---|
| Controller | 8-bit CPU (DIP) | with controllers | 8 controller chips |
| Memory | SIMM module | with memory | 6 memory chips |
| Graphics processor | diamond | with graphics processors | 2 or 3 graphics processors |
| Processor | 16-bit CPU (QFP) | with processors | processors from 32-bit to server |

All four reticles are in the `fcw:reticles` tag.

## Recipes

<RecipeFor id="fcw:reticle_controller" />
<RecipeFor id="fcw:reticle_memory" />
<RecipeFor id="fcw:reticle_gpu" />
<RecipeFor id="fcw:reticle_cpu" />
