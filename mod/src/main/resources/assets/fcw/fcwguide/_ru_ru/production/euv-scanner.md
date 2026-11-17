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
| Установка | из блоков набора: [Постройка литографа](euv-building.md) |
| Слоты | 4 входа, слот шаблона, 4 выхода |
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

Пластины загружаются и выгружаются через оба дока FOUP. Фотошаблон кладётся в порт маски и остаётся в машине. Лоток олова ведёт в те же входные слоты, что и доки; рецептам этой версии олово не нужно. Порт данных пока ничего не делает: он оставлен под управление с компьютера.

## Рецепты

<MachineRecipes type="fcw:lithography" />

Шаблоны: [Фотошаблоны](reticles.md).
