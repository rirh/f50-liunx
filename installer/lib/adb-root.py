#!/usr/bin/env python3
"""Run one installer command through the F50's local root socket.

Stream output and require the remote exit marker. Never replay a command after
a disconnect or timeout: installation writes may already have taken effect.
"""
import argparse
import os
import re
import selectors
import shlex
import signal
import subprocess
import sys
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', default='adb')
    target = parser.add_mutually_exclusive_group()
    target.add_argument('--serial')
    target.add_argument('--usb', action='store_true')
    parser.add_argument('--timeout', type=float, default=7200)
    parser.add_argument('command')
    args = parser.parse_args()
    selector = ['-s', args.serial] if args.serial else (['-d'] if args.usb else [])
    marker = 'F50_ROOT_' + uuid.uuid4().hex
    # Files created for adb pull must remain readable by Android's shell user.
    script = ('sh -c ' + shlex.quote('umask 022\n' + args.command) + ' </dev/null\n'
              '__f50_rc=$?\nprintf "\\n' + marker + '=%s\\n" "$__f50_rc"\nexit\n')
    process = subprocess.Popen(
        [args.adb, *selector, 'shell', '-T', 'nc', '-U', '/data/local/tmp/f50-root/command.sock'],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    tail = b''
    deadline = time.monotonic() + args.timeout
    try:
        process.stdin.write(script.encode())
        process.stdin.close()
        with selectors.DefaultSelector() as reader:
            reader.register(process.stdout, selectors.EVENT_READ)
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0 or not reader.select(remaining):
                    raise TimeoutError
                data = os.read(process.stdout.fileno(), 65536)
                if not data:
                    break
                tail += data
                if len(tail) > 160:
                    sys.stdout.buffer.write(tail[:-160])
                    sys.stdout.buffer.flush()
                    tail = tail[-160:]
        status = process.wait(timeout=max(0.1, deadline - time.monotonic()))
    except (TimeoutError, subprocess.TimeoutExpired, KeyboardInterrupt, BrokenPipeError):
        process.kill()
        process.wait()
        sys.stderr.buffer.write(tail)
        print('Root socket interrupted; the remote command may have run. Do not retry writes blindly.',
              file=sys.stderr)
        return 124
    finally:
        process.stdout.close()
    match = re.search(rb'\n' + marker.encode() + rb'=(\d+)\r?\n[\r\n]*\Z', tail)
    if match is None:
        sys.stderr.buffer.write(tail)
        print('Root socket completion marker missing; success is unknown.', file=sys.stderr)
        return status if status > 0 else 1
    sys.stdout.buffer.write(tail[:match.start()])
    sys.stdout.buffer.flush()
    return int(match[1]) or (status if status >= 0 else 128 - status)


def interrupted(*_):
    raise KeyboardInterrupt


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, interrupted)
    sys.exit(main())
