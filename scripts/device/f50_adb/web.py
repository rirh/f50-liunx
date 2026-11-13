"""Stock F50 Goform requests and authenticated USB/Samba settings."""

import hashlib
import http.client
import json
import time
from urllib.parse import urlencode


WEB_ERRORS = (OSError, http.client.HTTPException, ValueError, RuntimeError)


def web_ok(result):
    return str(result.get('result', '')).lower() in ('0', 'success')


class WebConsole:
    def __init__(self, config, host):
        self.config = config
        self.host = host
        self.port = None

    def request(self, fields, cookie='', post=False, sent=None, port=None, timeout=8):
        port = port or self.port
        path = '/goform/goform_set_cmd_process' if post else '/goform/goform_get_cmd_process'
        headers = {'User-Agent': 'Mozilla/5.0', 'Referer': f'http://{self.config.ip}:{port}/index.html'}
        if cookie:
            headers['Cookie'] = cookie
        body = None
        if post:
            headers['Content-Type'] = 'application/x-www-form-urlencoded; charset=UTF-8'
            body = urlencode(fields)
        else:
            path += '?' + urlencode(dict(fields, isTest='false', _=str(time.time_ns() // 1000000)))
        connection = http.client.HTTPConnection(self.config.ip, port, timeout=timeout)
        # Log only the operation name, never password/hash, AD, LD, RD or Cookie.
        operation = fields.get('goformId', fields.get('cmd', 'goform'))
        self.host.say(f'Web {"POST" if post else "GET"} {self.config.ip}:{port}：{operation}')
        try:
            connection.request('POST' if post else 'GET', path, body=body, headers=headers)
            if sent:
                sent()  # Arm the adb waiter as soon as the reboot is sent, before the HTTP response.
            response = connection.getresponse()
            self.host.say(f'Web {operation}：HTTP {response.status}')
            data = response.read()
            if response.status != 200:
                raise RuntimeError(f'设备 Web 接口返回 HTTP {response.status}')
            result = json.loads(data)
            if not isinstance(result, dict):
                raise ValueError('设备 Web 接口返回了无法识别的 JSON')
            return result, (response.getheader('Set-Cookie') or '').split(';', 1)[0]
        finally:
            connection.close()

    def find_web_port(self):
        # The console is on port 80 (http://192.168.0.1/index.html); some firmware also serves it on 8080.
        tried = []
        for port in ([self.config.port] if self.config.port else [80, 8080]):
            try:
                result, _ = self.request({'cmd': 'LD'}, port=port)
                if result.get('LD'):
                    return port
                tried.append(f'{port}: 无 LD')
            except WEB_ERRORS as exc:
                tried.append(f'{port}: {exc}')
        raise RuntimeError(f'连不上 F50 后台 {self.config.ip}（{"; ".join(tried)}）。请确认电脑连着 F50 的 Wi-Fi 或 USB 网络，'
                           f'并能在浏览器打开 {self.config.console_url}')

    def usb_switch(self, cookie=''):
        # Some firmware requires the authenticated session even for this read.
        try:
            result, _ = self.request({'cmd': 'usb_port_switch'}, cookie)
        except WEB_ERRORS as exc:
            self.host.say(f'后台 USB 开关读回失败，状态未知：{exc}')
            return ''
        state = str(result.get('usb_port_switch', ''))
        if state not in ('0', '1'):
            self.host.say(f'后台 USB 开关状态未知：{result.get("Error", "返回中没有有效的 usb_port_switch")}')
            return ''
        return state

    def wait_web(self, seconds=15):
        # Changing the USB setting re-enumerates the F50's USB functions: a computer on its USB network loses the
        # link for a few seconds (Errno 51, timeouts) until the interface is back and has an address again.
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            try:
                if self.request({'cmd': 'LD'}, timeout=min(2, deadline - time.monotonic()))[0].get('LD'):
                    return
            except WEB_ERRORS as exc:
                self.host.say(f'等待 USB 网络恢复：{exc}')
            time.sleep(min(0.5, max(0, deadline - time.monotonic())))
        raise RuntimeError(f'{seconds} 秒内没有连回 F50 后台 {self.config.ip}（USB 网络没有恢复）。'
                           f'请连接 F50 Wi-Fi，在 {self.config.console_url} 恢复打开 USB 调试；不要继续盲目重启。')

    def prepare_post(self, password, fields):
        # Like zreq.exe: log in again for every request, SHA256 challenge, the session cookie, then the AD token.
        def digest(value):
            return hashlib.sha256(value.encode('utf-8')).hexdigest().upper()

        ld, _ = self.request({'cmd': 'LD'})
        login, cookie = self.request({'goformId': 'LOGIN', 'isTest': 'false',
                                      'password': digest(digest(password) + ld['LD'])}, post=True)
        if not cookie or str(login.get('result', '')) not in ('0', 'success'):
            raise RuntimeError(f'后台登录失败（{login}）：请检查后台密码（不是 Wi-Fi 密码）')
        info, _ = self.request({'cmd': 'Language,cr_version,wa_inner_version', 'multi_data': '1'}, cookie)
        rd, _ = self.request({'cmd': 'RD'}, cookie)
        ad = digest(digest(info['wa_inner_version'] + info['cr_version']) + rd['RD'])
        return dict(fields, isTest='false', AD=ad), cookie

    def ensure_web_samba(self, password):
        # UFI-TOOLS main.js initSMBStatus/handleSambaPath: enable sharing before preparing its root hook.
        try:
            fields, cookie = self.prepare_post(password, {'goformId': 'SAMBA_SETTING', 'samba_switch': '1'})
            state, _ = self.request({'cmd': 'samba_switch'}, cookie)
            if str(state.get('samba_switch', '')) == '1':
                self.host.say('后台文件共享已开启，不重复设置。')
                return
            self.host.say('通过原厂 SAMBA_SETTING 开启文件共享……')
            result, _ = self.request(fields, cookie, post=True)
            if not web_ok(result):
                raise RuntimeError(f'SAMBA_SETTING 返回：{result}')
            state, _ = self.request({'cmd': 'samba_switch'}, cookie)
            value = str(state.get('samba_switch', ''))
            self.host.say(f'后台文件共享读回：{value if value in ("0", "1") else "未知"}；后续仍检查实际 SMB 会话。')
        except WEB_ERRORS as exc:
            self.host.say(f'后台文件共享未确认：{exc}；捕获 ADB 后会检查本机 samba_enable。')
