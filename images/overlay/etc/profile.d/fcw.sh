# FCW: язык из командной строки ядра (его задаёт игра: язык игрока)
for fcw_arg in $(cat /proc/cmdline 2>/dev/null); do
  case "$fcw_arg" in
    fcw.lang=*) export LANG="${fcw_arg#fcw.lang=}" ;;
    fcw.kbd=*)  export XKB_DEFAULT_LAYOUT="${fcw_arg#fcw.kbd=}" ;;
  esac
done
unset fcw_arg

# FCW: на tty1 при наличии видеокарты запускаем рабочий стол
if [ "$(tty 2>/dev/null)" = "/dev/tty1" ] && [ -z "${WAYLAND_DISPLAY:-}" ] && [ "$(id -u)" != 0 ]; then
  fcw_i=0
  while ! ls /dev/dri/card* >/dev/null 2>&1 && [ $fcw_i -lt 50 ]; do sleep 0.1; fcw_i=$((fcw_i + 1)); done
  unset fcw_i
  if ls /dev/dri/card* >/dev/null 2>&1 && [ ! -e "$HOME/.fcw-no-desktop" ]; then
    . /etc/fcw/session.env
    mkdir -p "$HOME/.cache"; labwc >"$HOME/.cache/labwc.log" 2>&1
    echo "Desktop exited. Start again: labwc   (log: ~/.cache/labwc.log)"
  fi
fi
