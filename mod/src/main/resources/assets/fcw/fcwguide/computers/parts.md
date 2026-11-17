---
navigation:
  title: "Parts"
  parent: computers/index.md
  position: 20
  icon: fcw:motherboard
item_ids:
  - fcw:motherboard
  - fcw:graphics_card
  - fcw:cpu_dip
  - fcw:cpu_qfp
  - fcw:cpu_pga
  - fcw:cpu_lga
  - fcw:cpu_multicore
  - fcw:cpu_server
  - fcw:ram_dip
  - fcw:ram_simm
  - fcw:ram_dimm
  - fcw:ram_ddr
  - fcw:ram_ddr_rgb
  - fcw:ram_server
  - fcw:ram_sodimm
---

# Parts

Parts go into the computer window. They set how many cores and how much memory the virtual machine gets.

| Slot | Required | Gives |
|---|---|---|
| Motherboard | yes | the computer does not start without it |
| Processor | yes | number of cores (vCPU) |
| Memory, 4 slots | at least one module | memory size, modules add up |
| Graphics card | no | graphical screen and desktop |

## Processors

| Processor | vCPU | Made at |
|---|---|---|
| 8-bit (DIP) | 1 | workbench |
| 16-bit (QFP) | 1 | workbench |
| 32-bit (PGA) | 2 | electronics line |
| 64-bit | 2 | electronics line |
| Multi-core | 4 | electronics line |
| Server | 8 | electronics line |

A machine never gets more cores than the server has.

## Memory

| Module | MB | Made at |
|---|---|---|
| DIP chip | 256 | workbench |
| SIMM | 512 | workbench |
| DIMM | 1024 | electronics line |
| DDR | 2048 | electronics line |
| SO-DIMM | 2048 | electronics line |
| DDR with lighting | 4096 | electronics line |
| Server | 8192 | electronics line |

The limit per machine is the `vm.maxMemoryMb` setting (8192 MB by default). A text console runs in 256 MB. A desktop needs at least 1 GB, better 2 GB; the exact minimum of each OS is shown in the image list.

Part recipes: [Parts without machines](../production/workbench.md), [Electronics line](../production/chip-line.md).
