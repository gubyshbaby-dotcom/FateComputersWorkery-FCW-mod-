---
navigation:
  title: "Линия электроники"
  parent: production/index.md
  position: 50
  icon: fcw:chip_line_kit
item_ids:
  - fcw:chip_line_kit
  - fcw:chip_controller
  - fcw:chip_memory
  - fcw:chip_gpu
---

# Линия электроники

Одна машина с двумя участками. Корпусировка режет пластину с рисунком на микросхемы и процессоры. Сборка плат ставит микросхемы на заготовку платы.

| | |
|---|---|
| Размер | 14 x 3 x 3 (ширина, высота, глубина), 102 клетки |
| Установка | комплект, ПКМ по земле |
| Слоты корпусировки | 4 входа, 4 выхода |
| Слоты сборки плат | 6 входов, 2 выхода |
| Расход | 256–768 FE/т на участок во время работы |

## Вид спереди

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/chip_line.nbt" />
  <MachinePorts machine="chip_line" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

## Вид сзади

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/chip_line.nbt" />
  <MachinePorts machine="chip_line" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Порты

<MachinePortTable machine="chip_line" />

Стороны названы так, как их видит оператор, стоящий перед пультом. Как загружать и запускать: [Управление машинами](operation.md).

Участки работают независимо и одновременно. Сборка плат берёт микросхемы не только из своих входных слотов, но и прямо из лотка корпусировки.

Из одной пластины с процессорами выходит разное число процессоров в зависимости от того, что ещё лежит во входных слотах: чем старше процессор, тем больше пластин и металла.

## Рецепты

### Корпусировка

<MachineRecipes type="fcw:chip_packaging" />

### Сборка плат

<MachineRecipes type="fcw:pcb_assembly" />

## Комплект

<RecipeFor id="fcw:chip_line_kit" />
