---
navigation:
  title: "Компоненты машин"
  parent: production/index.md
  position: 4
  icon: fcw:electric_motor
item_ids:
  - fcw:iron_plate
  - fcw:steel_plate
  - fcw:iron_frame
  - fcw:steel_frame
  - fcw:enclosure_panel
  - fcw:granite_plate_blank
  - fcw:granite_plate
  - fcw:fireclay
  - fcw:firebrick
  - fcw:refractory_lining
  - fcw:graphite
  - fcw:graphite_electrode
  - fcw:graphite_heater
  - fcw:crucible_blank
  - fcw:quartz_crucible
  - fcw:water_cooled_panel
  - fcw:oven_module
  - fcw:copper_bar
  - fcw:copper_pipe
  - fcw:copper_coil
  - fcw:furnace_transformer
  - fcw:electric_motor
  - fcw:pump
  - fcw:hydraulic_cylinder
  - fcw:pulling_head
  - fcw:linear_stage
  - fcw:conveyor_section
  - fcw:vacuum_chamber_section
  - fcw:control_cabinet_1
  - fcw:control_cabinet_2
  - fcw:control_cabinet_3
  - fcw:steel_wire
  - fcw:diamond_wire
  - fcw:diamond_blade
  - fcw:polishing_head
  - fcw:placement_head
  - fcw:bonding_head
  - fcw:polished_mirror_blank
  - fcw:multilayer_mirror
  - fcw:laser_module
  - fcw:precision_stage
  - fcw:turbo_pump
  - fcw:chiller_unit
  - fcw:proto_chip_controller
  - fcw:proto_chip_memory
  - fcw:proto_cpu_pga
---

# Компоненты машин

Из компонентов собирают машины на стройплощадке: [Постройка машин](construction.md). Ведомости ниже даны для уровня `engineer`; на уровне `standard` нужна четверть, с округлением вверх. Свою ведомость площадки показывает команда `bom` на поддоне.

## Ведомости

### Дуговая печь

| Этап | Время | Компоненты |
|---|---|---|
| Фундамент и башня | 3 мин | Железная рама 24, Железный лист 24 |
| Горн и футеровка | 4 мин | Огнеупорная футеровка 36, Водоохлаждаемая панель 8, Железный лист 16 |
| Электроды и механика | 3 мин | Графитовый электрод 3, Гидроцилиндр 4, Электродвигатель 4, Насос 2 |
| Трансформатор и шины | 2 мин | Печной трансформатор 1, Медная шина 12 |
| Обшивка и пульт | 2 мин | Панель обшивки 32, Тонированное стекло 8, Шкаф управления I 1 |

Всего этапов: 5, время сборки: 14 мин.

### Ростовая печь

| Этап | Время | Компоненты |
|---|---|---|
| Рама и основание | 2 мин | Стальная рама 12, Стальной лист 16 |
| Камера | 3 мин | Секция вакуумной камеры 8, Водоохлаждаемая панель 6, Огнеупорная футеровка 8 |
| Тепловой узел | 2 мин | Кварцевый тигель 1, Графитовый нагреватель 4, Печной трансформатор 1, Медная шина 6 |
| Вытягивание и вакуум | 2 мин | Механизм вытягивания 1, Насос 2, Электродвигатель 2 |
| Обшивка и пульт | 1,5 мин | Панель обшивки 20, Тонированное стекло 6, Шкаф управления I 1 |

Всего этапов: 5, время сборки: 10,5 мин.

### Резка и полировка пластин

| Этап | Время | Компоненты |
|---|---|---|
| Рама и плита | 2 мин | Стальная рама 8, Стальной лист 12, Гранитная плита 6 |
| Пила | 2 мин | Алмазная проволока 8, Алмазный диск 2, Линейный привод 3, Электродвигатель 4 |
| Полировка и вода | 1,5 мин | Полировальная голова 3, Насос 2, Медная трубка 8 |
| Обшивка и пульт | 1,5 мин | Панель обшивки 16, Тонированное стекло 8, Шкаф управления II 1 |

Всего этапов: 4, время сборки: 7 мин.

### Литограф EUV Mk2

| Этап | Время | Компоненты |
|---|---|---|
| Рама и корпус | 10 мин | Каркас литографа 24, Корпус литографа 97, Корпус литографа с решёткой 12 |
| Вакуумная камера | 10 мин | Стенка вакуумной камеры 23, Турбомолекулярный насос 2 |
| Драйв-лазер и источник | 15 мин | Усилитель CO2-лазера 22, Генератор капель олова 1, Коллектор EUV 1, Чиллер 9, Силовой шкаф 1 |
| Оптика | 15 мин | Многослойное зеркало 22 |
| Столики и загрузчик | 10 мин | Столик пластины 24, Столик маски 9, Модуль робота-загрузчика 13, Порт загрузки FOUP 2 |
| Остекление, порты, пусконаладка | 10 мин | Корпус литографа со стеклом 29, Порт охлаждения 2, Порт данных 1, Ввод питания 1, Порт вытяжки 1, Порт газов 2, Люк предметов 2, Контроллер литографа 1 |

Всего этапов: 6, время сборки: 70 мин.

### Линия электроники

| Этап | Время | Компоненты |
|---|---|---|
| Рама | 2,5 мин | Стальная рама 14, Стальной лист 20 |
| Конвейер | 2,5 мин | Секция конвейера 14, Электродвигатель 6, Воронка 4 |
| Корпусировка | 3 мин | Голова разварки 2, Линейный привод 2, Насос 1 |
| Сборка плат | 3 мин | Голова установщика 2, Печной модуль 2, Линейный привод 2 |
| Обшивка, пульты, наладка | 2,5 мин | Панель обшивки 30, Тонированное стекло 12, Шкаф управления II 2, Кремниевая пластина 8 |

Всего этапов: 5, время сборки: 13,5 мин.

Блоки литографа и их рецепты: [Блоки литографа](euv-blocks.md).

## Рецепты компонентов

<RecipeFor id="fcw:iron_plate" />
<RecipeFor id="fcw:steel_plate" />
<RecipeFor id="fcw:iron_frame" />
<RecipeFor id="fcw:steel_frame" />
<RecipeFor id="fcw:enclosure_panel" />
<RecipeFor id="fcw:granite_plate_blank" />
<RecipeFor id="fcw:granite_plate" />
<RecipeFor id="fcw:fireclay" />
<RecipeFor id="fcw:firebrick" />
<RecipeFor id="fcw:refractory_lining" />
<RecipeFor id="fcw:graphite" />
<RecipeFor id="fcw:graphite_electrode" />
<RecipeFor id="fcw:graphite_heater" />
<RecipeFor id="fcw:crucible_blank" />
<RecipeFor id="fcw:quartz_crucible" />
<RecipeFor id="fcw:water_cooled_panel" />
<RecipeFor id="fcw:oven_module" />
<RecipeFor id="fcw:copper_bar" />
<RecipeFor id="fcw:copper_pipe" />
<RecipeFor id="fcw:copper_coil" />
<RecipeFor id="fcw:furnace_transformer" />
<RecipeFor id="fcw:electric_motor" />
<RecipeFor id="fcw:pump" />
<RecipeFor id="fcw:hydraulic_cylinder" />
<RecipeFor id="fcw:pulling_head" />
<RecipeFor id="fcw:linear_stage" />
<RecipeFor id="fcw:conveyor_section" />
<RecipeFor id="fcw:vacuum_chamber_section" />
<RecipeFor id="fcw:control_cabinet_1" />
<RecipeFor id="fcw:control_cabinet_2" />
<RecipeFor id="fcw:control_cabinet_3" />
<RecipeFor id="fcw:steel_wire" />
<RecipeFor id="fcw:diamond_wire" />
<RecipeFor id="fcw:diamond_blade" />
<RecipeFor id="fcw:polishing_head" />
<RecipeFor id="fcw:placement_head" />
<RecipeFor id="fcw:bonding_head" />
<RecipeFor id="fcw:polished_mirror_blank" />
<RecipeFor id="fcw:multilayer_mirror" />
<RecipeFor id="fcw:laser_module" />
<RecipeFor id="fcw:precision_stage" />
<RecipeFor id="fcw:turbo_pump" />
<RecipeFor id="fcw:chiller_unit" />

Опытные чипы для первого литографа описаны на странице [Постройка машин](construction.md).
