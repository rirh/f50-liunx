"""Command-line options and preparation deadlines."""
import argparse
from dataclasses import dataclass
import ipaddress
from typing import Optional

WAIT = 300
ROOT_WAIT = 10


@dataclass(frozen=True)
class Config:
    ip: str
    port: Optional[int]
    serial: Optional[str]
    adb_only: bool

    @property
    def endpoint(self):
        return self.ip + ':5555'

    @property
    def console_url(self):
        return f'http://{self.ip}/index.html#usb_port'


def parse_args(argv=None):
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
    args = parser.parse_args(argv)
    try:
        ipaddress.IPv4Address(args.ip)
    except ValueError:
        parser.error('--ip 必须是 IPv4 地址')
    if args.serial and ':' in args.serial:
        parser.error('--serial 应为 USB 序列号，不能是无线 ADB 地址')
    return Config(args.ip, args.port, args.serial, args.adb_only)
