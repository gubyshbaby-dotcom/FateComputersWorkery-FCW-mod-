---
navigation:
  title: "Console Programs"
  parent: production/index.md
  position: 7
  icon: fcw:crt_monitor
---

# Console Programs

A computer linked to machines by a data cable or through a router has full-screen programs: a wafer map with the progress of the exposure and the defects by field, the melt, the growth of a boule, the takt of the line, a summary of every machine in the network and a shift report. They run in the console of the guest system or in a terminal window.

## Commands

| Command | What it shows | Tabs |
|---|---|---|
| `fcw top` | every machine of the network: state, recipe, progress, power use; power, buffer, computed corrections | Machines, Power, Log, Yield |
| `fcw litho euv-1` | the scanner: a wafer map of 61 fields, the mask, the environment, the queue | Wafer, Lot, Mask, Log |
| `fcw furnace eaf-1` | the arc furnace: temperature by the phases of a melt, electrodes, charge | Melt, Charge, Log |
| `fcw puller cz-1` | the crystal puller: the boule along its length, conditions, the class forecast | Growth, Heat, Log |
| `fcw saw saw-1` | the wafering cell: the boule by cuts, a table of wafers | Wafers, Tool, Log |
| `fcw line line-1` | the electronics line: two sections by position, takt, output | Line, Yield, Log |
| `fcw yield` | the shift report: yield by recipe, causes of defects and what to fix, downtime | Summary, Wafers, Causes, Downtime |
| `fcw mon euv-1` | the program that fits the machine of that name | |

The same programs answer to the names `fcw-top`, `fcw-litho` and so on. The machine name may be left out when the network has only one machine of that kind. `fcw machine list` shows the names.

## The wafer map

The scanner exposes 61 fields in a serpentine. On the map a field is grey while it waits; dark violet once it is exposed and good; bright while it is being exposed; red with a letter if it is lost. The letter names the cause: `P` particle, `W` polish, `C` crystal, `M` mask, `T` heat, `V` vibration, `E` power. What to do about each cause: [Defects](defects.md).

In a terminal of 150 by 46 cells or more the map is drawn larger: every field shows its number, the result and the focus deviation. From 120 columns a column with the log appears on the right. Below 80 by 24 the program does not draw the screen.

Temperature, currents, dose and focus are shown for looks and do not affect the game. Only what the [Defects](defects.md) page describes affects the game.

## Keys

In the game window the F keys may be taken, so each has a letter.

| Key | Action |
|---|---|
| `F1` or `?` | help |
| `1`–`4`, `Tab` | tabs; `F2`–`F4` open the second, third and fourth |
| `F5` or `s` | start |
| `F6` or `x` | stop, confirmed with `y` |
| `F7` or `r` | choose a recipe: a list, `Enter` |
| `F9` or `t` | go to `fcw top` |
| `F10`, `q` or `Esc` | quit |
| arrows, `PgUp`, `PgDn` | table row, log scrolling |
| `Enter` | in `fcw top`: open the program of the selected machine |
| `/` | on the Log tab: filter |

In `fcw yield` the key `F2` or `p` switches between today's and yesterday's shift, `F3` or `m` picks a machine, `F8` or `w` writes the report to the file `fcw-yield-day.txt` in the home directory.

## States

| Header | What it means |
|---|---|
| `RUN` | a recipe is running |
| `IDLE` | the machine is on and waits for input or a mask |
| `OFF` | the breaker is off; `F5` switches it on |
| `FAULT` on red | the machine has stopped; the cause is in red in the first section and in the log |
| `NO LINK` on red | no data comes from the machine; the screen shows the last known data, the program waits and carries on by itself |

If the network has no machine of that name, the program does not open and returns code 2.

## The computing service

A computer linked to machines gives them three quarters of the computing power of its processor. It gives the full power while the computing service is on in the guest system.

| Command | Action |
|---|---|
| `fcw apc on` | switch the service on for every machine of the network |
| `fcw apc on euv-1 cz-1` | switch it on only for the named machines |
| `fcw apc off` | switch it off |
| `fcw apc status` | whether the service is on and how many units each machine receives |

A service that is on survives a reboot of the computer. The programs show the power as "received / needed", for example `8 / 8 CU`. Without the service the same place reads `6 / 8 CU` with the note "no service": this is not a fault, the machine works, but there are more defects and recipes take longer.

## Colours

The programs adapt to the terminal: full colour in a terminal window, 16 colours in the console. The `--mono` option or the `NO_COLOR` variable removes colour altogether: the current field is shown in inverse, a defect as a letter in brackets. The cause of a defect is always named by a letter, so the map reads with any number of colours.
