"""Host tools, Wi-Fi checks, logging and cancellable subprocesses."""
import ipaddress
import logging
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import threading
import time


class Host:
    def __init__(self, top):
        self.top = top
        self.env = dict(os.environ, LC_ALL='C')
        # Match the installers' lookup while keeping the caller's PATH first.
        host_path = self.env.get('PATH', os.defpath).split(os.pathsep)
        for directory in ('/opt/homebrew/bin', '/usr/local/bin'):
            if Path(directory).is_dir() and directory not in host_path:
                host_path.append(directory)
        self.env['PATH'] = os.pathsep.join(host_path)
        self.adb = shutil.which('adb', path=self.env['PATH'])
        if not self.adb:
            adb_dirs = [top, top / 'platform-tools', top / 'work/deploy/bin', Path.cwd() / 'platform-tools',
                        Path.home() / 'platform-tools', Path.home() / 'Downloads/platform-tools',
                        Path.home() / 'Library/Android/sdk/platform-tools']
            adb_dirs.extend(Path(self.env[key]) / 'platform-tools'
                            for key in ('ANDROID_HOME', 'ANDROID_SDK_ROOT') if self.env.get(key))
            for directory in adb_dirs:
                candidate = directory / 'adb'
                if candidate.is_file() and os.access(candidate, os.X_OK):
                    self.adb = str(candidate)
                    self.env['PATH'] = str(directory) + os.pathsep + self.env['PATH']
                    break
        if not self.adb:
            raise RuntimeError('缺少 adb：请安装 Android platform-tools（macOS: brew install android-platform-tools）。')
        # Always use the local server and explicit USB/TCP selectors.
        for key in ('ANDROID_SERIAL', 'ADB_SERVER_SOCKET', 'ANDROID_ADB_SERVER_ADDRESS', 'ANDROID_ADB_SERVER_PORT'):
            self.env.pop(key, None)
        self.active = set()
        self.process_lock = threading.Lock()
        self.stopped = threading.Event()
        log_dir = top / 'work/logs'
        log_dir.mkdir(parents=True, exist_ok=True)
        self.log_path = log_dir / f'f50-adb-{time.strftime("%Y%m%d-%H%M%S")}-{os.getpid()}.log'
        self.log_file = os.fdopen(os.open(self.log_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600),
                                  'w', encoding='utf-8')
        self.log = logging.Logger('f50-adb', level=logging.INFO)
        for destination in (sys.stdout, self.log_file):
            handler = logging.StreamHandler(destination)
            handler.setFormatter(logging.Formatter('[%(asctime)s] %(message)s', datefmt='%H:%M:%S'))
            self.log.addHandler(handler)

    def say(self, message):
        self.log.info(message)

    def run(self, command, timeout=8, label=None):
        started = time.monotonic()
        if label:
            self.say(f'{label}：开始（超时 {timeout:.1f}s）')
        with self.process_lock:
            if self.stopped.is_set():
                raise RuntimeError('任务已停止')
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT, text=True, errors='replace',
                                       env=self.env, start_new_session=True)
            self.active.add(process)
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
                self.say(f'{label}：{reason}，耗时 {time.monotonic() - started:.2f}s')
                if output.strip():
                    self.say(f'{label} 输出：\n{output.strip()}')
            raise
        finally:
            with self.process_lock:
                self.active.discard(process)
        if label:
            self.say(f'{label}：退出码 {process.returncode}，耗时 {time.monotonic() - started:.2f}s')
            self.say(f'{label} 输出：\n{output.strip() or "（无输出）"}')
        return process.returncode, output.strip()

    def require_f50_wifi(self, ip):
        # USB networking disappears in adb-only mode; check before toggling USB.
        if sys.platform != 'darwin':
            self.say('请确认 Wi-Fi 已连接 F50；本机未自动核对无线网段。')
            return
        rc, ports = self.run(['/usr/sbin/networksetup', '-listallhardwareports'], timeout=3)
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
            rc, config = self.run(['/sbin/ifconfig', interface], timeout=3)
            if rc:
                continue
            for address, mask in re.findall(r'\binet ([0-9.]+) netmask (0x[0-9a-fA-F]+|[0-9.]+)', config):
                netmask = str(ipaddress.IPv4Address(int(mask, 16))) if mask.startswith('0x') else mask
                network = ipaddress.IPv4Network(f'{address}/{netmask}', strict=False)
                addresses.append(f'{interface}={address} ({network})')
                if ipaddress.IPv4Address(ip) in network:
                    self.say(f'Wi-Fi 网段检查通过：{interface}={address}，目标 {ip} 同网段；请确认连接的是 F50 热点。')
                    self.run(['/sbin/route', '-n', 'get', ip], timeout=3, label='当前 F50 网络路径')
                    return
        raise RuntimeError('Wi-Fi 不在 F50 的本地网段，不能作为 USB 掉线后的备用通道。'
                           f'\n当前 Wi-Fi：{", ".join(addresses) or "没有 IPv4 地址"}；目标：{ip}。'
                           '\n请连接 F50 自己的 Wi-Fi，保持 USB 线连接，再运行 make adb。尚未切换 USB 或重启设备。')

    def stop(self):
        # Cancel commands before joining the workflow's worker and closing its log.
        with self.process_lock:
            self.stopped.set()
            processes = list(self.active)
        for process in processes:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()

    def close(self):
        self.stop()
        for handler in self.log.handlers:
            handler.flush()
            handler.close()
        self.log_file.close()
