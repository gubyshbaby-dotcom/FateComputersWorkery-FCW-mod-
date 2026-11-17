#!/usr/bin/env bash
# Сборка гостевого образа для FCW.
#
#   sudo ./build-image.sh <fedora|ubuntu|arch> [OUT_DIR] [WORK_DIR]
#
# Результат: OUT_DIR/<id>/{manifest.json, vmlinuz, initrd.img, rootfs.qcow2}
# и рядом архив OUT_DIR/fcw-image-<id>.tar.zst для раздачи (версия в manifest.json).
#
# Этапы (каждый можно перезапустить отдельно через FCW_STAGES):
#   base       скачать официальную базу дистрибутива и распаковать её
#   packages   поставить пакеты родным менеджером внутри chroot
#   configure  общая настройка: пользователь, автологин, labwc, панель, Alacritty
#   initramfs  initrd с драйверами virtio
#   pack       ext4 -> qcow2, ядро, манифест, архив
#
# Переменные окружения:
#   FCW_STAGES          список этапов через пробел (по умолчанию все)
#   FCW_AGENT_BIN       путь к собранному fcw-agent (musl), кладётся в образ
#   FCW_BUILD_CA        PEM-файл с дополнительным корневым сертификатом,
#                       нужен только при сборке за TLS-прокси; в образ не попадает
#   FCW_DISK_SIZE       виртуальный размер корневой ФС (по умолчанию 8G)
#   FCW_SKIP_ARCHIVE=1  не делать tar.zst
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DISTRO="${1:?укажите дистрибутив: fedora | ubuntu | arch}"
OUT_DIR="$(realpath -m "${2:-$HERE/out}")"
WORK_DIR="$(realpath -m "${3:-$HERE/work}")"
STAGES="${FCW_STAGES:-base packages configure initramfs pack}"
DISK_SIZE="${FCW_DISK_SIZE:-8G}"

[[ $EUID -eq 0 ]] || { echo "нужен root (chroot, mount)" >&2; exit 1; }
[[ -f "$HERE/distros/$DISTRO.sh" ]] || { echo "нет описания дистрибутива: $DISTRO" >&2; exit 1; }

ROOT="$WORK_DIR/$DISTRO/rootfs"
CACHE="$WORK_DIR/cache"
mkdir -p "$ROOT" "$CACHE" "$OUT_DIR"

log() { printf '\e[1;36m[%s %s]\e[0m %s\n' "$DISTRO" "$(date +%H:%M:%S)" "$*"; }

# --- chroot -----------------------------------------------------------------

MOUNTED=()
RESOLV_SAVED=""
mount_chroot() {
  local m
  for m in proc sys dev dev/pts run tmp; do mkdir -p "$ROOT/$m"; done
  # корень chroot сам должен быть точкой монтирования (pacman проверяет место)
  mount --bind "$ROOT" "$ROOT";               MOUNTED+=("$ROOT")
  mount -t proc proc "$ROOT/proc";            MOUNTED+=("$ROOT/proc")
  mount --rbind /sys "$ROOT/sys";             MOUNTED+=("$ROOT/sys")
  mount --make-rslave "$ROOT/sys"
  mount --rbind /dev "$ROOT/dev";             MOUNTED+=("$ROOT/dev")
  mount --make-rslave "$ROOT/dev"
  mount -t tmpfs tmpfs "$ROOT/run";           MOUNTED+=("$ROOT/run")
  mount -t tmpfs tmpfs "$ROOT/tmp";           MOUNTED+=("$ROOT/tmp")
  # resolv.conf хоста на время сборки; потом возвращаем как было
  RESOLV_SAVED="$(mktemp -d)"
  if [[ -e "$ROOT/etc/resolv.conf" || -L "$ROOT/etc/resolv.conf" ]]; then
    cp -a "$ROOT/etc/resolv.conf" "$RESOLV_SAVED/"
  fi
  rm -f "$ROOT/etc/resolv.conf"
  cp -L /etc/resolv.conf "$ROOT/etc/resolv.conf"
  if [[ -n "${FCW_BUILD_CA:-}" ]]; then
    cp "$FCW_BUILD_CA" "$ROOT/tmp/fcw-build-ca.pem"
  fi
}

umount_chroot() {
  local i
  for ((i=${#MOUNTED[@]}-1; i>=0; i--)); do
    umount -R "${MOUNTED[$i]}" 2>/dev/null || umount -lR "${MOUNTED[$i]}" 2>/dev/null || true
  done
  MOUNTED=()
  if [[ -n "$RESOLV_SAVED" ]]; then
    rm -f "$ROOT/etc/resolv.conf"
    if [[ -e "$RESOLV_SAVED/resolv.conf" || -L "$RESOLV_SAVED/resolv.conf" ]]; then
      cp -a "$RESOLV_SAVED/resolv.conf" "$ROOT/etc/resolv.conf"
    fi
    rm -rf "$RESOLV_SAVED"; RESOLV_SAVED=""
  fi
}
trap umount_chroot EXIT

# Удаление корня только если внутри ничего не смонтировано: иначе rm пройдёт
# по bind-монтированию /dev хоста и снесёт его устройства.
mounts_under_root() {
  awk -v r="$ROOT" '$5 == r || index($5, r "/") == 1 {print $5}' /proc/self/mountinfo
}
safe_wipe_root() {
  [[ -e "$ROOT" ]] || return 0
  local left
  left="$(mounts_under_root)"
  if [[ -n "$left" ]]; then
    echo "внутри $ROOT остались точки монтирования, удалять не буду:" >&2
    echo "$left" >&2
    exit 1
  fi
  rm -rf --one-file-system "$ROOT"
}

# Команда внутри chroot с чистым окружением (прокси пробрасываем, если есть).
in_root() {
  local env=(PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
             HOME=/root LANG=C.UTF-8 TERM=dumb DEBIAN_FRONTEND=noninteractive)
  local v
  for v in HTTPS_PROXY https_proxy NO_PROXY no_proxy; do
    [[ -n "${!v:-}" ]] && env+=("$v=${!v}")
  done
  [[ -n "${FCW_BUILD_CA:-}" ]] && env+=(SSL_CERT_FILE=/tmp/fcw-build-ca.pem CURL_CA_BUNDLE=/tmp/fcw-build-ca.pem)
  chroot "$ROOT" /usr/bin/env -i "${env[@]}" "$@"
}

# shellcheck source=distros/fedora.sh
source "$HERE/distros/$DISTRO.sh"

# --- этапы ------------------------------------------------------------------

stage_base() {
  log "база: $BASE_URL"
  local file="$CACHE/$(basename "$BASE_URL")"
  if [[ ! -s "$file" ]]; then
    curl -fL --retry 4 -o "$file.part" "$BASE_URL"
    mv "$file.part" "$file"
  fi
  safe_wipe_root
  mkdir -p "$ROOT"
  distro_unpack_base "$file" "$ROOT"
}

stage_packages() {
  mount_chroot
  log "пакеты"
  distro_install_packages
  # Никаких других эмуляторов терминала, кроме Alacritty.
  distro_remove_other_terminals
  umount_chroot
}

stage_configure() {
  mount_chroot
  log "настройка"
  cp -a "$HERE/overlay/." "$ROOT/"
  # Оформление под дистрибутив
  sed -i "s/@DISTRO_NAME@/$PRETTY/g; s/@ACCENT@/$ACCENT/g" \
    "$ROOT/etc/xdg/labwc/autostart" "$ROOT/etc/xdg/labwc/themerc-override" "$ROOT/etc/xdg/alacritty/alacritty.toml" \
    "$ROOT/etc/skel/.config/alacritty/alacritty.toml" "$ROOT/etc/issue.d/fcw.issue" 2>/dev/null || true
  chmod 0440 "$ROOT/etc/sudoers.d/10-fcw-wheel"
  if [[ -f "$HERE/wallpapers/$DISTRO.png" ]]; then
    install -Dm644 "$HERE/wallpapers/$DISTRO.png" "$ROOT/usr/share/fcw/wallpaper.png"
  fi
  if [[ -f "$HERE/wallpapers/$DISTRO-menu.png" ]]; then
    install -Dm644 "$HERE/wallpapers/$DISTRO-menu.png" "$ROOT/usr/share/fcw/menu.png"
  fi
  if [[ -n "${FCW_AGENT_BIN:-}" ]]; then
    install -Dm755 "$FCW_AGENT_BIN" "$ROOT/usr/local/bin/fcw"
    ln -sf fcw "$ROOT/usr/local/bin/fcw-agent"
  fi

  echo "fcw-$DISTRO" > "$ROOT/etc/hostname"
  # часовой пояс по умолчанию UTC; настоящий передаёт демон (fcw.tz=)
  ln -sf ../usr/share/zoneinfo/UTC "$ROOT/etc/localtime"
  printf '127.0.0.1 localhost\n::1 localhost\n127.0.1.1 fcw-%s\n' "$DISTRO" > "$ROOT/etc/hosts"

  # Пользователь user / пароль fcw, root заблокирован.
  if ! in_root id user >/dev/null 2>&1; then
    in_root useradd -m -s /bin/bash -G "$SUDO_GROUP" -c "FCW user" user
  fi
  for g in video audio input render; do
    in_root getent group "$g" >/dev/null && in_root usermod -aG "$g" user || true
  done
  echo 'user:fcw' | in_root chpasswd
  in_root passwd -l root >/dev/null
  # домашний каталог всегда приводим к актуальному /etc/skel
  cp -a "$ROOT/etc/skel/." "$ROOT/home/user/"
  in_root chown -R user:user /home/user

  distro_configure

  # Службы: автологин на tty1 (рабочий стол) и hvc0 (текстовая консоль)
  in_root systemctl set-default multi-user.target
  in_root systemctl enable getty@tty1.service >/dev/null 2>&1 || true
  in_root systemctl enable systemd-networkd.service >/dev/null 2>&1 || true
  in_root systemctl enable systemd-resolved.service >/dev/null 2>&1 || true
  in_root systemctl enable systemd-timesyncd.service >/dev/null 2>&1 || true
  if [[ -x "$ROOT/usr/local/bin/fcw" ]]; then
    in_root systemctl enable fcw-agent.service
  fi
  in_root systemctl --global enable pipewire.socket pipewire-pulse.socket wireplumber.service >/dev/null 2>&1 || true
  # Всё, что тормозит загрузку на медленной виртуалке и не нужно
  for u in systemd-networkd-wait-online.service NetworkManager-wait-online.service \
           apt-daily.timer apt-daily-upgrade.timer man-db.timer motd-news.timer \
           dnf-makecache.timer fstrim.timer e2scrub_all.timer plymouth-start.service \
           ModemManager.service avahi-daemon.service cups.service; do
    in_root systemctl mask "$u" >/dev/null 2>&1 || true
  done
  : > "$ROOT/etc/machine-id"   # пустой: у каждой машины будет свой при первой загрузке
  umount_chroot
  # DNS в госте даёт systemd-resolved
  rm -f "$ROOT/etc/resolv.conf"
  ln -s ../run/systemd/resolve/stub-resolv.conf "$ROOT/etc/resolv.conf"
}

stage_initramfs() {
  mount_chroot
  log "initramfs"
  KVER="$(distro_kernel_version)"
  [[ -n "$KVER" ]] || { echo "ядро не найдено" >&2; exit 1; }
  distro_make_initramfs "$KVER" /boot/fcw-initrd.img
  umount_chroot
  # initrd храним рядом с корнем, а не внутри: в образ он не идёт
  mv "$ROOT/boot/fcw-initrd.img" "$WORK_DIR/$DISTRO/initrd.img"
}

# Убираем то, что в виртуальной машине не нужно: чужие языки, man-страницы,
# модули ядра для настоящего железа (Wi-Fi, видеокарты, ТВ-тюнеры...).
slim_rootfs() {
  local kver="$1" mods d
  if [[ -d "$ROOT/usr/share/locale" ]]; then
    find "$ROOT/usr/share/locale" -mindepth 1 -maxdepth 1 -type d \
      ! -name 'en' ! -name 'en_*' ! -name 'ru' ! -name 'ru_*' ! -name 'C*' -exec rm -rf {} +
  fi
  rm -rf "$ROOT/usr/share/man/"* "$ROOT/usr/share/info/"* "$ROOT/usr/share/gtk-doc/"* 2>/dev/null || true
  mods="$(dirname "$(distro_kernel_path "$kver")")"
  [[ -d "$mods/kernel" ]] || mods="$ROOT/lib/modules/$kver"
  for d in drivers/media drivers/net/wireless drivers/net/ethernet drivers/net/wwan drivers/net/can \
           drivers/net/usb drivers/bluetooth drivers/infiniband drivers/staging drivers/isdn drivers/iio \
           drivers/gpu/drm/amd drivers/gpu/drm/radeon drivers/gpu/drm/nouveau drivers/gpu/drm/i915 \
           drivers/gpu/drm/xe drivers/gpu/drm/nova drivers/scsi/qla2xxx drivers/scsi/lpfc drivers/scsi/mpt3sas \
           drivers/scsi/bfa drivers/scsi/be2iscsi drivers/scsi/qedf drivers/scsi/qedi drivers/net/fddi \
           sound/pci sound/usb sound/soc sound/firewire sound/isa sound/pcmcia net/bluetooth net/wireless net/mac80211; do
    rm -rf "${mods:?}/kernel/$d"
  done
  in_root_nomount depmod -a "$kver"
}

# depmod не нужны /proc и /dev, chroot без монтирования
in_root_nomount() {
  chroot "$ROOT" /usr/bin/env -i PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin "$@"
}

stage_pack() {
  local id="$DISTRO" dst="$OUT_DIR/$DISTRO" ver
  KVER="$(distro_kernel_version)"
  ver="$(date +%Y.%m.%d)"
  local build_id
  build_id="$(date -u +%Y%m%d%H%M%S)-$(head -c 8 /dev/urandom | od -An -tx1 | tr -d ' \n' | head -c 8)"
  log "упаковка (ядро $KVER)"
  rm -rf "$dst"; mkdir -p "$dst"
  cp "$(distro_kernel_path "$KVER")" "$dst/vmlinuz"
  cp "$WORK_DIR/$DISTRO/initrd.img" "$dst/initrd.img"

  chmod 0644 "$dst/vmlinuz" "$dst/initrd.img"   # демон работает не от root

  distro_cleanup
  slim_rootfs "$KVER"
  rm -rf "$ROOT/tmp/"* "$ROOT/var/tmp/"* "$ROOT/var/log/journal/"* "$ROOT/root/.cache" 2>/dev/null || true
  find "$ROOT/var/log" -type f -exec truncate -s0 {} + 2>/dev/null || true

  local raw="$WORK_DIR/$DISTRO/rootfs.ext4"
  rm -f "$raw"
  truncate -s "$DISK_SIZE" "$raw"
  mkfs.ext4 -q -F -L fcw-root -m 1 -E lazy_itable_init=1,lazy_journal_init=1 -d "$ROOT" "$raw"
  qemu-img convert -c -O qcow2 -o cluster_size=65536 "$raw" "$dst/rootfs.qcow2"
  rm -f "$raw"

  local used
  used=$(du -sb --exclude=proc --exclude=sys --exclude=dev "$ROOT" | cut -f1)
  cat > "$dst/manifest.json" <<EOF
{
  "format": 1,
  "id": "$id",
  "name": "$PRETTY",
  "version": "$ver",
  "build_id": "$build_id",
  "kernel_version": "$KVER",
  "kernel": "vmlinuz",
  "initrd": "initrd.img",
  "rootfs": "rootfs.qcow2",
  "cmdline": "root=/dev/vda rw rootfstype=ext4 console=tty1 console=hvc0 quiet loglevel=3 systemd.show_status=auto rd.udev.log_level=3 systemd.tty.term.hvc0=xterm-256color systemd.tty.rows.hvc0=25 systemd.tty.columns.hvc0=80",
  "min_memory_mb": $MIN_MEM,
  "recommended_memory_mb": $REC_MEM,
  "desktop": "labwc + lxqt-panel",
  "terminal": "alacritty",
  "user": "user",
  "password": "fcw",
  "rootfs_used_bytes": $used,
  "accent": "$ACCENT"
}
EOF
  if [[ "${FCW_SKIP_ARCHIVE:-0}" != 1 ]]; then
    tar -C "$OUT_DIR" -cf - "$DISTRO" | zstd -T0 -10 -q -o "$OUT_DIR/fcw-image-$id.tar.zst" -f
  fi
  log "готово: $dst ($(du -sh "$dst" | cut -f1))"
}

for s in $STAGES; do
  "stage_$s"
done
