---
navigation:
  title: "Redstone and fcw"
  parent: computers/index.md
  position: 80
  icon: minecraft:redstone
---

# Redstone and fcw

Every guest system has the `fcw` command. A program inside the computer uses it to read the redstone signal on the sides of the case and to emit its own.

| Command | Action |
|---|---|
| `fcw redstone get` | input signal on all six sides |
| `fcw redstone get front` | input signal on one side, a number from 0 to 15 |
| `fcw redstone set top 15` | emit signal 15 on the top side |
| `fcw info` | state of the link to the game |

Sides: `front`, `back`, `left`, `right`, `top`, `bottom`. They are counted from the computer itself: `front` is where its front panel is, `left` and `right` are its own.

When the computer shuts down, all outputs drop to 0.

## Example

The signal arriving at the front side is repeated on the top side:

```
while true; do
level=$(fcw redstone get front)
fcw redstone set top "$level"
sleep 0.2
done
```
