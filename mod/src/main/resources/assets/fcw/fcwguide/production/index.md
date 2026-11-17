---
navigation:
  title: "Production"
  position: 20
  icon: fcw:silicon_wafer
---

# Production

The chain turns quartz, coal and iron into processors, memory and boards. It has five machines; each takes up several dozen blocks.

<GameScene zoom="0.8" interactive={true}>
  <ImportStructure src="../assets/structures/chain.nbt" />
  <IsometricCamera yaw="195" pitch="35" />
</GameScene>

| Step | Machine | Input | Output |
|---|---|---|---|
| 1 | Arc furnace | quartz, coal | metallurgical silicon |
| 1 | Arc furnace | iron, coal | steel |
| 2 | Arc furnace | metallurgical silicon, redstone | polysilicon |
| 3 | Crystal puller | 8 polysilicon | silicon boule |
| 4 | Wafering cell | silicon boule | 8 wafers |
| 5 | EUV scanner | wafer and reticle | patterned wafer |
| 6 | Electronics line: packaging | patterned wafer, copper or gold | chips and processors |
| 7 | Electronics line: board assembly | board blank, chips | boards and memory modules |

One wafer costs 4 quartz. A boule (8 wafers) takes 32 quartz, 16 coal and 8 redstone.

## Build order

1. Workbench. The first computer needs no machines: [Parts without machines](workbench.md).
2. Arc furnace. The blueprint and the components are made from vanilla materials.
3. Crystal puller and wafering cell. They need steel and metallurgical silicon.
4. EUV scanner. 301 blocks; needs steel, wafers and prototype chips from the crafting table.
5. Electronics line. Needs steel and wafers.

How to build: [Building Machines](construction.md). How to operate: [Operating Machines](operation.md). What the chip output depends on: [Defects](defects.md). How to watch the plant from a computer: [Console Programs](programs.md).

<SubPages />
