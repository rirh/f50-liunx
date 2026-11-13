"""Build the SMB helper and trigger a known share, with a bounded macOS fallback."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from urllib.parse import quote


def prepare_helper(host):
    # Build before touching USB, never during its short connection window.
    helper = host.top / 'work/bin/f50-smb-trigger'
    source_dir = host.top / 'scripts/device/smb-trigger'
    if not os.access(helper, os.X_OK) or any(
            source.stat().st_mtime_ns > helper.stat().st_mtime_ns
            for source in source_dir.glob('*')):
        go = shutil.which('go', path=host.env.get('PATH'))
        if not go:
            raise RuntimeError('首次运行需要 Go 1.20+ 构建 SMB 客户端；尚未操作设备。')
        helper.parent.mkdir(parents=True, exist_ok=True)
        temporary = helper.with_name(f'{helper.name}.tmp.{os.getpid()}')
        host.say('准备用户态 SMB 客户端（首次运行会下载固定版本的 Go 依赖）……')
        try:
            result = subprocess.run([go, 'build', '-mod=readonly', '-trimpath', '-o', str(temporary), '.'],
                                    cwd=source_dir, env=host.env)
            if result.returncode:
                raise RuntimeError(f'SMB 客户端构建失败（退出码 {result.returncode}）；尚未操作设备。')
            temporary.replace(helper)
        finally:
            temporary.unlink(missing_ok=True)


def trigger_samba(host, ip, shares, deadline):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError('SMB 阶段已到达总等待上限')
    # Known share names avoid the IPC$/srvsvc enumeration path that smbutil stalled in.
    native_shares = [name for name in shares if '/' not in name and '\\' not in name]
    if sys.platform != 'darwin':
        native_shares = []
    helper_timeout = min(3 if native_shares else 6, remaining)
    command = [str(host.top / 'work/bin/f50-smb-trigger'), '--address', ip + ':445',
               '--timeout', str(helper_timeout)]
    for name in shares:
        command += ['--share', name]
    try:
        rc, output = host.run(command, timeout=min(remaining, helper_timeout + 0.25), label='用户态 SMB 会话与共享访问')
        if rc == 0 and 'F50_SMB_TRIGGERED' in output.splitlines():
            return '共享访问成功；以随后的 UID=0 结果确认 root。'
        last = output or f'SMB helper 退出码 {rc}'
    except (OSError, subprocess.TimeoutExpired) as exc:
        last = str(exc)
        host.say('用户态 SMB 触发失败：' + last)
    for name in native_shares:
        remaining = deadline - time.monotonic()
        # Reserve time for unmounting and the next boot-ID/UID probe.
        if remaining <= 3:
            break
        mountpoint = Path(tempfile.mkdtemp(prefix='f50-smb-', dir=host.top / 'work'))
        try:
            rc, output = host.run(['/sbin/mount_smbfs', '-N', '-o', 'ro,nobrowse',
                                   f'//guest:@{ip}/{quote(name, safe="")}', str(mountpoint)],
                                  timeout=min(3, remaining - 3), label='macOS 只读 SMB 共享回退')
            if rc == 0:
                return 'macOS 共享访问成功；以随后的启动标识与 UID=0 结果确认 root。'
            last = output or f'macOS SMB 挂载退出码 {rc}'
        except (OSError, subprocess.TimeoutExpired) as exc:
            last = str(exc)
            host.say('macOS SMB 回退失败：' + last)
        finally:
            if mountpoint.is_mount():
                # Cleanup must also run after STOP is set, so it cannot use run().
                try:
                    result = subprocess.run(['/sbin/umount', str(mountpoint)], stdin=subprocess.DEVNULL,
                                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                            text=True, env=host.env, timeout=2, start_new_session=True)
                    if result.returncode:
                        raise RuntimeError(result.stdout.strip() or f'umount 退出码 {result.returncode}')
                except (OSError, subprocess.TimeoutExpired, RuntimeError) as exc:
                    message = f'临时 SMB 挂载未能卸载，已保留目录 {mountpoint}：{exc}'
                    host.say(message)
                    raise RuntimeError(message) from exc
            mountpoint.rmdir()
    return last
