#!/bin/bash
# F50 stock Android: enable USB + wireless ADB (macOS only; equivalent of the F50-ADB aaadddbbb.exe flow).
# Normally run through `make adb`; `make install` runs it first. A device that already has ADB is only checked.
# Protocol/commands: work/analysis/F50-ADB-aaadddbbb-2026-11-12/提取命令.txt
# No firmware flashing. Samba guest access is enabled, as in the original tool.
set -eu
[ "$(uname -s)" = Darwin ] || { echo '只支持 macOS。' >&2; exit 1; }
TOP=$(cd "$(dirname "$0")/../.." && pwd)
. "$TOP/installer/lib/host-tools.sh"
command -v adb >/dev/null || { echo '缺少 adb：请安装 Android platform-tools（brew install android-platform-tools）。' >&2; exit 1; }
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
    description='F50 原厂 Android：已有 ADB 时先检查配置，缺失才写入；未连接时重启并捕获 USB ADB 窗口。',
    epilog='先把 F50 的 USB 分配给 Mac（不要分配给 Parallels），并连接 F50 的 Wi-Fi。'
           '默认使用原程序的 Web 端口 8080；会修改 Samba 访客访问配置。'
)
parser.add_argument('--ip', default='192.168.0.1', help='F50 管理 IPv4 地址（默认 192.168.0.1）')
parser.add_argument('--serial', help='只操作指定的 USB ADB 序列号；默认只允许一台 USB 设备')
parser.add_argument('--manual-reboot', action='store_true', help='ADB 未连接时跳过 Web 操作，等待你手动重启 F50')
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


def request(fields, cookie='', post=False, sent=None):
    # Reconstructed from zreq.exe: port 8080, SHA256 challenge, first session cookie.
    path = '/goform/goform_set_cmd_process' if post else '/goform/goform_get_cmd_process'
    headers = {'User-Agent': 'Mozilla/5.0', 'Referer': f'http://{args.ip}:8080/index.html'}
    if cookie:
        headers['Cookie'] = cookie
    body = None
    if post:
        headers['Content-Type'] = 'application/x-www-form-urlencoded; charset=UTF-8'
        body = urlencode(fields)
    else:
        path += '?' + urlencode(dict(fields, isTest='false', _=str(time.time_ns() // 1000000)))
    connection = http.client.HTTPConnection(args.ip, 8080, timeout=8)
    try:
        connection.request('POST' if post else 'GET', path, body=body, headers=headers)
        if sent:
            sent()  # Arm ADB immediately after sending reboot, before waiting for HTTP response.
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


def prepare_post(password, fields):
    # Each invocation of zreq.exe logs in again and obtains a new RD/AD.
    def digest(value):
        return hashlib.sha256(value.encode('utf-8')).hexdigest().upper()

    ld, _ = request({'cmd': 'LD'})
    _, cookie = request({'goformId': 'LOGIN', 'isTest': 'false',
                         'password': digest(digest(password) + ld['LD'])}, post=True)
    if not cookie:
        raise RuntimeError('登录未返回会话 Cookie，请检查设备后台密码')
    info, _ = request({'cmd': 'Language,cr_version,wa_inner_version', 'multi_data': '1'}, cookie)
    rd, _ = request({'cmd': 'RD'}, cookie)
    ad = digest(digest(info['wa_inner_version'] + info['cr_version']) + rd['RD'])
    return dict(fields, isTest='false', AD=ad), cookie


# One shell invocation instead of ten adb processes. Repeated attempts preserve the
# first backup and replace our Samba entries rather than accumulating duplicates.
# USB persistence is last because a USB reconfiguration can terminate this session.
PAYLOAD = r'''
set -eu
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


def inject_configuration(target):
    deadline = time.monotonic() + 180
    last = ''
    while time.monotonic() < deadline:
        try:
            # ADB accepts one wait prefix followed by shell, without host polling.
            rc, last = run([ADB, *target, 'wait-for-device', 'shell', '-T',
                            'sh -c ' + shlex.quote(PAYLOAD)],
                           timeout=max(1, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            break
        if 'F50_ERROR_' in last:
            raise RuntimeError('设备不符合原厂工具所需条件：\n' + last)
        if 'F50_BASE_SAVED' in last:
            say('ADB 端口及 Samba 配置已写入；原配置备份为 smb.conf.f50-adb.bak。')
            if 'F50_CONFIG_SAVED' not in last:
                say('USB 会话在最后一步断开，将通过无线 ADB 检查 USB 属性。')
            return
        if 'more than one' in last:
            raise RuntimeError('发现多台 USB ADB 设备，请拔掉其他设备或使用 --serial')
        if 'F50_APPLYING' in last:
            say('写入过程中连接断开，立即等待下一次连接重试……')
        time.sleep(0.1)
    raise RuntimeError('180 秒内未完成配置。确认 F50 USB 分配给 Mac、设备已重启，'
                       '并允许 ADB 授权。\n' + last[-1200:])


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
            rc, detail = run(['/sbin/mount', '-t', 'smbfs', '-o',
                              'rdonly,nobrowse,soft,nopassprompt',
                              f'//Guest:@{args.ip}/{quote(match[1], safe="")}', str(mountpoint)], timeout=5)
            if rc == 0:
                say('已访问 Samba 共享，触发 preexec = start adbd。')
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


CHECK = r'''
printf 'SERVICE=%s\nPERSIST=%s\nUSB=%s\n' "$(getprop service.adb.tcp.port)" "$(getprop persist.adb.tcp.port)" "$(getprop persist.sys.usb.config)"
cfg=/data/samba/etc/smb.conf
[ -r "$cfg" ] &&
grep -q '^[[:space:]]*preexec[[:space:]]*=[[:space:]]*start adbd[[:space:]]*$' "$cfg" &&
grep -q '^[[:space:]]*map to guest[[:space:]]*=[[:space:]]*Bad User[[:space:]]*$' "$cfg" &&
! grep -Eq 'guest ok = no|restrict anonymous = 1|public = no|nt acl support = no' "$cfg" &&
echo F50_SAMBA_OK
'''
CONFIG_VALUES = ('SERVICE=5555', 'PERSIST=5555', 'USB=adb,rndis', 'F50_SAMBA_OK')


def connected_target():
    targets = [USB] if args.serial else [USB, ['-s', ENDPOINT]]
    for target in targets:
        rc, output = run([ADB, *target, 'get-state'], timeout=4)
        if 'more than one' in output:
            raise RuntimeError('发现多台 USB 设备，请使用 --serial 或只连接 F50')
        if rc == 0 and output == 'device':
            return target
    return None


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
            rc, output = run([ADB, '-s', ENDPOINT, 'shell', CHECK], timeout=4)
            if rc == 0 and all(item in output.splitlines() for item in
                               ('SERVICE=5555', 'PERSIST=5555', 'F50_SAMBA_OK')):
                if 'USB=adb,rndis' not in output.splitlines():
                    # The USB shell may have disconnected before its final setprop.
                    run([ADB, '-s', ENDPOINT, 'shell', 'setprop persist.sys.usb.config adb,rndis'])
                return
        except subprocess.TimeoutExpired:
            pass
        time.sleep(1)
    raise RuntimeError('USB 配置阶段已结束，但无线 ADB 未就绪。请保持连接 F50 Wi-Fi。'
                       '\n最近的 SMB 信息：' + last[-1000:])


def main():
    say('请把 F50 USB 连接到 Mac，并保持连接 F50 的 Wi-Fi。')
    say('本次会按原程序修改 Samba 访客访问配置，保留一份原配置备份。')
    rc, output = run([ADB, 'start-server'])
    if rc:
        raise RuntimeError('无法启动 Mac ADB 服务：' + output)
    target = connected_target()
    if target:
        rc, output = run([ADB, *target, 'get-serialno'], timeout=4)
        if output.startswith('MU300LINUX'):
            say('设备当前运行 MU300 Linux，ADB 已可用，跳过原厂 Android 配置。')
            return
        rc, output = run([ADB, *target, 'shell', CHECK], timeout=4)
        if rc == 0 and all(item in output.splitlines() for item in CONFIG_VALUES):
            say('ADB 已连接，端口、USB 和 Samba 配置均已完整，无需重复写入或重启。')
            return
        say('ADB 已连接，但配置尚不完整；跳过 Web 操作和重启，直接补写配置。')
    password = None
    if not target and not args.manual_reboot:
        password = getpass.getpass('F50 设备后台密码（不是 Wi-Fi 密码）：')
        for value in ('0', '1'):
            say(f'设置 USB_PORT_SETTING usb_port_switch={value}……')
            fields, cookie = prepare_post(password, {'goformId': 'USB_PORT_SETTING', 'usb_port_switch': value})
            result, _ = request(fields, cookie, post=True)
            # Continue only with the usual successful ZTE result values.
            if str(result.get('result', '')).lower() not in ('0', 'success'):
                raise RuntimeError(f'USB 配置请求未成功：{result}')
            time.sleep(0.7)
        fields, cookie = prepare_post(password, {'goformId': 'REBOOT_DEVICE'})
        password = None

    # USB setup or a manual reconnect may have made ADB available in the meantime.
    if not target:
        target = connected_target()
        if target:
            rc, output = run([ADB, *target, 'shell', CHECK], timeout=4)
            if rc == 0 and all(item in output.splitlines() for item in CONFIG_VALUES):
                say('ADB 已连接且配置完整，无需重复写入或重启。')
                return
    outcome = []

    def worker():
        try:
            inject_configuration(target or USB)
        except Exception as exc:
            outcome.append(exc)

    thread = threading.Thread(target=worker, daemon=True)
    if target:
        thread.start()
        say('使用现有 ADB 连接写入配置，不重启设备。')
    elif args.manual_reboot:
        thread.start()
        say('已开始等待。现在请手动重启 F50；出现 ADB 后会立即写入。')
    else:
        say('发送重启请求，并立即等待重启后的 USB ADB……')
        try:
            result, _ = request(fields, cookie, post=True, sent=thread.start)
            if str(result.get('result', '')).lower() not in ('0', 'success'):
                say(f'重启接口返回 {result}；如果设备没有重启，请现在手动重启。')
        except (OSError, http.client.HTTPException, ValueError) as exc:
            if not thread.ident:
                raise RuntimeError('重启请求尚未发出：' + str(exc)) from exc
            say('重启请求已发出，HTTP 响应中断；继续捕获 ADB。若未重启，请手动重启。')
    while thread.is_alive():
        thread.join(0.5)
    if outcome:
        raise outcome[0]

    say('通过 Samba 触发 adbd，并连接无线 ADB……')
    connect_wireless()
    say('无线 ADB 已连接，观察 30 秒内是否再次断开……')
    for _ in range(15):
        time.sleep(2)
        rc, output = run([ADB, '-s', ENDPOINT, 'shell', CHECK], timeout=4)
        if rc or not all(item in output.splitlines() for item in CONFIG_VALUES):
            raise RuntimeError('连接或配置在观察期间发生变化，未达到持续开启目标：\n' + output)
    say(f'完成：无线 ADB 在 30 秒观察期间持续可用。\n连接命令：adb connect {ENDPOINT}')
    say('这是本次开机的结果；原程序也提示重启后可能需要重新开启。')


try:
    main()
except KeyboardInterrupt:
    say('\n已停止。已写入设备的配置不会自动撤销。')
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
