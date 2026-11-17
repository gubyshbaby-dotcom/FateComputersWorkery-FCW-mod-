---
navigation:
  title: "Electronics Line"
  parent: production/index.md
  position: 50
  icon: fcw:chip_line_blueprint
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
| Building | blueprint and components: [Building Machines](construction.md) |
| Packaging stack places | 4 inputs, 4 outputs |
| Board assembly stack places | 6 inputs, 2 outputs |
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

The sections work independently and at the same time. Board assembly takes chips not only from its own inputs but also straight from the packaging tray.

One wafer with processors yields a different number of processors depending on what else lies in the inputs: the higher the processor, the more wafers and metal it takes.

## Loading

Patterned wafers and the metal go onto the wafer input, chips are taken from the tray; chips for the assembly are added to the same tray. Board blanks go into the board magazine, finished boards are taken from the output. How many chips a wafer gives depends on its yield class: [Defects](defects.md).

## Recipes

### Packaging

<MachineRecipes type="fcw:chip_packaging" />

### Board assembly

<MachineRecipes type="fcw:pcb_assembly" />

## Blueprint

<RecipeFor id="fcw:chip_line_blueprint" />

The bill and the recipes of the components: [Machine Components](components.md).
