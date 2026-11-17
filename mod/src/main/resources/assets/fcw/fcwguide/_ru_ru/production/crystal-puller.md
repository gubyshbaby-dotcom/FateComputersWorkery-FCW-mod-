---
navigation:
  title: "Ростовая печь"
  parent: production/index.md
  position: 20
  icon: fcw:crystal_puller_blueprint
item_ids:
  - fcw:crystal_puller_kit
  - fcw:silicon_boule
---

# Ростовая печь

Выращивает монокристалл кремния из расплава поликремния по методу Чохральского.

| | |
|---|---|
| Размер | 5 x 7 x 4 (ширина, высота, глубина), 75 клеток |
| Постройка | проект и компоненты: [Постройка машин](construction.md) |
| Места для стопок | 4 входа, 4 выхода |
| Расход | 512 FE/т во время работы |

## Вид спереди

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/crystal_puller.nbt" />
  <MachinePorts machine="crystal_puller" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Вид сзади

<GameScene zoom="2" interactive={true}>
  <ImportStructure src="../assets/structures/crystal_puller.nbt" />
  <MachinePorts machine="crystal_puller" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Порты

<MachinePortTable machine="crystal_puller" />

Стороны названы так, как их видит оператор, стоящий перед пультом. Как загружать и запускать: [Управление машинами](operation.md).

## Загрузка

Поликремний кладут в загрузочный люк, слиток забирают из двери выгрузки. Вода в порту подачи охлаждения и работа без остановок по питанию дают слиток класса A: [Брак](defects.md).

## Рецепты

<MachineRecipes type="fcw:crystal_puller" />

## Проект

<RecipeFor id="fcw:crystal_puller_blueprint" />

Ведомость и рецепты компонентов: [Компоненты машин](components.md).
