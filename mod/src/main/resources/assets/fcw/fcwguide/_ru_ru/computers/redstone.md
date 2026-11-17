---
navigation:
  title: "Редстоун и fcw"
  parent: computers/index.md
  position: 80
  icon: minecraft:redstone
---

# Редстоун и fcw

В каждой гостевой системе есть команда `fcw`. Через неё программа в компьютере читает сигнал редстоуна на сторонах корпуса и выдаёт свой.

| Команда | Действие |
|---|---|
| `fcw redstone get` | входной сигнал на всех шести сторонах |
| `fcw redstone get front` | входной сигнал на одной стороне, число 0–15 |
| `fcw redstone set top 15` | выдать сигнал 15 на верхнюю сторону |
| `fcw info` | состояние связи с игрой |

Стороны: `front`, `back`, `left`, `right`, `top`, `bottom`. Они считаются от самого компьютера: `front` там, где его лицевая панель, `left` и `right` его собственные.

Когда компьютер выключается, все выходы сбрасываются в 0.

## Пример

Сигнал, пришедший на лицевую сторону, повторяется на верхней:

```
while true; do
level=$(fcw redstone get front)
fcw redstone set top "$level"
sleep 0.2
done
```
