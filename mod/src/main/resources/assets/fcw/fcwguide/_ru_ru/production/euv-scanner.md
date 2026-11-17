---
navigation:
  title: "Литограф EUV Mk2"
  parent: production/index.md
  position: 40
  icon: fcw:euv_controller
item_ids:
  - fcw:euv_controller
  - fcw:wafer_cpu
  - fcw:wafer_memory
  - fcw:wafer_gpu
  - fcw:wafer_controller
---

# Литограф EUV Mk2

Переносит рисунок фотошаблона на кремниевую пластину. Какие чипы получатся, определяет шаблон.

| | |
|---|---|
| Размер | 13 x 6 x 5 (ширина, высота, глубина), 301 клетка |
| Постройка | проект и блоки набора или блоки руками по схеме: [Постройка литографа](euv-building.md) |
| Места для стопок | 4 входа, место шаблона, 4 выхода |
| Расход | 1024–2048 FE/т во время работы |

## Вид спереди

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_scanner_mk2.nbt" />
  <MachinePorts machine="euv_scanner_mk2" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Вид сзади

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_scanner_mk2.nbt" />
  <MachinePorts machine="euv_scanner_mk2" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Порты

<MachinePortTable machine="euv_scanner_mk2" />

Стороны названы так, как их видит оператор, стоящий перед пультом. Как загружать и запускать: [Управление машинами](operation.md).

Пластины кладут в док 1, пластины с рисунком забирают из дока 2. Фотошаблон кладут в порт маски, и он остаётся в машине. Лоток олова ведёт в те же входы, что и док; рецептам этой версии олово не нужно. К порту данных подключают кабель данных от компьютера.

Закрытое помещение и вода в порту подачи охлаждения уменьшают брак: [Брак](defects.md).

## Рецепты

<MachineRecipes type="fcw:lithography" />

Шаблоны: [Фотошаблоны](reticles.md).
