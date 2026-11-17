---
navigation:
  title: "Operating a machine"
  parent: production/index.md
  position: 5
  icon: minecraft:lever
---

# Operating a machine

This page describes the current version. The way machines are loaded and started is being reworked and will change.

## Placing and removing

- The arc furnace, crystal puller, wafering cell and electronics line are placed with a kit: right-click the ground. The whole machine appears, its panel towards the player. If there is not enough room, the blocks in the way are marked with red dust.
- The EUV scanner is built from its building blocks: [Building the scanner](euv-building.md).
- Removal: break any cell of the machine while sneaking. The kit and the contents of the slots drop. The scanner turns back into its building blocks.

## Running

1. Switch the main breaker on (right-click).
2. Put items into the input slots: through the panel window (right-click the panel) or through an input port.
3. The recipe starts by itself once the inputs are sufficient and the output slots have room.
4. Take the result from the panel window or from an output port.

When several recipes fit, the machine takes the one with more different inputs, then the one with more items. Put in exactly what the wanted result needs.

## States

| State | Cause |
|---|---|
| Off | the breaker is off |
| Idle | no matching recipe in the input slots |
| Working | a recipe is running |
| No power | `requirePower` is on and the buffer is short of FE |
| Output full | the result does not fit into the output slots |

## Ports

- Item ports work as inventories: hoppers and pipes of other mods connect to them. An input port fills the input slots, an output port empties the output slots.
- The power port takes FE: [Energy](../energy/index.md).
- Fluid ports accept fluids, but the recipes of this version do not need them.
- Doors and drawers open with a right-click; while working, the machine opens and closes them itself.

The speed of all recipes is set by `production.speedMultiplier`.
