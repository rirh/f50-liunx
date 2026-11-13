"""Extras: optional components that are not part of the image and live on the Linux partition
(F50_DISK/extra/<name>), and the VPN module (vpn, vpn-mihomo) from its own repository, dikeckaan/mu300-linux-vpn:
f50-update installs and updates them (sourced with F50_LIB=1 against a fake partition). The helpers here are shared
with other tests."""
import hashlib
import io
import os
import shutil
import subprocess
import tarfile
import unittest

from helpers import BIN, ShellTest

ENGINES = ('xray', 'hev-socks5-tunnel', 'sing-box')
VPN_URL = 'http://10.0.0.2:8000/vpn-module'


def extra_tarball(path, name='vpn', release='v2026.10.10', bins=ENGINES, components='xray 26.3.27\n'):
    """an extra in the format of the f50-linux releases (./name ./release ./components ./bin): for the VPN, what
    the releases before the module carried (the engines alone, no f50-vpn)"""
    with tarfile.open(path, 'w:gz') as t:
        def add(fn, data, mode=0o644):
            ti = tarfile.TarInfo(fn)
            ti.size, ti.mode = len(data), mode
            t.addfile(ti, io.BytesIO(data))
        if name is not None:
            add('./name', (name + '\n').encode())
        add('./release', (release + '\n').encode())
        add('./components', components.encode())
        for b in bins:
            add(f'./bin/{b}', f'#!/bin/sh\necho {b} {release}\n'.encode(), 0o755)
    return path


# the module's hooks: they record what they were asked (into $STUBLOG/hooks) and, for a system on the disk, do what
# the real ones do in that system: f50-vpn linked to the extra's
HOOK_LINK = ('echo "link [$1]" >> "$STUBLOG/hooks"\n'
             '[ -z "$1" ] || { mkdir -p "$1/opt/f50/bin" && ln -sfn "$F50_EXTRA_DIR/bin/f50-vpn" "$1/opt/f50/bin/f50-vpn"; }\n')
HOOK_UNLINK = 'echo "unlink [$1]" >> "$STUBLOG/hooks"\n[ -z "$1" ] || rm -f "$1/opt/f50/bin/f50-vpn"\n'


def module_tarball(path, version='v1.0.0', requires='f50-linux v2026.10.16', f50_vpn=True, hooks=True,
                   bins=ENGINES, extra=()):
    """f50-linux-vpn.tar.gz as the module repository publishes it (the spec's layout)"""
    files = [('./VERSION', version + '\n', 0o644), ('./components', f'f50-linux-vpn {version}\nxray 26.3.27\n', 0o644)]
    if requires is not None:
        files.append(('./requires', requires + '\n', 0o644))
    if f50_vpn:
        files.append(('./bin/f50-vpn', f'#!/bin/sh\necho f50-vpn {version} "$@"\n', 0o755))
    files += [(f'./bin/{b}', f'#!/bin/sh\necho {b} {version}\n', 0o755) for b in bins]
    files += [('./lib/vpn/x.sh', 'drv_start() { :; }\n', 0o644), ('./lib/vpn/openvpn-up', '#!/bin/sh\n', 0o755),
              ('./etc/vpn.conf.example', 'ENABLE=0\n', 0o644),
              ('./service/systemd/f50-vpn.service', '[Service]\n', 0o644),
              ('./service/procd/f50-vpn', '#!/bin/sh /etc/rc.common\n', 0o755),
              ('./install.sh', '#!/bin/sh\n', 0o755)]
    if hooks:
        files += [('./hooks/link', HOOK_LINK, 0o755), ('./hooks/unlink', HOOK_UNLINK, 0o755)]
    files += list(extra)
    with tarfile.open(path, 'w:gz') as t:
        for fn, data, mode in files:
            data = data.encode() if isinstance(data, str) else data
            ti = tarfile.TarInfo(fn)
            ti.size, ti.mode = len(data), mode
            t.addfile(ti, io.BytesIO(data))
    return path


def mihomo_tarball(path, version='v1.0.0'):
    with tarfile.open(path, 'w:gz') as t:
        for fn, data, mode in (('./VERSION', f'{version}\n', 0o644), ('./components', 'mihomo 1.19.32\n', 0o644),
                               ('./requires', 'f50-linux v2026.10.16\n', 0o644),
                               ('./bin/mihomo', '#!/bin/sh\necho mihomo\n', 0o755)):
            ti = tarfile.TarInfo(fn)
            ti.size, ti.mode = len(data), mode
            t.addfile(ti, io.BytesIO(data.encode()))
    return path


def sums(d, *names):
    """SHA256SUMS in directory D for the files NAMES there"""
    (d / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256((d / n).read_bytes()).hexdigest()}  {n}\n' for n in names))


class ExtrasBase(ShellTest):
    def setUp(self):
        super().setUp()
        self.disk = self.tmp / 'disk'
        for os_ in ('ubuntu', 'openwrt'):
            (self.disk / os_ / 'etc/f50').mkdir(parents=True)
        self.root = self.tmp / 'root'
        (self.root / 'run/f50').mkdir(parents=True)
        (self.root / 'run/f50/device').write_text('f50\n')
        self.links = self.tmp / 'links'
        self.links.mkdir()
        self.stub('id', 'echo 0')

    def up(self, shell, code, **env):
        # (F50_VPN_UNVERIFIED: the tests' lone module files have no SHA256SUMS; the refusal is tested with it empty)
        e = dict(F50_LIB=1, F50_DISK=self.disk, F50_BIN=BIN, F50_SYSROOT=self.root, F50_FETCH_DELAY=0,
                 F50_VPN_UNVERIFIED=1)
        e.update(env)
        return self.sh(shell, f'. "{BIN}/f50-update"; {code}', **e)

    def vpn(self):
        return self.disk / 'extra' / 'vpn'

    def old_vpn_extra(self, release='v2026.10.15'):
        """the vpn extra of a release before the module: the engines alone (an older f50-update installed it)"""
        shutil.rmtree(self.vpn(), ignore_errors=True)
        (self.vpn() / 'bin').mkdir(parents=True)
        (self.vpn() / 'name').write_text('vpn\n')
        (self.vpn() / 'release').write_text(release + '\n')
        for e in ENGINES:
            (self.vpn() / 'bin' / e).write_text('#!/bin/sh\n')
            (self.vpn() / 'bin' / e).chmod(0o755)

    def versions(self, v):
        """every system on the disk, and the running one, is release V"""
        for os_ in ('ubuntu', 'openwrt'):
            if (self.disk / os_).exists():
                (self.disk / os_ / 'etc/f50/image-version').write_text(v + '\n')
        (self.root / 'etc/f50').mkdir(parents=True, exist_ok=True)
        (self.root / 'etc/f50/image-version').write_text(v + '\n')

    def hooks(self):
        f = self.tmp / 'hooks'
        lines = f.read_text().splitlines() if f.exists() else []
        f.unlink(missing_ok=True)
        return lines

    def serve(self, release='v2026.10.10', assets=None):
        """a release on a 'server': a directory curl (stubbed) reads from, with its SHA256SUMS. The URL's last two
        parts name the directory and the file (.../vpn-module/SHA256SUMS: the module's 'release')."""
        d = self.tmp / 'server' / release
        d.mkdir(parents=True, exist_ok=True)
        if assets is None:
            assets = {'f50-extra-lang.tar.gz': lang_tarball(self.tmp / 'payload.tar.gz', release=release)}
        for name, src in assets.items():
            (d / name).write_bytes(src.read_bytes())
        sums(d, *assets)
        self.stub('curl', f'out=; url=; while [ $# -gt 0 ]; do case $1 in -o) out=$2; shift ;; -*) ;; *) url=$1 ;; esac; '
                          f'shift; done; echo "$url" >> "$STUBLOG/urls"; f=${{url##*/}}; r=${{url%/*}}; r=${{r##*/}}; '
                          f'case $url in */releases/latest) echo \'"tag_name": "v2026.10.10"\'; exit 0 ;; esac; '
                          f'src="{self.tmp}/server/$r/$f"; [ -f "$src" ] || exit 22; '
                          f'if [ -n "$out" ]; then cat "$src" > "$out"; else cat "$src"; fi')
        return d

    def serve_module(self, **kw):
        """the module's latest release at VPN_URL"""
        return self.serve('vpn-module', assets={
            'f50-linux-vpn.tar.gz': module_tarball(self.tmp / 'module.tar.gz', **kw),
            'f50-linux-vpn-mihomo.tar.gz': mihomo_tarball(self.tmp / 'mihomo.tar.gz')})

    def offline(self):
        self.stub('curl', 'for a; do last=$a; done; echo "$last" >> "$STUBLOG/urls"; exit 7')
        self.stub('wget', 'exit 4')

    def urls(self):
        f = self.tmp / 'urls'
        u = f.read_text() if f.exists() else ''
        f.unlink(missing_ok=True)
        return u


class UpdateWithTheVpn(ExtrasBase):
    """f50-update apply and the VPN module: a device that uses the VPN gets the module before anything changes, or
    the update stops; the new systems are linked to it before the reboot; an older vpn extra is replaced"""

    def setUp(self):
        super().setUp()
        self.versions('v2026.10.15')
        self.stage = self.disk / '.f50-update'
        img = self.tmp / 'img'
        for f, data in (('sbin/init', '#!/bin/sh\n'), ('etc/f50/image-version', 'v2026.10.16\n')):
            (img / f).parent.mkdir(parents=True, exist_ok=True)
            (img / f).write_text(data)
        (img / 'sbin/init').chmod(0o755)
        self.img = img
        self.stub('df', 'echo "Filesystem 1K-blocks Used Available"; echo "x 1 1 99999999"')

    def put_image(self):
        self.stage.mkdir(exist_ok=True)
        subprocess.run(['tar', '-czf', str(self.stage / 'f50-ubuntu-rootfs.tar.gz'), '-C', str(self.img), '.'],
                       check=True)

    def apply(self, shell, pre='', **env):
        """apply ubuntu to v2026.10.16, with the release's files already downloaded (the rootfs staged; no kernel
        bundle) and the boot image left alone"""
        self.put_image()
        code = (pre + 'latest_release() { echo v2026.10.16; }; '
                'fetch_verified() { [ -s "$STAGE/$2" ] && return 0; return 2; }; '
                'boot_update() { return 0; }; is_root() { false; }; '
                'apply ubuntu')
        e = dict(F50_VPN_URL=VPN_URL)
        e.update(env)
        return self.up(shell, code, **e)

    def reset(self, shell, enable=1, extra=None):
        for d in ('ubuntu', 'ubuntu.old', 'extra', '.f50-update'):
            shutil.rmtree(self.disk / d, ignore_errors=True)
        (self.disk / 'ubuntu/etc/f50').mkdir(parents=True)
        (self.disk / 'ubuntu/sbin').mkdir()
        (self.disk / 'ubuntu/etc/f50/image-version').write_text('v2026.10.15\n')
        (self.disk / 'ubuntu/etc/f50/vpn.conf').write_text(f'ENABLE={enable}\n')
        if extra == 'old':
            self.old_vpn_extra()
        elif extra == 'module':
            f = module_tarball(self.tmp / 'm.tar.gz', version='v0.9.0', requires='f50-linux v2026.10.01')
            r = self.up(shell, f'module_install vpn "{f}" "{"1" * 64}" v2026.10.15')
            self.assertEqual(r.returncode, 0, r.stderr)
        self.hooks()
        self.urls()

    def test_no_module_to_be_had_refuses_before_anything_changes(self):
        self.offline()
        for shell in self.each_shell():
            for enable, extra in ((1, None), (0, 'old'), (1, 'old')):
                self.reset(shell, enable, extra)
                before = sorted(p.name for p in (self.disk / 'extra').iterdir()) if extra else []
                r = self.apply(shell)
                self.assertNotEqual(r.returncode, 0, (shell, enable, extra, r.stdout))
                self.assertIn('nothing was changed', r.stderr)
                self.assertIn(f'{self.disk}/.f50-update/vpn', r.stderr)            # how to stage it
                self.assertEqual((self.disk / 'ubuntu/etc/f50/image-version').read_text(), 'v2026.10.15\n')
                self.assertFalse((self.disk / 'ubuntu.old').exists())
                if extra:
                    self.assertEqual(sorted(p.name for p in (self.disk / 'extra').iterdir()), before)
                    self.assertFalse((self.vpn() / 'bin/f50-vpn').exists())
                self.assertEqual(self.hooks(), [])

    def test_a_staged_module_is_installed_and_linked_into_the_new_system(self):
        self.offline()
        for shell in self.each_shell():
            for how in ('stage', 'env-file', 'env-dir'):
                self.reset(shell, 1, 'old')                     # the engines of an older release: replaced
                usb = self.tmp / 'usb'
                shutil.rmtree(usb, ignore_errors=True)
                usb.mkdir()
                env = {}
                if how == 'stage':
                    d = self.disk / '.f50-update/vpn'
                    d.mkdir(parents=True)
                    module_tarball(d / 'f50-linux-vpn.tar.gz')
                    sums(d, 'f50-linux-vpn.tar.gz')
                elif how == 'env-file':
                    env['F50_VPN_MODULE'] = module_tarball(usb / 'whatever.tar.gz')
                else:
                    module_tarball(usb / 'f50-linux-vpn.tar.gz')
                    sums(usb, 'f50-linux-vpn.tar.gz')
                    env['F50_VPN_MODULE'] = usb
                r = self.apply(shell, **env)
                self.assertEqual(r.returncode, 0, (shell, how, r.stdout, r.stderr))
                self.assertEqual((self.disk / 'ubuntu/etc/f50/image-version').read_text(), 'v2026.10.16\n')
                self.assertEqual((self.vpn() / 'VERSION').read_text().strip(), 'v1.0.0', how)
                self.assertTrue(os.access(self.vpn() / 'bin/f50-vpn', os.X_OK))
                self.assertIn('extra vpn: v1.0.0\n', r.stdout, how)         # named, not "extra 4" (_n reused)
                self.assertEqual(self.hooks(), [f'link [{self.disk}/ubuntu]'], how)
                self.assertTrue(os.path.islink(self.disk / 'ubuntu/opt/f50/bin/f50-vpn'))
                self.assertNotIn('WARNING', r.stderr)
                self.assertEqual(self.urls(), '', how)                  # offline all along
                self.assertFalse(self.stage.exists())                   # the staged copy is used up

    def test_a_module_that_cannot_be_put_in_place_refuses_before_anything_changes(self):
        # fails late in extra_unpack, after it counted the programs: the loop must still know it is the vpn module
        self.offline()
        for shell in self.each_shell():
            self.reset(shell, 1, 'old')
            d = self.disk / '.f50-update/vpn'
            d.mkdir(parents=True)
            module_tarball(d / 'f50-linux-vpn.tar.gz')
            sums(d, 'f50-linux-vpn.tar.gz')
            r = self.apply(shell, pre='extra_put() { return 1; }; ')
            self.assertNotEqual(r.returncode, 0, (shell, r.stdout))
            self.assertIn('the VPN module could not be installed', r.stderr)
            self.assertIn('nothing was changed', r.stderr)
            self.assertEqual((self.disk / 'ubuntu/etc/f50/image-version').read_text(), 'v2026.10.15\n')
            self.assertFalse((self.disk / 'ubuntu.old').exists())
            self.assertEqual(self.hooks(), [])

    def test_online_the_module_comes_from_its_repository(self):
        self.serve_module()
        for shell in self.each_shell():
            self.reset(shell, 1, None)
            r = self.apply(shell)
            self.assertEqual(r.returncode, 0, (r.stdout, r.stderr))
            self.assertIn(f'{VPN_URL}/f50-linux-vpn.tar.gz', self.urls())
            self.assertTrue(os.access(self.vpn() / 'bin/f50-vpn', os.X_OK))
            self.assertEqual(self.hooks(), [f'link [{self.disk}/ubuntu]'])

    def test_an_installed_module_follows_its_latest_release_or_stays(self):
        for shell in self.each_shell():
            # online: v0.9.0 -> v1.0.0
            self.serve_module()
            self.reset(shell, 1, 'module')
            r = self.apply(shell)
            self.assertEqual(r.returncode, 0, (r.stdout, r.stderr))
            self.assertEqual((self.vpn() / 'VERSION').read_text().strip(), 'v1.0.0')
            # offline: the installed one stays and the update goes on
            self.offline()
            self.reset(shell, 1, 'module')
            r = self.apply(shell)
            self.assertEqual(r.returncode, 0, (r.stdout, r.stderr))
            self.assertIn('the installed one (v0.9.0) stays', r.stderr)
            self.assertEqual((self.vpn() / 'VERSION').read_text().strip(), 'v0.9.0')
            self.assertEqual(self.hooks(), [f'link [{self.disk}/ubuntu]'])

    def test_a_module_newer_than_the_release_stops_a_device_without_one(self):
        self.serve_module(requires='f50-linux v2026.12.01')
        for shell in self.each_shell():
            self.reset(shell, 1, 'old')
            r = self.apply(shell)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn('needs f50-linux v2026.12.01', r.stderr)
            self.assertIn('nothing was changed', r.stderr)
            self.assertEqual((self.disk / 'ubuntu/etc/f50/image-version').read_text(), 'v2026.10.15\n')
            self.assertEqual((self.vpn() / 'release').read_text().strip(), 'v2026.10.15')

    def test_a_module_that_fails_its_checks_stops_a_kill_switch_device_before_any_switch(self):
        # the module can be had but is refused at install (not the module, or one that needs a newer system): with
        # the VPN on and its kill switch on, apply dies with nothing switched, the old extra and system in place
        self.offline()
        for shell in self.each_shell():
            for bad in ('notmodule', 'newer'):
                self.reset(shell, 1, 'old')
                (self.disk / 'ubuntu/etc/f50/vpn.conf').write_text('ENABLE=1\nKILL_SWITCH=1\n')
                f = (extra_tarball(self.tmp / 'bad.tar.gz') if bad == 'notmodule' else
                     module_tarball(self.tmp / 'bad.tar.gz', requires='f50-linux v2027.01.01'))
                r = self.apply(shell, F50_VPN_MODULE=f)
                self.assertNotEqual(r.returncode, 0, (bad, r.stdout))
                self.assertIn('nothing was changed', r.stderr, bad)
                self.assertEqual((self.disk / 'ubuntu/etc/f50/image-version').read_text(), 'v2026.10.15\n', bad)
                self.assertFalse((self.disk / 'ubuntu.old').exists(), bad)
                self.assertEqual((self.vpn() / 'release').read_text().strip(), 'v2026.10.15', bad)
                self.assertEqual(self.hooks(), [], bad)

    def test_a_kill_switch_system_left_without_f50_vpn_is_a_loud_warning(self):
        # a hook that links nothing (it failed): the systems are switched already, so the warning says the system
        # keeps its uplinks down (lib/vpn-orphan.sh) until the module is linked
        self.offline()
        for shell in self.each_shell():
            for ks, loud in (('1', True), ('0', False)):
                self.reset(shell, 1, None)
                (self.disk / 'ubuntu/etc/f50/vpn.conf').write_text(f'ENABLE=1\nKILL_SWITCH={ks}\n')
                f = module_tarball(self.tmp / 'm.tar.gz', hooks=False,
                                   extra=[('./hooks/link', 'exit 0\n', 0o755), ('./hooks/unlink', 'exit 0\n', 0o755)])
                r = self.apply(shell, F50_VPN_MODULE=f)
                self.assertEqual(r.returncode, 0, r.stderr)
                self.assertEqual('!!! WARNING: ubuntu has the VPN on with its kill switch' in r.stderr, loud, ks)
                self.assertIn('WARNING', r.stderr)

    def test_without_the_vpn_nothing_is_fetched(self):
        self.offline()
        for shell in self.each_shell():
            self.reset(shell, 0, None)
            r = self.apply(shell)
            self.assertEqual(r.returncode, 0, (r.stdout, r.stderr))
            self.assertFalse(self.vpn().exists())
            self.assertEqual(self.urls(), '')

    def test_vpn_mihomo_follows_and_is_never_fatal(self):
        for shell in self.each_shell():
            self.serve_module()
            self.reset(shell, 0, None)
            self.up(shell, f'module_install vpn-mihomo "{mihomo_tarball(self.tmp / "mh.tar.gz", version="v0.9.0")}" x v2026.10.16')
            r = self.apply(shell)
            self.assertEqual(r.returncode, 0, (r.stdout, r.stderr))
            self.assertEqual((self.disk / 'extra/vpn-mihomo/VERSION').read_text().strip(), 'v1.0.0')
            self.offline()
            self.reset(shell, 0, None)
            self.up(shell, f'module_install vpn-mihomo "{mihomo_tarball(self.tmp / "mh.tar.gz", version="v0.9.0")}" x v2026.10.16')
            r = self.apply(shell)
            self.assertEqual(r.returncode, 0, (r.stdout, r.stderr))
            self.assertEqual((self.disk / 'extra/vpn-mihomo/VERSION').read_text().strip(), 'v0.9.0')


LANGS = {'de': 'Deutsch (German)', 'pt_br': 'Português do Brasil (Brazilian Portuguese)', 'ja': '日本語 (Japanese)'}
LMOS = ('base.de.lmo', 'firewall.de.lmo', 'f50.de.lmo', 'base.pt-br.lmo', 'base.ja.lmo', 'f50.ja.lmo')


def lang_tarball(path, release='v2026.10.10', langs=None, lmos=LMOS, extra=(), lines=None, manifest=True,
                 tamper=None):
    """the lang extra: ./name ./release ./components ./languages ./manifest ./i18n/*.lmo; EXTRA: (name, data, type)
    more; TAMPER: a file whose bytes differ from what the manifest says"""
    langs = LANGS if langs is None else langs
    text = lines if lines is not None else ''.join(f'{k}\t{v}\n' for k, v in langs.items())
    files = {'./name': b'lang\n', './release': (release + '\n').encode(), './components': b'luci-i18n-base-de 26.275\n',
             './languages': text.encode()}
    files.update({f'./i18n/{n}': f'lmo {n} {release}'.encode() for n in lmos})
    man = ''.join(f'{hashlib.sha256(d).hexdigest()}  {n}\n' for n, d in sorted(files.items()))
    if tamper:
        files[tamper] = b'other bytes'
    with tarfile.open(path, 'w:gz') as t:
        def add(fn, data=b'', typ=tarfile.REGTYPE, link=''):
            ti = tarfile.TarInfo(fn)
            ti.type, ti.linkname = typ, link
            ti.size = len(data) if typ == tarfile.REGTYPE else 0
            ti.mode = 0o755 if typ == tarfile.DIRTYPE else 0o644
            t.addfile(ti, io.BytesIO(data) if typ == tarfile.REGTYPE else None)
        for n in ('./name', './release', './components', './languages'):
            add(n, files[n])
        if manifest:
            add('./manifest', man.encode())
        add('./i18n', typ=tarfile.DIRTYPE)
        for n in lmos:
            add(f'./i18n/{n}', files[f'./i18n/{n}'])
        for e in extra:
            add(*e)
    return path



if __name__ == '__main__':
    unittest.main()
