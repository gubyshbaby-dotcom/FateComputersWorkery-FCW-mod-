---
navigation:
  title: "Команды /fcw"
  parent: reference/index.md
  position: 10
  icon: minecraft:command_block
---

# Команды /fcw

| Команда | Кому | Действие |
|---|---|---|
| `/fcw status` | всем | состояние демона виртуальных машин, ускоритель, сеть, адрес для клиентов |
| `/fcw images` | всем | список образов ОС и какие из них скачаны |
| `/fcw images fetch id` | оператор | скачать и установить образ: `fedora`, `ubuntu`, `arch` |
| `/fcw vms` | оператор | машины, известные демону, и их состояние |
| `/fcw qemu` | всем | какой QEMU найден |
| `/fcw qemu fetch` | оператор | скачать переносимый QEMU |
| `/fcw recipes` | оператор | число загруженных рецептов машин и самопроверка поиска рецептов |
| `/fcw machine place имя` | оператор | поставить машину перед собой, как комплектом |
| `/fcw machine blank` | оператор | выложить заготовку литографа из блоков набора |

Оператор: уровень прав 2.

Имена машин для `/fcw machine place`: `arc_furnace`, `crystal_puller`, `wafering_cell`, `euv_scanner_mk2`, `chip_line`.

## В гостевой системе

| Команда | Действие |
|---|---|
| `fcw redstone get` | входной сигнал на всех сторонах |
| `fcw redstone get front` | входной сигнал на одной стороне |
| `fcw redstone set top 15` | выдать сигнал на сторону |
| `fcw info` | состояние связи с игрой |

Подробнее: [Редстоун и fcw](../computers/redstone.md).
