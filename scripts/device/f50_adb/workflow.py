"""Coordinate Web recovery, the short USB window and root preparation."""
import getpass
import http.client
import shlex
import threading
import time

from .adb import AdbClient
from .config import ROOT_WAIT, WAIT
from .root import ensure_stable, inject_configuration
from .smb import prepare_helper
from .web import WebConsole, web_ok


def enable_via_web(web, adb, password):
    host = adb.host
    config = adb.config
    web.port = web.find_web_port()
    host.say(f'F50 后台：http://{config.ip}:{web.port}/')
    if adb.usb_target():
        host.say('实际 USB ADB 已连接，跳过开关切换和重启。')
        return None
    host.say('实际 USB ADB 未连接，按原 EXE 执行 USB_PORT_SETTING 0 → 1。')
    for value in ('0', '1'):
        # Reauthenticate after each USB re-enumeration, as each zreq invocation does.
        fields, cookie = web.prepare_post(password, {'goformId': 'USB_PORT_SETTING', 'usb_port_switch': value})
        state = web.usb_switch(cookie)
        host.say(f'设置 USB 调试（usb_port_switch {state or "未知"} → {value}）……')
        try:
            result, _ = web.request(fields, cookie, post=True)
            if not web_ok(result):
                raise RuntimeError(f'USB 调试设置未成功：{result}')
        except (OSError, http.client.HTTPException, ValueError):
            host.say(f'设置 {value} 的 HTTP 响应中断，先等待网络恢复，再读回结果。')
        time.sleep(0.7)
        web.wait_web()
        state = web.usb_switch(cookie)
        host.say(f'后台开关读回：{state or "未知（会话可能已失效）"}；尚不能据此认定 USB ADB 已出现。')
        if value == '1' and state == '0':
            raise RuntimeError(f'开启请求后仍读到关闭状态，请在 {config.console_url} 打开 USB 调试。')
    if not config.adb_only:
        web.ensure_web_samba(password)
    if adb.usb_target():
        host.say('0 → 1 切换后已检测到实际 USB ADB，跳过重启。')
        return None
    return web.prepare_post(password, {'goformId': 'REBOOT_DEVICE'})


def prepare(config, host):
    adb = AdbClient(config, host)
    web = WebConsole(config, host)
    thread = None
    try:
        rc, output = host.run([host.adb, 'start-server'])
        if rc:
            raise RuntimeError('无法启动本机 ADB 服务：' + output)
        target = adb.usb_target(verbose=False)
        if target and not config.prepare_root:
            host.say('USB ADB 连接成功。')
            host.say('进入设备：' + shlex.join([host.adb, *target, 'shell']))
            return
        host.say(f'日志文件：{host.log_path}')
        if not config.adb_only:
            prepare_helper(host)
        host.say(f'设备 {config.ip}；USB 目标 {config.serial or "单台 USB 设备"}；Samba/root 等待上限 {ROOT_WAIT}s。')
        if not config.adb_only:
            host.say('默认开启 TCP 5555 备用 ADB。请连接 F50 Wi-Fi：USB 稳定模式会移除 USB 网卡/MTP 功能。')
        if target:
            if config.adb_only:
                adb.wait_usb_only()
                return
            rc, serial = host.run([host.adb, *target, 'get-serialno'], timeout=4)
            if serial.startswith('F50LINUX'):
                host.say('设备当前运行 F50 Linux，ADB 已可用；安装器会先让它重启回 Android。')
                return
            host.require_f50_wifi(config.ip)
            host.say('USB ADB 已连接，复用连接准备 TCP/root；已有正确属性不会重复写入，不重启设备。')
            inject_configuration(adb, target)
        else:
            if not config.adb_only:
                host.require_f50_wifi(config.ip)
            host.say('没有检测到 USB ADB。前置步骤：')
            host.say('  1. 用数据线把 F50 连到这台电脑（部分充电线不带数据）。')
            host.say(f'  2. 浏览器打开 {config.console_url}，用后台密码登录，开启 ADB 调试并保存。')
            host.say('  3. 若电脑仍看不到设备，重启一次 F50。')
            try:
                web.port = web.find_web_port()
                if web.usb_switch() == '0':
                    host.say('注意：后台显示 USB 调试未开启（usb_port_switch=0），直接回车等待不会出现 ADB，请输入后台密码。')
            except RuntimeError:
                pass
            try:
                password = getpass.getpass('完成后直接按回车等待；或输入 F50 后台密码，由脚本自动开启并重启：')
            except EOFError:
                password = ''
            if not password:
                host.say(f'等待 USB ADB（最多 {60 if config.adb_only else WAIT} 秒）……')
                if config.adb_only:
                    adb.wait_usb_only()
                else:
                    inject_configuration(adb, adb.usb)
            else:
                reboot_request = enable_via_web(web, adb, password)
                password = None
                outcome = []

                def worker():
                    try:
                        if config.adb_only:
                            adb.wait_usb_only()
                        else:
                            inject_configuration(adb, adb.usb)
                    except Exception as exc:
                        outcome.append(exc)

                # The worker must finish temporary SMB mount cleanup before exit.
                thread = threading.Thread(target=worker)
                if reboot_request is None or adb.usb_target():
                    host.say('USB ADB 已出现，直接确认连接，不重启。' if config.adb_only else
                             '开启 USB 调试后 ADB 已连接，直接准备 root 通道，不重启。')
                    thread.start()
                else:
                    fields, cookie = reboot_request
                    host.say('重启 F50，并等待重启后的 USB ADB……')
                    try:
                        result, _ = web.request(fields, cookie, post=True, sent=thread.start)
                        if not web_ok(result):
                            host.say(f'重启接口返回 {result}；如果设备没有重启，请现在手动重启。')
                    except (OSError, http.client.HTTPException, ValueError) as exc:
                        if not thread.ident:
                            raise RuntimeError('重启请求没有发出：' + str(exc)) from exc
                        host.say('重启请求已发出（连接随重启断开），继续等待 ADB。')
                while thread.is_alive():
                    thread.join(0.5)
                if outcome:
                    raise outcome[0]
            if config.adb_only:
                return
        ensure_stable(adb)
    finally:
        # Keep logging available until cancelled commands and mount cleanup finish.
        host.stop()
        if thread is not None and thread.ident is not None:
            thread.join()
