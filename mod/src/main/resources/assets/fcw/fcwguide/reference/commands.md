---
navigation:
  title: "/fcw commands"
  parent: reference/index.md
  position: 10
  icon: minecraft:command_block
---

# /fcw commands

| Command | Who | Action |
|---|---|---|
| `/fcw status` | everyone | state of the VM daemon, accelerator, network, address for clients |
| `/fcw images` | everyone | list of OS images and which of them are downloaded |
| `/fcw images fetch id` | operator | download and install an image: `fedora`, `ubuntu`, `arch` |
| `/fcw vms` | operator | machines known to the daemon and their state |
| `/fcw qemu` | everyone | which QEMU was found |
| `/fcw qemu fetch` | operator | download the portable QEMU |
| `/fcw recipes` | operator | number of loaded machine recipes and the recipe lookup self-test |
| `/fcw machine place name` | operator | place a finished machine in front of you, without building |
| `/fcw machine blank` | operator | lay out the scanner blank from its building blocks |

Operator: permission level 2.

Machine names for `/fcw machine place`: `arc_furnace`, `crystal_puller`, `wafering_cell`, `euv_scanner_mk2`, `chip_line`.

## In the guest system

| Command | Action |
|---|---|
| `fcw redstone get` | input signal on all sides |
| `fcw redstone get front` | input signal on one side |
| `fcw redstone set top 15` | emit a signal on a side |
| `fcw info` | state of the link to the game |
| `fcw machine list` | machines in the network of this computer: name, kind, state |
| `fcw machine euv-1 status` | any panel command for the machine of that name |
| `fcw machine euv-1 log 20 --follow` | the journal; new entries are printed as they appear |
| `fcw machine euv-1 wait idle --timeout 600` | wait for the state `idle`, `run` or `fault` |
| `fcw top` | a full-screen summary of the machines of the network |
| `fcw litho euv-1`, `fcw mon euv-1` | the full-screen program of a machine: wafer map, defects, log |
| `fcw yield` | the shift report |
| `fcw apc on`, `fcw apc off`, `fcw apc status` | the computing service for the machines |

More: [Redstone and fcw](../computers/redstone.md), [Console Programs](../production/programs.md).

## Machines from a computer

A computer sees the machines it is linked to by a data cable and the machines within the radius of a router in the same network. The commands are those of the panel: `status`, `start`, `stop`, `recipe`, `log`, `name`, `yield`, and on a build site also `bom`, `stage`, `cancel`. They are described in [Operating Machines](../production/operation.md).

- The `--json` switch prints the answer as JSON: for scripts.
- Exit code 0 means success, 1 a refusal, 2 that the machine is not in the network or the game does not answer.
- The language of the answer comes from the locale of the guest system.

## Machine panel and pallet

Right-clicking the panel of a machine or the pallet of a build site with an empty hand opens the same console in the game, without a computer. `help` prints the list of commands, `exit` closes the console.
