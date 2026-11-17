---
navigation:
  title: "Energy"
  position: 30
  icon: fcw:euv_power_unit
---

# Energy

Machines count energy in FE (Forge Energy). Every recipe states its draw: FE per tick and in total per run.

## Modes

| `production.requirePower` | Behaviour |
|---|---|
| `false` (default) | machines run on a built-in transformer and need no FE |
| `true` | a recipe advances only while the machine's buffer holds enough FE |

The mode is one for the whole server and is set in `fcw-server.toml`.

## Connection

| | |
|---|---|
| Port | power input at the back of the machine; the red frame in the scenes |
| Buffer | 400 000 FE |
| Intake | up to 20 000 FE/t |
| Source | generators and cables of any mod with FE |

If energy runs out in the middle of a recipe, the machine stops in the "No power" state and continues from the same point once the buffer holds a reserve for one second of work.

## How much is needed

| Machine | FE/t |
|---|---|
| Arc furnace | 256 to 384 |
| Crystal puller | 512 |
| Wafering cell | 256 |
| EUV scanner | 1024 to 2048 |
| Electronics line | 256 to 768 per section |
| [Projector](../computers/cinema.md) | 30 while the lamp is lit |

The most expensive run: a wafer with processors, 2 457 600 FE in 60 seconds.

The mod has no generators of its own yet: they are being designed. Until then use the default mode or FE sources of other mods.
