---
navigation:
  title: "Crystal Puller"
  parent: production/index.md
  position: 20
  icon: fcw:crystal_puller_kit
item_ids:
  - fcw:crystal_puller_kit
  - fcw:silicon_boule
---

# Crystal Puller

Grows a single crystal of silicon from molten polysilicon by the Czochralski method.

| | |
|---|---|
| Size | 5 x 7 x 4 (width, height, depth), 75 cells |
| Placement | kit, right-click the ground |
| Slots | 4 inputs, 4 outputs |
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

## Recipes

<MachineRecipes type="fcw:crystal_puller" />

## Kit

<RecipeFor id="fcw:crystal_puller_kit" />
