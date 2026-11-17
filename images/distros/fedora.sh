# Fedora Linux: база из официального OCI-контейнера, пакеты через dnf5.
FEDORA_RELEASE="${FEDORA_RELEASE:-45}"
FEDORA_COMPOSE="${FEDORA_COMPOSE:-1.4}"
PRETTY="Fedora Linux $FEDORA_RELEASE"
ACCENT="#51a2da"
SUDO_GROUP=wheel
MIN_MEM=1024
REC_MEM=2048
BASE_URL="https://dl.fedoraproject.org/pub/fedora/linux/releases/$FEDORA_RELEASE/Container/x86_64/images/Fedora-Container-Base-Generic-$FEDORA_RELEASE-$FEDORA_COMPOSE.x86_64.oci.tar.xz"

FEDORA_PACKAGES=(
  kernel-core kernel-modules-core kernel-modules
  systemd systemd-udev systemd-networkd systemd-resolved dracut
  passwd shadow-utils util-linux sudo procps-ng iproute iputils less nano htop
  bash-completion kbd glibc-langpack-en glibc-langpack-ru dbus-broker
  labwc swaybg lxqt-panel lxqt-menu-data lxqt-themes alacritty pcmanfm-qt
  qt6-qtwayland breeze-icon-theme dejavu-sans-fonts dejavu-sans-mono-fonts
  mesa-dri-drivers mesa-libEGL mesa-libGL xdg-utils
  pipewire wireplumber pipewire-pulseaudio
)

distro_unpack_base() {  # $1 = oci.tar.xz, $2 = корень
  local tmp; tmp="$(mktemp -d)"
  tar -xJf "$1" -C "$tmp"
  # OCI: index.json -> manifest -> слои
  python3 -I - "$tmp" "$2" <<'PY'
import json, os, subprocess, sys
oci, root = sys.argv[1], sys.argv[2]
def blob(d): return os.path.join(oci, "blobs", *d.split(":", 1))
idx = json.load(open(os.path.join(oci, "index.json")))
man = json.load(open(blob(idx["manifests"][0]["digest"])))
if man.get("mediaType", "").endswith("index.v1+json"):
    man = json.load(open(blob(man["manifests"][0]["digest"])))
for layer in man["layers"]:
    subprocess.run(["tar", "-xf", blob(layer["digest"]), "-C", root,
                    "--numeric-owner", "--xattrs", "--xattrs-include=*"], check=True)
PY
  rm -rf "$tmp"
}

dnf_in_root() {
  local opts=(-y --setopt=install_weak_deps=False --setopt=tsflags=nodocs)
  [[ -n "${FCW_BUILD_CA:-}" ]] && opts+=(--setopt=sslcacert=/tmp/fcw-build-ca.pem)
  in_root dnf "${opts[@]}" "$@"
}

distro_install_packages() {
  # контейнерная база не ставит документацию и языки: для ВМ это нормально,
  # но ядро и модули ставить нужно — снимаем исключения контейнера
  rm -f "$ROOT/etc/dnf/protected.d/"*container* 2>/dev/null || true
  dnf_in_root upgrade --refresh
  dnf_in_root install "${FEDORA_PACKAGES[@]}"
}

distro_remove_other_terminals() {
  local t
  for t in qterminal xterm foot konsole gnome-terminal ptyxis kitty xfce4-terminal lxterminal rxvt-unicode; do
    if in_root rpm -q "$t" >/dev/null 2>&1; then in_root rpm -e --nodeps "$t"; fi
  done
}

distro_configure() {
  printf 'LANG=en_US.UTF-8\n' > "$ROOT/etc/locale.conf"
  printf 'KEYMAP=us\n' > "$ROOT/etc/vconsole.conf"
  # SELinux без политики в образе не нужен
  if [[ -f "$ROOT/etc/selinux/config" ]]; then
    sed -i 's/^SELINUX=.*/SELINUX=disabled/' "$ROOT/etc/selinux/config"
  fi
}

distro_kernel_version() {
  local d
  for d in "$ROOT"/lib/modules/*; do
    [[ -f "$d/vmlinuz" ]] && { basename "$d"; return; }
  done
}

distro_kernel_path() { echo "$ROOT/lib/modules/$1/vmlinuz"; }

distro_make_initramfs() {  # $1 = версия ядра, $2 = файл внутри корня
  in_root dracut --force --no-hostonly --no-hostonly-cmdline --kver "$1" \
    --add-drivers "virtio_pci virtio_blk virtio_console virtio_gpu ext4" \
    --omit "plymouth network network-manager nfs iscsi multipath lvm dm mdraid crypt nvdimm fcoe biosdevname" \
    --compress zstd "$2"
}

distro_cleanup() {
  in_root dnf clean all >/dev/null 2>&1 || true
  rm -rf "$ROOT/var/cache/dnf" "$ROOT/var/cache/libdnf5"
  rm -rf "$ROOT"/boot/initramfs-* "$ROOT"/boot/vmlinuz-* "$ROOT"/boot/System.map-* 2>/dev/null || true
}
