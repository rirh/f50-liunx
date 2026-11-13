#!/bin/sh
# Stock F50 (ZTE F50, Android): turn on USB debugging so the installer gets a root adb shell. macOS or Linux.
# Normally run through `make adb`; `make install` runs it first. A device that already has adb is only checked.
#
# What it needs (README, "First installation"):
#   * the F50 on its stock firmware, plugged into this computer by a data cable,
#   * USB debugging on: in the F50 web console, http://192.168.0.1/index.html#usb_port (log in with the
#     admin password, not the Wi-Fi password), turn ADB debugging on and save. Without that, this script can
#     do it itself with the admin password, the way the F50-ADB tool (aaadddbbb.exe) does on Windows.
#
# What it writes on the device: persist.sys.usb.config=adb,rndis, so adb stays on across reboots (the installer
# restarts Android). --wireless also writes adb over TCP port 5555 and opens Samba to guests (the F50-ADB tool's
# trick for starting adbd), macOS only - an installation does not need it.
# Protocol and commands: work/analysis/F50-ADB-aaadddbbb-2026-11-12/提取命令.txt. Nothing is flashed.
set -eu
TOP=$(cd "$(dirname "$0")/../.." && pwd)
. "$TOP/installer/lib/host-tools.sh"
find_adb || { echo '缺少 adb：请安装 Android platform-tools（macOS: brew install android-platform-tools）。' >&2; exit 1; }
command -v python3 >/dev/null || { echo '缺少 Python 3。' >&2; exit 1; }
exec python3 - "$@" <<'PY'
import argparse
import getpass
import hashlib
import http.client
import ipaddress
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import quote, urlencode

parser = argparse.ArgumentParser(
    description='原厂 F50：开启 USB 调试（root adb shell），供安装器使用。已开启时只做检查。',
    epilog='前置条件：数据线连接电脑；在 http://192.168.0.1/index.html#usb_port 开启 ADB 调试，'
           '或在提示时输入后台密码由本脚本开启。')
parser.add_argument('--ip', default='192.168.0.1', help='F50 后台 IPv4 地址（默认 192.168.0.1）')
parser.add_argument('--port', type=int, help='F50 后台 Web 端口（默认先试 80，再试 8080）')
parser.add_argument('--serial', help='只操作指定的 USB ADB 序列号；默认只允许一台 USB 设备')
parser.add_argument('--wireless', action='store_true',
                    help='另外开启无线 ADB（TCP 5555，并按原工具放开 Samba 访客访问）；仅 macOS，安装不需要')
args = parser.parse_args()
try:
    ipaddress.IPv4Address(args.ip)
except ValueError:
    parser.error('--ip 必须是 IPv4 地址')
if args.serial and ':' in args.serial:
    parser.error('--serial 应为 USB 序列号，不能是无线 ADB 地址')
if args.wireless and sys.platform != 'darwin':
    parser.error('--wireless 只支持 macOS（需要 smbutil 触发 adbd）')

ADB = shutil.which('adb')
USB = ['-s', args.serial] if args.serial else ['-d']
ENDPOINT = args.ip + ':5555'
CONSOLE = f'http://{args.ip}/index.html#usb_port'
WAIT = 300   # seconds for adb to show up after USB debugging was turned on
# Use the local ADB server even if the parent terminal selected a remote server/device.
ENV = dict(os.environ, LC_ALL='C')
for key in ('ANDROID_SERIAL', 'ADB_SERVER_SOCKET', 'ANDROID_ADB_SERVER_ADDRESS', 'ANDROID_ADB_SERVER_PORT'):
    ENV.pop(key, None)
ACTIVE = set()
PROCESS_LOCK = threading.Lock()
STOP = threading.Event()


def say(message):
    print(message, flush=True)


def run(command, timeout=8):
    with PROCESS_LOCK:
        if STOP.is_set():
            raise RuntimeError('任务已停止')
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, errors='replace',
                                   env=ENV, start_new_session=True)
        ACTIVE.add(process)
    try:
        output, _ = process.communicate(timeout=timeout)
    except BaseException:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.communicate()
        raise
    finally:
        with PROCESS_LOCK:
            ACTIVE.discard(process)
    return process.returncode, output.strip()


# --- the F50 web console (goform), only when the admin password is given

WEB_PORT = None


def request(fields, cookie='', post=False, sent=None, port=None):
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
    connection = http.client.HTTPConnection(args.ip, port, timeout=8)
    try:
        connection.request('POST' if post else 'GET', path, body=body, headers=headers)
        if sent:
            sent()  # Arm the adb waiter as soon as the reboot is sent, before the HTTP response.
        response = connection.getresponse()
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


# --- the device side. One shell invocation; the USB setting last, because changing it can end the session.

USB_PAYLOAD = r'''
set -eu
[ "$(id -u)" = 0 ] || { echo F50_ERROR_NOT_ROOT; exit 1; }
echo F50_APPLYING
setprop persist.sys.usb.config adb,rndis
echo F50_CONFIG_SAVED
'''

WIRELESS_PAYLOAD = r'''
set -eu
[ "$(id -u)" = 0 ] || { echo F50_ERROR_NOT_ROOT; exit 1; }
cfg=/data/samba/etc/smb.conf
[ -f "$cfg" ] && [ -w "$cfg" ] || { echo F50_ERROR_SAMBA_NOT_WRITABLE; exit 1; }
grep -q '^\[global\]' "$cfg" || { echo F50_ERROR_NO_GLOBAL_SECTION; exit 1; }
echo F50_APPLYING
setprop service.adb.tcp.port 5555
setprop persist.adb.tcp.port 5555
[ -e "$cfg.f50-adb.bak" ] || cp -p "$cfg" "$cfg.f50-adb.bak"
tmp="$cfg.f50-adb.tmp"
cp -p "$cfg" "$tmp"
sed '/^[[:space:]]*preexec[[:space:]]*=[[:space:]]*start adbd[[:space:]]*$/d
/^[[:space:]]*map to guest[[:space:]]*=[[:space:]]*Bad User[[:space:]]*$/d
/^\[global\]/a\
        preexec = start adbd\
        map to guest = Bad User
s/guest ok = no/guest ok = yes/g
s/restrict anonymous = 1/restrict anonymous = 0/g
s/public = no/public = yes/g
s/nt acl support = no/nt acl support = yes/g' "$cfg" > "$tmp"
mv -f "$tmp" "$cfg"
echo F50_BASE_SAVED
setprop persist.sys.usb.config adb,rndis
echo F50_CONFIG_SAVED
'''

CHECK = r'''
printf 'UID=%s\nSERVICE=%s\nPERSIST=%s\nUSB=%s\n' "$(id -u)" "$(getprop service.adb.tcp.port)" "$(getprop persist.adb.tcp.port)" "$(getprop persist.sys.usb.config)"
cfg=/data/samba/etc/smb.conf
[ -r "$cfg" ] &&
grep -q '^[[:space:]]*preexec[[:space:]]*=[[:space:]]*start adbd[[:space:]]*$' "$cfg" &&
grep -q '^[[:space:]]*map to guest[[:space:]]*=[[:space:]]*Bad User[[:space:]]*$' "$cfg" &&
! grep -Eq 'guest ok = no|restrict anonymous = 1|public = no|nt acl support = no' "$cfg" &&
echo F50_SAMBA_OK
'''
USB_VALUES = ('UID=0', 'USB=adb,rndis')
WIRELESS_VALUES = USB_VALUES + ('SERVICE=5555', 'PERSIST=5555', 'F50_SAMBA_OK')
WANT = WIRELESS_VALUES if args.wireless else USB_VALUES
PAYLOAD = WIRELESS_PAYLOAD if args.wireless else USB_PAYLOAD


def configured(target):
    rc, output = run([ADB, *target, 'shell', CHECK], timeout=6)
    lines = output.splitlines()
    if rc == 0 and 'UID=0' not in lines:
        raise RuntimeError('ADB 已连接，但 shell 不是 root。安装需要原厂 F50 通过 '
                           f'{CONSOLE} 开启的 ADB 调试（root shell），或 Magisk 的 su。')
    return rc == 0 and all(item in lines for item in WANT)


def inject_configuration(target):
    # adb accepts one wait prefix followed by shell: it catches adb the moment it appears, even when the
    # firmware only offers it for a short while after a reboot.
    deadline = time.monotonic() + WAIT
    last = ''
    while time.monotonic() < deadline:
        try:
            rc, last = run([ADB, *target, 'wait-for-usb-device', 'shell', '-T',
                            'sh -c ' + shlex.quote(PAYLOAD)],
                           timeout=max(1, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            break
        if 'F50_ERROR_NOT_ROOT' in last:
            raise RuntimeError('ADB 已连接，但 shell 不是 root，无法写入。')
        if 'F50_ERROR_' in last:
            raise RuntimeError('设备不符合无线 ADB 所需条件：\n' + last)
        if 'F50_CONFIG_SAVED' in last or 'F50_BASE_SAVED' in last:
            return
        if 'more than one' in last:
            raise RuntimeError('发现多台 USB ADB 设备，请拔掉其他设备或使用 --serial')
        if 'unauthorized' in last:
            say('请在设备上允许这台电脑的 USB 调试……')
        elif 'F50_APPLYING' in last:
            say('写入过程中连接断开，等待下一次连接重试……')
        time.sleep(0.2)
    raise RuntimeError(f'{WAIT} 秒内没有等到 USB ADB。请确认：数据线支持数据传输、已在 {CONSOLE} '
                       '开启 ADB 调试；仍不出现时重启一次 F50。\n' + last[-800:])


def usb_target():
    rc, output = run([ADB, *USB, 'get-state'], timeout=4)
    if 'more than one' in output:
        raise RuntimeError('发现多台 USB 设备，请使用 --serial 或只连接 F50')
    return USB if rc == 0 and output == 'device' else None


# --- wireless adb (--wireless): the F50-ADB tool's Samba trigger, macOS only

def trigger_samba():
    rc, output = run(['/usr/bin/smbutil', 'view', '-N', '-g', f'//Guest:@{args.ip}'], timeout=5)
    if rc:
        return output
    for line in output.splitlines():
        match = re.match(r'^\s*(.*?)\s{2,}Disk\b', line)
        if not match or match[1] in ('IPC$', 'ADMIN$', 'C$'):
            continue
        mountpoint = Path(tempfile.mkdtemp(prefix='f50-adb-smb-'))
        try:
            rc, detail = run(['/sbin/mount', '-t', 'smbfs', '-o', 'rdonly,nobrowse,soft,nopassprompt',
                              f'//Guest:@{args.ip}/{quote(match[1], safe="")}', str(mountpoint)], timeout=5)
            if rc == 0:
                return ''
            output = detail
        finally:
            # Never recursively delete a path that might still be mounted.
            if os.path.ismount(mountpoint):
                run(['/sbin/umount', str(mountpoint)], timeout=5)
            try:
                mountpoint.rmdir()
            except OSError:
                say(f'临时挂载点未移除，请手动卸载后处理：{mountpoint}')
    return output


def connect_wireless():
    deadline = time.monotonic() + 100
    last = ''
    while time.monotonic() < deadline:
        try:
            last = trigger_samba()
        except (OSError, subprocess.TimeoutExpired) as exc:
            last = str(exc)
        try:
            run([ADB, 'connect', ENDPOINT], timeout=4)
            if configured(['-s', ENDPOINT]):
                return
        except subprocess.TimeoutExpired:
            pass
        time.sleep(1)
    raise RuntimeError('USB 部分已完成，但无线 ADB 未就绪（需要连着 F50 的 Wi-Fi）。'
                       '安装只需要 USB ADB，可以不加 --wireless 重试。\n最近的 SMB 信息：' + last[-800:])


# ---

def enable_via_web(password):
    global WEB_PORT
    WEB_PORT = find_web_port()
    say(f'F50 后台：http://{args.ip}:{WEB_PORT}/')
    for value in ('0', '1'):
        say(f'设置 USB 调试（usb_port_switch={value}）……')
        fields, cookie = prepare_post(password, {'goformId': 'USB_PORT_SETTING', 'usb_port_switch': value})
        result, _ = request(fields, cookie, post=True)
        if not web_ok(result):
            raise RuntimeError(f'USB 调试设置未成功：{result}')
        time.sleep(0.7)
    return prepare_post(password, {'goformId': 'REBOOT_DEVICE'})


def main():
    rc, output = run([ADB, 'start-server'])
    if rc:
        raise RuntimeError('无法启动本机 ADB 服务：' + output)
    target = usb_target()
    if target:
        rc, serial = run([ADB, *target, 'get-serialno'], timeout=4)
        if serial.startswith('F50LINUX'):
            say('设备当前运行 F50 Linux，ADB 已可用；安装器会先让它重启回 Android。')
            return
        if configured(target):
            say('USB ADB 已开启（root shell），配置完整，无需写入。')
            return
        say('USB ADB 已连接（root shell），写入开机保持 ADB 的设置……')
        inject_configuration(target)
    else:
        say('没有检测到 USB ADB。前置步骤：')
        say('  1. 用数据线把 F50 连到这台电脑（部分充电线不带数据）。')
        say(f'  2. 浏览器打开 {CONSOLE}，用后台密码登录，开启 ADB 调试并保存。')
        say('  3. 若电脑仍看不到设备，重启一次 F50。')
        try:
            password = getpass.getpass('完成后直接按回车等待；或输入 F50 后台密码，由脚本自动开启并重启：')
        except EOFError:
            password = ''
        if not password:
            say(f'等待 USB ADB（最多 {WAIT} 秒）……')
            inject_configuration(USB)
        else:
            fields, cookie = enable_via_web(password)
            password = None
            outcome = []

            def worker():
                try:
                    inject_configuration(USB)
                except Exception as exc:
                    outcome.append(exc)

            thread = threading.Thread(target=worker, daemon=True)
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

    # The last setprop may have cut the session: check again over USB once adb is back.
    deadline = time.monotonic() + 60
    while not configured(target):
        if time.monotonic() > deadline:
            raise RuntimeError('写入后检查未通过：persist.sys.usb.config 不是 adb,rndis。')
        try:
            run([ADB, *target, 'wait-for-usb-device'], timeout=30)
        except subprocess.TimeoutExpired:
            continue
        if not configured(target):
            run([ADB, *target, 'shell', 'setprop persist.sys.usb.config adb,rndis'], timeout=6)
            time.sleep(2)
    say('完成：USB ADB 已开启（root shell），重启后保持。可以运行 make install。')

    if args.wireless:
        say('通过 Samba 触发 adbd，并连接无线 ADB……')
        connect_wireless()
        say(f'无线 ADB 已连接：adb connect {ENDPOINT}（本次开机有效，原工具也提示重启后可能失效）')


try:
    main()
except KeyboardInterrupt:
    say('\n已停止。已写入设备的设置不会自动撤销。')
    sys.exit(130)
except (RuntimeError, OSError, ValueError, KeyError, http.client.HTTPException,
        subprocess.TimeoutExpired) as exc:
    print(f'错误：{exc}', file=sys.stderr)
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
