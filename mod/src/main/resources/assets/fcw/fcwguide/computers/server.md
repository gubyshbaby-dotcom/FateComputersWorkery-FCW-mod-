---
navigation:
  title: "Servers and admins"
  parent: computers/index.md
  position: 90
  icon: fcw:case_server
---

# Servers and admins

Virtual machines run where the Minecraft server runs: on the player's computer in single player, on the server in multiplayer.

## What the server needs

| | |
|---|---|
| QEMU | `qemu-system-x86_64` and `qemu-img`; if missing, the mod downloads a portable build |
| Acceleration | KVM on Linux, WHPX on Windows, HVF on macOS; without it machines run in slow software emulation |
| Port | UDP 25600 open to players: screen, input and sound travel over it |
| Memory | per running computer: from 256 MB for a console, 1 to 2 GB for a desktop |
| Disk | 1 to 1.5 GB per OS image plus the machines' disks |

## First steps

1. Check the state: `/fcw status`.
2. Download an image: `/fcw images fetch fedora`.
3. If the server is behind NAT, set the public address in `daemon.publicHost`.

## Container hosting

Pelican and Pterodactyl give containers no access to KVM. The options: run in software emulation, or install the VM daemon as a service on the node itself (`daemon.mode = "remote"`).

Full instructions for installation, hosting and frp: the project README, section "Установка".

https://github.com/gubyshbaby-dotcom/FateComputersWorkery-FCW-mod-

See also: [Commands](../reference/commands.md), [Settings](../reference/config.md).
