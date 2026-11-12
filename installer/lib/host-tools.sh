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
