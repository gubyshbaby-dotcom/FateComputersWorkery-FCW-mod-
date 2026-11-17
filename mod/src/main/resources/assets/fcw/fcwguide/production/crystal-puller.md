---
navigation:
  title: "Crystal Puller"
  parent: production/index.md
  position: 20
  icon: fcw:crystal_puller_blueprint
item_ids:
  - fcw:crystal_puller_kit
  - fcw:silicon_boule
---

# Crystal Puller

Grows a single crystal of silicon from molten polysilicon by the Czochralski method.

| | |
|---|---|
| Size | 5 x 7 x 4 (width, height, depth), 75 cells |
| Building | blueprint and components: [Building Machines](construction.md) |
| Stack places | 4 inputs, 4 outputs |
| Draw | 512 FE/t while working |

## Front view

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/crystal_puller.nbt" />
  <MachinePorts machine="crystal_puller" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Back view

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/crystal_puller.nbt" />
  <MachinePorts machine="crystal_puller" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Ports

<MachinePortTable machine="crystal_puller" />

Sides are named as the operator sees them, standing in front of the panel. Loading and starting: [Operating a machine](operation.md).

## Loading

Polysilicon goes into the charge hatch, the boule is taken from the ingot door. Water in the coolant supply port and work without power stops give a boule of class A: [Defects](defects.md).

## Recipes

<MachineRecipes type="fcw:crystal_puller" />

## Blueprint

<RecipeFor id="fcw:crystal_puller_blueprint" />

The bill and the recipes of the components: [Machine Components](components.md).
