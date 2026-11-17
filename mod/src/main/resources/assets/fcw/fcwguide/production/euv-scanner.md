---
navigation:
  title: "EUV Scanner Mk2"
  parent: production/index.md
  position: 40
  icon: fcw:euv_controller
item_ids:
  - fcw:euv_controller
  - fcw:wafer_cpu
  - fcw:wafer_memory
  - fcw:wafer_gpu
  - fcw:wafer_controller
---

# EUV Scanner Mk2

Prints the pattern of a reticle onto a silicon wafer. The reticle decides which chips come out.

| | |
|---|---|
| Size | 13 x 6 x 5 (width, height, depth), 301 cells |
| Building | blueprint and building blocks, or the blocks laid by hand: [Building the scanner](euv-building.md) |
| Stack places | 4 inputs, reticle place, 4 outputs |
| Draw | 1024 to 2048 FE/t while working |

## Front view

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_scanner_mk2.nbt" />
  <MachinePorts machine="euv_scanner_mk2" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Back view

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_scanner_mk2.nbt" />
  <MachinePorts machine="euv_scanner_mk2" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Ports

<MachinePortTable machine="euv_scanner_mk2" />

Sides are named as the operator sees them, standing in front of the panel. Loading and starting: [Operating a machine](operation.md).

Wafers go into dock 1, patterned wafers are taken from dock 2. The reticle goes into the reticle port and stays in the machine. The tin drawer leads to the same inputs as the dock; the recipes of this version need no tin. The data port takes a data cable from a computer.

A closed room and water in the coolant supply port reduce defects: [Defects](defects.md).

## Recipes

<MachineRecipes type="fcw:lithography" />

Reticles: [Reticles](reticles.md).
