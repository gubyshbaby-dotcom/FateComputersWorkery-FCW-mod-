---
navigation:
  title: "Chip tiers and tags"
  parent: production/index.md
  position: 70
  icon: fcw:cpu_multicore
---

# Chip tiers and tags

The tier of a chip is the lithography generation it went through. Datapacks and modpacks should ask for "a chip of tier N" by tag in their recipes, not for a specific item: new chips are then picked up automatically.

## Tiers

| Tag | Contents |
|---|---|
| `fcw:chips/micron` | everything from the workbench: DIP and QFP processors, DIP and SIMM memory |
| `fcw:chips/duv` | empty: the DUV scanner comes later |
| `fcw:chips/immersion` | empty: the immersion scanner comes later |
| `fcw:chips/euv` | everything past the EUV scanner: processors from 32-bit up, chips from the line, memory modules made of them |
| `fcw:chips/any` | all four tiers |

## By purpose

| Tag | Contents |
|---|---|
| `fcw:chips/cpu` | all processors |
| `fcw:chips/memory` | the memory chip and all memory modules |
| `fcw:chips/gpu` | the graphics processor |
| `fcw:chips/controller` | the controller chip |

Every chip is in exactly one tier and exactly one purpose. The motherboard and the graphics card do not count as chips.

To require a processor of at least EUV, use a `neoforge:intersection` ingredient of `fcw:chips/euv` and `fcw:chips/cpu`.

## Other tags

| Tag | Contents | Purpose |
|---|---|---|
| `c:ingots/steel` | steel ingot | steel of other mods fits every recipe, and the other way round |
| `c:silicon` | metallurgical silicon | silicon of other mods can be refined to polysilicon |
| `fcw:reticles` | the four reticles | filter of the reticle slot |
| `fcw:patterned_wafers` | the four patterned wafers | filter of the packaging input |
