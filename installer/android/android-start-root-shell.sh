#!/system/bin/sh
# Started by Samba's root preexec on stock F50 firmware. Local socket only;
# this does not install Magisk or change boot/TrustOS partitions.
set -eu
[ "$(id -u)" = 0 ] || exit 1
dir=/data/local/tmp/f50-root
socket=$dir/command.sock
umask 077
[ ! -L "$dir" ] || exit 1
mkdir -p "$dir"
chown 0:2000 "$dir"
chmod 0750 "$dir"

# A socket file can survive a reboot even though its listener did not.
if [ -S "$socket" ] && [ "$(printf 'id -u\nexit\n' | /system/bin/nc -w 1 -U "$socket" 2>/dev/null)" = 0 ]; then
    exit 0
fi
rm -f "$socket"
nohup setsid /system/bin/nc -U -L -E -s "$socket" /system/bin/sh \
    </dev/null >"$dir/server.log" 2>&1 &
pid=$!
printf '%s\n' "$pid" > "$dir/server.pid"
attempt=0
while [ ! -S "$socket" ] && [ "$attempt" -lt 50 ]; do
    sleep 0.1
    attempt=$((attempt + 1))
done
[ -S "$socket" ] || { kill "$pid" 2>/dev/null || true; exit 1; }
chown 0:2000 "$socket"
chmod 0660 "$socket"
id > "$dir/server.identity"
chmod 0640 "$dir/server.identity"
chown 0:2000 "$dir/server.identity"

# Keep the startup script root-owned after the first successful bootstrap.
if [ "$0" != "$dir/start.sh" ]; then
    cp /data/local/tmp/f50-start-root-shell.sh "$dir/start.sh"
    chown 0:0 "$dir/start.sh"
    chmod 0755 "$dir/start.sh"
fi
sed -i 's|root preexec = /system/bin/sh /data/local/tmp/f50-start-root-shell.sh|root preexec = /system/bin/sh /data/local/tmp/f50-root/start.sh|' /data/samba/etc/smb.conf
setprop persist.sys.usb.config adb,rndis
start adbd
