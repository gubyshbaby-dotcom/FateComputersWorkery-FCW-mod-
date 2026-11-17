# FCW: контракты между частями

Схема: мод (Java, NeoForge 1.21.1) на сервере и клиенте, демон ВМ (Rust,
отдельный процесс), гостевые ВМ под QEMU, клиентская нативная библиотека
(Rust, JNI). Пиксели и ввод идут мимо игрового тика: клиент игрока говорит с
демоном по QUIC напрямую, сервер Minecraft только выдаёт токены.

```
repo/
  mod/      мод NeoForge (Gradle, ModDevGradle), modid "fcw", пакет com.fatecomputers.fcw
  native/   Cargo workspace
    proto/  control.proto (мод <-> демон), stream.proto (клиент <-> демон)
    crates/fcw-proto    сгенерированные типы prost + кадрирование
    crates/fcw-daemon   демон (bin)
    crates/fcw-client   клиентская библиотека (cdylib, JNI)
    crates/fcw-probe    консольный клиент для проверок без игры (bin)
    crates/fcw-agent    агент гостя (bin, x86_64-unknown-linux-musl)
  images/   сборка гостевых образов (bash, root, chroot)
  docs/
```

## 1. Пакет гостевого образа

`<images_dir>/<id>/`:

| файл | что |
|---|---|
| `manifest.json` | описание (ниже) |
| `vmlinuz` | ядро дистрибутива |
| `initrd.img` | initramfs с virtio_pci, virtio_blk, virtio_console, virtio_gpu, ext4 |
| `rootfs.qcow2` | корневая ext4 **без таблицы разделов**, гость видит её как `/dev/vda` |

```json
{
  "format": 1, "id": "fedora", "name": "Fedora Linux 45", "version": "2026.11.17",
  "build_id": "20261117093000-1a2b3c4d",
  "kernel_version": "...", "kernel": "vmlinuz", "initrd": "initrd.img", "rootfs": "rootfs.qcow2",
  "cmdline": "root=/dev/vda rw ... console=tty1 console=hvc0 ...",
  "min_memory_mb": 1024, "recommended_memory_mb": 2048,
  "desktop": "labwc + lxqt-panel", "terminal": "alacritty",
  "user": "user", "password": "fcw", "rootfs_used_bytes": 123, "accent": "#51a2da"
}
```

Каждая машина закреплена за конкретной сборкой образа (`build_id`, иначе
`version`): демон держит копию в `<images_dir>/.store/<id>/<build>/`, слой
диска и ядро берутся оттуда, поэтому обновление образа не ломает старые машины.

Раздаётся архивом `fcw-image-<id>.tar.zst` (внутри каталог `<id>/`, версия в manifest.json),
в релизе GitHub с тегом `images`.

Внутри гостя:
- tty1: автологин `user`, при наличии `/dev/dri/card*` запускается labwc
  (pixman) + lxqt-panel, терминал Alacritty. Без видеокарты остаётся только консоль.
- hvc0 (virtio-console): автологин `user`, TERM=xterm-256color, 80x25.
  Это «текстовый экран» машины.
- virtio-serial порт с именем `fcw.agent` -> служба `fcw agent` (агент мира).
- Сеть: systemd-networkd, DHCP на любом ethernet.
- Звук: PipeWire, нужен virtio-sound.
- Дополнительные параметры ядра от демона: `fcw.lang=ru_RU.UTF-8`, `fcw.kbd=us,ru`,
  `fcw.tz=Europe/Moscow` (часовой пояс хоста; по умолчанию в образе UTC).

## 2. Запуск демона

Мод (серверная сторона) запускает демон сам:

```
fcw-daemon --images-dir <gameDir>/fcw/images --vms-dir <world>/fcw/vms \
           --control <путь к unix-сокету> --quic-bind 0.0.0.0:25600 \
           [--qemu <qemu-system-x86_64>] [--max-vms N] [--accel auto|kvm|whpx|hvf|tcg]
```

- Готовность: одна строка в stdout
  `FCW-READY {"control":"<путь>","quic_port":25600,"cert_sha256":"<hex>","accelerator":"kvm"}`.
  Остальной вывод демона (логи) идёт в stderr.
- Демон завершается, когда закрывается его stdin (умер родитель), и по `Shutdown`.
  При выходе гасит все ВМ (ACPI, потом kill).
- Сертификат QUIC самоподписанный, хранится в `--vms-dir/../daemon-cert.der` +
  ключ, переживает перезапуск.
- QEMU ищется при каждом запуске машины (так скачанный QEMU подхватывается
  без перезапуска демона): `--qemu`, `$FCW_QEMU`, переносимый QEMU в
  `--qemu-dir` (по умолчанию `<images-dir>/../qemu`, то есть
  `<gameDir>/fcw/qemu/bin/qemu-system-x86_64`), `qemu-system-x86_64` в PATH.
  qemu-img берётся рядом с найденным QEMU, иначе из PATH. Для переносимого
  QEMU демон передаёт `-L <qemu-dir>/share/qemu`.
- Переносимый QEMU: архив `fcw-qemu-<os>-<arch>.tar.zst` из релиза `qemu`;
  внутри каталог `qemu/` с `bin/qemu-system-x86_64`, `bin/qemu-img`,
  `share/qemu/` (BIOS, option ROM, keymaps), `VERSION`, `LICENSE`, `SOURCE.txt`.
  Скачивается запросом `FetchQemu` (распаковка во временный каталог рядом и
  переименование в `--qemu-dir`). Нужен на хостингах без пакетов (Pelican).
  Ускоритель `auto`: kvm (если /dev/kvm доступен на запись), whpx (Windows),
  hvf (macOS на x86_64), иначе tcg (`-accel tcg,thread=multi`).

### Демон на отдельном хосте (служба)

Когда сервер Minecraft работает в контейнере без доступа к KVM (Pelican,
Pterodactyl), демон запускают на самом узле как службу systemd, а мод
подключается к нему по TCP:

```
fcw-daemon --service --images-dir /var/lib/fcw/images --vms-dir /var/lib/fcw/vms \
           --control-tcp 172.18.0.1:25700 --token-file /etc/fcw/token \
           --quic-bind 0.0.0.0:<UDP-порт для игроков>
```

- `--service`: не завершаться при закрытии stdin; выход по SIGTERM/SIGINT
  (машины гасятся так же, как при обычном выходе). `--control` (Unix-сокет)
  в этом режиме необязателен.
- `--control-tcp <ip:port>`: слушать канал управления по TCP (тот же формат
  кадров). Если адреса ещё нет (интерфейс Docker поднимется позже), демон
  повторяет bind каждые 2 с.
- `--token-file <путь>`: общий секрет для TCP. Если файла нет, демон создаёт
  его (32 случайных байта, base64url, права 0600) и пишет путь в журнал.
  Первое сообщение TCP-соединения — Hello с верным `auth_token` (сравнение за
  постоянное время), иначе ответ с ошибкой «unauthorized» и разрыв.
- Подключений может быть несколько (перезапуск сервера Minecraft): события
  рассылаются всем авторизованным. `Shutdown` по TCP в режиме службы
  отклоняется — службу останавливает systemd.
- Мод в режиме `daemon.mode = "remote"` (`daemon.remoteAddress`,
  `daemon.remoteToken`) не запускает свой демон, подключается к удалённому и
  переподключается при обрыве; при остановке сервера гасит свои машины
  (StopVm), а не демон.

## 3. Канал управления (мод <-> демон)

`native/proto/control.proto`, пакет `fcw.control.v1`. Unix stream-сокет, кадр
= u32 BE длина + `Envelope`. Запросы с id > 0, ответ с тем же id, события id = 0.
Первым запросом идёт `Hello`.

Машина на стороне демона:
- `vms-dir/<vm_id>/config.json` (последний CreateVm), `disk.qcow2` (слой
  поверх `images/<id>/rootfs.qcow2`, создаётся `qemu-img create -b`),
  сокеты QMP, VNC, консоли и агента.
- QEMU примерно так:

```
qemu-system-x86_64 -name fcw-<vm> -machine q35 -accel <..> -cpu <host|max> -smp N -m M
  -nodefaults -no-user-config -display none
  -kernel vmlinuz -initrd initrd.img -append "<cmdline> fcw.lang=.. fcw.kbd=.."
  -drive if=none,id=root,file=disk.qcow2,format=qcow2,discard=unmap -device virtio-blk-pci,drive=root
  -device virtio-serial-pci
  -chardev socket,id=con0,path=tty.sock,server=on,wait=off -device virtconsole,chardev=con0
  -chardev socket,id=ag0,path=agent.sock,server=on,wait=off -device virtserialport,chardev=ag0,name=fcw.agent
  -object rng-random,id=rng0,filename=/dev/urandom -device virtio-rng-pci,rng=rng0
  -qmp unix:qmp.sock,server=on,wait=off
  # видеокарта есть:
  -vga none -device virtio-gpu-pci,xres=W,yres=H,edid=on
  -device virtio-keyboard-pci -device virtio-tablet-pci
  -vnc unix:vnc.sock[,audiodev=snd0]
  # колонки есть:
  -audiodev none,id=snd0 -device virtio-sound-pci,audiodev=snd0
```

- Экран `gfx0`: демон — единственный VNC-клиент QEMU (RFB 3.8, Raw +
  DesktopSize + Cursor + QEMU Extended Key Event + QEMU Audio). Обновления
  складываются в тайлы 64x64, отдаются подписчикам как `FrameUpdate`.
- Экран `tty0`: байты с `tty.sock` идут в эмулятор терминала (80x25), изменения
  строк отдаются как `TextUpdate`. Ввод в текстовый экран — байты в тот же сокет.
- Пауза/продолжение: QMP `stop` / `cont` (мод ставит на паузу при выгрузке чанка).
- Остановка: QMP `system_powerdown`, по таймауту `quit`/kill.
- Ввод: один управляющий игрок на машину (последний предъявивший токен с
  KEYBOARD|MOUSE; предыдущему приходит ReleaseAll).

### Сеть гостей (крейт fcw-net)

Если у машины `network=true`, демон подключает к ней virtio-net через свой
userspace-свитч:

```
-netdev stream,id=net0,server=off,addr.type=unix,addr.path=<vm>/net.sock,reconnect=1
-device virtio-net-pci,netdev=net0,mac=<NetSwitch::mac_for_vm(vm_id)>,romfile=
```

- L2-свитч между всеми машинами демона; шлюз 10.42.0.1 (DHCP, DNS, NAT).
- DNS: `<hostname>.mc` для машин, остальное через резолвер хоста.
- NAT наружу (если разрешён): TCP через userspace-стек, UDP-потоки; для 443
  при наличии HTTPS_PROXY соединения идут через CONNECT.
- Файрвол: запрещены loopback, RFC1918, link-local, CGNAT, мультикаст, адреса
  самого хоста и шлюз (кроме DHCP/DNS/ICMP), порт 25; списки allow/deny.
- Лимиты: скорость (по умолчанию 8 МиБ/с на машину), число соединений.
- Имя машины передаётся в гостя параметром ядра `systemd.hostname=`.

Флаги демона: `--network on|off` (по умолчанию on), `--internet on|off`
(по умолчанию on), `--net-subnet`, `--net-rate-limit <байт/с>`,
`--net-allow <CIDR>`, `--net-deny <CIDR>` (повторяемые), `--upstream-proxy <url>`
(по умолчанию из HTTPS_PROXY).

## 4. Клиент <-> демон (QUIC)

`native/proto/stream.proto`, пакет `fcw.stream.v1`, ALPN `fcw/1`. Подробности
потоков описаны в шапке файла. Токен одноразовой привязки: 32 случайных байта
в base64url, живёт ttl, привязан к vm_id, игроку и правам.

Клавиши в протоколе — коды Linux evdev. Перевод GLFW -> evdev делает
клиентская библиотека.

## 5. Клиентская библиотека (JNI)

Класс `com.fatecomputers.fcw.client.nativelib.FcwNative`, все методы `static native`:

```java
long   connect(String host, int port, String certSha256Hex); // сразу возвращает сессию, соединение в фоне
int    sessionState(long session);   // 0 соединяется, 1 готово, 2 ошибка, 3 закрыто
String sessionError(long session);
void   close(long session);
void   attach(long session, String token);            // результат придёт событием
long   openView(long session, String vmId, String screenId, int maxFps); // подписка; 0 = ошибка
void   closeView(long view);
// width, height, kind (1 граф., 2 текст), seq, cursorVisible, cursorX, cursorY
boolean viewInfo(long view, int[] out7);
// Копирует изменившиеся области в direct-буфер RGBA (width*height*4).
// В rects пишет x,y,w,h по 4 числа; возвращает число областей, -1 = обновить всё,
// 0 = ничего не изменилось. Текстовые экраны библиотека сама рисует шрифтом в RGBA.
int    copyDirty(long view, java.nio.ByteBuffer dst, int[] rects);
void   sendKey(long session, String vmId, int glfwKey, int glfwScancode, boolean down);
void   sendText(long session, String vmId, String text);
void   sendPointer(long session, String vmId, int x, int y);        // датаграмма
void   sendButtons(long session, String vmId, int mask, int x, int y);
void   sendWheel(long session, String vmId, int dx, int dy, int x, int y);
void   releaseAll(long session, String vmId);
String pollEvent(long session); // JSON-строка или null: attach, error, vm_state, screen_changed
// Звук (Opus -> PCM 48 кГц стерео s16), для позиционного OpenAL на стороне Java
int    readAudio(long session, String vmId, short[] pcm);           // число сэмплов
```

Курсор: библиотека сама рисует курсор гостя поверх кадра в буфере вида
(позиция от демона; для управляющего клиента — локальная позиция сразу).

## 6. Нативные файлы в jar мода

```
/natives/<os>-<arch>/fcw-daemon[.exe]
/natives/<os>-<arch>/libfcw_client.so | fcw_client.dll | libfcw_client.dylib
```
`os` = linux | windows | macos, `arch` = x86_64 | aarch64. Мод распаковывает
их в `<gameDir>/fcw/natives/<sha256>/` и грузит `System.load`. Путь к своему
демону можно задать в конфиге сервера.

Ещё один файл не для хоста, а для гостей:

```
/natives/guest-x86_64/fcw      агент гостя (статический musl, крейт fcw-agent)
```

Его мод не распаковывает, а рассылает в ВМ (§7, «Самообновление агента»). В jar он
попадает из `native/target/x86_64-unknown-linux-musl/release/fcw` (локальная сборка)
или из `-Pfcw.nativesDir=<dir>/guest-x86_64/fcw` (CI: артефакт `fcw-agent`). Нет
файла — нет обновления, остальное работает.

## 7. Агент гостя

Порт virtio-serial `fcw.agent` (`/dev/virtio-ports/fcw.agent`), строки JSON:

- гость -> хост: `{"type":"hello","agent":"0.1.0","os":"Fedora Linux 45","hostname":"...","sha256":"<hex>","update":1}`,
  `{"type":"redstone_set","side":"front","level":15}`, `{"type":"heartbeat"}`
- хост -> гость: `{"type":"redstone_inputs","levels":{"front":0,"back":0,"left":0,"right":0,"top":0,"bottom":0}}`,
  `{"type":"shutdown"}`, `{"type":"ping"}`, `{"type":"time","unix_ms":...}`

Демон строки агента не разбирает дальше `hello`, `heartbeat` и `redstone_set`: всё
остальное он пересылает моду событием `AgentMessage`, а строки мода — гостю запросом
`AgentSend`. Строка не длиннее 64 КиБ.

Внутри гостя `fcw` — один статический бинарник: `fcw agent` (служба),
`fcw redstone get [side]`, `fcw redstone set <side> <0-15>`, `fcw info`,
`fcw machine …`. CLI говорит со службой через `/run/fcw/agent.sock` (одна строка
JSON туда, одна обратно; для проверок сокет можно задать в `FCW_AGENT_SOCK`).

### Машины: `machine_req` / `machine_resp`

Сообщения и поведение `fcw machine` описаны в `docs/MACHINE_CONTROL.md`. Здесь —
только то, как запрос идёт внутри гостя:

- CLI шлёт службе `{"cmd":"machine","req":{"type":"machine_req","op":"exec",…}}` без `id`;
- служба ставит свой `id` (счётчик), пишет `machine_req` в порт и ждёт `machine_resp`
  с тем же `id` 5 с; несколько клиентов сокета могут ждать одновременно;
- ответ клиенту: `{"ok":true,"resp":{…machine_resp…}}` или `{"ok":false,"error":"timeout"}`;
  во втором случае (и если службы нет) CLI пишет «агент не отвечает» и выходит с кодом 2.

### Машины: APC, отчёт и подписка на снимки

Обрабатывает `production/machine/net/MachineBridge` (сторона мода). Правила расчёта — `docs/PRODUCTION.md`,
раздел «Брак».

гость → хост:

```json
{"type":"apc_beat","machines":["euv-1","cz-1"],"shares":{"euv-1":8}}
{"type":"machine_watch","id":21,"machines":["euv-1"],"hz":4}
{"type":"machine_unwatch"}
{"type":"machine_req","id":22,"op":"yield","json":true,"lang":"ru_ru"}
```

хост → гость:

```json
{"type":"machine_frame","id":21,"seq":57,"machines":[{…status --json…}]}
```

- `apc_beat` — служба `fcw apc` жива; слать раз в секунду. Три пропуска (70 тиков) — служба считается
  остановленной, машины получают 75 % CU. `machines` (необязательно) — каким машинам отдавать CU;
  пусто — всем машинам сети, которым APC нужен, поровну. `shares` (необязательно) — CU по именам;
  сумма не проверяется, каждая доля не больше CU процессора. Ответа нет.
- `machine_watch` — подписка: не больше 8 машин, `hz` от 0,2 до 4 (по умолчанию 4, чаще нельзя).
  Одна подписка на компьютер: новая заменяет прежнюю. Подписка кончается с `machine_unwatch`,
  выключением ВМ или потерей агента.
- `machine_frame` — снимки подписанных машин: каждый элемент `machines` — тот же объект, что отдаёт
  `status --json` (состав — `docs/design/CONSOLE_APPS.md`). `seq` растёт на 1; кадр, который не
  помещается в строку агента, пропускается.
- `machine_req` с `op` = `yield` — отчёт за смену по всем машинам сети: `json` =
  `{"machines":[{"name","kind","today":{…},"yesterday":{…},"litho":{…}}]}`, `text` — то же
  строками, как у команды пульта `yield`.

### Самообновление агента

Агент зашит в образ (`/usr/local/bin/fcw`), но новые команды не должны требовать
пересборки образов: мод носит свежий бинарник в своём jar (§6) и ставит его в гостя.

1. В `hello` агент сообщает `sha256` своего исполняемого файла и `"update":1`
   («умею обновляться»). Агенты старых образов этих полей не шлют и не обновляются:
   такие образы нужно один раз выпустить заново.
2. Если `sha256` отличается от хэша бинарника в jar (и `vm.updateGuestAgent = true`),
   мод шлёт:

   ```json
   {"type":"update_begin","version":"0.1.0","size":705328,"sha256":"<hex>","chunks":16}
   {"type":"update_chunk","seq":0,"data":"<base64, до 45 000 байт>"}
   …
   {"type":"update_end"}
   ```

   Передача идёт с подтверждениями, по одному сообщению: на `update_begin` агент
   отвечает `{"type":"update_ack","seq":0}`, на кусок `seq` = N —
   `{"type":"update_ack","seq":N+1}` («принято кусков»). Следующее сообщение мод шлёт
   только после подтверждения; 20 с тишины — передача брошена.
3. Агент пишет куски в `<свой путь>.new` (`/usr/local/bin/fcw.new`), считая SHA-256 на
   лету. На `update_end` он сверяет размер и хэш, ставит права 0755, запускает
   `fcw.new --version` (файл, который не стартует, на место не попадёт), делает
   `rename` поверх себя и отвечает `{"type":"update_result","ok":true}`.
   Любая ошибка — `{"type":"update_result","ok":false,"error":"…"}`, временный файл
   удаляется, старый агент работает дальше.
4. После успеха служба заменяет свой процесс новым бинарником (`exec` с теми же
   аргументами; если `exec` не удался — выходит, и systemd с `Restart=always`
   поднимает уже новый файл) и снова шлёт `hello`, теперь с новым хэшем.
5. Если после обновления агент всё ещё сообщает чужой хэш, мод повторяет попытку
   только после следующего включения машины — чтобы не зациклиться.

Файл меняется на диске машины, так что обновление переживает перезагрузку гостя.
