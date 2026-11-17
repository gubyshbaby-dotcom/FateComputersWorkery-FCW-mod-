---
navigation:
  title: "Building Machines"
  parent: production/index.md
  position: 3
  icon: fcw:wrench
item_ids:
  - fcw:wrench
  - fcw:arc_furnace_blueprint
  - fcw:crystal_puller_blueprint
  - fcw:wafering_cell_blueprint
  - fcw:chip_line_blueprint
  - fcw:euv_scanner_mk2_blueprint
---

# Building Machines

A machine is built on the spot: a blueprint marks out the build site, components are brought to it, and the machine is assembled stage by stage.

## Blueprint

A blueprint is cheap and consumes no components. Its tooltip shows the size, the number of stages and the build time; holding Shift shows the bill of components.

Right-clicking the ground with a blueprint places the build site with the operator panel towards the player. If there is not enough room, the blocks in the way are marked with red dust.

<RecipeFor id="fcw:arc_furnace_blueprint" />
<RecipeFor id="fcw:crystal_puller_blueprint" />
<RecipeFor id="fcw:wafering_cell_blueprint" />
<RecipeFor id="fcw:chip_line_blueprint" />
<RecipeFor id="fcw:euv_scanner_mk2_blueprint" />

## Build site

The site fills every cell of the future machine with scaffolding. A pallet stands where the operator panel will be: the site has a name, a journal and a console from the start.

| Action | Result |
|---|---|
| Right-click the pallet or the scaffolding with a component | deliver the stack; nothing beyond the bill is taken |
| Sneak + right-click the pallet with an empty hand | take back the last delivered stack |
| Right-click the pallet with an empty hand | open the console |
| A hopper or a pipe into the pallet | deliver without hands |

Console commands on a site:

| Command | Action |
|---|---|
| `status` | the stage, its progress, what is missing |
| `bom` | the bill by stages: needed and delivered |
| `stage` | the current stage and the time left |
| `cancel` | cancel the build and return what was delivered |
| `log` | the journal of the site |

## Stages

Components for all stages can be delivered at once. A stage starts by itself when the previous one is finished and its components are there. The cells of the machine appear from the bottom up. The last stage is followed by commissioning, 30 seconds, and then the breaker is unlocked.

With the `production.requirePower` setting on, the build needs power: 64 FE/t fed into the scaffolding or the pallet.

## Wrench

<RecipeFor id="fcw:wrench" />

- Sneak + right-click the panel of a switched-off machine with the wrench, then once more to confirm: teardown. The contents drop at once, the stages run in reverse order twice as fast as the build.
- The blueprint, the control cabinets and a share of the other components come back: the share depends on the difficulty level.
- The wrench on a build site cancels the build. If the site is being torn down, the wrench resumes the build.
- A pickaxe does not break the cells of a machine.

## Difficulty levels

The server setting `production.difficulty`.

| Level | Bill and time | Refund on teardown | Notes |
|---|---|---|---|
| `standard` | a quarter | 100 % | no defects |
| `engineer` | as in the bill | 75 % | with [defects](defects.md); the default level |
| `fab` | twice the time | 60 % | the build always needs power, 256 FE/t; prototype chips from the crafting table do not replace scanner chips |

## The first scanner

The components of the scanner need chips, and chips are made by the scanner. For the first machine, prototype chips are made on the crafting table: they go only into machine components and cannot be installed in a computer.

| Prototype chip | On the crafting table, shapeless |
|---|---|
| Prototype Controller Chip | 3 16-bit CPUs (QFP) |
| Prototype Memory Chip | 3 SIMM memory modules |
| Prototype PGA Processor | 3 16-bit CPUs (QFP) and a gold ingot |

On the `standard` level one chip is enough instead of three; on the `fab` level these recipes do not exist.

## Old kits

Kits of earlier versions work as a blueprint with every component already delivered: the site starts building at once.

All components and their recipes: [Machine Components](components.md).
