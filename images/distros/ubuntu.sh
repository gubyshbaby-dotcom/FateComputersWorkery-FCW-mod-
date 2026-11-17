# Ubuntu: база ubuntu-base, пакеты через apt.
UBUNTU_VERSION="${UBUNTU_VERSION:-26.04.1}"
UBUNTU_SUITE="${UBUNTU_SUITE:-resolute}"
PRETTY="Ubuntu ${UBUNTU_VERSION%.*} LTS"
ACCENT="#e95420"
SUDO_GROUP=sudo
MIN_MEM=1024
REC_MEM=2048
UBUNTU_MIRROR="${UBUNTU_MIRROR:-https://archive.ubuntu.com/ubuntu}"
BASE_URL="https://cdimage.ubuntu.com/ubuntu-base/releases/$UBUNTU_VERSION/release/ubuntu-base-$UBUNTU_VERSION-base-amd64.tar.gz"

UBUNTU_PACKAGES=(
  linux-image-virtual dracut zstd tzdata
  systemd systemd-sysv systemd-resolved systemd-timesyncd udev dbus-user-session
  sudo login passwd locales kbd procps iproute2 iputils-ping less nano htop bash-completion
  ca-certificates
  labwc swaybg lxqt-panel lxqt-menu-data lxqt-themes alacritty pcmanfm-qt
  qt6-wayland breeze-icon-theme fonts-dejavu-core
  libgl1-mesa-dri libegl1 libegl-mesa0 libgl1 xdg-utils
  pipewire wireplumber pipewire-pulse
)

distro_unpack_base() {
  tar -xzf "$1" -C "$2" --numeric-owner
}

distro_install_packages() {
  # https-зеркала: прокси сборки пропускает только https
  cat > "$ROOT/etc/apt/sources.list.d/ubuntu.sources" <<EOF
Types: deb
URIs: $UBUNTU_MIRROR
Suites: $UBUNTU_SUITE $UBUNTU_SUITE-updates $UBUNTU_SUITE-backports
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg

Types: deb
URIs: $UBUNTU_MIRROR
Suites: $UBUNTU_SUITE-security
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
EOF
  rm -f "$ROOT/etc/apt/sources.list"
  if [[ -n "${FCW_BUILD_CA:-}" ]]; then
    echo 'Acquire::https::CAInfo "/tmp/fcw-build-ca.pem";' > "$ROOT/etc/apt/apt.conf.d/99fcw-build"
  fi
  # службы в chroot не запускаем
  printf '#!/bin/sh\nexit 101\n' > "$ROOT/usr/sbin/policy-rc.d"; chmod +x "$ROOT/usr/sbin/policy-rc.d"
  # Минимальная база Ubuntu выкидывает man и доки: оставляем как есть.
  in_root apt-get update
  in_root apt-get -y -o Dpkg::Options::=--force-confold full-upgrade
  in_root apt-get -y --no-install-recommends install "${UBUNTU_PACKAGES[@]}"
  rm -f "$ROOT/usr/sbin/policy-rc.d" "$ROOT/etc/apt/apt.conf.d/99fcw-build"
}

distro_remove_other_terminals() {
  local t
  for t in qterminal xterm foot konsole gnome-terminal ptyxis kitty xfce4-terminal lxterminal rxvt-unicode; do
    if in_root dpkg -s "$t" >/dev/null 2>&1; then in_root apt-get -y purge "$t"; fi
  done
  if [[ -x "$ROOT/usr/bin/alacritty" ]]; then
    in_root update-alternatives --install /usr/bin/x-terminal-emulator x-terminal-emulator /usr/bin/alacritty 60 || true
    in_root update-alternatives --set x-terminal-emulator /usr/bin/alacritty || true
  fi
}

distro_configure() {
  sed -i 's/^# *\(en_US.UTF-8\)/\1/; s/^# *\(ru_RU.UTF-8\)/\1/' "$ROOT/etc/locale.gen" 2>/dev/null || true
  grep -q '^en_US.UTF-8' "$ROOT/etc/locale.gen" 2>/dev/null || echo 'en_US.UTF-8 UTF-8' >> "$ROOT/etc/locale.gen"
  grep -q '^ru_RU.UTF-8' "$ROOT/etc/locale.gen" 2>/dev/null || echo 'ru_RU.UTF-8 UTF-8' >> "$ROOT/etc/locale.gen"
  in_root locale-gen >/dev/null
  printf 'LANG=en_US.UTF-8\n' > "$ROOT/etc/default/locale"
  printf 'LANG=en_US.UTF-8\n' > "$ROOT/etc/locale.conf"
}

distro_kernel_version() {
  local f
  for f in "$ROOT"/boot/vmlinuz-*; do
    [[ -f "$f" ]] && { basename "$f" | sed 's/^vmlinuz-//'; return; }
  done
}

distro_kernel_path() { echo "$ROOT/boot/vmlinuz-$1"; }

distro_make_initramfs() {
  if [[ -x "$ROOT/usr/bin/dracut" || -x "$ROOT/usr/sbin/dracut" ]]; then
    in_root dracut --force --no-hostonly --no-hostonly-cmdline --kver "$1" \
      --add-drivers "virtio_pci virtio_blk virtio_console virtio_gpu ext4" \
      --omit "plymouth network network-manager nfs iscsi multipath lvm dm mdraid crypt nvdimm fcoe biosdevname" \
      --compress zstd "$2"
  else
    printf '%s\n' virtio_pci virtio_blk virtio_console virtio_gpu ext4 > "$ROOT/etc/initramfs-tools/modules"
    sed -i 's/^MODULES=.*/MODULES=list/' "$ROOT/etc/initramfs-tools/initramfs.conf"
    in_root mkinitramfs -o "$2" "$1"
  fi
}

distro_cleanup() {
  in_root apt-get clean >/dev/null 2>&1 || true
  rm -rf "$ROOT/var/lib/apt/lists/"* "$ROOT/var/cache/apt/"*.bin
  rm -f "$ROOT"/boot/initrd.img-* 2>/dev/null || true
}
