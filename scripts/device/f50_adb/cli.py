"""Initialize one preparation run and report its exit status."""
import http.client
from pathlib import Path
import subprocess
import sys

from .config import parse_args
from .host import Host
from .workflow import prepare


def main(argv=None):
    config = parse_args(argv)
    host = None
    try:
        host = Host(Path(__file__).resolve().parents[3])
        prepare(config, host)
        return 0
    except KeyboardInterrupt:
        if host:
            host.say('\n已停止。已写入设备的设置不会自动撤销。')
        return 130
    except (RuntimeError, OSError, ValueError, KeyError, http.client.HTTPException,
            subprocess.TimeoutExpired) as exc:
        if host:
            host.say(f'错误：{exc}')
        else:
            print(f'错误：{exc}', file=sys.stderr)
        return 1
    finally:
        if host:
            host.close()
