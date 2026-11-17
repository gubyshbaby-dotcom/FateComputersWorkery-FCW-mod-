#!/usr/bin/env bash
# Локальная сборка переносимого QEMU без Docker: Alpine minirootfs + chroot.
#
#   sudo ./qemu/build-in-chroot.sh [OUT_DIR] [WORK_DIR]
#
# Как и сборщик образов, работает в своём пространстве монтирования, так что
# монтирования chroot не утекают на хост.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$(realpath -m "${1:-$HERE/out}")"
WORK="$(realpath -m "${2:-$HERE/work}")"
ALPINE="${ALPINE:-3.24.4}"
ARCH="$(uname -m)"

[[ $EUID -eq 0 ]] || { echo "нужен root (chroot, mount)" >&2; exit 1; }
if [[ -z "${FCW_IN_MOUNT_NS:-}" ]]; then
  exec unshare --mount --propagation private env FCW_IN_MOUNT_NS=1 "$0" "$@"
fi

ROOT="$WORK/alpine"
mkdir -p "$WORK" "$OUT"
if [[ ! -x "$ROOT/sbin/apk" ]]; then
  rm -rf --one-file-system "$ROOT"; mkdir -p "$ROOT"
  curl -fL --retry 4 "https://dl-cdn.alpinelinux.org/alpine/v${ALPINE%.*}/releases/$ARCH/alpine-minirootfs-$ALPINE-$ARCH.tar.gz" \
    | tar -xz -C "$ROOT"
fi
cp -L /etc/resolv.conf "$ROOT/etc/resolv.conf"
# CA хоста (в том числе прокси, если он есть) для apk и curl внутри
for f in /etc/ssl/certs/ca-certificates.crt /etc/pki/tls/certs/ca-bundle.crt; do
  if [[ -s "$f" ]]; then mkdir -p "$ROOT/etc/ssl/certs"; cp "$f" "$ROOT/etc/ssl/certs/ca-certificates.crt"; break; fi
done
sed -i 's|^http://|https://|' "$ROOT/etc/apk/repositories"

mount -t proc proc "$ROOT/proc"
mount --rbind /dev "$ROOT/dev"; mount --make-rslave "$ROOT/dev"
mkdir -p "$ROOT/out" "$ROOT/src"
mount --bind "$OUT" "$ROOT/out"
mount --bind "$HERE" "$ROOT/src"

env_args=(PATH=/usr/sbin:/usr/bin:/sbin:/bin HOME=/root OUT=/out WORK=/build
          QEMU_VERSION="${QEMU_VERSION:-11.1.3}")
for v in HTTPS_PROXY https_proxy JOBS; do [[ -n "${!v:-}" ]] && env_args+=("$v=${!v}"); done
chroot "$ROOT" /usr/bin/env -i "${env_args[@]}" /bin/sh /src/build-static.sh
