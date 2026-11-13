"""Prepare the root channel and keep USB/TCP ADB available for the current boot."""
import shlex
import subprocess
import time

from .adb import boot_id
from .config import ROOT_WAIT, WAIT
from .payloads import CHECK, FIRST_CONNECTION, ROOT_PROBE, WANT, root_bootstrap, usb_payload
from .smb import trigger_samba


def wait_root(adb, deadline=None):
    host = adb.host
    bootstrap = root_bootstrap(host.top)
    host.say(f'尝试通过 SMB 触发 root（最多 {ROOT_WAIT}s）。')
    deadline = min(deadline or float('inf'), time.monotonic() + ROOT_WAIT)
    last = '尚未取得 root'
    repaired = False
    next_smb_try = 0
    while time.monotonic() < deadline:
        current = adb.live_target(deadline)
        if current and time.monotonic() < deadline:
            method, status = adb.root_method(current, timeout=min(2, deadline - time.monotonic()))
            if method:
                host.say(f'root 通道已就绪：{method}，transport={current}，UID=0。')
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
                    rc, detail = host.run([host.adb, *current, 'shell', '-T',
                                           'sh -c ' + shlex.quote(adb.boot_guard() + bootstrap)],
                                          timeout=min(2, remaining), label='重新核对并准备文件共享/root hook')
                    if 'F50_ERROR_' in detail:
                        raise RuntimeError('文件共享准备失败：' + detail)
                except subprocess.TimeoutExpired:
                    host.say('文件共享准备时连接中断；只读回状态，不立即重复写入。')
        if time.monotonic() >= deadline:
            break
        if time.monotonic() >= next_smb_try:
            try:
                last = trigger_samba(host, adb.config.ip, adb.shares, deadline)
            except (OSError, subprocess.TimeoutExpired) as exc:
                last = str(exc)
                host.say('SMB 触发失败：' + last)
            next_smb_try = time.monotonic() + 1
        time.sleep(min(0.2, max(0, deadline - time.monotonic())))
    raise RuntimeError(f'{ROOT_WAIT}s 内没有确认 UID=0，已停止。最后 SMB 结果：{last[-1200:]}\n日志：{host.log_path}')


def inject_configuration(adb, target):
    host = adb.host
    bootstrap = root_bootstrap(host.top)
    # Prepare both TCP properties in the very first shell, before slow root probes.
    deadline = time.monotonic() + WAIT
    dispatch = (FIRST_CONNECTION + '\nprobe=$(\n' + ROOT_PROBE + '\n)\nprintf "%s\\n" "$probe"\n'
                'if printf "%s\\n" "$probe" | grep -Eq "^F50_ROOT=(shell|socket|su)$"; then exit 0; fi\n'
                + bootstrap)
    while time.monotonic() < deadline:
        if adb.expected_boot:
            current = adb.live_target(deadline)
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
            rc, output = host.run([host.adb, *current, *prefix, 'shell', '-T',
                                   'sh -c ' + shlex.quote(adb.boot_guard() + dispatch)],
                                  timeout=timeout, label='捕获 ADB 窗口并准备 TCP/Samba')
        except subprocess.TimeoutExpired:
            if not adb.expected_boot:
                raise
            host.say('准备会话超时；重新核对 USB/TCP 身份和状态。')
            continue
        if 'F50_ERROR_NOT_F50' in output or 'F50_ERROR_BOOT_CHANGED' in output or 'more than one' in output:
            raise RuntimeError('目标设备不明确，已停止：' + output)
        if not boot_id(output):
            time.sleep(0.2)
            continue
        adb.remember_device(output)
        adb.start_tcp(adb.usb if current == target else current)
        if 'F50_ERROR_' in output:
            raise RuntimeError('已尝试保持 ADB，但 root 配置未完成：\n' + output)
        if 'F50_BOOTSTRAP_SAVED' in output or any('F50_ROOT=' + method in output.splitlines()
                                                 for method in ('shell', 'su', 'socket')):
            return wait_root(adb)
        host.say('准备完成前断开；将通过已核对身份的 USB 或 TCP 继续，尚未确认 root。')
        time.sleep(0.2)
    raise RuntimeError(f'{WAIT}s 内未完成 USB/TCP 准备，详见 {host.log_path}')


def ensure_stable(adb):
    host = adb.host
    payload = usb_payload(host.top)
    host.say('开始连接保持：USB、TCP 5555 与 root 必须连续可用 30 秒，保持阶段最多 60 秒。')
    host.say('实际 USB 恢复为 adb-only 时，USB 网卡可能断开；TCP 备用连接需要电脑连着 F50 Wi-Fi。')
    deadline = time.monotonic() + 60
    stable_since = None
    last_apply = 0
    last_report = 0
    tcp_restarts = 0
    next_tcp_restart = 0
    while time.monotonic() < deadline:
        current = adb.live_target(deadline)
        if not current:
            stable_since = None
            host.say('USB/TCP 暂时均不可用，等待已配置的连接恢复……')
            time.sleep(0.5)
            continue
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        method, _ = adb.root_method(current, timeout=min(2, remaining))
        if not method:
            current, method = wait_root(adb, deadline)
        try:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            rc, output = adb.run_root(current, method, CHECK, timeout=min(3, remaining),
                                      label='读取实际 USB/端口/root 状态')
        except subprocess.TimeoutExpired:
            stable_since = None
            continue
        if 'F50_ERROR_BOOT_CHANGED' in output:
            raise RuntimeError('设备已重启，停止使用旧身份继续配置；请重新执行 make adb。')
        if rc or boot_id(output) != adb.expected_boot:
            stable_since = None
            time.sleep(0.5)
            continue
        ready = all(item in output.splitlines() for item in WANT)
        if not ready:
            stable_since = None
            if time.monotonic() - last_apply >= 5:
                # This changes only idempotent ADB settings, never installation writes.
                host.say('实际 USB 或端口配置尚未保持，使用 root 恢复（USB 切换在独立会话中完成）。')
                last_apply = time.monotonic()
                remaining = deadline - last_apply
                if remaining <= 0:
                    break
                try:
                    adb.run_root(current, method, payload, timeout=min(5, remaining), label='恢复实际 ADB 功能')
                except subprocess.TimeoutExpired:
                    host.say('设置请求期间断线，结果未知；重新读回实际状态后再决定是否补写。')
            time.sleep(1)
            continue
        usb_ok = adb.probe_transport(adb.usb, deadline)
        tcp_ok = adb.tcp_target(deadline) is not None
        if not usb_ok or not tcp_ok:
            stable_since = None
            host.say(f'连接尚未全部就绪：USB={usb_ok}，TCP={tcp_ok}。')
            if usb_ok and not tcp_ok and tcp_restarts < 2 and time.monotonic() >= next_tcp_restart:
                tcp_restarts += 1
                host.say(f'已确认 USB/root 恢复，补发 TCP 监听请求（{tcp_restarts}/2）。')
                adb.start_tcp(adb.usb, deadline)
                next_tcp_restart = time.monotonic() + 5
            time.sleep(1)
            continue
        now = time.monotonic()
        if stable_since is None:
            stable_since = now
        if now - last_report >= 5:
            host.say(f'USB/TCP/root 持续可用 {now - stable_since:.1f}/30s。')
            last_report = now
        if now - stable_since >= 30:
            host.say(f'完成：USB、{adb.config.endpoint} 和 root 通道连续 30 秒可用。可以运行 make install。')
            host.say('本次开机已确认；Android 重启后会重新准备 root，不能据此承诺永久开启。')
            return
        time.sleep(1)
    raise RuntimeError(f'60s 内未达到 USB/TCP/root 连续 30s 可用，未宣告成功。请查看 {host.log_path}')
