# Host tool lookup for the installers (macOS only). Needs $TOP. A window or a fresh terminal may not have
# Homebrew on PATH: keep the caller's choices first, then try the Apple Silicon and Intel Homebrew locations.
for _host_bin in /opt/homebrew/bin /usr/local/bin; do
    [ -d "$_host_bin" ] || continue
    case ":$PATH:" in *":$_host_bin:"*) ;; *) PATH="$PATH:$_host_bin" ;; esac
done
export PATH
unset _host_bin

# adb from platform-tools unzipped next to the project (or into the usual places) needs no PATH entry.
find_adb() {
    command -v adb >/dev/null && return 0
    for d in "$TOP" "$TOP/platform-tools" "$TOP/work/deploy/bin" "$PWD/platform-tools" "$HOME/platform-tools" \
             "$HOME/Downloads/platform-tools" "$HOME/Library/Android/sdk/platform-tools" \
             "${ANDROID_HOME:+$ANDROID_HOME/platform-tools}" "${ANDROID_SDK_ROOT:+$ANDROID_SDK_ROOT/platform-tools}"; do
        [ -n "$d" ] && [ -x "$d/adb" ] && { PATH="$d:$PATH"; export PATH; return 0; }
    done
    return 1
}
find_adb || true

# Root on the device: direct adb, Magisk su, or the stock F50 Samba bootstrap's local socket.
# USB debugging alone does not imply root (notably on B15). A device running F50 Linux has a root shell too:
# installer/lib/linux-mode.sh tells it apart by its serial number before anything here runs.
# adb shell reads stdin: never let it eat the answers typed (or piped) into the installer.
F50_ROOT_VIA=${F50_ROOT_VIA:-}
root_via() {
    case "$F50_ROOT_VIA" in shell|su|socket) return 0 ;; esac
    if [ "$(adb shell id -u </dev/null 2>/dev/null | tr -d '\r')" = 0 ]; then
        F50_ROOT_VIA=shell
    elif [ "$(python3 "$TOP/installer/lib/adb-root.py" --timeout 4 'id -u' 2>/dev/null)" = 0 ]; then
        F50_ROOT_VIA=socket
    elif [ "$(adb shell 'su -c "id -u"' </dev/null 2>/dev/null | tr -d '\r')" = 0 ]; then
        F50_ROOT_VIA=su
    else
        F50_ROOT_VIA=
        echo 'No usable root channel; run python3 scripts/device/f50-enable-adb.py --prepare-root from this project on stock Android.' >&2
        return 1
    fi
}
su_do() {
    root_via || return 1
    case "$F50_ROOT_VIA" in
        shell) adb shell "$1" </dev/null | tr -d '\r' ;;
        socket) python3 "$TOP/installer/lib/adb-root.py" "$1" ;;
        su) _root_cmd=$(printf '%s' "$1" | sed "s/'/'\\\\''/g")
            adb shell "su -c '$_root_cmd'" </dev/null | tr -d '\r' ;;
    esac
}
