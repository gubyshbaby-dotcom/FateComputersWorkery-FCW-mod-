---
navigation:
  title: "Operating Machines"
  parent: production/index.md
  position: 5
  icon: minecraft:lever
---

# Operating Machines

Machines have no slot windows. Items are put into the machine by hand, and the text console of the operator panel runs the commands. How to build a machine: [Building Machines](construction.md).

## Loading by hand

Every machine has places for loading and unloading: a hatch, a dock, a tray, a cassette. Aim at one: a line above the hotbar says what the place is, what lies in it and what a right-click will do. A white outline means the item in your hand fits here.

| Action | Result |
|---|---|
| Right-click with an item | put the stack from your hand |
| Right-click with an empty hand | take one stack |
| Sneak + right-click with an empty hand | take everything |
| Right-click a fluid port with a bucket | fill or drain the fluid |

A place takes only what the recipes of its section use; the reticle port takes only reticles. A refusal makes a dull click, and the reason stays on the same line for two seconds. Doors and trays open by themselves. The client setting `machines.hints` turns the hint line off.

## Running

1. Turn on the main breaker (right-click).
2. Load the inputs: by hand or through the ports.
3. The recipe starts by itself once everything it needs is loaded and the output has room.
4. Take the result from the unloading place or from the port.

If several recipes match, the machine takes the one with more different inputs. The `recipe` command on the panel picks one.

## Operator panel

Right-clicking the panel with an empty hand opens a console of 80 x 25. The server runs the commands; `exit` or Esc closes the console.

| Command | Action |
|---|---|
| `status` | state, recipe and its progress, load, output, power, yield |
| `start`, `stop`, `stop now` | switch on; stop after the cycle; stop at once |
| `recipe`, `recipe name`, `recipe auto` | list the recipes, pin one, give the choice back to the machine |
| `log`, `log 20` | journal: the last events |
| `name`, `name new-name` | show or change the name of the machine |
| `yield` | shift report: [Defects](defects.md) |
| `bom`, `stage`, `cancel` | on a build site: [Building Machines](construction.md) |
| `help` | the list of commands |

A machine has a name such as `euv-1` or `eaf-2`, unique in the world, and a journal of 64 events. The same commands work from a computer: [Commands](../reference/commands.md).

## Lamp and readout

The panel carries a two-line readout and a signal lamp. The readout shows the state and the progress of the recipe. The lamp blinks red when the machine stands because of a fault. The power input has a lamp of its own.

| State | Cause |
|---|---|
| Off | the breaker is off |
| Idle | what the recipe needs is not loaded |
| Running | a recipe is in progress |
| No power | `requirePower` is on and the buffer is short of FE |
| Output full | the result does not fit into the output |
| No reticle | the scanner needs a reticle in the reticle port |

## Ports

- Item ports work as inventories: hoppers and pipes of other mods connect to them. A loading port fills the inputs, an unloading port hands out the result.
- The power port accepts FE: [Energy](../energy/index.md).
- Fluid ports accept fluids. Water in the cooling affects quality: [Defects](defects.md).
- The data port and any cell of the machine accept a data cable from a computer.

The `production.speedMultiplier` setting changes the speed of all recipes.
