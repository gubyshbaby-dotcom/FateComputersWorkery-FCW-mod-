---
navigation:
  title: "Arc Furnace"
  parent: production/index.md
  position: 10
  icon: fcw:arc_furnace_kit
item_ids:
  - fcw:arc_furnace_kit
  - fcw:metallurgical_silicon
  - fcw:steel_ingot
  - fcw:polysilicon
---

# Arc Furnace

Melts the charge with an electric arc. Makes metallurgical silicon, steel and polysilicon. The first machine of the chain.

| | |
|---|---|
| Size | 8 x 6 x 5 (width, height, depth), 164 cells |
| Placement | kit, right-click the ground |
| Slots | 4 inputs, 4 outputs |
| Draw | 256 to 384 FE/t while working |

## Front view

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/arc_furnace.nbt" />
  <MachinePorts machine="arc_furnace" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Back view

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/arc_furnace.nbt" />
  <MachinePorts machine="arc_furnace" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Ports

<MachinePortTable machine="arc_furnace" />

Sides are named as the operator sees them, standing in front of the panel. Loading and starting: [Operating a machine](operation.md).

## Recipes

<MachineRecipes type="fcw:arc_furnace" />

The polysilicon recipe takes any silicon from the `c:silicon` tag, including silicon of other mods.

## Kit

<RecipeFor id="fcw:arc_furnace_kit" />
