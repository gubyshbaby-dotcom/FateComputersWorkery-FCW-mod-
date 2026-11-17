#!/usr/bin/env bash
# Установка демона FCW на узел (хост) как службы systemd — для серверов
# Minecraft в контейнерах без KVM (Pelican, Pterodactyl). Демон с QEMU
# работает на узле с доступом к /dev/kvm, мод подключается к нему по TCP.
#
#   sudo ./install-host-daemon.sh <путь к fcw-*.jar> [опции]
#
# Опции:
#   --control-addr IP:PORT  адрес канала управления (по умолчанию шлюз
#                           docker-сети pelican_nw / pterodactyl_nw / docker0
#                           и порт 25700)
#   --control-port N        порт канала управления, если адрес определяется
#                           автоматически (25700)
#   --quic-port N           UDP-порт для игроков (25600)
#
# Повторный запуск обновляет бинарник и юнит; токен сохраняется.
# Для проверки без установки: FCW_DESTDIR=/tmp/x (без пользователя и systemctl).
set -euo pipefail

die() { echo "ошибка: $*" >&2; exit 1; }
info() { echo "==> $*"; }

usage() { sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

JAR=""
CONTROL_ADDR=""
CONTROL_PORT=25700
QUIC_PORT=25600
while [[ $# -gt 0 ]]; do
  case "$1" in
    --control-addr) CONTROL_ADDR="${2:?}"; shift 2 ;;
    --control-port) CONTROL_PORT="${2:?}"; shift 2 ;;
    --quic-port) QUIC_PORT="${2:?}"; shift 2 ;;
    -h|--help) usage 0 ;;
    -*) die "неизвестная опция $1" ;;
    *) [[ -z "$JAR" ]] || die "лишний аргумент $1"; JAR="$1"; shift ;;
  esac
done
[[ -n "$JAR" ]] || usage 1
[[ -f "$JAR" ]] || die "нет файла $JAR"
[[ "$QUIC_PORT" =~ ^[0-9]+$ && "$CONTROL_PORT" =~ ^[0-9]+$ ]] || die "порт должен быть числом"

DEST="${FCW_DESTDIR:-}"
if [[ -z "$DEST" && $EUID -ne 0 ]]; then
  die "нужен root: sudo $0 ..."
fi

BIN="$DEST/usr/local/bin/fcw-daemon"
UNIT="$DEST/etc/systemd/system/fcw-daemon.service"
ETC="$DEST/etc/fcw"
TOKEN="$ETC/token"
STATE="$DEST/var/lib/fcw"

# --- бинарник из jar --------------------------------------------------------
case "$(uname -m)" in
  x86_64|amd64) ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
  *) die "архитектура $(uname -m) не поддерживается" ;;
esac
ENTRY="natives/linux-$ARCH/fcw-daemon"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
if command -v unzip >/dev/null 2>&1; then
  unzip -p "$JAR" "$ENTRY" > "$TMP" 2>/dev/null || true
elif command -v python3 >/dev/null 2>&1; then
  python3 - "$JAR" "$ENTRY" "$TMP" <<'PY' || true
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z, open(sys.argv[3], "wb") as out:
    out.write(z.read(sys.argv[2]))
PY
else
  die "нужен unzip или python3, чтобы достать демон из jar"
fi
[[ -s "$TMP" ]] || die "в $JAR нет $ENTRY"
head -c 4 "$TMP" | grep -q $'\x7fELF' || die "$ENTRY в jar — не исполняемый файл Linux"

if [[ -z "$DEST" ]] && systemctl is-active --quiet fcw-daemon 2>/dev/null; then
  info "останавливаю работающую службу (машины будут погашены)"
  systemctl stop fcw-daemon
fi
install -D -m 0755 "$TMP" "$BIN"
info "демон: $BIN ($("$BIN" --version 2>/dev/null || echo '?'))"

# --- пользователь и каталоги ------------------------------------------------
if [[ -z "$DEST" ]]; then
  if ! getent group kvm >/dev/null; then
    # без группы systemd не запустит юнит с SupplementaryGroups=kvm
    groupadd --system kvm
    info "создана группа kvm"
  fi
  if ! id fcw >/dev/null 2>&1; then
    useradd --system --user-group --home-dir /var/lib/fcw --shell /usr/sbin/nologin \
      --comment "Fate Computers Workery VM daemon" fcw
    info "создан пользователь fcw"
  fi
  [[ -e /dev/kvm ]] || echo "внимание: /dev/kvm нет — машины будут работать в программной эмуляции (медленно)" >&2
fi
OWN=(); [[ -z "$DEST" ]] && OWN=(-o fcw -g fcw)
install -d -m 0750 "${OWN[@]}" "$STATE" "$STATE/images" "$STATE/vms" "$STATE/qemu"
install -d -m 0750 "$ETC"
[[ -z "$DEST" ]] && chgrp fcw "$ETC"

# --- токен ------------------------------------------------------------------
if [[ ! -s "$TOKEN" ]]; then
  ( umask 077; head -c 32 /dev/urandom | base64 | tr '+/' '-_' | tr -d '=\n' > "$TOKEN" )
  info "создан токен $TOKEN"
fi
chmod 0600 "$TOKEN"
[[ -z "$DEST" ]] && chown fcw:fcw "$TOKEN"

# --- адрес канала управления ------------------------------------------------
SUBNET=""
if [[ -z "$CONTROL_ADDR" ]]; then
  GW=""
  if command -v docker >/dev/null 2>&1; then
    for net in pelican_nw pterodactyl_nw; do
      # у сети может быть и IPv4, и IPv6: берём только IPv4
      GW="$(docker network inspect "$net" -f '{{range .IPAM.Config}}{{.Gateway}} {{end}}' 2>/dev/null \
            | tr ' ' '\n' | grep -E '^[0-9]+(\.[0-9]+){3}$' | head -n1 || true)"
      if [[ -n "$GW" ]]; then
        SUBNET="$(docker network inspect "$net" -f '{{range .IPAM.Config}}{{.Subnet}} {{end}}' 2>/dev/null \
                  | tr ' ' '\n' | grep -E '^[0-9]+(\.[0-9]+){3}/[0-9]+$' | head -n1 || true)"
        info "docker-сеть $net: шлюз $GW${SUBNET:+, подсеть $SUBNET}"
        break
      fi
    done
  fi
  if [[ -z "$GW" ]] && command -v ip >/dev/null 2>&1; then
    GW="$(ip -4 -o addr show docker0 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -n1)"
    SUBNET="$(ip -4 -o addr show docker0 2>/dev/null | awk '{print $4}' | head -n1)"
    [[ -n "$GW" ]] && info "интерфейс docker0: $GW"
  fi
  [[ -n "$GW" ]] || die "не нашёл сеть Docker; укажите --control-addr IP:PORT (адрес узла, видимый из контейнера)"
  CONTROL_ADDR="$GW:$CONTROL_PORT"
fi
CONTROL_IP="${CONTROL_ADDR%:*}"
CONTROL_PORT="${CONTROL_ADDR##*:}"
# IPv4:порт или [IPv6]:порт
if ! [[ "$CONTROL_ADDR" =~ ^[0-9]+(\.[0-9]+){3}:[0-9]+$ || "$CONTROL_ADDR" =~ ^\[[0-9a-fA-F:]+\]:[0-9]+$ ]]; then
  die "адрес управления '$CONTROL_ADDR' не похож на IPv4:порт или [IPv6]:порт; укажите --control-addr"
fi

# --- юнит systemd (синхронно с packaging/fcw-daemon.service) ----------------
mkdir -p "$(dirname "$UNIT")"
cat > "$UNIT" <<UNIT
# FCW: демон виртуальных машин на узле. Создан install-host-daemon.sh.
[Unit]
Description=Fate Computers Workery VM daemon (QEMU/KVM)
After=network-online.target docker.service
Wants=network-online.target docker.service

[Service]
Type=simple
User=fcw
Group=fcw
SupplementaryGroups=kvm
Environment=FCW_LOG=info
ExecStart=/usr/local/bin/fcw-daemon --service \\
    --images-dir /var/lib/fcw/images \\
    --vms-dir /var/lib/fcw/vms \\
    --qemu-dir /var/lib/fcw/qemu \\
    --control-tcp $CONTROL_ADDR \\
    --token-file /etc/fcw/token \\
    --quic-bind 0.0.0.0:$QUIC_PORT \\
    --screen-max-mbps 20 --screen-max-fps 30
WorkingDirectory=/var/lib/fcw
StateDirectory=fcw
Restart=on-failure
RestartSec=5
KillMode=mixed
TimeoutStopSec=90
LimitNOFILE=65536
NoNewPrivileges=yes
ProtectSystem=strict
ReadWritePaths=/var/lib/fcw
PrivateTmp=yes
ProtectHome=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes
ProtectClock=yes
ProtectHostname=yes
RestrictSUIDSGID=yes
RestrictRealtime=yes
RestrictNamespaces=yes
LockPersonality=yes
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK

[Install]
WantedBy=multi-user.target
UNIT
info "юнит: $UNIT"

if [[ -z "$DEST" ]]; then
  systemctl daemon-reload
  systemctl enable --now fcw-daemon
  sleep 2
  systemctl --no-pager --lines=5 status fcw-daemon || true
fi

# --- что прописать в моде ---------------------------------------------------
cat <<EOF

Готово. В config/fcw-server.toml сервера Minecraft:

  [daemon]
  mode = "remote"
  remoteAddress = "$CONTROL_ADDR"
  remoteToken = "$(cat "$TOKEN")"

Игроки подключаются к демону напрямую по UDP $QUIC_PORT на этом узле:
daemon.publicHost — адрес узла, который видят игроки.

Файрвол:
  * TCP $CONTROL_PORT на $CONTROL_IP — только из сети контейнеров${SUBNET:+ ($SUBNET)}, например
      ufw allow from ${SUBNET:-<подсеть контейнеров>} to $CONTROL_IP port $CONTROL_PORT proto tcp
      iptables -I INPUT -s ${SUBNET:-<подсеть>} -d $CONTROL_IP -p tcp --dport $CONTROL_PORT -j ACCEPT
  * UDP $QUIC_PORT — открыть для игроков: ufw allow $QUIC_PORT/udp

Журнал: journalctl -u fcw-daemon -f
EOF
