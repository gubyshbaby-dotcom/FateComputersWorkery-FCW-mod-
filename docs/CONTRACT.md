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
  "kernel_version": "...", "kernel": "vmlinuz", "initrd": "initrd.img", "rootfs": "rootfs.qcow2",
  "cmdline": "root=/dev/vda rw ... console=tty1 console=hvc0 ...",
  "min_memory_mb": 1024, "recommended_memory_mb": 2048,
  "desktop": "labwc + lxqt-panel", "terminal": "alacritty",
  "user": "user", "password": "fcw", "rootfs_used_bytes": 123, "accent": "#51a2da"
}
```

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
- QEMU ищется: `--qemu`, `$FCW_QEMU`, `qemu-system-x86_64` в PATH.
  Ускоритель `auto`: kvm (если /dev/kvm доступен на запись), whpx (Windows),
  hvf (macOS на x86_64), иначе tcg (`-accel tcg,thread=multi`).

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

## 7. Агент гостя

Порт virtio-serial `fcw.agent` (`/dev/virtio-ports/fcw.agent`), строки JSON:

- гость -> хост: `{"type":"hello","agent":"0.1.0","os":"Fedora Linux 45","hostname":"..."}`,
  `{"type":"redstone_set","side":"front","level":15}`, `{"type":"heartbeat"}`
- хост -> гость: `{"type":"redstone_inputs","levels":{"front":0,"back":0,"left":0,"right":0,"top":0,"bottom":0}}`,
  `{"type":"shutdown"}`, `{"type":"ping"}`, `{"type":"time","unix_ms":...}`

Внутри гостя `fcw` — один статический бинарник: `fcw agent` (служба),
`fcw redstone get [side]`, `fcw redstone set <side> <0-15>`, `fcw info`.
CLI говорит со службой через `/run/fcw/agent.sock`.
