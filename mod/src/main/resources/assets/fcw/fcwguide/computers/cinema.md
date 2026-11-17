---
navigation:
  title: "Cinema"
  parent: computers/index.md
  position: 45
  icon: fcw:projector
item_ids:
  - fcw:projector
  - fcw:projector_lamp
  - fcw:projection_screen
  - fcw:cinema_seat
---

# Cinema

A projector throws the screen of a computer onto a wall or a projection screen. Everyone nearby sees the picture. The film is started inside the computer: the images include the `mpv` player.

## What you need

| Block | What for |
|---|---|
| Computer with a graphics card | plays the film |
| Projector | makes the picture |
| Projection screen | the picture is brighter and fits the canvas exactly |
| Speakers, two or more | stereo sound; a subwoofer adds the lows |
| Seats | to sit on |
| Monitor and keyboard | to operate the computer; they can stand aside |

## Projector

It stands on the floor or a table, and a click on the underside of a block hangs it from the ceiling. The lens points where the player looks when placing it. It links to the nearest computer like a monitor; re-linking is the same: sneak + right click the computer with an empty hand, then the projector.

| Action | How |
|---|---|
| Switch on, switch off | right click with an empty hand |
| Replace the lamp | right click a switched-off, cooled-down projector with a lamp |
| Show the link | sneak + right click |

The lamp warms up for 20 seconds: the brightness rises gradually. After switching off, the fan cools it for 30 seconds, and the projector cannot be switched on again during that time. While the computer is off, the projector shows a blue rectangle.

## Where and how it shines

The beam leaves the lens straight ahead and stops at the first solid face, no farther than 24 blocks.

| Surface | Picture |
|---|---|
| Bare wall | as wide as the distance divided by 1.5; dimmer, the wall shows through slightly |
| Projection screen | fitted into the canvas with small margins, but no wider than the distance to the screen |

The lens does not tilt, so on a wall the picture is shifted: from a floor projector it starts at lens level and goes up, from a ceiling projector it goes down. The screen does not have to hang exactly opposite the lens: it is enough that it is on the same wall above or below the point the beam hits.

The picture has the proportions of the computer screen. The default is 1024 x 768; for a wide screen the administrator changes `vm.displayWidth` and `vm.displayHeight`.

## Darkness

The brightness of the picture depends on the light where it falls. In a dark hall the image is rich. Daylight from a window or a torch near the screen turns black into grey and washes the colours out. At night it can be watched under the open sky.

The beam of the projector and the dust in it are seen only in the dark. Whoever stands in the beam casts a shadow on the screen.

## Screen

A canvas one sixteenth of a block thick. A click on a wall hangs it on that wall. Adjacent canvases join into one screen with a black frame around it, up to 16 x 9 blocks. The screen must be a rectangle: the projector takes the row it hit and the rows of the same width above and below.

## Lamp

A lamp lasts 40 hours of projection. The picture dims as the lamp wears, down to half brightness at the end; in its last hours the lamp flickers, then burns out. The message on switching on shows the hours used. A projector that was picked up keeps its lamp hours; the item tooltip shows them.

## Seats

Right click a seat: sit down. Sneak: stand up. Seats standing in a row and facing the same way share an armrest.

## Sound

| Speakers linked to the computer | What plays |
|---|---|
| One | both channels together |
| Two or more | the left channel in the speakers to the left of the screen, the right channel in those to the right |
| A subwoofer next to ordinary speakers | low frequencies only |

Left and right are those of a viewer who looks where the projector points. Without a projector the sides are taken from the monitor, without a monitor from the computer case. A speaker exactly in the middle plays both channels. If all speakers ended up on one side, they play both channels.

The sound is heard from 48 blocks; within 6 blocks of a speaker it is at full volume and fades smoothly beyond.

## A large hall

The projector and the speakers link to a computer within 8 blocks. In a larger hall put the computer halfway between the projector and the screen, or ask the administrator to raise `access.linkRadius`.

## Power

With `production.requirePower = true` the projector draws 30 FE per tick while the lamp is lit. It accepts energy from any side; with AE2 it takes it from the ME network through a cable. Without energy the lamp goes out and cools down. More: [Energy](../energy/index.md).

## Recipes

<RecipeFor id="fcw:projector_lamp" />
<RecipeFor id="fcw:projector" />
<RecipeFor id="fcw:projection_screen" />
<RecipeFor id="fcw:cinema_seat" />
