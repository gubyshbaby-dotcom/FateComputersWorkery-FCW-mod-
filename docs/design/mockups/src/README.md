# Исходники макетов

Все PNG в `docs/design/mockups/` собраны этими скриптами (Python 3 + Pillow).

| скрипт | что делает |
|---|---|
| `fcwui.py` | библиотека: токены цветов, каркас пульта (9-slice), стекло, слоты, кнопки, шрифты |
| `screens.py` | макеты GUI: литограф (6 состояний), линия электроники, дуговая печь, генератор, ИБП (два скина), манипулятор, станция; сводные листы; GIF мигания |
| `atlas.py` | первый проход атласа `atlas/machine_gui.png` (256×256) + `machine_gui.json` с координатами, атлас шрифта терминала `atlas/term_font.png` |
| `concepts.py` | схемы энергоблоков и автоматики для ТЗ художнику |
| `factory_plan.py` | планы и разрезы демо-завода |

Запуск:

```
FCW_MC_JAR=~/.gradle/caches/neoformruntime/artifacts/minecraft_1.21.1_client.jar python3 screens.py
FCW_MC_JAR=... python3 atlas.py
python3 concepts.py; python3 factory_plan.py
```

Заголовки на корпусе пульта рисуются ванильным шрифтом Minecraft: он читается из клиентского jar
(`FCW_MC_JAR`) или из распакованной папки (`FCW_MC_ASSETS=<...>/assets/minecraft`); текстуры Mojang в
репозиторий не кладём. Текст на экране терминала: Terminus 6×12 (SIL OFL 1.1), путь к TTF можно задать
в `FCW_TERM_TTF` (по умолчанию пакет `fonts-terminus` / `xfonts-terminus` в Debian: `TerminusTTF-4.46.0.ttf`).
Схемы и планы подписаны DejaVu Sans.
