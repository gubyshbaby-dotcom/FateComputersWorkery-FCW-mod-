---
navigation:
  title: "Постройка литографа"
  parent: production/index.md
  position: 41
  icon: fcw:euv_casing
item_ids:
  - fcw:euv_casing
  - fcw:euv_casing_glass
  - fcw:euv_casing_vent
  - fcw:euv_frame
---

# Постройка литографа

Литограф собирается из 301 блока 23 видов в объёме 13 x 6 x 5. Игрок выкладывает заготовку по схеме, затем ПКМ по контроллеру превращает её в машину.

## Порядок

1. Выбрать место 13 x 5 и сторону оператора: длинная сторона, на которой стоит контроллер.
2. Выложить шесть слоёв снизу вверх по сценам ниже. Название блока видно при наведении.
3. Порты, порты FOUP, люки предметов, силовой шкаф и контроллер ставить лицом наружу.
4. ПКМ по контроллеру. Если блок не тот или смотрит не туда, в чате будут названы клетка и нужный блок, а место отмечено красной пылью.
5. Когда всё верно, заготовка становится машиной. Включить главный рубильник: он на силовом шкафу сзади.

Операторам доступна команда `/fcw machine blank`: она выкладывает готовую заготовку перед игроком.

## Заготовка целиком

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank.nbt" />
  <IsometricCamera yaw="195" pitch="30" />
</GameScene>

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank.nbt" />
  <IsometricCamera yaw="15" pitch="30" />
</GameScene>

## Слой 1, 65 блоков

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_1.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Корпус 25, столик пластины 12, усилитель CO2-лазера 9, стенка вакуумной камеры 6, модуль робота-загрузчика 6, каркас 3, порт охлаждения 1, порт данных 1, ввод питания 1, порт газов 1.

## Слой 2, 62 блока

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_2.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Столик пластины 12, корпус 10, корпус со стеклом 9, усилитель CO2-лазера 6, модуль робота-загрузчика 6, стенка вакуумной камеры 4, чиллер 3, каркас 3, порт загрузки FOUP 2, турбомолекулярный насос 2, контроллер 1, порт охлаждения 1, порт газов 1, люк предметов 1, силовой шкаф 1.

## Слой 3, 57 блоков

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_3.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Корпус 14, многослойное зеркало 12, корпус со стеклом 11, усилитель CO2-лазера 6, стенка вакуумной камеры 6, каркас 4, чиллер 3, модуль робота-загрузчика 1.

## Слой 4, 63 блока

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_4.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Корпус 25, многослойное зеркало 10, корпус с решёткой 7, корпус со стеклом 6, стенка вакуумной камеры 5, каркас 4, чиллер 3, коллектор EUV 1, усилитель CO2-лазера 1, порт вытяжки 1.

## Слой 5, 31 блок

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_5.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Корпус 10, столик маски 9, каркас 4, корпус со стеклом 3, стенка вакуумной камеры 2, корпус с решёткой 1, генератор капель олова 1, люк предметов 1.

## Слой 6, 23 блока

<GameScene zoom="1.5" interactive={true}>
  <ImportStructure src="../assets/structures/euv_blank_layer_6.nbt" />
  <IsometricCamera yaw="195" pitch="60" />
</GameScene>

Корпус 13, каркас 6, корпус с решёткой 4.

Рецепты блоков: [Блоки литографа](euv-blocks.md).
