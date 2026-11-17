---
navigation:
  title: "Defects"
  parent: production/index.md
  position: 6
  icon: fcw:wafer_cpu
---

# Defects

On the `engineer` and `fab` difficulty levels not every wafer is equally good. The scanner exposes 61 fields on a wafer, and some of them are lost. The more good fields, the more chips the packaging gives. The `standard` level has no defects.

## Causes

The `status` command on the panel of the scanner shows the map of the wafer in progress: a dot is a good field, a letter is the cause of a lost one.

| Code | Cause | When it appears | What to do |
|---|---|---|---|
| `P` | particle | always; most often under the open sky | put the scanner into a closed room |
| `W` | polish | a wafer of class B or C; more often at the edge | water at the wafering cell |
| `C` | crystal | a boule of class B or C | water and steady power at the crystal puller |
| `E` | power | the scanner stopped without power: three fields in a row | spare FE, a buffer |
| `T` | overheating | no water in the coolant port; more often towards the end of the wafer | fill in water |
| `M` | mask wear | more than half of the reticle is used up | a new reticle |
| `V` | vibration | an arc furnace, a wafering cell or an unloading crystal puller works nearby | move the scanner away |

## Room

The scanner checks the air around it. If the open sky can be reached from the machine through air, the room is open: particles are at their worst. Walls and a ceiling of any blocks, glass included, give class ISO 8 and almost half the particles. The class of the room is on the `env` line of `status`.

An arc furnace disturbs within 16 blocks, a wafering cell and a crystal puller within 8.

## Water

Three machines need water: the scanner and the crystal puller in the coolant supply port, the wafering cell in the clean water port. Fill it with a bucket, right-clicking the port, or with pipes. Work uses 1 mB per tick.

## Classes

| Item | Class | Depends on |
|---|---|---|
| Silicon boule | A, B, C | water in the crystal puller and work without power stops: both give A, one gives B, none gives C |
| Silicon wafer | crystal and polish, A, B, C | the crystal class comes from the boule; the polish depends on water and power at the wafering cell |
| Patterned wafer | S, A, B, C, D | the number of good fields |

The class is in the tooltip of the item. Items of one class stack.

| Yield class | Good fields of 61 | Packaging output |
|---|---|---|
| S | 58–61 | x1.25 |
| A | 52–57 | x1 |
| B | 43–51 | x0.8 |
| C | 31–42 | x0.6 |
| D | up to 30 | x0.4 |

## Corrections computed by a computer

A computer linked to a machine by a data cable or through a router computes corrections for it. The faster the processor of the computer, the more machines it serves. A linked computer gives three quarters of its power even when its virtual machine is off. The full power comes from a computing service in the guest system; this version does not have it yet.

| Machine | What the computing gives |
|---|---|
| Scanner | fewer `M`, `T`, `V` and `W` defects |
| Crystal puller, electronics line | the recipe runs up to 20 % faster |
| Wafering cell | a chance of a ninth wafer from a boule |

The `apc` line of `status` shows which computer computes and how many units it gives.

## Shift report

The `yield` command on the panel shows, for one in-game day, the number of wafers, the share of good fields, the classes, the causes of defects and the downtime, and for the scanner the last wafers as well. From a computer, `fcw machine name yield` gives the report of one machine.
