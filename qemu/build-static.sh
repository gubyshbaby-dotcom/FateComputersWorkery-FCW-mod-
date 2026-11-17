#!/bin/sh
# Сборка переносимого QEMU для FCW: статические qemu-system-x86_64 и qemu-img
# (musl, без зависимостей от системы) + нужные x86-прошивки.
#
# Запускать внутри Alpine Linux от root (в CI: контейнер alpine; локально:
# build-in-chroot.sh). Результат: $OUT/fcw-qemu-linux-<arch>.tar.zst,
# внутри каталог qemu/ (bin/, share/qemu/, VERSION, LICENSE, SOURCE.txt).
#
#   QEMU_VERSION=11.1.3 OUT=/out ./build-static.sh
set -eu

QEMU_VERSION="${QEMU_VERSION:-11.1.3}"
OUT="${OUT:-$(pwd)/out}"
WORK="${WORK:-/tmp/qemu-build}"
JOBS="${JOBS:-$(nproc)}"
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) PLATFORM=linux-x86_64 ;;
  aarch64) PLATFORM=linux-aarch64 ;;
  *) echo "неизвестная архитектура $ARCH" >&2; exit 1 ;;
esac

apk add --no-cache \
  build-base linux-headers python3 samurai pkgconf perl bash bison flex git \
  xz zstd tar curl \
  glib-dev glib-static pcre2-dev pcre2-static gettext-dev gettext-static libffi-dev \
  pixman-dev pixman-static zlib-dev zlib-static zstd-dev zstd-static \
  util-linux-dev util-linux-static

mkdir -p "$WORK" "$OUT"
cd "$WORK"
TARBALL="qemu-$QEMU_VERSION.tar.xz"
[ -s "$TARBALL" ] || curl -fL --retry 4 -o "$TARBALL" "https://download.qemu.org/$TARBALL"
SHA256="$(sha256sum "$TARBALL" | cut -d' ' -f1)"
# REUSE_BUILD=1: продолжить прошлую сборку (только для отладки сборщика)
if [ "${REUSE_BUILD:-0}" != 1 ] || [ ! -d "qemu-$QEMU_VERSION/build" ]; then
  rm -rf "qemu-$QEMU_VERSION"
  tar -xJf "$TARBALL"
fi
cd "qemu-$QEMU_VERSION"

# Только то, что нужно демону FCW: x86_64-гость, VNC (кадры, звук, ввод),
# virtio-устройства, qcow2. Никаких GUI, сетевых бэкендов и внешних библиотек.
CONFIGURE_FLAGS="--target-list=x86_64-softmmu --static --prefix=/qemu
  --disable-docs --disable-user --enable-tools --disable-guest-agent
  --enable-vnc --disable-vnc-jpeg --disable-vnc-sasl --disable-png
  --enable-kvm --enable-tcg --enable-zstd
  --disable-gtk --disable-sdl --disable-opengl --disable-virglrenderer
  --disable-spice --disable-curses --disable-dbus-display --disable-vte
  --disable-gnutls --disable-nettle --disable-gcrypt --disable-crypto-afalg
  --disable-curl --disable-libssh --disable-rbd --disable-glusterfs
  --disable-libiscsi --disable-libnfs --disable-capstone --disable-slirp
  --disable-alsa --disable-pa --disable-pipewire --disable-jack --disable-oss
  --disable-sndio --disable-xen --disable-fuse --disable-linux-io-uring
  --disable-libudev --disable-mpath --disable-seccomp --disable-selinux
  --disable-lzo --disable-snappy --disable-bzip2 --disable-lzfse
  --disable-libusb --disable-usb-redir --disable-smartcard --disable-u2f
  --disable-brlapi --disable-libdaxctl --disable-libpmem --disable-numa
  --disable-cap-ng --disable-attr --disable-werror"

# В разных версиях QEMU набор опций немного отличается: оставляем только
# известные этому configure, остальные пропускаем с предупреждением.
HELP="$(./configure --help 2>&1)"
FLAGS=""
for f in $CONFIGURE_FLAGS; do
  case "$f" in
    --enable-*|--disable-*)
      name="${f#--enable-}"; name="${name#--disable-}"
      if printf '%s\n' "$HELP" | grep -qE "^[[:space:]]+$name([[:space:]]|$)"; then
        FLAGS="$FLAGS $f"
      else
        echo "configure: опции $f нет в QEMU $QEMU_VERSION, пропускаю" >&2
      fi ;;
    *) FLAGS="$FLAGS $f" ;;
  esac
done
CONFIGURE_FLAGS="$FLAGS"
# shellcheck disable=SC2086
./configure $CONFIGURE_FLAGS
# только два нужных бинарника: без тестов и остальных утилит QEMU
make -j"$JOBS" qemu-system-x86_64 qemu-img

STAGE="$WORK/stage/qemu"
rm -rf "$WORK/stage"; mkdir -p "$STAGE/bin" "$STAGE/share/qemu"
install -m755 build/qemu-system-x86_64 build/qemu-img "$STAGE/bin/"
strip "$STAGE/bin/"*
# x86-прошивки и option ROM (BIOS для прямой загрузки ядра, linuxboot),
# keymaps — готовыми лежат в исходниках (pc-bios/); edk2 и прошивки других
# архитектур не нужны
SRC_SHARE="pc-bios"
for f in bios.bin bios-256k.bin bios-microvm.bin qboot.rom linuxboot.bin linuxboot_dma.bin \
         multiboot.bin multiboot_dma.bin pvh.bin kvmvapic.bin vgabios*.bin efi-*.rom pxe-*.rom; do
  for g in $SRC_SHARE/$f; do [ -f "$g" ] && cp "$g" "$STAGE/share/qemu/"; done
done
cp -r "$SRC_SHARE/keymaps" "$STAGE/share/qemu/"
rm -f "$STAGE/share/qemu/keymaps/meson.build"
cp COPYING "$STAGE/LICENSE"
echo "$QEMU_VERSION" > "$STAGE/VERSION"
cat > "$STAGE/SOURCE.txt" <<EOF
QEMU $QEMU_VERSION, статическая сборка для Fate Computers Workery.
Лицензия: GNU GPL v2 (LICENSE). Исходный код:
  https://download.qemu.org/$TARBALL
  sha256 $SHA256
Сборка: qemu/build-static.sh из репозитория мода, Alpine Linux (musl).
configure:
$(echo $CONFIGURE_FLAGS)
EOF

# проверка: бинарники действительно статические и запускаются
file "$STAGE/bin/qemu-system-x86_64" 2>/dev/null || true
"$STAGE/bin/qemu-system-x86_64" --version
"$STAGE/bin/qemu-img" --version
if ldd "$STAGE/bin/qemu-system-x86_64" 2>&1 | grep -q '=>'; then
  echo "qemu-system-x86_64 не статический" >&2; exit 1
fi

tar -C "$WORK/stage" -cf - qemu | zstd -19 -T0 -q -o "$OUT/fcw-qemu-$PLATFORM.tar.zst" -f
ls -la "$OUT/fcw-qemu-$PLATFORM.tar.zst"
