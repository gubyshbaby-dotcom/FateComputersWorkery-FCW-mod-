---
navigation:
  title: "Резка и полировка"
  parent: production/index.md
  position: 30
  icon: fcw:wafering_cell_blueprint
item_ids:
  - fcw:wafering_cell_kit
  - fcw:silicon_wafer
---

# Резка и полировка

Режет монокристалл на пластины и полирует их по одной.

| | |
|---|---|
| Размер | 8 x 3 x 3 (ширина, высота, глубина), 52 клетки |
| Постройка | проект и компоненты: [Постройка машин](construction.md) |
| Места для стопок | 4 входа, 4 выхода |
| Расход | 256 FE/т во время работы |

## Вид спереди

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/wafering_cell.nbt" />
  <MachinePorts machine="wafering_cell" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Вид сзади

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/wafering_cell.nbt" />
  <MachinePorts machine="wafering_cell" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Порты

<MachinePortTable machine="wafering_cell" />

Стороны названы так, как их видит оператор, стоящий перед пультом. Как загружать и запускать: [Управление машинами](operation.md).

## Загрузка

Слиток кладут на подачу, пластины забирают из кассеты. Вода в порту чистой воды и работа без остановок по питанию дают полировку класса A: [Брак](defects.md).

## Рецепты

<MachineRecipes type="fcw:wafering" />

## Проект

<RecipeFor id="fcw:wafering_cell_blueprint" />

Ведомость и рецепты компонентов: [Компоненты машин](components.md).
