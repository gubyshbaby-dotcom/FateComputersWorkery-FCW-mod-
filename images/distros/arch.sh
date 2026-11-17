# Arch Linux: база archlinux-bootstrap, пакеты через pacman.
PRETTY="Arch Linux"
ACCENT="#1793d1"
SUDO_GROUP=wheel
MIN_MEM=1024
REC_MEM=2048
ARCH_MIRROR="${ARCH_MIRROR:-https://geo.mirror.pkgbuild.com}"
BASE_URL="$ARCH_MIRROR/iso/latest/archlinux-bootstrap-x86_64.tar.zst"

ARCH_PACKAGES=(
  linux mkinitcpio sudo less nano htop bash-completion iproute2 iputils inetutils
  labwc swaybg lxqt-panel lxqt-menu-data lxqt-themes alacritty pcmanfm-qt
  qt6-wayland breeze-icons ttf-dejavu mesa xdg-utils
  pipewire wireplumber pipewire-pulse
)

distro_unpack_base() {
  local tmp; tmp="$(mktemp -d)"
  tar --zstd -xf "$1" -C "$tmp" --numeric-owner
  cp -a "$tmp/root.x86_64/." "$2/"
  rm -rf "$tmp"
}

distro_install_packages() {
  echo "Server = $ARCH_MIRROR/\$repo/os/\$arch" > "$ROOT/etc/pacman.d/mirrorlist"
  cp "$ROOT/etc/pacman.conf" "$ROOT/etc/pacman.conf.fcw-orig"
  sed -i 's/^#\?ParallelDownloads.*/ParallelDownloads = 8/' "$ROOT/etc/pacman.conf"
  # не тащим man/doc/локали в ВМ
  sed -i 's|^#\?NoExtract.*|NoExtract = usr/share/doc/* usr/share/man/* usr/share/info/* usr/share/gtk-doc/* usr/share/help/*|' "$ROOT/etc/pacman.conf"
  cp "$ROOT/etc/pacman.conf" "$ROOT/etc/pacman.conf.fcw-final"
  if [[ -n "${FCW_BUILD_CA:-}" ]]; then
    # внешний загрузчик с CA прокси; он работает от root, поэтому без DownloadUser
    sed -i "/^\[options\]/a XferCommand = /usr/bin/curl --cacert /tmp/fcw-build-ca.pem -L -C - -f -s -o %o %u\nDisableSandbox" "$ROOT/etc/pacman.conf"
    sed -i 's/^DownloadUser/#DownloadUser/' "$ROOT/etc/pacman.conf"
  fi
  in_root pacman-key --init
  in_root pacman-key --populate archlinux
  in_root pacman -Syu --noconfirm --needed "${ARCH_PACKAGES[@]}"
  mv "$ROOT/etc/pacman.conf.fcw-final" "$ROOT/etc/pacman.conf"
  rm -f "$ROOT/etc/pacman.conf.fcw-orig"
}

distro_remove_other_terminals() {
  local t
  for t in qterminal xterm foot konsole gnome-terminal ptyxis kitty xfce4-terminal lxterminal rxvt-unicode; do
    if in_root pacman -Q "$t" >/dev/null 2>&1; then in_root pacman -Rdd --noconfirm "$t"; fi
  done
}

distro_configure() {
  # Несколько зеркал: если одно недоступно из сети сервера (бывает, что
  # провайдер режет отдельные хостинги), pacman переходит к следующему.
  cat > "$ROOT/etc/pacman.d/mirrorlist" <<'EOF'
## FCW: зеркала Arch Linux, сверху вниз. Подобрать самые быстрые для своей сети:
##   sudo pacman -S reflector && sudo reflector --latest 20 --sort rate --save /etc/pacman.d/mirrorlist
Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch
Server = https://mirror.yandex.ru/archlinux/$repo/os/$arch
Server = https://mirrors.kernel.org/archlinux/$repo/os/$arch
Server = https://mirror.rackspace.com/archlinux/$repo/os/$arch
Server = https://mirror.leaseweb.net/archlinux/$repo/os/$arch
Server = https://ftp.halifax.rwth-aachen.de/archlinux/$repo/os/$arch
EOF
  sed -i 's/^#\(en_US.UTF-8\)/\1/; s/^#\(ru_RU.UTF-8\)/\1/' "$ROOT/etc/locale.gen"
  in_root locale-gen >/dev/null
  printf 'LANG=en_US.UTF-8\n' > "$ROOT/etc/locale.conf"
  printf 'KEYMAP=us\n' > "$ROOT/etc/vconsole.conf"
}

distro_kernel_version() {
  local d
  for d in "$ROOT"/usr/lib/modules/*; do
    [[ -f "$d/vmlinuz" ]] && { basename "$d"; return; }
  done
}

distro_kernel_path() { echo "$ROOT/usr/lib/modules/$1/vmlinuz"; }

distro_make_initramfs() {
  cat > "$ROOT/etc/fcw-mkinitcpio.conf" <<'EOF'
MODULES=(virtio_pci virtio_blk virtio_console virtio_gpu ext4)
BINARIES=()
FILES=()
HOOKS=(base udev modconf filesystems)
COMPRESSION="zstd"
EOF
  # после чистки модулей mkinitcpio ругается на отсутствующие мягкие
  # зависимости; образ при этом собирается, virtio-модули в нём есть
  in_root mkinitcpio -c /etc/fcw-mkinitcpio.conf -k "$1" -g "$2" || [[ -s "$ROOT$2" ]]
}

distro_cleanup() {
  rm -rf "$ROOT/var/cache/pacman/pkg/"*
  rm -f "$ROOT"/boot/initramfs-linux*.img 2>/dev/null || true
  # ключи pacman пересоздаются на машине при первом обновлении
}
