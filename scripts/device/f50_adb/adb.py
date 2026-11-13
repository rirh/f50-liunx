"""ADB transports, device identity and commands through the available root channel."""
import re
import shlex
import socket
import subprocess
import sys
import time

from .payloads import IDENTITY, ROOT_PROBE


def boot_id(output):
    return next((line.partition('=')[2].strip() for line in output.splitlines()
                 if line.startswith('F50_BOOT_ID=')), '')


class AdbClient:
    def __init__(self, config, host):
        self.config = config
        self.host = host
        self.usb = ['-s', config.serial] if config.serial else ['-d']
        self.expected_boot = None
        self.device_serial = config.serial
        self.tcp_last_reconnect = 0
        self.tcp_last_state = None
        self.shares = []

    def root_method(self, target, timeout=6):
        try:
            rc, output = self.host.run([self.host.adb, *target, 'shell', '-T', ROOT_PROBE],
                                       timeout=timeout, label='root 探测')
        except subprocess.TimeoutExpired:
            return None, ''
        if rc == 0:
            for method in ('shell', 'socket', 'su'):
                if 'F50_ROOT=' + method in output.splitlines():
                    return method, output
        return None, output

    def run_root(self, target, method, command, timeout=8, label='root 命令'):
        command = self.boot_guard() + command
        if method == 'socket':
            selector = ['--serial', target[1]] if target[0] == '-s' else ['--usb']
            return self.host.run([sys.executable, str(self.host.top / 'installer/lib/adb-root.py'),
                                  '--adb', self.host.adb, *selector,
                                  '--timeout', str(max(0.1, timeout - 0.25)), command], timeout=timeout,
                                 label=f'{label}（socket）')
        remote = ('su -c ' if method == 'su' else 'sh -c ') + shlex.quote(command)
        return self.host.run([self.host.adb, *target, 'shell', '-T', remote],
                             timeout=timeout, label=f'{label}（{method}）')

    def boot_guard(self):
        if not self.expected_boot:
            return ''
        return ('[ "$(cat /proc/sys/kernel/random/boot_id)" = ' + shlex.quote(self.expected_boot) +
                ' ] || { echo F50_ERROR_BOOT_CHANGED; exit 1; };\n')

    def remember_device(self, output):
        found = boot_id(output)
        if not re.fullmatch(r'[0-9a-fA-F-]{36}', found):
            raise RuntimeError('没有取得本次 USB 设备的启动标识，拒绝切换到未确认身份的网络设备。')
        if self.expected_boot and self.expected_boot != found:
            raise RuntimeError('设备启动标识变化，请重新执行 make adb。')
        serial = next((line.partition('=')[2].strip() for line in output.splitlines()
                       if line.startswith('F50_DEVICE_SERIAL=')), '')
        if not re.fullmatch(r'[A-Za-z0-9_.-]+', serial):
            raise RuntimeError('未取得明确的物理序列号，停止后续设备写入。')
        if self.device_serial and self.device_serial != serial:
            raise RuntimeError('设备序列号变化，停止后续设备写入。')
        self.device_serial = serial
        self.usb = ['-s', serial]
        self.expected_boot = found
        self.shares = [line.partition('=')[2].strip() for line in output.splitlines()
                       if line.startswith('F50_SHARE=')]
        self.shares = [name for name in self.shares
                       if name and name.upper() not in ('GLOBAL', 'IPC$', 'ADMIN$', 'C$')]
        self.host.say(f'已记录 USB 设备身份，启动标识 {self.expected_boot}；共享：{self.shares or "待枚举"}')

    def probe_transport(self, target, deadline):
        remaining = deadline - time.monotonic()
        if not self.expected_boot or remaining <= 0:
            return False
        try:
            rc, output = self.host.run([self.host.adb, *target, 'shell', '-T', IDENTITY],
                                       timeout=min(1, remaining))
        except subprocess.TimeoutExpired:
            return False
        return rc == 0 and boot_id(output) == self.expected_boot

    def tcp_target(self, deadline):
        remaining = deadline - time.monotonic()
        if not self.expected_boot or remaining <= 0:
            return None
        target = ['-s', self.config.endpoint]
        try:
            rc, state = self.host.run([self.host.adb, *target, 'get-state'], timeout=min(0.5, remaining))
            if (rc, state) != self.tcp_last_state:
                self.host.say(f'TCP ADB 状态：{state or "无返回"}（退出码 {rc}）；仅 device 且身份匹配才可用。')
                self.tcp_last_state = (rc, state)
            if 'unauthorized' in state.lower():
                return None
            if rc or state != 'device':
                if time.monotonic() - self.tcp_last_reconnect < 2:
                    return None
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return None
                self.tcp_last_reconnect = time.monotonic()
                if state == 'offline' or 'device offline' in state.lower():
                    # Drop only this stale transport; never restart the user's whole ADB server.
                    self.host.run([self.host.adb, 'disconnect', self.config.endpoint],
                                  timeout=min(0.5, remaining), label='清理指定地址的 offline TCP 连接')
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        return None
                with socket.create_connection((self.config.ip, 5555), timeout=min(0.5, remaining)):
                    pass
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return None
                self.host.run([self.host.adb, 'connect', self.config.endpoint],
                              timeout=min(1, remaining), label='连接 TCP 备用 ADB')
            if self.probe_transport(target, deadline):
                return target
        except (OSError, subprocess.TimeoutExpired):
            pass
        return None

    def live_target(self, deadline):
        if self.probe_transport(self.usb, deadline):
            return self.usb
        target = self.tcp_target(deadline)
        if target:
            self.host.say('USB 暂不可用，已核对启动标识，使用同一设备的 TCP ADB。')
        return target

    def start_tcp(self, target, deadline=None):
        deadline = min(deadline or float('inf'), time.monotonic() + 6)
        if self.tcp_target(min(deadline, time.monotonic() + 2)):
            self.host.say('同一设备的 TCP 5555 已可用，无需重启 adbd。')
            return
        # Setting the properties alone may not reopen adbd's listener until it restarts.
        if not self.probe_transport(target, min(deadline, time.monotonic() + 1)):
            self.host.say('原 USB 会话已断开，跳过向未确认的目标发送 tcpip。')
            return
        try:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return
            rc, output = self.host.run([self.host.adb, *target, 'tcpip', '5555'], timeout=min(3, remaining),
                                       label='启用 TCP 5555（adbd 会短暂重连）')
            if rc:
                self.host.say('TCP 启用请求未完成，将通过实际连接确认：' + output)
        except subprocess.TimeoutExpired:
            self.host.say('TCP 启用请求期间连接中断，将通过实际连接确认。')

    def usb_target(self):
        rc, output = self.host.run([self.host.adb, *self.usb, 'get-state'], timeout=4, label='USB ADB 状态')
        if 'more than one' in output:
            raise RuntimeError('发现多台 USB 设备，请使用 --serial 或只连接 F50')
        return self.usb if rc == 0 and output == 'device' else None

    def wait_usb_only(self):
        # Read actual runtime properties in the first usable shell; do not inject root here.
        command = "id; getprop sys.usb.config; getprop sys.usb.state; getprop persist.sys.usb.config; echo F50_USB_ADB_CONNECTED"
        rc, output = self.host.run([self.host.adb, *self.usb, 'wait-for-usb-device', 'shell', '-T', command],
                                   timeout=60, label='等待实际 USB ADB 并读取 USB 模式')
        if rc or 'F50_USB_ADB_CONNECTED' not in output.splitlines():
            raise RuntimeError('尚未确认可用的 USB ADB shell：' + output)
        self.host.say('已确认 USB ADB shell 可用；本次只恢复 ADB，不进入 root 注入。')
