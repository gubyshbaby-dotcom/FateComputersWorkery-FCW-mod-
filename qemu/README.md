# Переносимый QEMU

Статические `qemu-system-x86_64` и `qemu-img` (Alpine, musl) для серверов,
где QEMU нельзя поставить пакетом: хостинги на Pelican или Pterodactyl
(игровой сервер работает в Docker-контейнере без root), shared-хостинги и т.п.

Мод скачивает его сам: если демон не нашёл QEMU, сервер загружает
`fcw-qemu-<os>-<arch>.tar.zst` из релиза `qemu` в `<папка сервера>/fcw/qemu/`
(`daemon.qemuAutoDownload`, `daemon.qemuUrlTemplate` в `config/fcw-server.toml`).
Вручную: `/fcw qemu fetch`, или распаковать архив так, чтобы получилось
`fcw/qemu/bin/qemu-system-x86_64`.

Внутри архива каталог `qemu/`:

| путь | что |
|---|---|
| `bin/qemu-system-x86_64`, `bin/qemu-img` | статические, без зависимостей |
| `share/qemu/` | BIOS, option ROM для прямой загрузки ядра, keymaps |
| `VERSION`, `SOURCE.txt` | версия QEMU, ссылка на исходники и флаги сборки |
| `LICENSE` | GNU GPL v2 |

В сборку включено только нужное демону: гость x86_64, VNC (кадры, ввод,
звук), virtio-устройства, qcow2, KVM и TCG. Без GUI, сетевых бэкендов QEMU и
внешних библиотек.

Сборка:

```bash
sudo ./qemu/build-in-chroot.sh
```

или в Docker:

```bash
docker run --rm -v "$PWD/qemu:/src:ro" -v "$PWD/out:/out" -e OUT=/out alpine:3.24 sh /src/build-static.sh
```

CI (`.github/workflows/qemu.yml`) собирает linux-x86_64 и linux-aarch64 и по
тегу `qemu-*` выкладывает архивы в релиз `qemu`.

Лицензия QEMU — GPL v2: исходный код той же версии доступен по ссылке из
`SOURCE.txt`.
