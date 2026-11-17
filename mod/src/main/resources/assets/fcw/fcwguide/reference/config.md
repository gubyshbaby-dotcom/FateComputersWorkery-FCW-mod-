---
navigation:
  title: "Settings"
  parent: reference/index.md
  position: 20
  icon: minecraft:writable_book
---

# Settings

Two files in the `config` folder: `fcw-server.toml` (server; also applies in single player) and `fcw-client.toml` (client). The main keys are listed below; every key has a comment in the file.

## fcw-server.toml

### daemon: the VM daemon

| Key | Default | Meaning |
|---|---|---|
| `enabled` | true | use the daemon |
| `mode` | local | local: the server starts the daemon itself; remote: it connects to a service on the node |
| `remoteAddress`, `remoteToken` | | address and secret for remote mode |
| `qemuPath` | empty | path to `qemu-system-x86_64`; empty: find it automatically |
| `qemuAutoDownload` | true | download the portable QEMU when none is installed |
| `quicPort` | 25600 | UDP port for screens, input and sound |
| `publicHost`, `publicPort` | empty, 0 | public address and port when the server is behind NAT |
| `maxVms` | 4 | how many machines run at the same time |
| `accelerator` | auto | auto, kvm, whpx, hvf or tcg |

### vm: virtual machines

| Key | Default | Meaning |
|---|---|---|
| `lang` | ru_RU.UTF-8 | language of the guest system |
| `keyboardLayouts` | us,ru | keyboard layouts |
| `network` | true | network card and the `.mc` network |
| `maxMemoryMb` | 8192 | memory limit of one machine |
| `displayWidth`, `displayHeight` | 1024, 768 | resolution of the graphical screen |
| `restartRunningOnLoad` | true | after a server restart, start the machines that were running |

### network: guest network

| Key | Default | Meaning |
|---|---|---|
| `internet` | true | Internet access through NAT |
| `rateLimitKiBps` | 8192 | speed limit per machine, KiB/s |
| `allow` | empty | CIDR ranges allowed despite the firewall |
| `deny` | empty | additional denied CIDR ranges |

### access: devices and access

| Key | Default | Meaning |
|---|---|---|
| `linkRadius` | 8 | radius in which devices link, blocks |
| `viewDistance` | 128 | beyond this distance a player gets no screen |
| `ownerOnlyControl` | false | only the owner of a computer and operators may control it |

### production: machines

| Key | Default | Meaning |
|---|---|---|
| `requirePower` | false | true: machines run only on FE |
| `speedMultiplier` | 1.0 | speed of all recipes: 2.0 is twice as fast |

### cinema: cinema

| Key | Default | Meaning |
|---|---|---|
| `projectorRange` | 24 | how many blocks a projector throws its picture |
| `lampLifeHours` | 40 | life of a projector lamp in hours of projection; 0: lamps do not wear |
| `projectorPower` | 30 | FE per tick a projector draws with `requirePower = true` |
| `linkRadius` (cinema) | 32 | radius in which projectors and speakers link, blocks |

## fcw-client.toml

| Key | Default | Meaning |
|---|---|---|
| `screens.renderDistance` | 32 | beyond this distance a monitor shows a still picture |
| `screens.nearFps`, `screens.farFps` | 30, 5 | frame rate of near and far screens |
| `screens.farDistance` | 12 | distance from which a screen counts as far |
| `screens.controlFps` | 60 | frame rate while a player sits at the computer |
| `audio.enabled`, `audio.volume` | true, 1.0 | computer sound through speakers |
| `cinema.projectorBeam` | true | draw the beam of a projector and the dust in it |
