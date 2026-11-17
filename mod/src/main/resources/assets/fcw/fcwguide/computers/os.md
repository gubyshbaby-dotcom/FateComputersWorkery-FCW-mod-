---
navigation:
  title: "Operating systems"
  parent: computers/index.md
  position: 30
  icon: fcw:monitor
---

# Operating systems

| Image id | System |
|---|---|
| `fedora` | Fedora 45 |
| `ubuntu` | Ubuntu 26.04 LTS |
| `arch` | Arch Linux |

All three come with the same environment: the labwc desktop with the LXQt panel and the Alacritty terminal.

| | |
|---|---|
| User | `user` |
| Password | `fcw` |
| Administrator rights | `sudo` |
| Terminal | Super+Enter or Ctrl+Alt+T |
| Keyboard layouts | us and ru, switched with Alt+Shift |

## Installing an image

Images are downloaded by the server administrator: `/fcw images fetch fedora` (also `ubuntu`, `arch`). List and state: `/fcw images`. One image takes 1 to 1.5 GB on the server's disk.

## The machine's disk

Every computer has its own disk: a thin layer over the shared image. Whatever is installed or saved in the system stays on that disk between power cycles.

The disk is wiped in three cases: the "Reinstall" button, the "Update OS" button, breaking the computer block.

## Updates

The image version is pinned to the machine. When the administrator installs a new image build, existing machines keep running their version and the window shows "Update OS".
