# Host tool lookup for the macOS/Linux installers. Needs $TOP. On macOS, a window or a fresh terminal may not
# have Homebrew on PATH. Keep the caller's choices first, then try its standard Apple Silicon and Intel locations.
if [ "$(uname -s)" = Darwin ]; then
    for _host_bin in /opt/homebrew/bin /usr/local/bin; do
        [ -d "$_host_bin" ] || continue
        case ":$PATH:" in *":$_host_bin:"*) ;; *) PATH="$PATH:$_host_bin" ;; esac
    done
    export PATH
    unset _host_bin
fi

# adb from platform-tools unzipped next to the project (or into the usual places) needs no PATH entry.
find_adb() {
    command -v adb >/dev/null && return 0
    for d in "$TOP" "$TOP/platform-tools" "$PWD/platform-tools" "$HOME/platform-tools" "$HOME/Downloads/platform-tools" \
             "$HOME/Library/Android/sdk/platform-tools" "$HOME/Android/Sdk/platform-tools"; do
        [ -x "$d/adb" ] && { PATH="$d:$PATH"; export PATH; return 0; }
    done
    if [ "$(uname -s)" = Darwin ]; then
        for d in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}"; do
            [ -n "$d" ] && [ -x "$d/platform-tools/adb" ] && { PATH="$d/platform-tools:$PATH"; export PATH; return 0; }
        done
    fi
    return 1
}

# The other host scripts used to require adb on PATH even when install.sh had found it beside the project.
# Their Linux lookup stays as it was; macOS uses the same adb search as the installer.
if [ "$(uname -s)" = Darwin ]; then find_adb || true; fi
