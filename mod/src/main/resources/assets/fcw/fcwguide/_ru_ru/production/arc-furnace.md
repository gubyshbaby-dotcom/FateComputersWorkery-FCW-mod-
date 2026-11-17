---
navigation:
  title: "Дуговая печь"
  parent: production/index.md
  position: 10
  icon: fcw:arc_furnace_blueprint
item_ids:
  - fcw:arc_furnace_kit
  - fcw:metallurgical_silicon
  - fcw:steel_ingot
  - fcw:polysilicon
---

# Дуговая печь

Плавит шихту электрической дугой. Даёт технический кремний, сталь и поликремний. Первая машина цепочки.

| | |
|---|---|
| Размер | 8 x 6 x 5 (ширина, высота, глубина), 164 клетки |
| Постройка | проект и компоненты: [Постройка машин](construction.md) |
| Места для стопок | 4 входа, 4 выхода |
| Расход | 256–384 FE/т во время работы |

## Вид спереди

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/arc_furnace.nbt" />
  <MachinePorts machine="arc_furnace" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Вид сзади

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/arc_furnace.nbt" />
  <MachinePorts machine="arc_furnace" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Порты

<MachinePortTable machine="arc_furnace" />

Стороны названы так, как их видит оператор, стоящий перед пультом. Как загружать и запускать: [Управление машинами](operation.md).

## Загрузка

Шихту кладут в проём под зонтом: двери откроются сами. Забрать шихту обратно: Shift + ПКМ пустой рукой. Слитки забирают из лотка у ковша.

## Рецепты

<MachineRecipes type="fcw:arc_furnace" />

В рецепт поликремния идёт любой кремний из тега `c:silicon`, в том числе из других модов.

## Проект

<RecipeFor id="fcw:arc_furnace_blueprint" />

Ведомость и рецепты компонентов: [Компоненты машин](components.md).
