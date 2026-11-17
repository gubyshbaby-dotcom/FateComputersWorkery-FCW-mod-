---
navigation:
  title: "Internet access"
  parent: computers/index.md
  position: 70
  icon: fcw:monitor
---

# Internet access

Computers reach the Internet through the server's NAT. Packages install the usual way: `dnf`, `apt`, `pacman`.

## What is blocked

Guest systems cannot reach:

- the server itself and its services;
- the server's local networks: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 and other private ranges;
- port 25 (mail).

## Limits

| | |
|---|---|
| Speed | 8 MiB/s per computer by default |
| Incoming connections from the Internet | none |

An administrator can switch the Internet off entirely (`network.internet`), change the speed limit and add allowed and denied addresses: [Settings](../reference/config.md).
