---
navigation:
  title: "Wafering Cell"
  parent: production/index.md
  position: 30
  icon: fcw:wafering_cell_kit
item_ids:
  - fcw:wafering_cell_kit
  - fcw:silicon_wafer
---

# Wafering Cell

Slices the boule into wafers and polishes them one by one.

| | |
|---|---|
| Size | 8 x 3 x 3 (width, height, depth), 52 cells |
| Placement | kit, right-click the ground |
| Slots | 4 inputs, 4 outputs |
| Draw | 256 FE/t while working |

## Front view

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/wafering_cell.nbt" />
  <MachinePorts machine="wafering_cell" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Back view

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/wafering_cell.nbt" />
  <MachinePorts machine="wafering_cell" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Ports

<MachinePortTable machine="wafering_cell" />

Sides are named as the operator sees them, standing in front of the panel. Loading and starting: [Operating a machine](operation.md).

## Recipes

<MachineRecipes type="fcw:wafering" />

## Kit

<RecipeFor id="fcw:wafering_cell_kit" />
