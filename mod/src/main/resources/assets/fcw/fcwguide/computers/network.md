---
navigation:
  title: "The .mc network"
  parent: computers/index.md
  position: 60
  icon: fcw:case_server
---

# The .mc network

All running computers of one server share a local network. Nothing has to be configured: the address comes from DHCP.

| | |
|---|---|
| Gateway, DHCP and DNS | 10.42.0.1 |
| Computer name | `name.mc` |
| Default name | `pc-` and eight characters, for example `pc-1a2b3c4d` |

## Naming a computer

Rename the case on an anvil before placing it. The name is reduced to lowercase Latin letters, digits and hyphens; Cyrillic is transliterated. A computer called "Server 1" becomes `server-1.mc`.

Name and address are shown in the computer window, in the "Network" line.

## Checking

From one computer to another:

```
ping server-1.mc
```

Any protocol works between computers: SSH, HTTP, your own servers on any port.
