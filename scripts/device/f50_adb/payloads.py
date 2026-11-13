"""Android shell commands used by ADB preparation and root recovery."""

FIRST_CONNECTION = r'''
case "$(getprop ro.product.model) / $(getprop ro.product.device)" in
    *F50*|*f50*|*MU300*|*mu300*) ;; *) echo F50_ERROR_NOT_F50; exit 1 ;;
esac
echo "F50_BOOT_ID=$(cat /proc/sys/kernel/random/boot_id)"
serial=$(getprop ro.serialno)
[ -n "$serial" ] || serial=$(getprop ro.boot.serialno)
echo "F50_DEVICE_SERIAL=$serial"
for prop in service.adb.tcp.port persist.adb.tcp.port persist.service.adb.tcp.port; do
    if [ "$(getprop "$prop")" != 5555 ]; then
        if setprop "$prop" 5555; then echo "F50_SETPROP=$prop:0"; else echo "F50_SETPROP=$prop:$?"; fi
    fi
    echo "F50_PROPERTY=$prop:$(getprop "$prop")"
done
sed -n 's/^\[\(.*\)\][[:space:]]*$/F50_SHARE=\1/p' /data/samba/etc/smb.conf
'''

CHECK = r'''
printf 'UID=%s\nSERVICE=%s\nPERSIST=%s\nUSB_PERSIST=%s\nUSB_CONFIG=%s\nUSB_STATE=%s\nADBD=%s\n' \
    "$(id -u)" "$(getprop service.adb.tcp.port)" "$(getprop persist.adb.tcp.port)" \
    "$(getprop persist.sys.usb.config)" "$(getprop sys.usb.config)" "$(getprop sys.usb.state)" "$(getprop init.svc.adbd)"
echo "F50_BOOT_ID=$(cat /proc/sys/kernel/random/boot_id)"
'''
WANT = ('UID=0', 'SERVICE=5555', 'PERSIST=5555', 'USB_PERSIST=adb,rndis',
        'USB_CONFIG=adb', 'USB_STATE=adb', 'ADBD=running')
IDENTITY = 'printf "F50_BOOT_ID="; cat /proc/sys/kernel/random/boot_id'

ROOT_PROBE = r'''
uid=$(id -u)
echo "F50_SHELL_UID=$uid"
echo "F50_SAMBA_ENABLE=$(settings get global samba_enable)"
if grep -Eq '^[[:space:]]*root preexec[[:space:]]*= /system/bin/sh /data/local/tmp/(f50-start-root-shell.sh|f50-root/start.sh)[[:space:]]*$' /data/samba/etc/smb.conf; then
    echo F50_ROOT_HOOK=present
else
    echo F50_ROOT_HOOK=absent
fi
if [ -S /data/local/tmp/f50-root/command.sock ]; then
    echo F50_SOCKET=present
    ls -l /data/local/tmp/f50-root/command.sock
else
    echo F50_SOCKET=absent
fi
if [ "$uid" = 0 ]; then echo F50_ROOT=shell
elif [ -S /data/local/tmp/f50-root/command.sock ] &&
     [ "$(printf 'id -u\nexit\n' | nc -w 1 -U /data/local/tmp/f50-root/command.sock)" = 0 ]; then echo F50_ROOT=socket
elif command -v su >/dev/null 2>&1 && [ "$(su -c 'id -u')" = 0 ]; then echo F50_ROOT=su
else echo F50_ROOT=none
fi
'''


def usb_payload(top):
    return r'''
set -eu
[ "$(id -u)" = 0 ] || { echo F50_ERROR_NOT_ROOT; exit 1; }
echo F50_APPLYING
for prop in service.adb.tcp.port persist.adb.tcp.port; do
    [ "$(getprop "$prop")" = 5555 ] || setprop "$prop" 5555
done
# UFI-TOOLS also uses this alias; log unsupported/denied properties without making it the success criterion.
if [ "$(getprop persist.service.adb.tcp.port)" != 5555 ]; then
    if setprop persist.service.adb.tcp.port 5555; then echo F50_TCP_ALIAS_RC=0; else echo "F50_TCP_ALIAS_RC=$?"; fi
fi
[ "$(getprop persist.sys.usb.config)" = adb,rndis ] || setprop persist.sys.usb.config adb,rndis
cat > /data/local/tmp/f50-usb-keep.sh <<'F50_USB_KEEP'
''' + (top / 'installer/android/android-usb.sh').read_text() + r'''
F50_USB_KEEP
chmod 0700 /data/local/tmp/f50-usb-keep.sh
sh /data/local/tmp/f50-usb-keep.sh keep-adb
echo F50_CONFIG_SAVED
'''


def root_bootstrap(top):
    return r'''
set -eu
case "$(getprop ro.product.model) / $(getprop ro.product.device)" in
    *F50*|*f50*|*MU300*|*mu300*) ;; *) echo F50_ERROR_NOT_F50; exit 1 ;;
esac
cfg=/data/samba/etc/smb.conf
[ -r "$cfg" ] || { echo F50_ERROR_SAMBA_NOT_READABLE; exit 1; }
# Preserve the pre-enable configuration: stock firmware may regenerate it when Samba starts.
umask 077
[ -e /data/local/tmp/f50-smb-before-root.conf ] || cp "$cfg" /data/local/tmp/f50-smb-before-root.conf
samba_before=$(settings get global samba_enable)
echo "F50_SAMBA_BEFORE=$samba_before"
if [ "$samba_before" != 1 ]; then
    if settings put global samba_enable 1; then echo F50_SAMBA_ENABLE_RC=0; else echo "F50_SAMBA_ENABLE_RC=$?"; fi
fi
echo "F50_SAMBA_AFTER=$(settings get global samba_enable)"
# After reboot the hook may already be present; only its listener needs restarting.
for starter in /data/local/tmp/f50-root/start.sh /data/local/tmp/f50-start-root-shell.sh; do
    if [ -r "$starter" ] && grep -Fq "root preexec = /system/bin/sh $starter" "$cfg"; then
        echo "F50_BOOTSTRAP=reuse:$starter"
        echo F50_ROOT_HOOK=present
        echo F50_BOOTSTRAP_SAVED
        exit 0
    fi
done
[ -w "$cfg" ] || { echo F50_ERROR_SAMBA_NOT_WRITABLE; exit 1; }
grep -q '^\[global\]' "$cfg" || { echo F50_ERROR_NO_GLOBAL_SECTION; exit 1; }
# Keep any other application's root hook intact rather than replacing it.
if grep -E '^[[:space:]]*root preexec[[:space:]]*=' "$cfg" |
   grep -vE '= /system/bin/sh /data/local/tmp/(f50-start-root-shell.sh|f50-root/start.sh)[[:space:]]*$' >/dev/null; then
    echo F50_ERROR_OTHER_ROOT_PREEXEC; exit 1
fi
echo F50_APPLYING
cat > /data/local/tmp/f50-start-root-shell.sh <<'F50_ROOT_STARTER'
''' + (top / 'installer/android/android-start-root-shell.sh').read_text() + r'''
F50_ROOT_STARTER
chmod 0755 /data/local/tmp/f50-start-root-shell.sh
tmp=/data/local/tmp/f50-smb-root.$$.conf
sed '/^[[:space:]]*root preexec[[:space:]]*=/d
/^[[:space:]]*preexec[[:space:]]*=[[:space:]]*start adbd[[:space:]]*$/d
/^[[:space:]]*map to guest[[:space:]]*=/d
/^\[global\]/a\
        root preexec = /system/bin/sh /data/local/tmp/f50-start-root-shell.sh\
        preexec = start adbd\
        map to guest = Bad User
s/guest ok = no/guest ok = yes/g
s/restrict anonymous = 1/restrict anonymous = 0/g
s/public = no/public = yes/g
s/nt acl support = no/nt acl support = yes/g' "$cfg" > "$tmp"
# The shell user may write smb.conf without permission to rename files in its directory.
cat "$tmp" > "$cfg"
rm -f "$tmp"
echo F50_BOOTSTRAP=new
if ! grep -Fq 'root preexec = /system/bin/sh /data/local/tmp/f50-start-root-shell.sh' "$cfg"; then
    echo F50_ERROR_ROOT_HOOK_NOT_SAVED
    exit 1
fi
echo F50_ROOT_HOOK=present
echo F50_BOOTSTRAP_SAVED
# USB reconfiguration can cut this session, so leave it until the root hook is saved.
if setprop persist.sys.usb.config adb,rndis; then
    echo F50_USB_SETPROP_RC=0
else
    echo "F50_USB_SETPROP_RC=$?"
fi
echo "F50_USB_READBACK=$(getprop persist.sys.usb.config)"
'''
