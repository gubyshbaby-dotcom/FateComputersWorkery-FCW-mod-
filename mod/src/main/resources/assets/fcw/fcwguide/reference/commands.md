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
| `/fcw machine place name` | operator | place a machine in front of you, as its kit does |
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

More: [Redstone and fcw](../computers/redstone.md).
