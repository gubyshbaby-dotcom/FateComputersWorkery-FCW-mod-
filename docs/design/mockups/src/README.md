# Исходники макетов

Все PNG в `docs/design/mockups/` (кроме `rejected_*`) собраны этими скриптами (Python 3 + Pillow).

| скрипт | что делает |
|---|---|
| `terminal.py` | консоль машины 80×25 (`terminal_litho.png`, `terminal_furnace_fault.png`) и монтаж подсказки при наведении (`hint_overlay.png`) |
| `concepts.py` | схемы энергоблоков и автоматики для ТЗ художнику (`concept_energy.png`, `concept_automation.png`) |
| `factory_plan.py` | планы и разрезы демо-завода (`factory_floor1.png`, `factory_floor2.png`, `factory_roof.png`, `factory_sections.png`) |

Запуск из этой папки:

```
python3 terminal.py
python3 concepts.py
python3 factory_plan.py
```

- Консоль рисуется шрифтом самого мода: `native/crates/fcw-client/assets/terminus-uni2-8x16.psf`
  (другой путь — в `FCW_TERM_PSF`).
- `hint_overlay.png` собирается, только если заданы снимок игры и ванильный шрифт:
  `FCW_SHOT=<снимок 854×480> FCW_MC_JAR=<minecraft_1.21.1_client.jar> python3 terminal.py`
  (или `FCW_MC_ASSETS=<папка>/assets/minecraft`). Текстуры Mojang в репозиторий не кладём.
- Схемы и планы подписаны шрифтом DejaVu Sans (`/usr/share/fonts/truetype/dejavu`).

`rejected_sheet_*.png` — отклонённые окна-пульты первой версии, исходники удалены.
