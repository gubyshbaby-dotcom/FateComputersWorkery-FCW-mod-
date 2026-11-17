---
navigation:
  title: "Building the scanner"
  parent: production/index.md
  position: 41
  icon: fcw:euv_casing
item_ids:
  - fcw:euv_casing
  - fcw:euv_casing_glass
  - fcw:euv_casing_vent
  - fcw:euv_frame
---

# Building the scanner

The scanner is assembled from 301 blocks of 23 kinds in a box of 13 x 6 x 5. The player lays out a blank by the plan, then a right-click on the controller turns it into the machine.

## Steps

1. Pick a site of 13 x 5 and the operator side: the long side that holds the controller.
2. Lay out six layers bottom to top by the scenes below. Hover a block to see its name.
3. Ports, FOUP load ports, item hatches, the power cabinet and the controller must face outwards.
4. Right-click the controller. If a block is wrong or faces the wrong way, the chat names the cell and the block needed, and the spot is marked with red dust.
5. When everything is right, the blank becomes the machine. Switch the main breaker on: it is on the power cabinet at the back.

Operators can use `/fcw machine blank`: it lays out a finished blank in front of the player.

## The whole blank

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank.nbt" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank.nbt" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Layer 1, 65 blocks

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_1.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Casing 25, wafer stage 12, CO2 laser amplifier 9, vacuum chamber wall 6, wafer handler module 6, frame 3, coolant port 1, data port 1, energy port 1, gas port 1.

## Layer 2, 62 blocks

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_2.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Wafer stage 12, casing 10, glass casing 9, CO2 laser amplifier 6, wafer handler module 6, vacuum chamber wall 4, chiller 3, frame 3, FOUP load port 2, turbomolecular pump 2, controller 1, coolant port 1, gas port 1, item hatch 1, power cabinet 1.

## Layer 3, 57 blocks

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_3.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Casing 14, multilayer mirror 12, glass casing 11, CO2 laser amplifier 6, vacuum chamber wall 6, frame 4, chiller 3, wafer handler module 1.

## Layer 4, 63 blocks

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_4.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Casing 25, multilayer mirror 10, vent casing 7, glass casing 6, vacuum chamber wall 5, frame 4, chiller 3, EUV collector 1, CO2 laser amplifier 1, exhaust port 1.

## Layer 5, 31 blocks

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_5.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Casing 10, reticle stage 9, frame 4, glass casing 3, vacuum chamber wall 2, vent casing 1, tin droplet generator 1, item hatch 1.

## Layer 6, 23 blocks

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_6.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Casing 13, frame 6, vent casing 4.

Block recipes: [Scanner blocks](euv-blocks.md).
