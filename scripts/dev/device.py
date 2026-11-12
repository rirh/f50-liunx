#!/usr/bin/env python3
"""SSH commands and scoped LuCI deployment used by make/*.mk (Python 3.8+)."""
import argparse
import os
from pathlib import Path
import re
import shlex
import subprocess
import tarfile
import tempfile
from urllib.parse import urlsplit


TOP = Path(__file__).resolve().parents[2]
OPENWRT = 'test -r /etc/openwrt_release || { echo "This command requires OpenWrt" >&2; exit 1; }\n'
CLEAR_CACHE = 'rm -f /tmp/luci-indexcache /tmp/luci-indexcache.*; rm -rf /tmp/luci-modulecache\n'
COMMANDS = {
    'status': '''uname -r
cat /etc/mu300/image-version
uptime
free -m
/etc/init.d/rpcd status || true
/etc/init.d/uhttpd status || true
''',
    'cache-clear': CLEAR_CACHE,
    'restart': CLEAR_CACHE + '/etc/init.d/rpcd restart\n/etc/init.d/uhttpd restart\n',
    'reboot': 'sync\nreboot\n',
    'logs': 'exec logread -f\n',
    'debug': "ubus call system board\nubus call mu300dash status '{}'\n",
}

# Receive the whole archive before changing files. The backup contains only the
# existing files that this deployment replaces; newly created paths are recorded.
DEPLOY = '''set -eu
test -r /etc/openwrt_release || { echo "Deployment requires OpenWrt" >&2; exit 1; }
umask 077
stage=$(mktemp -d /tmp/mu300-deploy.XXXXXX)
trap 'rm -rf "$stage"' EXIT
cat > "$stage/payload.tar.gz"
tar -tzf "$stage/payload.tar.gz" > "$stage/files.list"
theme_helper=usr/libexec/mu300-ui-theme
theme_header=usr/share/ucode/luci/template/themes/aurora/header.ut
theme=0
if grep -qx "$theme_helper" "$stage/files.list"; then
    theme=1
    [ ! -f "/$theme_header" ] || printf '%s\\n' "$theme_header" >> "$stage/files.list"
fi
backup="/root/mu300-dev-backups/$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$backup"
: > "$backup/existing.list"
: > "$backup/new.list"
while IFS= read -r path; do
    if [ -e "/$path" ] || [ -L "/$path" ]; then
        printf '%s\\n' "$path" >> "$backup/existing.list"
    else
        printf '%s\\n' "$path" >> "$backup/new.list"
    fi
done < "$stage/files.list"
if [ -s "$backup/existing.list" ]; then
    tar -czf "$backup/previous.tar.gz" -C / -T "$backup/existing.list"
fi
printf 'Backup: %s\\n' "$backup"
umask 022
tar -xzf "$stage/payload.tar.gz" -C /
if [ "$theme" = 1 ]; then
    sh "/$theme_helper"
fi
rm -f /tmp/luci-indexcache /tmp/luci-indexcache.*
rm -rf /tmp/luci-modulecache
printf 'Deployed files. No services were restarted.\\n'
'''


def ssh_command():
    host = os.environ.get('SSH_HOST') or urlsplit(os.environ.get('DEVICE', 'http://192.168.77.1')).hostname
    user = os.environ.get('SSH_USER', 'root')
    port = os.environ.get('SSH_PORT', '22')
    if not host or host.startswith('-') or not re.fullmatch(r'[A-Za-z0-9_.:-]+', host):
        raise ValueError('SSH_HOST must be a hostname or IP address')
    if not re.fullmatch(r'[A-Za-z0-9_-]+', user):
        raise ValueError('Invalid SSH_USER')
    if not port.isdigit() or not 1 <= int(port) <= 65535:
        raise ValueError('SSH_PORT must be between 1 and 65535')
    args = ['ssh', '-o', 'ConnectTimeout=10', '-p', port, '-l', user]
    for variable, option in (('SSH_IDENTITY', 'IdentityFile'), ('SSH_KNOWN_HOSTS', 'UserKnownHostsFile')):
        value = os.environ.get(variable)
        if value:
            path = Path(value).expanduser()
            if not path.is_absolute():
                path = TOP / path
            quoted_path = str(path).replace('\\', '\\\\').replace('"', '\\"')
            args += ['-o', '%s="%s"' % (option, quoted_path)]
    alias = os.environ.get('SSH_HOST_KEY_ALIAS')
    if alias:
        args += ['-o', 'HostKeyAlias=' + alias]
    return args + [host]


def deployment_files(scope):
    roots = []
    if scope in ('ui', 'all'):
        # Same precedence as the image build and local development proxy.
        roots += [('systems/openwrt/overlay/www', 'www'), ('systems/openwrt/luci-app-mu300/htdocs', 'www')]
    if scope in ('backend', 'all'):
        # Keep /etc/config, startup policy and firmware out of a development deploy.
        roots += [('systems/openwrt/luci-app-mu300/root/usr', 'usr')]
    files = {}
    for directory, destination in roots:
        root = TOP / directory
        for source in sorted(root.rglob('*')):
            if source.is_symlink():
                raise ValueError('Deployment does not copy symbolic links: %s' % source)
            if not source.is_file():
                continue
            relative = source.relative_to(root)
            if any(part.startswith('.') or part == '__pycache__' for part in relative.parts):
                continue
            name = (Path(destination) / relative).as_posix()
            if '\n' in name or '\r' in name:
                raise ValueError('Invalid deployment path: %r' % name)
            files[name] = source
    if scope in ('ui', 'all'):
        helper = 'usr/libexec/mu300-ui-theme'
        files[helper] = TOP / 'systems/openwrt/luci-app-mu300/root' / helper
    if not files:
        raise ValueError('No files found for this deployment')
    return files


def deploy(ssh, scope):
    files = deployment_files(scope)
    with tempfile.TemporaryDirectory(prefix='mu300-deploy-') as scratch:
        archive = Path(scratch) / 'payload.tar.gz'
        with tarfile.open(archive, 'w:gz', format=tarfile.GNU_FORMAT) as bundle:
            for name, source in sorted(files.items()):
                info = bundle.gettarinfo(str(source), arcname=name)
                info.uid = info.gid = 0
                info.uname = info.gname = 'root'
                info.mode = 0o755 if name.startswith('usr/libexec/') else 0o644
                with source.open('rb') as stream:
                    bundle.addfile(info, stream)
        print('Deploying %d %s files to %s via SSH.' % (len(files), scope, ssh[-1]), flush=True)
        with archive.open('rb') as stream:
            subprocess.run(ssh + ['sh -c ' + shlex.quote(DEPLOY)], stdin=stream, check=True)
    if scope in ('backend', 'all'):
        print('Run make restart to reload the deployed RPC, menu and ACL files.')
    else:
        print('Refresh the device page with browser caching disabled to load the deployed assets.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['deploy', 'ssh', 'rpc'] + list(COMMANDS))
    parser.add_argument('--scope', choices=['ui', 'backend', 'all'], default='ui')
    args = parser.parse_args()
    try:
        ssh = ssh_command()
        if args.action == 'deploy':
            deploy(ssh, args.scope)
        elif args.action == 'ssh':
            subprocess.run(ssh, check=True)
        else:
            if args.action == 'rpc':
                obj = os.environ.get('RPC_OBJECT', 'mu300dash')
                command = 'ubus -v list ' + shlex.quote(obj)
            else:
                command = COMMANDS[args.action]
            subprocess.run(ssh + ['sh -c ' + shlex.quote('set -e\n' + OPENWRT + command)], check=True)
    except (ValueError, OSError) as error:
        parser.exit(1, '%s\n' % error)
    except subprocess.CalledProcessError as error:
        parser.exit(error.returncode if error.returncode > 0 else 1)
    except KeyboardInterrupt:
        parser.exit(130)


if __name__ == '__main__':
    main()
