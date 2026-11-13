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

# Root on the device. A stock F50 whose USB debugging was turned on in its web console (index.html#usb_port, or
# scripts/device/f50-enable-adb.sh) gives adb a root shell and has no su at all; a Magisk-rooted one has su. Which of
# the two is decided once per run. A device running F50 Linux has a root shell too: installer/lib/linux-mode.sh tells
# it apart by its serial number before anything here runs.
# adb shell reads stdin: never let it eat the answers typed (or piped) into the installer.
F50_ROOT_VIA=${F50_ROOT_VIA:-}
root_via() {
    [ -z "$F50_ROOT_VIA" ] || return 0
    if [ "$(adb shell id -u </dev/null 2>/dev/null | tr -d '\r')" = 0 ]; then F50_ROOT_VIA=shell; else F50_ROOT_VIA=su; fi
}
su_do() {
    root_via
    if [ "$F50_ROOT_VIA" = shell ]; then adb shell "$1" </dev/null | tr -d '\r'
    else adb shell "su -c '$1'" </dev/null | tr -d '\r'; fi
}
