#!/bin/sh
# Stock F50: keep USB ADB available and establish a root channel for the installer. macOS or Linux.
# Reuse direct root, su or the local root socket; otherwise bootstrap through writable Samba configuration.
# Normally run through `make adb`; `make install` runs it first. Complete configurations are only checked.
#
# What it needs (README, "First installation"):
#   * the F50 on its stock firmware, plugged into this computer by a data cable,
#   * USB debugging on: in the F50 web console, http://192.168.0.1/index.html#usb_port (log in with the
#     admin password, not the Wi-Fi password), turn ADB debugging on and save. Without that, this script can
#     do it itself with the admin password, the way the F50-ADB tool (aaadddbbb.exe) does on Windows.
#
# Prepares TCP 5555 in the first USB shell, then restores actual adb-only USB functions through root.
# Non-root firmware also needs the Samba guest/root-preexec bootstrap and a local Unix root socket.
# Protocol and commands: work/analysis/F50-ADB-aaadddbbb-2026-11-12/提取命令.txt. Nothing is flashed.
set -eu
TOP=$(cd "$(dirname "$0")/../.." && pwd)
. "$TOP/installer/lib/host-tools.sh"
find_adb || { echo '缺少 adb：请安装 Android platform-tools（macOS: brew install android-platform-tools）。' >&2; exit 1; }
command -v python3 >/dev/null || { echo '缺少 Python 3。' >&2; exit 1; }
# Build the small userspace SMB client before touching USB, never during its short connection window.
_need_smb=1
for _arg in "$@"; do case "$_arg" in --adb-only|-h|--help) _need_smb=0 ;; esac; done
F50_SMB_TRIGGER="$TOP/work/bin/f50-smb-trigger"
if [ "$_need_smb" = 1 ]; then
    _build_smb=0
    [ -x "$F50_SMB_TRIGGER" ] || _build_smb=1
    for _source in "$TOP/scripts/device/smb-trigger/"*; do
        if [ "$_source" -nt "$F50_SMB_TRIGGER" ]; then _build_smb=1; fi
    done
    if [ "$_build_smb" = 1 ]; then
        command -v go >/dev/null || { echo '首次运行需要 Go 1.20+ 构建 SMB 客户端；尚未操作设备。' >&2; exit 1; }
        mkdir -p "$TOP/work/bin"
        _smb_tmp="$F50_SMB_TRIGGER.tmp.$$"
        trap 'rm -f "$_smb_tmp"' EXIT
        echo '准备用户态 SMB 客户端（首次运行会下载固定版本的 Go 依赖）……'
        (cd "$TOP/scripts/device/smb-trigger" && go build -mod=readonly -trimpath -o "$_smb_tmp" .)
        mv "$_smb_tmp" "$F50_SMB_TRIGGER"
        trap - EXIT
    fi
fi
export F50_SMB_TRIGGER
exec python3 - "$TOP" "$@" <<'PY'
import argparse
import getpass
import hashlib
import http.client
import ipaddress
import json
import logging
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time
from urllib.parse import urlencode

TOP = Path(sys.argv.pop(1))
parser = argparse.ArgumentParser(
    description='原厂 F50：准备 USB/TCP ADB 与 root 通道，连续观察 30 秒后才报告保持成功。',
    epilog='前置条件：数据线连接电脑；在 http://192.168.0.1/index.html#usb_port 开启 ADB 调试，'
           '或在提示时输入后台密码由本脚本开启。')
parser.add_argument('--ip', default='192.168.0.1', help='F50 后台 IPv4 地址（默认 192.168.0.1）')
parser.add_argument('--port', type=int, help='F50 后台 Web 端口（默认先试 80，再试 8080）')
parser.add_argument('--serial', help='只操作指定的 USB ADB 序列号；默认只允许一台 USB 设备')
parser.add_argument('--adb-only', action='store_true', help='只恢复并确认 USB ADB，不写入 Samba/root 配置')
parser.add_argument('--wireless', action='store_true',
                    help='兼容旧参数；默认流程已包含 TCP 5555 备用连接')
args = parser.parse_args()
try:
    ipaddress.IPv4Address(args.ip)
except ValueError:
    parser.error('--ip 必须是 IPv4 地址')
if args.serial and ':' in args.serial:
    parser.error('--serial 应为 USB 序列号，不能是无线 ADB 地址')

ADB = shutil.which('adb')
USB = ['-s', args.serial] if args.serial else ['-d']
ENDPOINT = args.ip + ':5555'
CONSOLE = f'http://{args.ip}/index.html#usb_port'
WAIT = 300   # seconds for adb to show up after USB debugging was turned on
ROOT_WAIT = 10  # once ADB has appeared, fail promptly if the Samba/root stage cannot complete
EXPECTED_BOOT = None
DEVICE_SERIAL = args.serial
TCP_LAST_RECONNECT = 0
TCP_LAST_STATE = None
SMB_SHARES = []
SMB_TRIGGER = os.environ['F50_SMB_TRIGGER']
# Use the local ADB server even if the parent terminal selected a remote server/device.
ENV = dict(os.environ, LC_ALL='C')
for key in ('ANDROID_SERIAL', 'ADB_SERVER_SOCKET', 'ANDROID_ADB_SERVER_ADDRESS', 'ANDROID_ADB_SERVER_PORT'):
    ENV.pop(key, None)
ACTIVE = set()
PROCESS_LOCK = threading.Lock()
STOP = threading.Event()
LOG_DIR = TOP / 'work/logs'
LOG_DIR.mkdir(parents=True, exist_ok=True)
LOG_PATH = LOG_DIR / f'f50-adb-{time.strftime("%Y%m%d-%H%M%S")}-{os.getpid()}.log'
LOG_FILE = os.fdopen(os.open(LOG_PATH, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w', encoding='utf-8')
LOG = logging.getLogger('f50-adb')
LOG.setLevel(logging.INFO)
for destination in (sys.stdout, LOG_FILE):
    handler = logging.StreamHandler(destination)
    handler.setFormatter(logging.Formatter('[%(asctime)s] %(message)s', datefmt='%H:%M:%S'))
    LOG.addHandler(handler)


def say(message):
    LOG.info(message)


def run(command, timeout=8, label=None):
    started = time.monotonic()
    if label:
        say(f'{label}：开始（超时 {timeout:.1f}s）')
    with PROCESS_LOCK:
        if STOP.is_set():
            raise RuntimeError('任务已停止')
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, errors='replace',
                                   env=ENV, start_new_session=True)
        ACTIVE.add(process)
    try:
        output, _ = process.communicate(timeout=timeout)
    except BaseException as exc:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        output, _ = process.communicate()
        if label:
            reason = '超时' if isinstance(exc, subprocess.TimeoutExpired) else type(exc).__name__
            say(f'{label}：{reason}，耗时 {time.monotonic() - started:.2f}s')
            if output.strip():
                say(f'{label} 输出：\n{output.strip()}')
        raise
    finally:
        with PROCESS_LOCK:
            ACTIVE.discard(process)
    if label:
        say(f'{label}：退出码 {process.returncode}，耗时 {time.monotonic() - started:.2f}s')
        say(f'{label} 输出：\n{output.strip() or "（无输出）"}')
    return process.returncode, output.strip()


def require_f50_wifi():
    # USB networking disappears in adb-only mode. A phone hotspot cannot carry the
    # LAN fallback to the F50; check this before toggling USB or rebooting it.
    if sys.platform != 'darwin':
        say('请确认 Wi-Fi 已连接 F50；本机未自动核对无线网段。')
        return
    rc, ports = run(['/usr/sbin/networksetup', '-listallhardwareports'], timeout=3)
    if rc:
        raise RuntimeError('无法读取 Mac Wi-Fi 网卡信息，尚未操作设备：' + ports)
    addresses = []
    for block in ports.split('\n\n'):
        if not re.search(r'Hardware Port:\s*(Wi-Fi|AirPort)\s*(?:\n|$)', block, re.I):
            continue
        device = re.search(r'^Device:\s*(\S+)', block, re.M)
        if not device:
            continue
        interface = device.group(1)
        rc, config = run(['/sbin/ifconfig', interface], timeout=3)
        if rc:
            continue
        for address, mask in re.findall(r'\binet ([0-9.]+) netmask (0x[0-9a-fA-F]+|[0-9.]+)', config):
            netmask = str(ipaddress.IPv4Address(int(mask, 16))) if mask.startswith('0x') else mask
            network = ipaddress.IPv4Network(f'{address}/{netmask}', strict=False)
            addresses.append(f'{interface}={address} ({network})')
            if ipaddress.IPv4Address(args.ip) in network:
                say(f'Wi-Fi 网段检查通过：{interface}={address}，目标 {args.ip} 同网段；请确认连接的是 F50 热点。')
                run(['/sbin/route', '-n', 'get', args.ip], timeout=3, label='当前 F50 网络路径')
                return
    raise RuntimeError('Wi-Fi 不在 F50 的本地网段，不能作为 USB 掉线后的备用通道。'
                       f'\n当前 Wi-Fi：{", ".join(addresses) or "没有 IPv4 地址"}；目标：{args.ip}。'
                       '\n请连接 F50 自己的 Wi-Fi，保持 USB 线连接，再运行 make adb。尚未切换 USB 或重启设备。')


# --- the F50 web console (goform), only when the admin password is given

WEB_PORT = None


def request(fields, cookie='', post=False, sent=None, port=None, timeout=8):
    port = port or WEB_PORT
    path = '/goform/goform_set_cmd_process' if post else '/goform/goform_get_cmd_process'
    headers = {'User-Agent': 'Mozilla/5.0', 'Referer': f'http://{args.ip}:{port}/index.html'}
    if cookie:
        headers['Cookie'] = cookie
    body = None
    if post:
        headers['Content-Type'] = 'application/x-www-form-urlencoded; charset=UTF-8'
        body = urlencode(fields)
    else:
        path += '?' + urlencode(dict(fields, isTest='false', _=str(time.time_ns() // 1000000)))
    connection = http.client.HTTPConnection(args.ip, port, timeout=timeout)
    # Log only the operation name, never password/hash, AD, LD, RD or Cookie.
    operation = fields.get('goformId', fields.get('cmd', 'goform'))
    say(f'Web {"POST" if post else "GET"} {args.ip}:{port}：{operation}')
    try:
        connection.request('POST' if post else 'GET', path, body=body, headers=headers)
        if sent:
            sent()  # Arm the adb waiter as soon as the reboot is sent, before the HTTP response.
        response = connection.getresponse()
        say(f'Web {operation}：HTTP {response.status}')
        data = response.read()
        if response.status != 200:
            raise RuntimeError(f'设备 Web 接口返回 HTTP {response.status}')
        result = json.loads(data)
        if not isinstance(result, dict):
            raise ValueError('设备 Web 接口返回了无法识别的 JSON')
        return result, (response.getheader('Set-Cookie') or '').split(';', 1)[0]
    finally:
        connection.close()


def find_web_port():
    # The console is on port 80 (http://192.168.0.1/index.html); some firmware also serves it on 8080.
    tried = []
    for port in ([args.port] if args.port else [80, 8080]):
        try:
            result, _ = request({'cmd': 'LD'}, port=port)
            if result.get('LD'):
                return port
            tried.append(f'{port}: 无 LD')
        except (OSError, http.client.HTTPException, ValueError, RuntimeError) as exc:
            tried.append(f'{port}: {exc}')
    raise RuntimeError(f'连不上 F50 后台 {args.ip}（{"; ".join(tried)}）。请确认电脑连着 F50 的 Wi-Fi 或 USB 网络，'
                       f'并能在浏览器打开 {CONSOLE}')


WEB_ERRORS = (OSError, http.client.HTTPException, ValueError, RuntimeError)


def usb_switch(cookie=''):
    # Some firmware requires the authenticated session even for this read.
    try:
        result, _ = request({'cmd': 'usb_port_switch'}, cookie)
    except WEB_ERRORS as exc:
        say(f'后台 USB 开关读回失败，状态未知：{exc}')
        return ''
    state = str(result.get('usb_port_switch', ''))
    if state not in ('0', '1'):
        say(f'后台 USB 开关状态未知：{result.get("Error", "返回中没有有效的 usb_port_switch")}')
        return ''
    return state


def wait_web(seconds=15):
    # Changing the USB setting re-enumerates the F50's USB functions: a computer on its USB network loses the
    # link for a few seconds (Errno 51, timeouts) until the interface is back and has an address again.
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            if request({'cmd': 'LD'}, timeout=min(2, deadline - time.monotonic()))[0].get('LD'):
                return
        except WEB_ERRORS as exc:
            say(f'等待 USB 网络恢复：{exc}')
        time.sleep(min(0.5, max(0, deadline - time.monotonic())))
    raise RuntimeError(f'{seconds} 秒内没有连回 F50 后台 {args.ip}（USB 网络没有恢复）。'
                       f'请连接 F50 Wi-Fi，在 {CONSOLE} 恢复打开 USB 调试；不要继续盲目重启。')


def prepare_post(password, fields):
    # Like zreq.exe: log in again for every request, SHA256 challenge, the session cookie, then the AD token.
    def digest(value):
        return hashlib.sha256(value.encode('utf-8')).hexdigest().upper()

    ld, _ = request({'cmd': 'LD'})
    login, cookie = request({'goformId': 'LOGIN', 'isTest': 'false',
                             'password': digest(digest(password) + ld['LD'])}, post=True)
    if not cookie or str(login.get('result', '')) not in ('0', 'success'):
        raise RuntimeError(f'后台登录失败（{login}）：请检查后台密码（不是 Wi-Fi 密码）')
    info, _ = request({'cmd': 'Language,cr_version,wa_inner_version', 'multi_data': '1'}, cookie)
    rd, _ = request({'cmd': 'RD'}, cookie)
    ad = digest(digest(info['wa_inner_version'] + info['cr_version']) + rd['RD'])
    return dict(fields, isTest='false', AD=ad), cookie


def web_ok(result):
    return str(result.get('result', '')).lower() in ('0', 'success')


def ensure_web_samba(password):
    # UFI-TOOLS main.js initSMBStatus/handleSambaPath: enable sharing before preparing its root hook.
    try:
        fields, cookie = prepare_post(password, {'goformId': 'SAMBA_SETTING', 'samba_switch': '1'})
        state, _ = request({'cmd': 'samba_switch'}, cookie)
        if str(state.get('samba_switch', '')) == '1':
            say('后台文件共享已开启，不重复设置。')
            return
        say('通过原厂 SAMBA_SETTING 开启文件共享……')
        result, _ = request(fields, cookie, post=True)
        if not web_ok(result):
            raise RuntimeError(f'SAMBA_SETTING 返回：{result}')
        state, _ = request({'cmd': 'samba_switch'}, cookie)
        value = str(state.get('samba_switch', ''))
        say(f'后台文件共享读回：{value if value in ("0", "1") else "未知"}；后续仍检查实际 SMB 会话。')
    except WEB_ERRORS as exc:
        say(f'后台文件共享未确认：{exc}；捕获 ADB 后会检查本机 samba_enable。')


# --- The first transient USB shell must prepare TCP before any root/SMB work.
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

PAYLOAD = r'''
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
''' + (TOP / 'installer/android/android-usb.sh').read_text() + r'''
F50_USB_KEEP
chmod 0700 /data/local/tmp/f50-usb-keep.sh
sh /data/local/tmp/f50-usb-keep.sh keep-adb
echo F50_CONFIG_SAVED
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
ROOT_BOOTSTRAP = r'''
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
''' + (TOP / 'installer/android/android-start-root-shell.sh').read_text() + r'''
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


def root_method(target, timeout=6):
    try:
        rc, output = run([ADB, *target, 'shell', '-T', ROOT_PROBE], timeout=timeout, label='root 探测')
    except subprocess.TimeoutExpired:
        return None, ''
    if rc == 0:
        for method in ('shell', 'socket', 'su'):
            if 'F50_ROOT=' + method in output.splitlines():
                return method, output
    return None, output


def run_root(target, method, command, timeout=8, label='root 命令'):
    command = boot_guard() + command
    if method == 'socket':
        selector = ['--serial', target[1]] if target[0] == '-s' else ['--usb']
        return run([sys.executable, str(TOP / 'installer/lib/adb-root.py'), '--adb', ADB,
                    *selector, '--timeout', str(max(0.1, timeout - 0.25)), command], timeout=timeout,
                   label=f'{label}（socket）')
    remote = ('su -c ' if method == 'su' else 'sh -c ') + shlex.quote(command)
    return run([ADB, *target, 'shell', '-T', remote], timeout=timeout, label=f'{label}（{method}）')


def boot_id(output):
    return next((line.partition('=')[2].strip() for line in output.splitlines()
                 if line.startswith('F50_BOOT_ID=')), '')


def boot_guard():
    if not EXPECTED_BOOT:
        return ''
    return ('[ "$(cat /proc/sys/kernel/random/boot_id)" = ' + shlex.quote(EXPECTED_BOOT) +
            ' ] || { echo F50_ERROR_BOOT_CHANGED; exit 1; };\n')


def remember_device(output):
    global EXPECTED_BOOT, SMB_SHARES, DEVICE_SERIAL, USB
    found = boot_id(output)
    if not re.fullmatch(r'[0-9a-fA-F-]{36}', found):
        raise RuntimeError('没有取得本次 USB 设备的启动标识，拒绝切换到未确认身份的网络设备。')
    if EXPECTED_BOOT and EXPECTED_BOOT != found:
        raise RuntimeError('设备启动标识变化，请重新执行 make adb。')
    serial = next((line.partition('=')[2].strip() for line in output.splitlines()
                   if line.startswith('F50_DEVICE_SERIAL=')), '')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+', serial):
        raise RuntimeError('未取得明确的物理序列号，停止后续设备写入。')
    if DEVICE_SERIAL and DEVICE_SERIAL != serial:
        raise RuntimeError('设备序列号变化，停止后续设备写入。')
    DEVICE_SERIAL = serial
    USB = ['-s', serial]
    EXPECTED_BOOT = found
    SMB_SHARES = [line.partition('=')[2].strip() for line in output.splitlines()
                  if line.startswith('F50_SHARE=')]
    SMB_SHARES = [name for name in SMB_SHARES
                  if name and name.upper() not in ('GLOBAL', 'IPC$', 'ADMIN$', 'C$')]
    say(f'已记录 USB 设备身份，启动标识 {EXPECTED_BOOT}；共享：{SMB_SHARES or "待枚举"}')


def probe_transport(target, deadline):
    remaining = deadline - time.monotonic()
    if not EXPECTED_BOOT or remaining <= 0:
        return False
    try:
        rc, output = run([ADB, *target, 'shell', '-T', IDENTITY], timeout=min(1, remaining))
    except subprocess.TimeoutExpired:
        return False
    return rc == 0 and boot_id(output) == EXPECTED_BOOT


def tcp_target(deadline):
    global TCP_LAST_RECONNECT, TCP_LAST_STATE
    remaining = deadline - time.monotonic()
    if not EXPECTED_BOOT or remaining <= 0:
        return None
    target = ['-s', ENDPOINT]
    try:
        rc, state = run([ADB, *target, 'get-state'], timeout=min(0.5, remaining))
        if (rc, state) != TCP_LAST_STATE:
            say(f'TCP ADB 状态：{state or "无返回"}（退出码 {rc}）；仅 device 且身份匹配才可用。')
            TCP_LAST_STATE = (rc, state)
        if 'unauthorized' in state.lower():
            return None
        if rc or state != 'device':
            if time.monotonic() - TCP_LAST_RECONNECT < 2:
                return None
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            TCP_LAST_RECONNECT = time.monotonic()
            if state == 'offline' or 'device offline' in state.lower():
                # Drop only this stale transport; never restart the user's whole ADB server.
                run([ADB, 'disconnect', ENDPOINT], timeout=min(0.5, remaining), label='清理指定地址的 offline TCP 连接')
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return None
            with socket.create_connection((args.ip, 5555), timeout=min(0.5, remaining)):
                pass
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            run([ADB, 'connect', ENDPOINT], timeout=min(1, remaining), label='连接 TCP 备用 ADB')
        if probe_transport(target, deadline):
            return target
    except (OSError, subprocess.TimeoutExpired):
        pass
    return None


def live_target(deadline):
    if probe_transport(USB, deadline):
        return USB
    target = tcp_target(deadline)
    if target:
        say('USB 暂不可用，已核对启动标识，使用同一设备的 TCP ADB。')
    return target


def start_tcp(target, deadline=None):
    deadline = min(deadline or float('inf'), time.monotonic() + 6)
    if tcp_target(min(deadline, time.monotonic() + 2)):
        say('同一设备的 TCP 5555 已可用，无需重启 adbd。')
        return
    # Setting the properties alone may not reopen adbd's listener until it restarts.
    if not probe_transport(target, min(deadline, time.monotonic() + 1)):
        say('原 USB 会话已断开，跳过向未确认的目标发送 tcpip。')
        return
    try:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return
        rc, output = run([ADB, *target, 'tcpip', '5555'], timeout=min(3, remaining),
                         label='启用 TCP 5555（adbd 会短暂重连）')
        if rc:
            say('TCP 启用请求未完成，将通过实际连接确认：' + output)
    except subprocess.TimeoutExpired:
        say('TCP 启用请求期间连接中断，将通过实际连接确认。')


def wait_root(target, deadline=None):
    say(f'尝试通过用户态 SMB 触发 root（最多 {ROOT_WAIT}s）。')
    deadline = min(deadline or float('inf'), time.monotonic() + ROOT_WAIT)
    last = '尚未取得 root'
    repaired = False
    smb_attempts = 0
    next_smb_try = 0
    while time.monotonic() < deadline:
        current = live_target(deadline)
        if current and time.monotonic() < deadline:
            method, status = root_method(current, timeout=min(2, deadline - time.monotonic()))
            if method:
                say(f'root 通道已就绪：{method}，transport={current}，UID=0。')
                return current, method
            # Starting the stock sharing service may regenerate smb.conf. Re-read the hook
            # and repair our own preparation once, rather than flooding writes on every retry.
            if not repaired and ('F50_ROOT_HOOK=absent' in status or
                                 ('F50_SAMBA_ENABLE=' in status and 'F50_SAMBA_ENABLE=1' not in status.splitlines())):
                repaired = True
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    break
                try:
                    rc, detail = run([ADB, *current, 'shell', '-T',
                                      'sh -c ' + shlex.quote(boot_guard() + ROOT_BOOTSTRAP)],
                                     timeout=min(2, remaining), label='重新核对并准备文件共享/root hook')
                    if 'F50_ERROR_' in detail:
                        raise RuntimeError('文件共享准备失败：' + detail)
                except subprocess.TimeoutExpired:
                    say('文件共享准备时连接中断；只读回状态，不立即重复写入。')
        if time.monotonic() >= deadline:
            break
        if smb_attempts < 3 and time.monotonic() >= next_smb_try:
            smb_attempts += 1
            try:
                last = trigger_samba(deadline)
            except (OSError, subprocess.TimeoutExpired) as exc:
                last = str(exc)
                say('SMB 触发失败：' + last)
            next_smb_try = time.monotonic() + 1
        time.sleep(min(0.2, max(0, deadline - time.monotonic())))
    raise RuntimeError(f'{ROOT_WAIT}s 内没有确认 UID=0，已停止。最后 SMB 结果：{last[-1200:]}\n日志：{LOG_PATH}')


def inject_configuration(target):
    # Prepare both TCP properties in the very first shell, before slow root probes.
    deadline = time.monotonic() + WAIT
    dispatch = (FIRST_CONNECTION + '\nprobe=$(\n' + ROOT_PROBE + '\n)\nprintf "%s\\n" "$probe"\n'
                'if printf "%s\\n" "$probe" | grep -Eq "^F50_ROOT=(shell|socket|su)$"; then exit 0; fi\n'
                + ROOT_BOOTSTRAP)
    while time.monotonic() < deadline:
        if EXPECTED_BOOT:
            current = live_target(deadline)
            if not current:
                time.sleep(0.2)
                continue
            prefix = []
            timeout = min(8, max(0.05, deadline - time.monotonic()))
        else:
            current = target
            prefix = ['wait-for-usb-device']
            timeout = max(1, deadline - time.monotonic())
        try:
            rc, output = run([ADB, *current, *prefix, 'shell', '-T',
                              'sh -c ' + shlex.quote(boot_guard() + dispatch)],
                             timeout=timeout, label='捕获 ADB 窗口并准备 TCP/Samba')
        except subprocess.TimeoutExpired:
            if not EXPECTED_BOOT:
                raise
            say('准备会话超时；重新核对 USB/TCP 身份和状态。')
            continue
        if 'F50_ERROR_NOT_F50' in output or 'F50_ERROR_BOOT_CHANGED' in output or 'more than one' in output:
            raise RuntimeError('目标设备不明确，已停止：' + output)
        if not boot_id(output):
            time.sleep(0.2)
            continue
        remember_device(output)
        start_tcp(USB if current == target else current)
        if 'F50_ERROR_' in output:
            raise RuntimeError('已尝试保持 ADB，但 root 配置未完成：\n' + output)
        if 'F50_BOOTSTRAP_SAVED' in output or any('F50_ROOT=' + method in output.splitlines()
                                                 for method in ('shell', 'su', 'socket')):
            return wait_root(USB)
        say('准备完成前断开；将通过已核对身份的 USB 或 TCP 继续，尚未确认 root。')
        time.sleep(0.2)
    raise RuntimeError(f'{WAIT}s 内未完成 USB/TCP 准备，详见 {LOG_PATH}')


def usb_target():
    rc, output = run([ADB, *USB, 'get-state'], timeout=4, label='USB ADB 状态')
    if 'more than one' in output:
        raise RuntimeError('发现多台 USB 设备，请使用 --serial 或只连接 F50')
    return USB if rc == 0 and output == 'device' else None


def wait_usb_only():
    # Read actual runtime properties in the first usable shell; do not inject root here.
    command = "id; getprop sys.usb.config; getprop sys.usb.state; getprop persist.sys.usb.config; echo F50_USB_ADB_CONNECTED"
    rc, output = run([ADB, *USB, 'wait-for-usb-device', 'shell', '-T', command], timeout=60,
                     label='等待实际 USB ADB 并读取 USB 模式')
    if rc or 'F50_USB_ADB_CONNECTED' not in output.splitlines():
        raise RuntimeError('尚未确认可用的 USB ADB shell：' + output)
    say('已确认 USB ADB shell 可用；本次只恢复 ADB，不进入 root 注入。')


# --- Samba trigger for root bootstrap and optional wireless ADB

def trigger_samba(deadline):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError('SMB 阶段已到达总等待上限')
    # Known share names avoid the IPC$/srvsvc enumeration path that smbutil stalled in.
    command = [SMB_TRIGGER, '--address', args.ip + ':445', '--timeout', str(min(6, remaining))]
    for name in SMB_SHARES:
        command += ['--share', name]
    rc, output = run(command, timeout=remaining, label='用户态 SMB 会话与共享访问')
    if rc == 0 and 'F50_SMB_TRIGGERED' in output.splitlines():
        return '共享访问成功；以随后的 UID=0 结果确认 root。'
    return output or f'SMB helper 退出码 {rc}'


def ensure_stable():
    say('开始连接保持：USB、TCP 5555 与 root 必须连续可用 30 秒，保持阶段最多 60 秒。')
    say('实际 USB 恢复为 adb-only 时，USB 网卡可能断开；TCP 备用连接需要电脑连着 F50 Wi-Fi。')
    deadline = time.monotonic() + 60
    stable_since = None
    last_apply = 0
    last_report = 0
    tcp_restarts = 0
    next_tcp_restart = 0
    while time.monotonic() < deadline:
        current = live_target(deadline)
        if not current:
            stable_since = None
            say('USB/TCP 暂时均不可用，等待已配置的连接恢复……')
            time.sleep(0.5)
            continue
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        method, _ = root_method(current, timeout=min(2, remaining))
        if not method:
            current, method = wait_root(current, deadline)
        try:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            rc, output = run_root(current, method, CHECK, timeout=min(3, remaining), label='读取实际 USB/端口/root 状态')
        except subprocess.TimeoutExpired:
            stable_since = None
            continue
        if 'F50_ERROR_BOOT_CHANGED' in output:
            raise RuntimeError('设备已重启，停止使用旧身份继续配置；请重新执行 make adb。')
        if rc or boot_id(output) != EXPECTED_BOOT:
            stable_since = None
            time.sleep(0.5)
            continue
        ready = all(item in output.splitlines() for item in WANT)
        if not ready:
            stable_since = None
            if time.monotonic() - last_apply >= 5:
                # This changes only idempotent ADB settings, never installation writes.
                say('实际 USB 或端口配置尚未保持，使用 root 恢复（USB 切换在独立会话中完成）。')
                last_apply = time.monotonic()
                remaining = deadline - last_apply
                if remaining <= 0:
                    break
                try:
                    run_root(current, method, PAYLOAD, timeout=min(5, remaining), label='恢复实际 ADB 功能')
                except subprocess.TimeoutExpired:
                    say('设置请求期间断线，结果未知；重新读回实际状态后再决定是否补写。')
            time.sleep(1)
            continue
        usb_ok = probe_transport(USB, deadline)
        tcp_ok = tcp_target(deadline) is not None
        if not usb_ok or not tcp_ok:
            stable_since = None
            say(f'连接尚未全部就绪：USB={usb_ok}，TCP={tcp_ok}。')
            if usb_ok and not tcp_ok and tcp_restarts < 2 and time.monotonic() >= next_tcp_restart:
                tcp_restarts += 1
                say(f'已确认 USB/root 恢复，补发 TCP 监听请求（{tcp_restarts}/2）。')
                start_tcp(USB, deadline)
                next_tcp_restart = time.monotonic() + 5
            time.sleep(1)
            continue
        now = time.monotonic()
        if stable_since is None:
            stable_since = now
        if now - last_report >= 5:
            say(f'USB/TCP/root 持续可用 {now - stable_since:.1f}/30s。')
            last_report = now
        if now - stable_since >= 30:
            say(f'完成：USB、{ENDPOINT} 和 root 通道连续 30 秒可用。可以运行 make install。')
            say('本次开机已确认；Android 重启后会重新准备 root，不能据此承诺永久开启。')
            return
        time.sleep(1)
    raise RuntimeError(f'60s 内未达到 USB/TCP/root 连续 30s 可用，未宣告成功。请查看 {LOG_PATH}')


# ---

def enable_via_web(password):
    global WEB_PORT
    WEB_PORT = find_web_port()
    say(f'F50 后台：http://{args.ip}:{WEB_PORT}/')
    if usb_target():
        say('实际 USB ADB 已连接，跳过开关切换和重启。')
        return None
    say('实际 USB ADB 未连接，按原 EXE 执行 USB_PORT_SETTING 0 → 1。')
    for value in ('0', '1'):
        # Reauthenticate after each USB re-enumeration, as each zreq invocation does.
        fields, cookie = prepare_post(password, {'goformId': 'USB_PORT_SETTING', 'usb_port_switch': value})
        state = usb_switch(cookie)
        say(f'设置 USB 调试（usb_port_switch {state or "未知"} → {value}）……')
        try:
            result, _ = request(fields, cookie, post=True)
            if not web_ok(result):
                raise RuntimeError(f'USB 调试设置未成功：{result}')
        except (OSError, http.client.HTTPException, ValueError):
            say(f'设置 {value} 的 HTTP 响应中断，先等待网络恢复，再读回结果。')
        time.sleep(0.7)
        wait_web()
        state = usb_switch(cookie)
        say(f'后台开关读回：{state or "未知（会话可能已失效）"}；尚不能据此认定 USB ADB 已出现。')
        if value == '1' and state == '0':
            raise RuntimeError(f'开启请求后仍读到关闭状态，请在 {CONSOLE} 打开 USB 调试。')
    if not args.adb_only:
        ensure_web_samba(password)
    if usb_target():
        say('0 → 1 切换后已检测到实际 USB ADB，跳过重启。')
        return None
    return prepare_post(password, {'goformId': 'REBOOT_DEVICE'})


def main():
    global WEB_PORT
    say(f'日志文件：{LOG_PATH}')
    say(f'设备 {args.ip}；USB 目标 {args.serial or "单台 USB 设备"}；Samba/root 等待上限 {ROOT_WAIT}s。')
    if not args.adb_only:
        say('默认开启 TCP 5555 备用 ADB。请连接 F50 Wi-Fi：USB 稳定模式会移除 USB 网卡/MTP 功能。')
    rc, output = run([ADB, 'start-server'], label='启动 ADB 服务')
    if rc:
        raise RuntimeError('无法启动本机 ADB 服务：' + output)
    target = usb_target()
    if target:
        if args.adb_only:
            wait_usb_only()
            return
        rc, serial = run([ADB, *target, 'get-serialno'], timeout=4)
        if serial.startswith('F50LINUX'):
            say('设备当前运行 F50 Linux，ADB 已可用；安装器会先让它重启回 Android。')
            return
        require_f50_wifi()
        say('USB ADB 已连接，复用连接准备 TCP/root；已有正确属性不会重复写入，不重启设备。')
        inject_configuration(target)
    else:
        if not args.adb_only:
            require_f50_wifi()
        say('没有检测到 USB ADB。前置步骤：')
        say('  1. 用数据线把 F50 连到这台电脑（部分充电线不带数据）。')
        say(f'  2. 浏览器打开 {CONSOLE}，用后台密码登录，开启 ADB 调试并保存。')
        say('  3. 若电脑仍看不到设备，重启一次 F50。')
        try:
            WEB_PORT = find_web_port()
            if usb_switch() == '0':
                say('注意：后台显示 USB 调试未开启（usb_port_switch=0），直接回车等待不会出现 ADB，请输入后台密码。')
        except RuntimeError:
            pass
        try:
            password = getpass.getpass('完成后直接按回车等待；或输入 F50 后台密码，由脚本自动开启并重启：')
        except EOFError:
            password = ''
        if not password:
            say(f'等待 USB ADB（最多 {60 if args.adb_only else WAIT} 秒）……')
            if args.adb_only:
                wait_usb_only()
            else:
                inject_configuration(USB)
        else:
            reboot_request = enable_via_web(password)
            password = None
            outcome = []

            def worker():
                try:
                    if args.adb_only:
                        wait_usb_only()
                    else:
                        inject_configuration(USB)
                except Exception as exc:
                    outcome.append(exc)

            thread = threading.Thread(target=worker, daemon=True)
            if reboot_request is None or usb_target():
                say('USB ADB 已出现，直接确认连接，不重启。' if args.adb_only else
                    '开启 USB 调试后 ADB 已连接，直接准备 root 通道，不重启。')
                thread.start()
            else:
                fields, cookie = reboot_request
                say('重启 F50，并等待重启后的 USB ADB……')
                try:
                    result, _ = request(fields, cookie, post=True, sent=thread.start)
                    if not web_ok(result):
                        say(f'重启接口返回 {result}；如果设备没有重启，请现在手动重启。')
                except (OSError, http.client.HTTPException, ValueError) as exc:
                    if not thread.ident:
                        raise RuntimeError('重启请求没有发出：' + str(exc)) from exc
                    say('重启请求已发出（连接随重启断开），继续等待 ADB。')
            while thread.is_alive():
                thread.join(0.5)
            if outcome:
                raise outcome[0]
        target = USB
        if args.adb_only:
            return

    ensure_stable()


try:
    main()
except KeyboardInterrupt:
    say('\n已停止。已写入设备的设置不会自动撤销。')
    sys.exit(130)
except (RuntimeError, OSError, ValueError, KeyError, http.client.HTTPException,
        subprocess.TimeoutExpired) as exc:
    say(f'错误：{exc}')
    sys.exit(1)
finally:
    # Ctrl-C/errors must not leave a background waiter that later modifies a device.
    with PROCESS_LOCK:
        STOP.set()
        processes = list(ACTIVE)
    for process in processes:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
PY
