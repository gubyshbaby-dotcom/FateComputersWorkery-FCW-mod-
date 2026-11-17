---
navigation:
  title: "Electronics Line"
  parent: production/index.md
  position: 50
  icon: fcw:chip_line_kit
item_ids:
  - fcw:chip_line_kit
  - fcw:chip_controller
  - fcw:chip_memory
  - fcw:chip_gpu
---

# Electronics Line

One machine with two sections. Packaging cuts a patterned wafer into chips and processors. Board assembly puts chips on a board blank.

| | |
|---|---|
| Size | 14 x 3 x 3 (width, height, depth), 102 cells |
| Placement | kit, right-click the ground |
| Packaging slots | 4 inputs, 4 outputs |
| Board assembly slots | 6 inputs, 2 outputs |
| Draw | 256 to 768 FE/t per section while working |

## Front view

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/chip_line.nbt" />
  <MachinePorts machine="chip_line" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Back view

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/chip_line.nbt" />
  <MachinePorts machine="chip_line" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Ports

<MachinePortTable machine="chip_line" />

Sides are named as the operator sees them, standing in front of the panel. Loading and starting: [Operating a machine](operation.md).

The sections work independently and at the same time. Board assembly takes chips not only from its own input slots but also straight from the packaging tray.

One wafer with processors yields a different number of processors depending on what else lies in the input slots: the higher the processor, the more wafers and metal it takes.

## Recipes

### Packaging

<MachineRecipes type="fcw:chip_packaging" />

### Board assembly

<MachineRecipes type="fcw:pcb_assembly" />

## Kit

<RecipeFor id="fcw:chip_line_kit" />
