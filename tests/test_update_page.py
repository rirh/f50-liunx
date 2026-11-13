"""f50-update's additions for an update page (docs/research/FINDINGS.md 86), sourced for their functions as test_update
does: `status` and `releases` as JSON, an update from a file (a Magisk zip of a release, or a directory of release
files) with what it refuses, and the signature of SHA256SUMS, made here with a throwaway Ed25519 key in usign's
format. (Key and OPENSSL are shared with test_diff.)"""
import base64
import hashlib
import io
import json
import os
import shutil
import subprocess
import tarfile
import unittest
import zipfile
from pathlib import Path

from helpers import BIN, TOP
import test_update
from test_update import UpdateBase

UPDATE = BIN / 'f50-update'


def openssl():
    """An openssl that makes and checks Ed25519 signatures of a file (OpenSSL 3; macOS's LibreSSL does not)."""
    for c in (shutil.which('openssl'), '/opt/homebrew/bin/openssl', '/usr/local/opt/openssl@3/bin/openssl',
              '/opt/homebrew/opt/openssl@3/bin/openssl'):
        if not c or not os.path.exists(c):
            continue
        r = subprocess.run([c, 'pkeyutl', '-help'], capture_output=True, text=True)
        if '-rawin' in r.stdout + r.stderr:
            return c
    return None


OPENSSL = openssl()


class Key:
    """A throwaway release key: the public key and signatures as usign and signify write them (a comment line, then
    base64 of "Ed", an 8-byte key number and the 32-byte key or the 64-byte signature)."""

    def __init__(self, d, number=b'\x01\x02\x03\x04\x05\x06\x07\x08'):
        self.d, self.number = Path(d), number
        self.d.mkdir(parents=True, exist_ok=True)
        self.pem = self.d / 'key.pem'
        subprocess.run([OPENSSL, 'genpkey', '-algorithm', 'ed25519', '-out', str(self.pem)], check=True, capture_output=True)
        der = subprocess.run([OPENSSL, 'pkey', '-in', str(self.pem), '-pubout', '-outform', 'DER'], check=True,
                             capture_output=True).stdout
        self.pub = self.d / 'release.pub'
        self.pub.write_text('untrusted comment: a test key\n' + base64.b64encode(b'Ed' + number + der[-32:]).decode() + '\n')

    def sign(self, path, out=None):
        raw = self.d / 'sig.raw'
        subprocess.run([OPENSSL, 'pkeyutl', '-sign', '-inkey', str(self.pem), '-rawin', '-in', str(path), '-out', str(raw)],
                       check=True, capture_output=True)
        out = Path(out) if out else Path(str(path) + '.sig')
        out.write_text('untrusted comment: signed by a test key\n' +
                       base64.b64encode(b'Ed' + self.number + raw.read_bytes()).decode() + '\n')
        return out


def tgz(path, files):
    """a tar.gz with ./NAME members, as the release's files are"""
    with tarfile.open(path, 'w:gz') as t:
        for name, data, mode in files:
            data = data.encode() if isinstance(data, str) else data
            ti = tarfile.TarInfo('./' + name)
            ti.size, ti.mode = len(data), mode
            t.addfile(ti, io.BytesIO(data))
    return path


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


ROOTFS = {('openwrt', ''): 'f50-openwrt-rootfs.tar.gz', ('openwrt-luci', ''): 'f50-openwrt-luci-rootfs.tar.gz',
          ('ubuntu', '24.04'): 'f50-ubuntu-rootfs.tar.gz', ('ubuntu', '26.04'): 'f50-ubuntu-26.04-rootfs.tar.gz'}
KERNEL = {'5.4': 'f50-kernel.tar.gz', '6.18': 'f50-kernel-6.18.tar.gz', '7.2': 'f50-kernel-7.2.tar.gz', '7.3': 'f50-kernel-7.3.tar.gz'}


def release_dir(d, tag='v2026.11.02', os_='openwrt-luci', ubuntu='', kernel='5.4', devices='f50 u30air\n'):
    """a release's files for one system and one kernel, with their SHA256SUMS: the directory apply --from takes"""
    d = Path(d)
    d.mkdir(parents=True, exist_ok=True)
    rootfs, kasset = ROOTFS[(os_, ubuntu)], KERNEL[kernel]
    tgz(d / rootfs, [('sbin/init', '#!/bin/sh\n', 0o755), ('etc/f50/image-version', tag + '\n', 0o644),
                     ('etc/config/system', 'new\n', 0o644), ('opt/f50/bin/marker', tag + '\n', 0o644)])
    files = [('features', 'sdcard\nlinux-slot\n', 0o644), ('Image', 'kernel', 0o644)]
    if devices is not None:
        files.append(('devices', devices, 0o644))
    tgz(d / kasset, files)
    # f50-release: the tag as a file, listed like the others - what binds the checksums to their release
    (d / 'f50-release').write_text(tag + '\n')
    (d / 'SHA256SUMS').write_text(''.join(f'{sha(d / n)}  {n}\n' for n in (kasset, rootfs, 'f50-release')))
    return d


def magisk_zip(path, rel, tag='v2026.11.02', os_='openwrt-luci', ubuntu='', kernel='5.4', sums=False, sig=None,
               manifest=None, extra=()):
    """the Magisk zip of that release: f50/manifest and the stored payload;
    SUMS adds the release's SHA256SUMS (what a signature covers), SIG its signature"""
    rel = Path(rel)
    rootfs, kasset = ROOTFS[(os_, ubuntu)], KERNEL[kernel]
    system = os_ + ('-' + ubuntu if ubuntu else '')
    m = (f'TAG={tag}\nSYSTEM={system}\nOS={os_}\nUBUNTU={ubuntu}\nKERNEL={kernel}\nKERNEL_ASSET={kasset}\n'
         f'ROOTFS_ASSET={rootfs}\nSHA256_KERNEL={sha(rel / kasset)}\nSHA256_ROOTFS={sha(rel / rootfs)}\n')
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('f50/manifest', m if manifest is None else manifest)
        z.writestr('module.prop', 'id=f50_linux_switch\n')
        if sums:
            z.writestr('f50/SHA256SUMS', (rel / 'SHA256SUMS').read_text())
        if sig:
            z.writestr('f50/SHA256SUMS.sig', Path(sig).read_text())
        for n in (kasset, rootfs):
            z.write(rel / n, 'payload/' + n, zipfile.ZIP_STORED)
        for name, data in extra:
            z.writestr(name, data)
    return Path(path)


@unittest.skipUnless(shutil.which('unzip') or shutil.which('busybox'), 'unzip is needed to read a Magisk zip')
class Offline(UpdateBase):
    """apply --from: the release from a Magisk zip or a directory, and what is refused before anything changes."""

    def setUp(self):
        super().setUp()
        shutil.rmtree(self.disk / 'ubuntu')
        old = self.disk / 'openwrt-luci'
        for f, data in [('sbin/init', '#!/bin/sh\n'), ('etc/f50/image-version', 'v2026.10.25\n'),
                        ('etc/config/system', 'mine\n'), ('etc/f50/hotspot.conf', 'mine\n')]:
            (old / f).parent.mkdir(parents=True, exist_ok=True)
            (old / f).write_text(data)
        self.rel = release_dir(self.tmp / 'rel')
        self.zip = magisk_zip(self.tmp / 'f50-magisk-v2026.11.02-openwrt-luci-k5.4.zip', self.rel)
        self.nokey = self.tmp / 'no-release.pub'

    def frm(self, shell, src, code, **env):
        env.setdefault('F50_RELEASE_PUB', self.nokey)
        return self.up(shell, f'F50_FROM="{src}"; {code}', TMPDIR=self.tmp, **env)

    def inspect(self, shell, src, **env):
        env.setdefault('F50_RELEASE_PUB', self.nokey)
        r = self.up(shell, f'inspect_json "{src}"', TMPDIR=self.tmp, **env)
        return json.loads(r.stdout), r

    def test_a_zip_says_what_it_is(self):
        for shell in self.each_shell():
            o, r = self.inspect(shell, self.zip)
            self.assertEqual(o, {'ok': 1, 'kind': 'zip', 'sha256': sha(self.zip), 'size': self.zip.stat().st_size,
                                 'tag': 'v2026.11.02', 'system': 'openwrt-luci', 'ubuntu': None, 'kernel': '5.4',
                                 'signature': 'nokey', 'installed': 'v2026.10.25', 'downgrade': 0, 'same': 0, 'fits': 1,
                                 'why': None}, r.stderr)
            # nothing is left behind, and nothing was unpacked onto the partition
            self.assertFalse((self.disk / '.f50-update').exists())
            self.assertEqual([p.name for p in self.tmp.glob('tmp.*')], [])

    def test_what_is_not_for_this_device_says_why(self):
        other = release_dir(self.tmp / 'rel-ubuntu', os_='ubuntu', ubuntu='24.04')
        k72 = release_dir(self.tmp / 'rel-72', kernel='7.2')
        f50 = release_dir(self.tmp / 'rel-f50', devices='f50\n')
        old = release_dir(self.tmp / 'rel-olddev', devices=None)
        cases = [
            (magisk_zip(self.tmp / 'u.zip', other, os_='ubuntu', ubuntu='24.04'), 'f50', 'system-missing'),
            (magisk_zip(self.tmp / 'k.zip', k72, kernel='7.2'), 'f50', 'kernel'),
            (magisk_zip(self.tmp / 'd.zip', f50), 'u30air', 'device'),
            (magisk_zip(self.tmp / 'o.zip', old), 'u30air', 'device'),    # a bundle from before ./devices: the F50's
            (magisk_zip(self.tmp / 'o2.zip', old), 'f50', None),
            (magisk_zip(self.tmp / 'd2.zip', f50), 'f50', None),
        ]
        for shell in self.each_shell():
            for z, device, why in cases:
                self.device(device)
                o, r = self.inspect(shell, z)
                self.assertEqual((o['ok'], o['fits'], o['why']), (1, 0 if why else 1, why), (z.name, device, r.stderr))
                r = self.frm(shell, z, 'F50_ALLOW_UNSIGNED=1; from_begin; echo began')
                self.assertEqual('began' in r.stdout, why is None, (z.name, r.stderr))
                if why:
                    self.assertIn('is not for this device', r.stderr)
            self.device('f50')
        # the other Ubuntu release than the one installed
        (self.disk / 'ubuntu/etc').mkdir(parents=True)
        (self.disk / 'ubuntu/etc/os-release').write_text('ID=ubuntu\nVERSION_ID="26.04"\n')
        for shell in self.each_shell():
            o, _ = self.inspect(shell, self.tmp / 'u.zip')
            self.assertEqual((o['fits'], o['why']), (0, 'ubuntu-release'))

    def test_what_is_not_a_release_is_refused(self):
        m = 'TAG={tag}\nSYSTEM=x\nOS={os}\nUBUNTU=\nKERNEL={k}\nKERNEL_ASSET={ka}\nROOTFS_ASSET={ra}\nSHA256_KERNEL={h}\nSHA256_ROOTFS={h}\n'
        good = dict(tag='v2026.11.02', os='openwrt-luci', k='5.4', ka='f50-kernel.tar.gz',
                    ra='f50-openwrt-luci-rootfs.tar.gz', h='a' * 64)
        bad = {
            'tag of a test build': dict(good, tag='test-issue-102'),
            'tag with a command': dict(good, tag='v2026.11.02;reboot'),
            'tag with a path': dict(good, tag='../../etc'),
            'unknown system': dict(good, os='android'),
            'unknown kernel': dict(good, k='6.6'),
            'a rootfs name of its own': dict(good, ra='../../etc/shadow'),
            'a rootfs of another system': dict(good, ra='f50-ubuntu-rootfs.tar.gz'),
            'a kernel name of its own': dict(good, ka='payload/../x'),
            'a hash that is none': dict(good, h='$(reboot)'),
        }
        files = {n: magisk_zip(self.tmp / f'bad{i}.zip', self.rel, manifest=m.format(**v)) for i, (n, v) in enumerate(bad.items())}
        with zipfile.ZipFile(self.tmp / 'nomanifest.zip', 'w') as z:
            z.writestr('module.prop', 'id=x\n')
        files['a zip without a manifest'] = self.tmp / 'nomanifest.zip'
        (self.tmp / 'text.zip').write_text('PK but not a zip\n')
        files['a file that only starts like a zip'] = self.tmp / 'text.zip'
        (self.tmp / 'rootfs.tar.gz').write_bytes((self.rel / 'f50-openwrt-luci-rootfs.tar.gz').read_bytes())
        files['a root filesystem alone'] = self.tmp / 'rootfs.tar.gz'
        (self.tmp / 'link.zip').symlink_to(self.zip)
        files['a link to a release'] = self.tmp / 'link.zip'
        files['nothing'] = self.tmp / 'none.zip'
        (self.tmp / 'emptydir').mkdir()
        files['a directory without SHA256SUMS'] = self.tmp / 'emptydir'
        for shell in self.each_shell():
            for name, f in files.items():
                with self.subTest(case=name):
                    o, r = self.inspect(shell, f)
                    self.assertEqual(o['ok'], 0, r.stdout)
                    self.assertIn(o['why'], ('not-release', 'not-found', 'no-tag'))
                    r = self.frm(shell, f, 'F50_ALLOW_UNSIGNED=1; from_begin; echo began')
                    self.assertNotIn('began', r.stdout)
                    self.assertIn('is not a release of this project', r.stderr)
                    self.assertFalse((self.tmp / 'reboot').exists())
                    self.assertEqual([p.name for p in self.tmp.glob('tmp.*')], [])

    def test_a_damaged_file_is_seen_before_anything_is_applied(self):
        # the payload does not match the hashes the zip carries: the page says so, and apply stops at the file
        m = (f'TAG=v2026.11.02\nSYSTEM=openwrt-luci\nOS=openwrt-luci\nUBUNTU=\nKERNEL=5.4\nKERNEL_ASSET=f50-kernel.tar.gz\n'
             f'ROOTFS_ASSET=f50-openwrt-luci-rootfs.tar.gz\nSHA256_KERNEL={sha(self.rel / "f50-kernel.tar.gz")}\n'
             f'SHA256_ROOTFS={"0" * 64}\n')
        z = magisk_zip(self.tmp / 'damaged.zip', self.rel, manifest=m)
        for shell in self.each_shell():
            o, _ = self.inspect(shell, z)
            self.assertEqual((o['ok'], o['fits'], o['why']), (1, 0, 'checksum'))
            r = self.frm(shell, z, 'F50_ALLOW_UNSIGNED=1; from_begin; from_verified v2026.11.02 f50-openwrt-luci-rootfs.tar.gz; echo "rc=$?"')
            self.assertIn('rc=1', r.stdout)
            self.assertIn('does not match its checksum', r.stderr)
            self.assertFalse((self.disk / '.f50-update/f50-openwrt-luci-rootfs.tar.gz').exists())

    def test_files_come_out_of_the_source_checked(self):
        for shell in self.each_shell():
            for src in (self.zip, self.rel):
                shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
                r = self.frm(shell, src, 'F50_ALLOW_UNSIGNED=1; from_begin; for f in f50-openwrt-luci-rootfs.tar.gz f50-kernel.tar.gz '
                                         'f50-extra-lang.tar.gz f50-update; do fetch_verified v2026.11.02 $f; echo "$f=$?"; done')
                stage = self.disk / '.f50-update'
                self.assertIn('f50-openwrt-luci-rootfs.tar.gz=0', r.stdout, r.stderr)
                self.assertIn('f50-kernel.tar.gz=0', r.stdout)
                # what the source does not have is "not part of this release" (2), as on GitHub: the installed
                # language pack stays, and the run goes on with this updater
                self.assertIn('f50-extra-lang.tar.gz=2', r.stdout)
                self.assertIn('f50-update=2', r.stdout)
                self.assertEqual(sha(stage / 'f50-kernel.tar.gz'), sha(self.rel / 'f50-kernel.tar.gz'))
                self.assertEqual((stage / 'SHA256SUMS.tag').read_text(), 'v2026.11.02\n')
                # the staging directory is closed to everyone else before anything is checked in it
                self.assertEqual(stage.stat().st_mode & 0o777, 0o700)
            # a source that was not judged first gives nothing
            shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
            r = self.frm(shell, self.zip, 'fetch_verified v2026.11.02 f50-kernel.tar.gz; echo "rc=$?"')
            self.assertIn('rc=1', r.stdout)
            self.assertIn('was not checked', r.stderr)

    def test_checksums_swapped_after_the_check_stop_the_run(self):
        # time of check, time of use: the checksums are judged once and pinned by their hash; other bytes at their
        # place afterwards (and a payload that matches those) are not taken
        evil = release_dir(self.tmp / 'evil')
        tgz(evil / 'f50-kernel.tar.gz', [('Image', 'another kernel', 0o644)])
        swapped = f'{sha(evil / "f50-kernel.tar.gz")}  f50-kernel.tar.gz\n'
        for shell in self.each_shell():
            shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
            r = self.frm(shell, self.zip, 'F50_ALLOW_UNSIGNED=1; from_begin; fetch_verified v2026.11.02 f50-openwrt-luci-rootfs.tar.gz; '
                                          f'echo "first=$?"; printf "%s" "{swapped}" > "$FROM_TMP/SHA256SUMS"; '
                                          f'cp "{evil}/f50-kernel.tar.gz" "$STAGE/f50-kernel.tar.gz"; '
                                          'fetch_verified v2026.11.02 f50-kernel.tar.gz; echo "second=$?"')
            self.assertIn('first=0', r.stdout, r.stderr)
            self.assertIn('second=1', r.stdout)
            self.assertIn('changed after they were checked', r.stderr)

    def test_what_is_staged_is_roots_alone(self):
        # the staging directory 700 and every file this puts there 600, whatever the umask of the run - which stays
        # what it was for the rest of the update (the new system's directories are made under it)
        for shell in self.each_shell():
            shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
            r = self.frm(shell, self.zip, 'umask 022; F50_ALLOW_UNSIGNED=1; from_begin; fetch_verified v2026.11.02 f50-kernel.tar.gz; '
                                          'echo "umask=$(umask)"')
            self.assertIn('umask=0022', r.stdout.replace('umask=022', 'umask=0022'), r.stderr)
            stage = self.disk / '.f50-update'
            self.assertEqual(stage.stat().st_mode & 0o777, 0o700)
            open_ = {str(p.relative_to(stage)): oct(p.stat().st_mode & 0o777) for p in stage.rglob('*') if p.stat().st_mode & 0o077}
            self.assertEqual(open_, {})
            self.assertTrue((stage / 'f50-kernel.tar.gz').exists() and (stage / '.from/SHA256SUMS').exists())
            # one that others could write into is emptied before it is used: what they left there is not followed
            shutil.rmtree(stage)
            stage.mkdir()
            stage.chmod(0o777)
            victim = self.tmp / 'victim'
            victim.write_text('untouched')
            (stage / 'SHA256SUMS').symlink_to(victim)
            (stage / 'f50-kernel.tar.gz').symlink_to(victim)
            r = self.frm(shell, self.zip, 'F50_ALLOW_UNSIGNED=1; from_begin; fetch_verified v2026.11.02 f50-kernel.tar.gz; echo "rc=$?"')
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertEqual(victim.read_text(), 'untouched')
            self.assertEqual(stage.stat().st_mode & 0o777, 0o700)
            self.assertFalse((stage / 'f50-kernel.tar.gz').is_symlink())

    def test_the_file_that_was_looked_at_is_the_file_that_is_used(self):
        # the page shows a file's SHA-256 and the owner says yes to that file: the updater takes it by that hash
        other = magisk_zip(self.tmp / 'other.zip', release_dir(self.tmp / 'rel2', tag='v2026.11.03'), tag='v2026.11.03')
        for shell in self.each_shell():
            r = self.frm(shell, self.zip, f'apply_options --from "{self.zip}" --unsigned --sha256={sha(self.zip)}; from_begin; echo "began $F50_RELEASE"')
            self.assertIn('began v2026.11.02', r.stdout, r.stderr)
            # another file at that path since
            r = self.frm(shell, other, f'apply_options --from "{other}" --unsigned --sha256={sha(self.zip)}; from_begin; echo began')
            self.assertNotIn('began', r.stdout)
            self.assertIn('is not the file that was looked at', r.stderr)
            for bad in ('--sha256=xyz', '--sha256=', '--sha256=' + 'A' * 64):
                r = self.up(shell, f'apply_options --from /x.zip {bad}; echo parsed')
                self.assertNotIn('parsed', r.stdout, bad)
            # the hash is this run's own word, not the environment's
            r = self.up(shell, 'apply_options --from /x.zip; echo "[$F50_FROM_SHA]"', F50_FROM_SHA='ab' * 32)
            self.assertEqual(r.stdout.strip(), '[]')

    def test_a_system_under_another_releases_name_is_not_installed(self):
        # what is judged (the signature, the downgrade rule, what the page showed) is a name; what is installed is
        # the files. An unsigned zip that calls itself v2026.12.24 and carries the system of v2026.09.01 passes the
        # downgrade rule by its name - and stops at the root filesystem's own word, before anything is changed.
        old = release_dir(self.tmp / 'old', tag='v2026.09.01')
        liar = magisk_zip(self.tmp / 'liar.zip', old, tag='v2026.12.24')
        nameless = release_dir(self.tmp / 'nameless')
        tgz(nameless / 'f50-openwrt-luci-rootfs.tar.gz', [('sbin/init', '#!/bin/sh\n', 0o755)])
        (nameless / 'SHA256SUMS').write_text(''.join(f'{sha(nameless / n)}  {n}\n' for n in ('f50-kernel.tar.gz', 'f50-openwrt-luci-rootfs.tar.gz')))
        for shell in self.each_shell():
            for src, said, env in ((liar, 'says it is v2026.09.01, not v2026.12.24', {}),
                                   (nameless, 'says it is no release at all, not v2026.11.02', {'F50_RELEASE': 'v2026.11.02'})):
                shutil.rmtree(self.disk / 'openwrt-luci.old', ignore_errors=True)
                shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
                r = self.frm(shell, src, self.APPLY + 'F50_ALLOW_UNSIGNED=1; apply', **env, **self.busybox_applets(shell))
                self.assertNotEqual(r.returncode, 0, src.name)
                self.assertIn(said, r.stderr)
                self.assertEqual((self.disk / 'openwrt-luci/etc/f50/image-version').read_text(), 'v2026.10.25\n')
                self.assertFalse((self.disk / 'openwrt-luci.old').exists())
                self.assertNotIn('BOOT', r.stdout)
            # the same for a download: a mirror that serves the old system as the new release (no key on the system)
            shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
            serve = (f'fetch_stdout() {{ cat "{old}/${{1##*/}}" 2>/dev/null; }}; fetch_file() {{ cp "{old}/${{1##*/}}" "$2" 2>/dev/null; }}; sleep() {{ :; }}; ')
            (old / 'SHA256SUMS').write_text(''.join(l + '\n' for l in (old / 'SHA256SUMS').read_text().split('\n') if l and 'f50-release' not in l))
            r = self.up(shell, self.APPLY + serve + 'apply', F50_RELEASE='v2026.12.24', F50_RELEASE_PUB=self.nokey, **self.busybox_applets(shell))
            self.assertNotEqual(r.returncode, 0)
            self.assertIn('says it is v2026.09.01, not v2026.12.24', r.stderr)
            self.assertEqual((self.disk / 'openwrt-luci/etc/f50/image-version').read_text(), 'v2026.10.25\n')

    def test_the_boot_image_is_not_taken_back_by_a_system_update(self):
        # an update writes the release's kernel too: a system that was rolled back is older than the boot image, and
        # a release between the two would take the boot image back without a word
        (self.disk / 'boot').mkdir()
        (self.disk / 'boot/installed.tag').write_text('v2026.12.01\n')
        for shell in self.each_shell():
            r = self.up(shell, 'downgrade_ok v2026.11.02 openwrt-luci; echo "rc=$?"')
            self.assertIn('rc=1', r.stdout)
            self.assertIn('the boot image is v2026.12.01 and v2026.11.02 is older', r.stderr)
            self.assertIn('rc=0', self.up(shell, 'downgrade_ok v2026.12.01 openwrt-luci; echo "rc=$?"').stdout)
            self.assertIn('rc=0', self.up(shell, 'downgrade_ok v2026.11.02 openwrt-luci; echo "rc=$?"', F50_ALLOW_DOWNGRADE=1).stdout)

    def test_a_staging_directory_that_is_not_ours_is_not_used(self):
        elsewhere = self.tmp / 'elsewhere'
        elsewhere.mkdir()
        (self.disk / '.f50-update').symlink_to(elsewhere)
        for shell in self.each_shell():
            r = self.frm(shell, self.zip, 'F50_ALLOW_UNSIGNED=1; from_begin; echo began')
            self.assertNotIn('began', r.stdout)
            r = self.up(shell, 'fetch_stdout() { echo x; }; fetch_verified v2026.11.02 f50-update; echo "rc=$?"')
            self.assertIn('rc=1', r.stdout)
            self.assertIn('symbolic link', r.stderr)
            self.assertEqual(os.listdir(elsewhere), [])

    def test_an_older_release_is_never_taken_by_itself(self):
        # whatever the source - GitHub, a mirror (F50_RELEASE, F50_RELEASE_URL), a file - and before the
        # release's own updater is given the run: an old release is a way back to its faults
        (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.12.01\n')
        asked = 'fetch_stdout() { echo "FETCHED $1" >&2; return 1; }; fetch_file() { echo "FETCHED $1" >&2; return 1; }; '
        for shell in self.each_shell():
            for env in ({'F50_RELEASE': 'v2026.11.02'}, {'F50_RELEASE': 'v2026.11.02', 'F50_RELEASE_URL': 'http://mirror.invalid/x'}):
                r = self.up(shell, self.APPLY + asked + 'apply', **env)
                self.assertNotEqual(r.returncode, 0)
                self.assertIn('openwrt-luci is v2026.12.01 and v2026.11.02 is older', r.stderr)
                self.assertNotIn('FETCHED', r.stderr)             # not even the release's updater was asked for
                self.assertNotIn('BOOT', r.stdout)
                # said in words, it goes on (and here stops at the download)
                r = self.up(shell, self.APPLY + asked + 'apply_options --downgrade; apply $APPLY_ARGS', **env)
                self.assertIn('FETCHED', r.stderr)
            # the newest release as "latest" says it, older than a test release that is installed
            r = self.up(shell, self.APPLY + asked + 'latest_release() { echo v2026.11.02; }; apply')
            self.assertIn('is older', r.stderr)
            # the same release, or a newer one, needs no word
            r = self.up(shell, self.APPLY + asked + 'apply', F50_RELEASE='v2026.12.01')
            self.assertNotIn('is older', r.stderr)
            # a kernel or a boot image taken alone
            for code in ('boot_downgrade_ok v2026.11.02; echo "rc=$?"',):
                r = self.up(shell, 'installed_version() { echo v2026.12.01; }; ' + code)
                self.assertIn('rc=1', r.stdout)
                r = self.up(shell, 'installed_version() { echo v2026.12.01; }; ' + code, F50_ALLOW_DOWNGRADE=1)
                self.assertIn('rc=0', r.stdout)
            # the new names (FINDINGS 125): every vYYYY.MM.DD.N is newer than every vYYYY.MM.DD, whatever its date
            (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.10.27\n')
            r = self.up(shell, self.APPLY + asked + 'apply', F50_RELEASE='v2026.10.16.1')
            self.assertNotIn('is older', r.stderr)
            (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.10.16.10\n')
            for older in ('v2026.10.27', 'v2026.10.16.9'):
                r = self.up(shell, self.APPLY + asked + 'apply', F50_RELEASE=older)
                self.assertIn(f'openwrt-luci is v2026.10.16.10 and {older} is older', r.stderr)
            (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.12.01\n')
            src = UPDATE.read_text()
            self.assertLess(src.index('downgrade_ok "$tag" $(apply_targets "$@")'), src.index('exec sh "$STAGE/f50-update" apply'))
            self.assertIn('boot_downgrade_ok "$t" || exit 1', src)
            self.assertIn('boot_downgrade_ok "$t" || { echo "$old" > "$KERNEL_CHOICE"', src)

    def test_a_directory_names_its_release_by_its_root_filesystem(self):
        for shell in self.each_shell():
            o, r = self.inspect(shell, self.rel)
            self.assertEqual((o['ok'], o['kind'], o['tag'], o['fits'], o['sha256']), (1, 'dir', 'v2026.11.02', 1, None), r.stderr)
            o, _ = self.inspect(shell, self.rel, F50_RELEASE='v2026.11.09')
            self.assertEqual(o['tag'], 'v2026.11.09')

    def test_an_older_release_needs_the_word(self):
        (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.12.01\n')
        for shell in self.each_shell():
            o, _ = self.inspect(shell, self.zip)
            self.assertEqual((o['downgrade'], o['installed'], o['fits']), (1, 'v2026.12.01', 1))
            r = self.frm(shell, self.zip, 'F50_ALLOW_UNSIGNED=1; from_begin; echo began')
            self.assertNotIn('began', r.stdout)
            self.assertIn('older than what is installed (v2026.12.01)', r.stderr)
            r = self.frm(shell, self.zip, 'F50_ALLOW_UNSIGNED=1; F50_ALLOW_DOWNGRADE=1; from_begin; echo "began $F50_RELEASE"')
            self.assertIn('began v2026.11.02', r.stdout, r.stderr)

    def test_a_file_nothing_vouches_for_needs_the_word(self):
        # no key on this system, or no signature in the file: an upload has no origin, so it is never taken silently
        for shell in self.each_shell():
            r = self.frm(shell, self.zip, 'from_begin; echo began')
            self.assertNotIn('began', r.stdout)
            self.assertIn('is not signed with the project\'s release key', r.stderr)
            self.assertIn('nothing was changed', r.stderr)
            r = self.frm(shell, self.zip, 'F50_ALLOW_UNSIGNED=1; from_begin; echo "began $FROM_ONLY"')
            self.assertIn('began openwrt-luci', r.stdout, r.stderr)
            self.assertIn('used because you said so', r.stderr)

    def test_apply_options(self):
        for shell in self.each_shell():
            r = self.up(shell, 'apply_options --no-reinstall --from /x/y.zip openwrt --unsigned --downgrade; '
                               'echo "[$F50_FROM][$F50_ALLOW_UNSIGNED][$F50_ALLOW_DOWNGRADE][$APPLY_ARGS]"')
            self.assertEqual(r.stdout.strip(), '[/x/y.zip][1][1][ --no-reinstall openwrt]', r.stderr)
            # the owner's word reaches the release's own updater, which apply hands over to with the options taken
            # out (on the U30 Air "apply --unsigned" was refused by the next release's updater)
            r = self.up(shell, 'apply_options --unsigned --downgrade; '
                               'sh -c \'echo "[$F50_ALLOW_UNSIGNED][$F50_ALLOW_DOWNGRADE]"\'')
            self.assertEqual(r.stdout.strip(), '[1][1]', r.stderr)
            r = self.up(shell, 'apply_options ubuntu; echo "[${F50_FROM:-}][${F50_ALLOW_UNSIGNED:-}][$APPLY_ARGS]"')
            self.assertEqual(r.stdout.strip(), '[][][ ubuntu]')
            for bad in ('--from', 'openwrt "*"', '"a b"', '--from x "$(echo y)z;"'):
                r = self.up(shell, f'apply_options {bad}; echo parsed')
                self.assertNotIn('parsed', r.stdout, bad)
                self.assertNotEqual(r.returncode, 0)

    APPLY = ('boot_update() { echo "BOOT $1 $(kernel_asset)"; [ -s "$STAGE/$(kernel_asset)" ] || echo "NO BUNDLE"; }; '
             'pause_data() { :; }; resume_data() { :; }; modules_fetch() { :; }; modules_install() { :; }; '
             'modules_link() { :; }; extras_install() { :; }; is_root() { false; }; mounted_disk() { :; }; '
             'df() { printf "Filesystem 1K-blocks Used Available Use%% Mounted\\n/dev/x 9 9 9999999 1%% /\\n"; }; '
             'latest_release() { echo "ASKED GITHUB" >&2; }; ')

    def test_the_whole_update_from_a_zip(self):
        self.stub('curl', 'echo "network used" >> "$STUBLOG/net"; exit 7')
        self.stub('wget', 'echo "network used" >> "$STUBLOG/net"; exit 7')
        for shell in self.each_shell():
            for d in ('openwrt-luci.old', '.f50-update'):
                shutil.rmtree(self.disk / d, ignore_errors=True)
            (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.10.25\n')
            (self.disk / 'openwrt-luci/etc/config/system').write_text('mine\n')
            r = self.frm(shell, self.zip, self.APPLY + 'F50_ALLOW_UNSIGNED=1; apply_options --no-reinstall; apply $APPLY_ARGS',
                         F50_DEFAULT_BOOT=self.tmp / 'default-boot', **self.busybox_applets(shell))
            self.assertEqual(r.returncode, 0, r.stderr)
            new = self.disk / 'openwrt-luci'
            self.assertEqual((new / 'etc/f50/image-version').read_text(), 'v2026.11.02\n', r.stdout + r.stderr)
            self.assertEqual((new / 'opt/f50/bin/marker').read_text(), 'v2026.11.02\n')
            self.assertEqual((new / 'etc/config/system').read_text(), 'mine\n')
            self.assertEqual((new / 'etc/f50/hotspot.conf').read_text(), 'mine\n')
            self.assertEqual((self.disk / 'openwrt-luci.old/etc/f50/image-version').read_text(), 'v2026.10.25\n')
            self.assertIn('BOOT v2026.11.02 f50-kernel.tar.gz', r.stdout)
            self.assertNotIn('NO BUNDLE', r.stdout)
            # the release was the file's, and the network was never asked for it
            self.assertNotIn('ASKED GITHUB', r.stderr)
            self.assertFalse((self.tmp / 'net').exists())
            self.assertFalse((self.disk / '.f50-update').exists())
            self.assertTrue(self.zip.exists())

    def test_a_refused_file_changes_nothing(self):
        for shell in self.each_shell():
            shutil.rmtree(self.disk / 'openwrt-luci.old', ignore_errors=True)
            (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.10.25\n')
            r = self.frm(shell, self.zip, self.APPLY + 'apply', **self.busybox_applets(shell))
            self.assertNotEqual(r.returncode, 0)
            self.assertEqual((self.disk / 'openwrt-luci/etc/f50/image-version').read_text(), 'v2026.10.25\n')
            self.assertFalse((self.disk / 'openwrt-luci.old').exists())
            self.assertNotIn('BOOT', r.stdout)

    def test_the_run_goes_on_with_an_updater_that_knows_a_file_only(self):
        # apply execs the release's own f50-update when it differs: one from before --from would ask GitHub
        old = self.tmp / 'old-updater'
        old.write_text('#!/bin/sh\nfetch_verified() { :; }\n')
        for shell in self.each_shell():
            r = self.frm(shell, self.zip, f'from_open; from_handover "{old}"; echo "old=$?"; from_handover "{UPDATE}"; '
                                          'echo "new=$? [$FROM_KIND]"')
            self.assertIn('old=1', r.stdout, r.stderr)
            self.assertIn('new=0 []', r.stdout)
            r = self.up(shell, f'from_handover "{old}"; echo "online=$?"')
            self.assertIn('online=0', r.stdout)


@unittest.skipUnless(OPENSSL, 'an openssl with Ed25519 (OpenSSL 3) is needed to make a test key')
class Signature(UpdateBase):
    """SHA256SUMS.sig against the release key of the system: the key is made here and thrown away."""

    def setUp(self):
        super().setUp()
        self.key = Key(self.tmp / 'key')
        self.other = Key(self.tmp / 'other', number=b'\x09' * 8)
        self.stub('openssl', f'exec "{OPENSSL}" "$@"')
        self.sums = self.tmp / 'SHA256SUMS'
        self.sums.write_text('a' * 64 + '  f50-kernel.tar.gz\n')
        self.sig = self.key.sign(self.sums)

    def state(self, shell, sums, sig, pub=None, **env):
        r = self.up(shell, f'sig_state "{sums}" "{sig}"', F50_RELEASE_PUB=pub or self.key.pub, TMPDIR=self.tmp, **env)
        return r.stdout.strip()

    def test_states(self):
        changed = self.tmp / 'changed'
        changed.write_text('b' * 64 + '  f50-kernel.tar.gz\n')
        same_number = Key(self.tmp / 'same', number=self.key.number)
        garbage = self.tmp / 'garbage.sig'
        garbage.write_text('untrusted comment: x\nnot base64 at all !!\n')
        short = self.tmp / 'short.sig'
        short.write_text('untrusted comment: x\n' + base64.b64encode(b'Ed' + self.key.number + b'x' * 10).decode() + '\n')
        for shell in self.each_shell():
            self.assertEqual(self.state(shell, self.sums, self.sig), 'valid')
            self.assertEqual(self.state(shell, changed, self.sig), 'bad')                      # the file was changed
            self.assertEqual(self.state(shell, self.sums, self.other.sign(self.sums, self.tmp / 'o.sig')), 'bad')
            self.assertEqual(self.state(shell, self.sums, same_number.sign(self.sums, self.tmp / 's.sig')), 'bad')
            self.assertEqual(self.state(shell, self.sums, garbage), 'bad')
            self.assertEqual(self.state(shell, self.sums, short), 'bad')
            self.assertEqual(self.state(shell, self.sums, self.tmp / 'none.sig'), 'unsigned')
            self.assertEqual(self.state(shell, self.sums, self.sig, pub=self.tmp / 'no.pub'), 'nokey')
            # nothing was left in the temporary directory
            self.assertEqual([p.name for p in self.tmp.glob('tmp.*')], [])

    # What OpenWrt's usign itself wrote (usign -G, usign -S of the 11 bytes "abc  file1\n", in the 25.12 image; the
    # secret half was thrown away): the public key and the signature, to check that the openssl path reads them
    USIGN_PUB = 'untrusted comment: throwaway lab key\nRWRSqAkNKsFhk7kzU6iHnD8fkoPRhtR3ZZ/KrtsgyhRy8WNrOdUwT/5f\n'
    USIGN_SIG = ('untrusted comment: signed by key 52a8090d2ac16193\n'
                 'RWRSqAkNKsFhkz+TnEhFInvngZVj2+jIBlxdilrIgGI3qMLertumsyhiI8QvJ0EHjhgBfploxlZM7krZBNPohi0o6UH/0HuX5AE=\n')

    def test_a_signature_usign_made_is_read_without_usign(self):
        pub, sig, msg = self.tmp / 'usign.pub', self.tmp / 'usign.sig', self.tmp / 'usign.msg'
        pub.write_text(self.USIGN_PUB)
        sig.write_text(self.USIGN_SIG)
        for shell in self.each_shell():
            msg.write_text('abc  file1\n')
            self.assertEqual(self.state(shell, msg, sig, pub=pub), 'valid')
            msg.write_text('abc  file2\n')
            self.assertEqual(self.state(shell, msg, sig, pub=pub), 'bad')

    def secret(self, key, rounds=0):
        """KEY as the secret key file usign -G writes: "Ed", "BK", the passphrase rounds, a salt, a checksum, the
        key's number, the seed and the public key"""
        der = subprocess.run([OPENSSL, 'pkey', '-in', str(key.pem), '-outform', 'DER'], check=True, capture_output=True).stdout
        pub = base64.b64decode(key.pub.read_text().split('\n')[1])[10:]
        raw = b'EdBK' + rounds.to_bytes(4, 'big') + b'\x00' * 16 + b'\x00' * 8 + key.number + der[-32:] + pub
        return 'untrusted comment: a test secret key\n' + base64.b64encode(raw).decode() + '\n'

    def sign_release(self, sums, pub=None, **env):
        args = ['sh', str(TOP / 'scripts/release/sign-release.sh'), str(sums)] + ([str(pub)] if pub else [])
        return subprocess.run(args, capture_output=True, text=True, env=self.env(F50_OPENSSL=OPENSSL, TMPDIR=self.tmp, **env))

    def test_the_release_is_signed_as_the_devices_check_it(self):
        # scripts/release/sign-release.sh, what make-release.sh runs with the repository's secret: its signature is the one the
        # updater accepts, and a key that is not the images' stops the release
        text = self.secret(self.key)
        seed = text.split('\n')[1]
        r = self.sign_release(self.sums, self.key.pub, F50_RELEASE_KEY=text)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn(seed, r.stdout + r.stderr)
        made = Path(str(self.sums) + '.sig').read_text()
        self.assertEqual(made.split('\n')[1], self.sig.read_text().split('\n')[1])     # Ed25519: one signature per key and file
        for shell in self.each_shell():
            self.assertEqual(self.state(shell, self.sums, str(self.sums) + '.sig'), 'valid')
        # from a file, and without a public key to check against
        f = self.tmp / 'release.sec'
        f.write_text(text)
        Path(str(self.sums) + '.sig').unlink()
        self.assertEqual(self.sign_release(self.sums, F50_RELEASE_KEY_FILE=f).returncode, 0)
        self.assertEqual(Path(str(self.sums) + '.sig').read_text(), made)
        # refused, and no signature written: another key than the images', a key with a passphrase, no key, garbage
        for pub, env, says in ((self.other.pub, {'F50_RELEASE_KEY': text}, 'is not the one'),
                               (self.key.pub, {'F50_RELEASE_KEY': self.secret(self.key, rounds=42)}, 'passphrase'),
                               (self.key.pub, {}, 'no key'),
                               (self.key.pub, {'F50_RELEASE_KEY': 'untrusted comment: x\nnot a key\n'}, 'not a usign secret key'),
                               (self.key.pub, {'F50_RELEASE_KEY': self.secret(self.other)}, 'is not the one')):
            Path(str(self.sums) + '.sig').unlink(missing_ok=True)
            r = self.sign_release(self.sums, pub, **env)
            self.assertNotEqual(r.returncode, 0, says)
            self.assertIn(says, r.stderr)
            self.assertFalse(Path(str(self.sums) + '.sig').exists(), says)
            self.assertNotIn(seed, r.stdout + r.stderr)
        # nothing of the key is left in the temporary directory
        self.assertEqual([p.name for p in self.tmp.glob('tmp.*')], [])

    def test_usign_does_it_where_there_is_one(self):
        # OpenWrt: usign -V -m SUMS -p KEY -x SIG, and its status is the answer
        self.stub('usign', 'printf "%s\\n" "$*" >> "$STUBLOG/usign"; [ -z "$USIGN_FAIL" ]')
        for shell in self.each_shell():
            self.assertEqual(self.state(shell, self.sums, self.sig), 'valid')
            self.assertEqual(self.state(shell, self.sums, self.sig, USIGN_FAIL=1), 'bad')
            self.assertIn(f'-V -q -m {self.sums} -p {self.key.pub} -x {self.sig}', (self.tmp / 'usign').read_text())

    def test_what_a_state_means(self):
        # the checksums of v2026.11.02 (they say so: their f50-release line), of no release, and of another one
        named, plain, other = self.tmp / 'named', self.tmp / 'plain', self.tmp / 'other-release'
        tagsha = hashlib.sha256(b'v2026.11.02\n').hexdigest()
        named.write_text('a' * 64 + '  f50-kernel.tar.gz\n' + tagsha + '  f50-release\n')
        plain.write_text('a' * 64 + '  f50-kernel.tar.gz\n')
        other.write_text('a' * 64 + '  f50-kernel.tar.gz\n' + hashlib.sha256(b'v2026.10.01\n').hexdigest() + '  f50-release\n')
        # (state, checksums, offline source, the owner's word) -> taken or not
        cases = [
            ('valid', named, '', '', True), ('valid', named, '/x.zip', '', True),
            # a signature says nothing about checksums that name no release, or another one - with any word
            ('valid', plain, '', '1', False), ('valid', other, '', '1', False), ('valid', other, '/x.zip', '1', False),
            ('unsigned', other, '/x.zip', '1', False), ('nokey', other, '', '', False),
            ('bad', named, '', '1', False), ('bad', named, '/x.zip', '1', False),
            ('badkey', named, '', '1', False), ('badkey', named, '/x.zip', '1', False),
            ('nokey', named, '', '', True), ('nokey', plain, '', '', True),    # online, no key: as it always was
            ('nokey', named, '/x.zip', '', False), ('nokey', plain, '/x.zip', '1', True),
            ('unsigned', named, '/x.zip', '', False), ('unsigned', plain, '/x.zip', '1', True),
            # this system has a key: no unsigned release silently, newer or older - taking the signature away is
            # not a way in
            ('unsigned', named, '', '', False), ('unsigned', plain, '', '', False), ('unsigned', plain, '', '1', True),
            ('notool', named, '', '', False), ('notool', named, '', '1', True),
        ]
        for shell in self.each_shell():
            for installed in ('v2026.10.25', 'v2026.12.01'):
                for state, sums, frm, word, taken in cases:
                    r = self.up(shell, f'installed_version() {{ echo {installed}; }}; F50_FROM={frm}; F50_ALLOW_UNSIGNED={word}; '
                                       f'sums_verdict v2026.11.02 {state} "release v2026.11.02" "{sums}"; echo "rc=$?"')
                    self.assertIn('rc=0' if taken else 'rc=1', r.stdout, (state, sums.name, frm, word, r.stderr))

    def test_the_key_is_the_systems_own_file_or_nothing_is_trusted(self):
        # never a key that came with the release, and never one that somebody else could have put there: a link, a
        # file others can write. That is not "no key" (which would let everything through as before signing)
        rel = release_dir(self.tmp / 'rel')
        sig = self.key.sign(rel / 'SHA256SUMS')
        z = magisk_zip(self.tmp / 'signed.zip', rel, sums=True, sig=sig)
        (self.disk / 'openwrt-luci/etc/f50').mkdir(parents=True)
        (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.10.25\n')
        link = self.tmp / 'link.pub'
        link.symlink_to(self.key.pub)
        loose = self.tmp / 'loose.pub'
        loose.write_text(self.key.pub.read_text())
        loose.chmod(0o666)
        empty = self.tmp / 'empty.pub'
        empty.write_text('')
        get = (self.serve(rel) + 'installed_version() { echo v2026.10.25; }; '
               'fetch_verified v2026.11.02 f50-kernel.tar.gz; echo "rc=$?"')
        for shell in self.each_shell():
            for pub in (link, loose, empty, self.tmp):
                self.assertEqual(self.state(shell, rel / 'SHA256SUMS', sig, pub=pub), 'badkey', pub.name)
                shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
                for word in ('', '1'):
                    r = self.up(shell, get, F50_RELEASE_PUB=pub, F50_ALLOW_UNSIGNED=word, TMPDIR=self.tmp)
                    self.assertIn('rc=3', r.stdout, (pub.name, r.stderr))
                    r = self.up(shell, f'F50_FROM="{z}"; from_begin; echo began', F50_RELEASE_PUB=pub,
                                F50_ALLOW_UNSIGNED=word, TMPDIR=self.tmp)
                    self.assertNotIn('began', r.stdout, pub.name)
                o = json.loads(self.up(shell, f'inspect_json "{z}"', F50_RELEASE_PUB=pub, TMPDIR=self.tmp).stdout)
                self.assertEqual((o['signature'], o['fits'], o['why']), ('bad', 0, 'signature'), pub.name)
        # a zip that brings a key of its own, and is signed with it: the key is never read from there
        own = magisk_zip(self.tmp / 'own.zip', rel, sums=True, sig=self.other.sign(rel / 'SHA256SUMS', self.tmp / 'own.sig'),
                         extra=[('f50/release.pub', self.other.pub.read_text()), ('release.pub', self.other.pub.read_text()),
                                ('opt/f50/lib/release.pub', self.other.pub.read_text())])
        for shell in self.each_shell():
            o = json.loads(self.up(shell, f'inspect_json "{own}"', F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp).stdout)
            self.assertEqual(o['signature'], 'bad')
            o = json.loads(self.up(shell, f'inspect_json "{own}"', F50_RELEASE_PUB=self.tmp / 'none.pub', TMPDIR=self.tmp).stdout)
            self.assertEqual(o['signature'], 'nokey')
        src = UPDATE.read_text()
        self.assertEqual(src.count('release.pub'), 1)          # one place names the key: the system's own path

    def test_a_signature_of_one_release_under_the_name_of_another(self):
        # the signed checksums of the old v2026.10.01, handed out as v2026.11.02: by a mirror, and in a zip whose
        # manifest says the newer tag. The tag is inside what is signed.
        (self.disk / 'openwrt-luci/etc/f50').mkdir(parents=True)
        (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.10.25\n')
        old = release_dir(self.tmp / 'old', tag='v2026.10.01')
        sig = self.key.sign(old / 'SHA256SUMS')
        relabelled = magisk_zip(self.tmp / 'relabelled.zip', old, tag='v2026.11.02', sums=True, sig=sig)
        honest = magisk_zip(self.tmp / 'honest.zip', old, tag='v2026.10.01', sums=True, sig=sig)
        inst = 'installed_version() { echo v2026.10.25; }; '
        for shell in self.each_shell():
            shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
            r = self.up(shell, self.serve(old) + inst + 'fetch_verified v2026.11.02 f50-kernel.tar.gz; echo "rc=$?"',
                        F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
            self.assertIn('rc=3', r.stdout, r.stderr)
            self.assertIn('checksums of another release than v2026.11.02', r.stderr)
            self.assertFalse((self.disk / '.f50-update/f50-kernel.tar.gz').exists())
            o = json.loads(self.up(shell, f'inspect_json "{relabelled}"', F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp).stdout)
            self.assertEqual((o['signature'], o['fits'], o['why']), ('bad', 0, 'signature'))
            for word in ('', '1'):
                r = self.up(shell, f'F50_FROM="{relabelled}"; F50_ALLOW_UNSIGNED={word}; F50_ALLOW_DOWNGRADE={word}; from_begin; echo began',
                            F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
                self.assertNotIn('began', r.stdout, r.stderr)
            # under its own name it is what it is: signed, and older than what is installed
            o = json.loads(self.up(shell, f'inspect_json "{honest}"', F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp).stdout)
            self.assertEqual((o['signature'], o['downgrade'], o['fits']), ('valid', 1, 1))
            r = self.up(shell, f'F50_FROM="{honest}"; from_begin; echo began', F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
            self.assertNotIn('began', r.stdout)
            self.assertIn('older than what is installed', r.stderr)
        # signed checksums that name no release (a SHA256SUMS from before the line, signed by hand) are not "signed"
        plain = release_dir(self.tmp / 'plain')
        (plain / 'SHA256SUMS').write_text(''.join(l + '\n' for l in (plain / 'SHA256SUMS').read_text().split('\n')
                                                  if l and 'f50-release' not in l))
        psig = self.key.sign(plain / 'SHA256SUMS')
        for shell in self.each_shell():
            shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
            r = self.up(shell, self.serve(plain) + inst + 'fetch_verified v2026.11.02 f50-kernel.tar.gz; echo "rc=$?"',
                        F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
            self.assertIn('rc=3', r.stdout, r.stderr)
            self.assertIn('do not say which release', r.stderr)

    def test_checksums_swapped_between_two_downloads(self):
        # the SHA256SUMS that was verified is the one every file is checked against: the same bytes, or nothing
        rel = release_dir(self.tmp / 'rel')
        self.key.sign(rel / 'SHA256SUMS')
        evil = self.tmp / 'evil-kernel.tar.gz'
        tgz(evil, [('Image', 'another kernel', 0o644)])
        code = (self.serve(rel) + 'installed_version() { echo v2026.10.25; }; '
                'fetch_verified v2026.11.02 f50-openwrt-luci-rootfs.tar.gz; echo "first=$?"; '
                f'printf "%s  f50-kernel.tar.gz\n" "{sha(evil)}" > "$STAGE/SHA256SUMS"; cp "{evil}" "$STAGE/f50-kernel.tar.gz"; '
                'fetch_verified v2026.11.02 f50-kernel.tar.gz; echo "second=$?"')
        for shell in self.each_shell():
            for pub in (self.key.pub, self.tmp / 'no.pub'):
                shutil.rmtree(self.disk / '.f50-update', ignore_errors=True)
                r = self.up(shell, code, F50_RELEASE_PUB=pub, TMPDIR=self.tmp)
                self.assertIn('first=0', r.stdout, r.stderr)
                self.assertIn('second=3', r.stdout, r.stderr)
                self.assertIn('changed after it was checked', r.stderr)

    def serve(self, d):
        """fetch_stdout and fetch_file read from the directory D instead of the network"""
        return (f'fetch_stdout() {{ cat "{d}/${{1##*/}}" 2>/dev/null; }}; '
                f'fetch_file() {{ cp "{d}/${{1##*/}}" "$2" 2>/dev/null; }}; sleep() {{ :; }}; ')

    def test_a_download_is_checked_against_the_signature(self):
        rel = release_dir(self.tmp / 'rel')
        sig = self.key.sign(rel / 'SHA256SUMS')
        get = 'fetch_verified v2026.11.02 f50-kernel.tar.gz; echo "rc=$?"'
        inst = 'installed_version() { echo v2026.10.25; }; '
        stage = self.disk / '.f50-update'
        for shell in self.each_shell():
            shutil.rmtree(stage, ignore_errors=True)
            r = self.up(shell, self.serve(rel) + inst + get, F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertIn('signed with the project\'s release key', r.stdout)
            # a mirror that changed a file and its checksum, and left the signature (or took it away)
            evil = self.tmp / 'evil'
            shutil.rmtree(evil, ignore_errors=True)
            shutil.copytree(rel, evil)
            tgz(evil / 'f50-kernel.tar.gz', [('Image', 'another kernel', 0o644)])
            (evil / 'SHA256SUMS').write_text(f'{sha(evil / "f50-kernel.tar.gz")}  f50-kernel.tar.gz\n')
            for strip in (False, True):
                if strip:
                    (evil / 'SHA256SUMS.sig').unlink()
                shutil.rmtree(stage, ignore_errors=True)
                r = self.up(shell, self.serve(evil) + inst + get, F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
                # (3: the release was refused, which no second try changes - apply says no more than that)
                self.assertIn('rc=3', r.stdout, (strip, r.stderr))
                self.assertFalse((stage / 'f50-kernel.tar.gz').exists())
            # a release from before signing, older than what is installed: not silently either, once there is a key
            shutil.rmtree(stage, ignore_errors=True)
            unsigned = self.tmp / 'unsigned'
            shutil.rmtree(unsigned, ignore_errors=True)
            shutil.copytree(rel, unsigned)
            (unsigned / 'SHA256SUMS.sig').unlink()
            older = 'installed_version() { echo v2026.12.01; }; '
            r = self.up(shell, self.serve(unsigned) + older + get, F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
            self.assertIn('rc=3', r.stdout, r.stderr)
            # apply on a refusal: nothing changed, and no hint of a network fault or a download to continue (the U30
            # Air's log of an unsigned release said both)
            r = self.up(shell, self.serve(unsigned) + older + get.replace('; echo "rc=$?"', ' || fetch_stop $?'),
                        F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
            self.assertIn('ERROR: nothing was changed\n', r.stderr)
            self.assertNotIn('mobile data', r.stderr)
            self.assertNotIn('continue the download', r.stderr)
            r = self.up(shell, 'fetch_stop 1', TMPDIR=self.tmp)
            self.assertIn('continue the download', r.stderr)
            r = self.up(shell, self.serve(unsigned) + older + get, F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp, F50_ALLOW_UNSIGNED=1)
            self.assertIn('rc=0', r.stdout, r.stderr)
            # the same mirror, a system without a key: as before signing, the checksum is all there is
            shutil.rmtree(stage, ignore_errors=True)
            r = self.up(shell, self.serve(evil) + inst + get, F50_RELEASE_PUB=self.tmp / 'no.pub', TMPDIR=self.tmp)
            self.assertIn('rc=0', r.stdout, r.stderr)

    @unittest.skipUnless(shutil.which('unzip') or shutil.which('busybox'), 'unzip is needed to read a Magisk zip')
    def test_a_signed_zip_needs_no_word_and_a_forged_one_gets_none(self):
        (self.disk / 'openwrt-luci/etc/f50').mkdir(parents=True)
        (self.disk / 'openwrt-luci/etc/f50/image-version').write_text('v2026.10.25\n')
        rel = release_dir(self.tmp / 'rel')
        sig = self.key.sign(rel / 'SHA256SUMS')
        good = magisk_zip(self.tmp / 'good.zip', rel, sums=True, sig=sig)
        nosig = magisk_zip(self.tmp / 'nosig.zip', rel, sums=True)
        forged = magisk_zip(self.tmp / 'forged.zip', rel, sums=True, sig=self.other.sign(rel / 'SHA256SUMS', self.tmp / 'f.sig'))
        for shell in self.each_shell():
            for z, state, begins, with_word in ((good, 'valid', True, True), (nosig, 'unsigned', False, True),
                                                (forged, 'bad', False, False)):
                r = self.up(shell, f'inspect_json "{z}"', F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
                o = json.loads(r.stdout)
                self.assertEqual(o['signature'], state, r.stderr)
                self.assertEqual((o['fits'], o['why']), (0, 'signature') if state == 'bad' else (1, None))
                for word, want in (('', begins), ('1', with_word)):
                    r = self.up(shell, f'F50_FROM="{z}"; F50_ALLOW_UNSIGNED={word}; from_begin; echo began',
                                F50_RELEASE_PUB=self.key.pub, TMPDIR=self.tmp)
                    self.assertEqual('began' in r.stdout, want, (z.name, word, r.stderr))


class Status(test_update.Lock):
    """f50-update status and releases: JSON a page can take as it is."""

    def fill(self):
        for os_, tag in (('ubuntu', 'v2026.10.25'), ('openwrt-luci', 'v2026.10.25'), ('openwrt-luci.old', 'v2026.10.18')):
            (self.disk / os_ / 'etc/f50').mkdir(parents=True, exist_ok=True)
            (self.disk / os_ / 'etc/f50/image-version').write_text(tag + '\n')
        (self.disk / 'ubuntu/etc/os-release').write_text('ID=ubuntu\nVERSION_ID="26.04"\n')
        (self.disk / 'ubuntu/usr/lib').mkdir(parents=True)
        (self.disk / 'ubuntu/usr/lib/os-release').write_text('ID=ubuntu\nVERSION_ID="26.04"\n')
        boot = self.disk / 'boot'
        boot.mkdir()
        for f, v in (('installed.tag', 'v2026.10.25'), ('installed.kernel', '6.18'), ('kernel', '6.18'),
                     ('prev.tag', 'v2026.10.18'), ('prev.kernel', '5.4'), ('prev.body', 'image'), ('prev.slot', 'b')):
            (boot / f).write_text(v + '\n')
        (self.disk / '.f50').mkdir()
        (self.disk / '.f50/boot-attempts').write_text('3\n')
        (self.tmp / 'default-boot').write_text('linux\n')
        (self.tmp / 'notice').write_text('f50-linux v2026.11.02 is out (this is v2026.10.25): sudo f50-update apply\n')

    def status(self, shell, args='', **env):
        r = self.sh(shell, f'. "{UPDATE}"; installed_version() {{ echo v2026.10.25; }}; status_json {args}', F50_LIB=1,
                    F50_DISK=self.disk, F50_BIN=BIN, F50_SYSROOT=self.root, F50_LOCK=self.lock, F50_ROOT=self.disk / 'openwrt-luci',
                    F50_DEFAULT_BOOT=self.tmp / 'default-boot', F50_NOTICE=self.tmp / 'notice',
                    F50_RELEASE_PUB=self.tmp / 'none.pub', **env)
        return json.loads(r.stdout), r

    def test_status(self):
        self.fill()
        self.stub('curl', 'echo "network used" >> "$STUBLOG/net"; exit 7')
        self.stub('wget', 'echo "network used" >> "$STUBLOG/net"; exit 7')
        for shell in self.each_shell():
            o, r = self.status(shell, '--sizes')
            self.assertEqual((o['ok'], o['device'], o['installed'], o['running'], o['restart_pending']),
                             (1, 'f50', 'v2026.10.25', 'openwrt-luci', 0), r.stderr)
            by = {s['name']: s for s in o['systems']}
            self.assertEqual(set(by), {'ubuntu', 'openwrt-luci'})
            self.assertEqual((by['ubuntu']['release'], by['ubuntu']['ubuntu'], by['ubuntu']['previous']), ('v2026.10.25', '26.04', None))
            prev = by['openwrt-luci']['previous']
            self.assertEqual((prev['release'], prev['running']), ('v2026.10.18', 0))
            self.assertIsInstance(prev['kb'], int)
            k = o['kernel']
            self.assertEqual((k['choice'], k['boot'], k['boot_kernel'], k['slot']), ('6.18', 'v2026.10.25', '6.18', 'b'))
            # Ubuntu 26.04 is on this device: 5.4 is not offered (kernel_cmd refuses it)
            self.assertEqual([(c['name'], c['ok'], c['why']) for c in k['choices']],
                             [('5.4', 0, 'ubuntu-26.04'), ('6.18', 1, None), ('7.3', 1, None)])
            self.assertEqual((k['previous']['release'], k['previous']['kernel'], k['previous']['usable']), ('v2026.10.18', '5.4', 1))
            self.assertEqual((o['boot_default'], o['boot_attempts'], o['boot_locked']), ('linux', 3, 0))
            self.assertEqual((o['disk']['need_one_kb'], o['disk']['need_all_kb']), (2000000, 3000000))
            self.assertIsInstance(o['disk']['free_kb'], int)
            self.assertEqual(o['lock'], {'held': 0, 'pid': None, 'command': None})
            self.assertEqual(o['notice'], 'v2026.11.02')
            self.assertEqual(o['signing']['key'], 0)
            # without --sizes nothing is added up
            o, _ = self.status(shell)
            self.assertIsNone({s['name']: s for s in o['systems']}['openwrt-luci']['previous']['kb'])
            # the note of a release with a four-field tag (FINDINGS 125)
            (self.tmp / 'notice').write_text('f50-linux v2026.10.16.10 is out (this is v2026.10.25): sudo f50-update apply\n')
            self.assertEqual(self.status(shell)[0]['notice'], 'v2026.10.16.10')
            (self.tmp / 'notice').write_text('f50-linux v2026.11.02 is out (this is v2026.10.25): sudo f50-update apply\n')
        self.assertFalse((self.tmp / 'net').exists())

    def test_status_of_an_empty_device_is_still_json(self):
        shutil.rmtree(self.disk / 'ubuntu')
        (self.tmp / 'notice').write_text('f50-linux v1"; rm -rf / is out (this is x): y\n')
        for shell in self.each_shell():
            o, r = self.status(shell)
            self.assertEqual((o['systems'], o['kernel']['previous'], o['kernel']['boot'], o['notice'], o['boot_default']),
                             ([], None, None, None, 'android'), r.stderr)

    def test_what_is_written_into_the_files_cannot_break_the_json(self):
        self.fill()
        nasty = 'v1","x":"\\\n$(reboot)`id`\t\x01é\n'
        (self.disk / 'openwrt-luci/etc/f50/image-version').write_text(nasty)
        (self.disk / 'boot/installed.tag').write_text(nasty)
        (self.disk / 'boot/prev.tag').write_text(nasty)
        for shell in self.each_shell():
            o, r = self.status(shell)
            rel = {s['name']: s for s in o['systems']}['openwrt-luci']['release']
            self.assertRegex(rel, r'^[A-Za-z0-9._+:,()@ /-]*$')
            self.assertRegex(o['kernel']['boot'], r'^[A-Za-z0-9._+:,()@ /-]*$')

    def test_the_lock_holder_is_named_and_never_disturbed(self):
        self.fill()
        for shell in self.each_shell():
            for prelude in (':', self.NO_FLOCK):
                pid = self.hold(shell, prelude)
                o, r = self.status(shell)
                self.assertEqual((o['lock']['held'], o['lock']['pid']), (1, pid), r.stderr)
                if os.path.isdir('/proc/self'):
                    self.assertIn('f50-update', o['lock']['command'])
                # looking did not take the lock from its holder, nor leave it taken
                rc, err, _ = self.take(shell, prelude)
                self.assertEqual(rc, 1, err)
                self.end_holders()
                o, _ = self.status(shell)
                self.assertEqual(o['lock']['held'], 0)
                rc, err, _ = self.take(shell, prelude)
                self.assertEqual(rc, 0, err)

    def github(self, pretty, releases):
        """GitHub's answer to /releases as it writes it: (tag, prerelease, draft, {asset: size})"""
        out = []
        for i, (tag, pre, draft, assets) in enumerate(releases):
            out.append({
                'url': f'https://api.github.com/repos/x/y/releases/{i}', 'html_url': f'https://github.com/x/y/releases/tag/{tag}',
                'id': i, 'author': {'login': 'someone', 'id': 1, 'html_url': 'https://github.com/someone', 'type': 'User'},
                'tag_name': tag, 'target_commitish': 'main', 'name': f'F50 Linux {tag}', 'draft': draft, 'prerelease': pre,
                'created_at': '2026-11-01T10:00:00Z', 'published_at': '2026-11-02T10:00:00Z',
                'assets': [{'url': 'https://api.github.com/x', 'id': 9, 'name': n, 'label': '',
                            'uploader': {'login': 'github-actions[bot]', 'id': 2}, 'content_type': 'application/gzip',
                            'state': 'uploaded', 'size': s, 'download_count': 3,
                            'browser_download_url': f'https://github.com/x/y/releases/download/{tag}/{n}'} for n, s in assets.items()],
                'tarball_url': 'https://api.github.com/x/tarball', 'zipball_url': 'https://api.github.com/x/zipball',
                # notes that look like the fields this reads
                'body': 'Notes: "tag_name": "v2099.01.01", "prerelease": true,\r\n| "name": "f50-kernel.tar.gz", "size": 1 |\n{"assets": [1]}',
            })
        return json.dumps(out, indent=2) if pretty else json.dumps(out, separators=(',', ':'))

    ASSETS = {'f50-kernel.tar.gz': 20000000, 'f50-kernel-6.18.tar.gz': 30000000, 'f50-openwrt-luci-rootfs.tar.gz': 40000000,
              'f50-ubuntu-rootfs.tar.gz': 300000000, 'f50-ubuntu-26.04-rootfs.tar.gz': 310000000,
              'f50-extra-lang.tar.gz': 2000000, 'f50-update': 100000, 'SHA256SUMS': 900}

    def releases(self, shell, listing, latest):
        (self.tmp / 'list.json').write_text(listing)
        code = (f'. "{UPDATE}"; installed_version() {{ echo v2026.10.25; }}; latest_release() {{ echo "{latest}"; }}; '
                f'fetch_stdout() {{ echo "$1" >> "{self.tmp}/asked"; cat "{self.tmp}/list.json"; }}; releases_json')
        r = self.sh(shell, code, F50_LIB=1, F50_DISK=self.disk, F50_BIN=BIN, F50_SYSROOT=self.root, F50_LOCK=self.lock)
        return json.loads(r.stdout), r

    def test_releases(self):
        self.fill()
        (self.disk / 'extra/lang/i18n').mkdir(parents=True)
        (self.disk / 'extra/lang/name').write_text('lang\n')
        (self.disk / 'extra/lang/release').write_text('v2026.10.25\n')
        signed = dict(self.ASSETS, **{'SHA256SUMS.sig': 150})
        rels = [('v2026.11.09', True, False, signed), ('test-issue-102', True, False, {}), ('v2026.11.05', True, True, self.ASSETS),
                ('v2026.11.02', False, False, self.ASSETS), ('v2026.10.30', True, False, self.ASSETS),
                ('v2026.10.25', False, False, self.ASSETS)]
        for shell in self.each_shell():
            for pretty in (True, False):
                o, r = self.releases(shell, self.github(pretty, rels), 'v2026.11.02')
                self.assertEqual((o['ok'], o['installed'], o['boot']), (1, 'v2026.10.25', 'v2026.10.25'), r.stderr)
                la = o['latest']
                self.assertEqual((la['tag'], la['title'], la['published'], la['signed'], la['systems'], la['need_kb']),
                                 ('v2026.11.02', 'F50 Linux v2026.11.02', '2026-11-02', 0, 2, 3000000))
                self.assertEqual(la['url'], 'https://github.com/dikeckaan/mu300-linux/releases/tag/v2026.11.02')
                # what this device takes: both systems (Ubuntu's 26.04), the 6.18 kernel, the updater, the language pack
                self.assertEqual({f['name']: f['size'] for f in la['files']},
                                 {'f50-kernel-6.18.tar.gz': 30000000, 'f50-update': 100000,
                                  'f50-ubuntu-26.04-rootfs.tar.gz': 310000000, 'f50-openwrt-luci-rootfs.tar.gz': 40000000,
                                  'f50-extra-lang.tar.gz': 2000000})
                self.assertEqual(la['download'], 382100000)
                # the newest pre-release that is a release tag, not a draft, and newer than the newest release
                self.assertEqual((o['beta']['tag'], o['beta']['signed']), ('v2026.11.09', 1))
            # no pre-release newer than the newest release: none is offered
            o, _ = self.releases(shell, self.github(True, rels[3:]), 'v2026.11.02')
            self.assertIsNone(o['beta'])
            # up to date: nothing but the kernel and the updater would be looked at
            o, _ = self.releases(shell, self.github(True, rels[5:]), 'v2026.10.25')
            self.assertEqual((o['latest']['systems'], o['latest']['need_kb']), (0, 1000000))
        self.assertIn('https://api.github.com/repos/dikeckaan/mu300-linux/releases?per_page=10', (self.tmp / 'asked').read_text())

    def test_releases_of_the_new_names(self):
        # vYYYY.MM.DD.N (FINDINGS 125): newer than every three-field tag, whatever its date; .10 after .9
        self.fill()
        rels = [('v2026.10.16.9', True, False, self.ASSETS), ('v2026.10.16.10', True, False, self.ASSETS),
                ('v2026.10.26', True, False, self.ASSETS), ('v2026.10.16.0', True, False, self.ASSETS),
                ('v2026.10.27', False, False, self.ASSETS)]
        for shell in self.each_shell():
            o, r = self.releases(shell, self.github(True, rels), 'v2026.10.27')
            self.assertEqual((o['latest']['tag'], o['beta']['tag']), ('v2026.10.27', 'v2026.10.16.10'), r.stderr)
            o, r = self.releases(shell, self.github(True, rels[:2] + [('v2026.10.17.1', False, False, self.ASSETS)]), 'v2026.10.17.1')
            self.assertEqual((o['latest']['tag'], o['beta']), ('v2026.10.17.1', None), r.stderr)

    def test_without_an_answer_it_says_offline(self):
        self.fill()
        for shell in self.each_shell():
            for listing, latest in (('', 'v2026.11.02'), (self.github(True, []), ''), ('<html>rate limit</html>', '')):
                (self.tmp / 'list.json').write_text(listing)
                code = (f'. "{UPDATE}"; latest_release() {{ echo "{latest}"; }}; '
                        f'fetch_stdout() {{ [ -s "{self.tmp}/list.json" ] && cat "{self.tmp}/list.json"; }}; releases_json; echo "rc=$?"')
                r = self.sh(shell, code, F50_LIB=1, F50_DISK=self.disk, F50_BIN=BIN, F50_SYSROOT=self.root)
                self.assertEqual(r.stdout, '{"ok":0,"why":"offline"}\nrc=1\n', r.stderr)
            # no newest release: the list is not asked for at all (each question waits for its retries)
            code = (f'. "{UPDATE}"; latest_release() {{ echo ""; }}; '
                    'fetch_stdout() { echo "ASKED $1" >&2; return 1; }; releases_json; echo "rc=$?"')
            r = self.sh(shell, code, F50_LIB=1, F50_DISK=self.disk, F50_BIN=BIN, F50_SYSROOT=self.root)
            self.assertEqual((r.stdout, r.stderr), ('{"ok":0,"why":"offline"}\nrc=1\n', ''))

    def test_another_source(self):
        # /etc/f50/update-source: a mirror, for the command and the panel alike; root's own file, URLs only
        conf = self.tmp / 'update-source'
        show = 'source_conf; echo "[${F50_API_URL:-}][${F50_DOWNLOAD_URL:-}][${F50_REPO:-}]"'
        for shell in self.each_shell():
            conf.write_text('# a mirror\nF50_API_URL=http://192.168.0.2:8000/api\nF50_DOWNLOAD_URL=https://mirror.example/dl\n'
                            'F50_REPO=evil/repo\nF50_BIN=/tmp/x\n')
            conf.chmod(0o600)
            r = self.up(shell, show, F50_SOURCE_CONF=conf)
            self.assertEqual(r.stdout.strip(), '[http://192.168.0.2:8000/api][https://mirror.example/dl][]', r.stderr)
            # the environment wins
            r = self.up(shell, show, F50_SOURCE_CONF=conf, F50_API_URL='http://other/api')
            self.assertEqual(r.stdout.strip(), '[http://other/api][https://mirror.example/dl][]')
            # a value that is not a plain URL is not taken
            conf.write_text('F50_API_URL=http://x/$(reboot)\nF50_DOWNLOAD_URL=file:///etc\n')
            self.assertEqual(self.up(shell, show, F50_SOURCE_CONF=conf).stdout.strip(), '[][][]')
            # nor a file others can write, nor a link
            conf.write_text('F50_API_URL=http://x/api\n')
            conf.chmod(0o666)
            r = self.up(shell, show, F50_SOURCE_CONF=conf)
            self.assertEqual(r.stdout.strip(), '[][][]')
            self.assertIn('writable by others', r.stderr)
            conf.chmod(0o600)
            link = self.tmp / 'link'
            link.unlink(missing_ok=True)
            link.symlink_to(conf)
            self.assertEqual(self.up(shell, show, F50_SOURCE_CONF=link).stdout.strip(), '[][][]')
            # and the two URLs are where the release is asked for and fetched
            r = self.up(shell, 'fetch_stdout() { echo "$1" >&2; return 1; }; fetch_file() { echo "$1" >&2; return 1; }; '
                               'sleep() { :; }; source_conf; latest_release; fetch_verified v2026.11.02 f50-update',
                        F50_SOURCE_CONF=conf)
            self.assertIn('http://x/api/releases/latest', r.stderr)

    def test_the_status_carries_the_release_source(self):
        # what the page's field shows: the file's address, or GitHub (a panel never sees the environment, the adapter
        # clears those variables before it runs anything)
        conf = self.tmp / 'update-source'
        self.fill()
        url = 'https://proxy.example/https://github.com/dikeckaan/mu300-linux/releases/download'
        for shell in self.each_shell():
            conf.unlink(missing_ok=True)
            o, r = self.status(shell, F50_SOURCE_CONF=conf)
            self.assertEqual(o['source'], {'files': None, 'from': 'github'}, r.stderr)
            conf.write_text(f'F50_DOWNLOAD_URL={url}\n')
            conf.chmod(0o600)
            o, r = self.status(shell, F50_SOURCE_CONF=conf)
            self.assertEqual(o['source'], {'files': url, 'from': 'file'}, r.stderr)
            # a file with two lines of the same name: the reader and source_conf agree on the first one
            conf.chmod(0o600)
            conf.write_text(f'F50_DOWNLOAD_URL={url}\nF50_DOWNLOAD_URL=http://second/dl\n')
            self.assertEqual(self.status(shell, F50_SOURCE_CONF=conf)[0]['source'], {'files': url, 'from': 'file'})
            # a value that is not a plain http(s) URL, or a file others can write, is not a source
            conf.write_text('F50_DOWNLOAD_URL=file:///etc\n')
            self.assertEqual(self.status(shell, F50_SOURCE_CONF=conf)[0]['source'], {'files': None, 'from': 'github'})
            conf.chmod(0o666)
            conf.write_text(f'F50_DOWNLOAD_URL={url}\n')
            self.assertEqual(self.status(shell, F50_SOURCE_CONF=conf)[0]['source'], {'files': None, 'from': 'github'})

    def test_the_source_command(self):
        # `f50-update source`: show where the release files come from, put a mirror in /etc/f50/update-source, or
        # take it out again - written through the checks source_conf reads the file with, so the next run takes it
        conf = self.tmp / 'update-source'
        self.stub('id', 'echo 0')
        url = 'https://proxy.example/https://github.com/dikeckaan/mu300-linux/releases/download'
        default = ('release files  https://github.com/dikeckaan/mu300-linux/releases/download  (default)\n'
                   'release list   https://api.github.com/repos/dikeckaan/mu300-linux  (default)\n')
        for shell in self.each_shell():
            conf.unlink(missing_ok=True)
            r = self.script(shell, UPDATE, 'source', F50_SOURCE_CONF=conf)
            self.assertEqual(r.stdout, default, r.stderr)
            # a mirror goes in, root's own file and 0600, and the file's other lines are left as they are
            conf.write_text('# a mirror\nF50_API_URL=http://192.168.0.2:8000/api\n')
            conf.chmod(0o600)
            r = self.script(shell, UPDATE, 'source', url, F50_SOURCE_CONF=conf)
            self.assertEqual(r.stdout.split('\n')[0], f'release files  {url}  ({conf})', r.stderr)
            self.assertEqual(conf.read_text(),
                             f'# a mirror\nF50_API_URL=http://192.168.0.2:8000/api\nF50_DOWNLOAD_URL={url}\n')
            self.assertEqual(conf.stat().st_mode & 0o777, 0o600)
            # the environment wins over the file, and is named as where the value came from
            r = self.script(shell, UPDATE, 'source', F50_SOURCE_CONF=conf, F50_DOWNLOAD_URL='http://other/dl')
            self.assertEqual(r.stdout.split('\n')[0], 'release files  http://other/dl  (environment)')
            # the older whole-base variable is said to be the whole base
            r = self.script(shell, UPDATE, 'source', F50_SOURCE_CONF=conf, F50_RELEASE_URL='http://m/flat')
            self.assertIn('(F50_RELEASE_URL: the whole base, the release tag is not added)', r.stdout)
            # what is not a plain http(s) URL, or a second word, changes nothing
            for bad in [['file:///etc'], ['http://x/$(reboot)'], [url + 'x' * 250], ['off', 'extra']]:
                before = conf.read_text()
                r = self.script(shell, UPDATE, 'source', *bad, F50_SOURCE_CONF=conf)
                self.assertNotEqual(r.returncode, 0, bad)
                self.assertEqual(conf.read_text(), before, bad)
            # off takes that one line out and keeps the rest; a file with nothing left in it goes itself
            self.script(shell, UPDATE, 'source', 'off', F50_SOURCE_CONF=conf)
            self.assertEqual(conf.read_text(), '# a mirror\nF50_API_URL=http://192.168.0.2:8000/api\n')
            conf.write_text(f'F50_DOWNLOAD_URL={url}\n')
            conf.chmod(0o600)
            self.script(shell, UPDATE, 'source', 'off', F50_SOURCE_CONF=conf)
            self.assertFalse(conf.exists())

    def test_a_mirror_that_cannot_be_reached(self):
        # a mirror the device cannot reach: said once, and the rest of that run comes from github.com. An HTTP
        # error is the source's answer, and stops the run as it always did.
        conf = self.tmp / 'update-source'
        url = 'https://proxy.example/https://github.com/dikeckaan/mu300-linux/releases/download'
        field = 'a' * 64
        conf.write_text(f'F50_DOWNLOAD_URL={url}\n')
        conf.chmod(0o600)
        # the last word of curl's arguments is the URL it was asked for; the mirror is unreachable (7), github answers
        self.stub('curl', 'for u in "$@"; do :; done\n'
                          'printf "%s\\n" "$u" >> "$STUBLOG/urls"\n'
                          f'case $u in *proxy.example*) exit 7 ;; *) printf "%s\\n" "{field}  f50-update"; exit 0 ;; esac')
        self.stub('sleep', ':')     # fetch_verified waits between its tries; the test does not
        code = 'source_conf; fetch_verified v2026.11.02 f50-update; echo "rc=$?"; echo "mirror: [${F50_DOWNLOAD_URL:-}]"; echo "api: [${F50_API_URL:-}]"'
        for shell in self.each_shell():
            (self.tmp / 'urls').unlink(missing_ok=True)
            r = self.up(shell, code, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0)
            self.assertEqual(r.stderr.count('mirror unreachable: the rest comes from github.com'), 1, r.stderr)
            self.assertIn('mirror: []', r.stdout)
            # the announcement is true of both sides: an API mirror this device named is left too
            self.assertIn('api: []', r.stdout, 'a run that says github.com must not still ask an API mirror')
            urls = (self.tmp / 'urls').read_text().split()
            self.assertTrue(urls[0].startswith(url + '/v2026.11.02/'), urls[:2])
            self.assertIn('https://github.com/dikeckaan/mu300-linux/releases/download/v2026.11.02/SHA256SUMS', urls)
        # an HTTP error (22) is not a mirror that is gone: the run stops with the source still in place
        self.stub('curl', 'for u in "$@"; do :; done\nprintf "%s\\n" "$u" >> "$STUBLOG/urls"\ncase $u in *proxy.example*) exit 22 ;; *) exit 0 ;; esac')
        for shell in self.each_shell():
            r = self.up(shell, code, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0)
            self.assertEqual(r.stderr.count('mirror unreachable'), 0, r.stderr)
            self.assertIn(f'mirror: [{url}]', r.stdout)
        # and with no mirror set at all nothing is said: an unreachable GitHub is just an unreachable GitHub
        self.stub('curl', 'exit 7')
        for shell in self.each_shell():
            r = self.up(shell, code, F50_SOURCE_CONF=self.tmp / 'no-such-file', F50_FETCH_DELAY=0)
            self.assertEqual(r.stderr.count('mirror unreachable'), 0, r.stderr)

    def test_a_slow_mirror_is_not_an_unreachable_one(self):
        # a mirror that answers and then stalls wrote bytes before it died, so the run keeps taking the files from
        # it: the fallback is for a mirror that is not there, not for a slow line - the people who set this field
        # are often the ones who cannot reach github.com at all. A public https mirror, so the fallback would be
        # allowed if the failure counted.
        conf = self.tmp / 'update-source'
        url = 'https://mirror.example/f50/releases/download'
        field = 'b' * 64
        conf.write_text(f'F50_DOWNLOAD_URL={url}\n')
        conf.chmod(0o600)
        self.stub('sleep', ':')     # fetch_verified waits between its tries; the test does not
        code = 'source_conf; fetch_verified v2026.11.02 f50-update; echo "rc=$?"; echo "mirror: [${F50_DOWNLOAD_URL:-}]"'
        # -o FILE gets half a file and then curl's 28 (a transfer cut at --speed-time) or 56 (a reset mid-transfer);
        # the checksums come back fine
        for rc in (28, 56):
            self.stub('curl', 'for u in "$@"; do :; done\n'
                              'out=; prev=; for a in "$@"; do [ "$prev" = -o ] && out=$a; prev=$a; done\n'
                              'printf "%s\\n" "$u" >> "$STUBLOG/urls"\n'
                              f'case $u in *mirror.example*SHA256SUMS) printf "%s\\n" "{field}  f50-update"; exit 0 ;;'
                              f' *mirror.example*) [ -z "$out" ] || printf half >> "$out"; exit {rc} ;; *) exit 0 ;; esac')
            for shell in self.each_shell():
                (self.tmp / 'urls').unlink(missing_ok=True)
                r = self.up(shell, code, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0)
                self.assertEqual(r.stderr.count('mirror unreachable'), 0, (rc, r.stderr))
                self.assertIn(f'mirror: [{url}]', r.stdout)
                urls = (self.tmp / 'urls').read_text().split()
                self.assertTrue(all(u.startswith(url) for u in urls), (rc, urls))      # github was never asked
                self.assertGreaterEqual(urls.count(f'{url}/v2026.11.02/f50-update'), 2, urls)   # and it was retried
        # a stall before the first byte (28 with nothing written) is a mirror that is not there: github.com
        self.stub('curl', 'for u in "$@"; do :; done\n'
                          'printf "%s\\n" "$u" >> "$STUBLOG/urls"\n'
                          f'case $u in *mirror.example*SHA256SUMS) printf "%s\\n" "{field}  f50-update"; exit 0 ;;'
                          f' *mirror.example*) exit 28 ;; *github.com*SHA256SUMS) printf "%s\\n" "{field}  f50-update"; exit 0 ;;'
                          ' *) exit 0 ;; esac')
        for shell in self.each_shell():
            (self.tmp / 'urls').unlink(missing_ok=True)
            r = self.up(shell, code, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0)
            self.assertEqual(r.stderr.count('mirror unreachable: the rest comes from github.com'), 1, r.stderr)
            urls = (self.tmp / 'urls').read_text().split()
            self.assertIn('https://github.com/dikeckaan/mu300-linux/releases/download/v2026.11.02/f50-update', urls)

    def test_the_fetchers_tell_unreachable_from_an_answer(self):
        # fetch_unreachable_rc on its own: connect-class codes are "not there", 28/56 only when nothing arrived, an
        # HTTP error never; and nothing is a mirror's absence when no mirror is set or the run already fell back
        cases = (('7', '', '9'), ('6', '', '9'), ('35', '', '9'), ('52', '', '9'),
                 ('28', '', '1'), ('56', '', '1'), ('28', '0', '9'), ('56', '0', '9'), ('28', '1', '1'), ('7', '1', '1'),
                 ('22', '0', '1'), ('1', '0', '1'))
        code = '; '.join(f'echo "{rc}/{arr}=$(fetch_unreachable_rc {rc} {arr})"' for rc, arr, _ in cases)
        for shell in self.each_shell():
            r = self.up(shell, code, F50_DOWNLOAD_URL='https://mirror.example/dl')
            for rc, arr, want in cases:
                self.assertIn(f'{rc}/{arr}={want}\n', r.stdout, r.stdout)
            r = self.up(shell, 'echo "[$(fetch_unreachable_rc 7 0)]"')
            self.assertEqual(r.stdout, '[1]\n')
            r = self.up(shell, 'SOURCE_FELL_BACK=1; echo "[$(fetch_unreachable_rc 7 0)]"', F50_DOWNLOAD_URL='https://m.example/dl')
            self.assertEqual(r.stdout, '[1]\n')

    def test_the_fallback_can_be_turned_off(self):
        # F50_MIRROR_FALLBACK=0: an update the mirror cannot serve fails, as it did before there was a mirror -
        # over a mobile link taking the release from github.com can be the more expensive answer. Only the file
        # says so, never the environment.
        conf = self.tmp / 'update-source'
        url = 'http://mirror.lan/f50/releases/download'
        conf.write_text(f'F50_DOWNLOAD_URL={url}\nF50_MIRROR_FALLBACK=0\n')
        conf.chmod(0o600)
        self.stub('curl', 'for u in "$@"; do :; done\nprintf "%s\\n" "$u" >> "$STUBLOG/urls"\n'
                          'case $u in *mirror.lan*) exit 7 ;; *) exit 0 ;; esac')
        code = 'source_conf; fetch_verified v2026.11.02 f50-update; echo "rc=$?"; echo "state: [${F50_DOWNLOAD_URL:-} $MIRROR_FALLBACK]"'
        for shell in self.each_shell():
            (self.tmp / 'urls').unlink(missing_ok=True)
            r = self.up(shell, code, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0)
            self.assertEqual(r.stderr.count('the rest comes from github.com'), 0, r.stderr)
            # said once, with the reason
            self.assertEqual(r.stderr.count('mirror unreachable: the release is not taken from github.com instead '
                                            f'(F50_MIRROR_FALLBACK=0 in {conf})'), 1, r.stderr)
            self.assertIn(f'state: [{url} 0]', r.stdout)
            self.assertIn('cannot download SHA256SUMS of v2026.11.02', r.stderr)
            urls = (self.tmp / 'urls').read_text().split()
            self.assertTrue(all(u.startswith(url) for u in urls), urls)
        # without the line a public https mirror falls back, and the environment cannot turn it off
        self.stub('curl', 'for u in "$@"; do :; done\nprintf "%s\\n" "$u" >> "$STUBLOG/urls"\n'
                          'case $u in *mirror.example*) exit 7 ;; *) exit 0 ;; esac')
        conf.write_text('F50_DOWNLOAD_URL=https://mirror.example/f50/releases/download\n')
        for shell in self.each_shell():
            r = self.up(shell, code, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0, F50_MIRROR_FALLBACK='0')
            self.assertIn('state: [ auto]', r.stdout)
            self.assertEqual(r.stderr.count('mirror unreachable: the rest comes from github.com'), 1, r.stderr)

    def test_a_lan_or_plain_http_mirror_does_not_fall_back_by_itself(self):
        # a copy on the LAN is usually there so the release does not come over a metered SIM: when that box is off,
        # the run says so and stops instead of quietly taking the release from github.com over the mobile link
        conf = self.tmp / 'update-source'
        self.stub('sleep', ':')
        code = 'source_conf; fetch_verified v2026.11.02 f50-update; echo "rc=$?"'
        for url in ('http://mirror.example/dl', 'https://192.168.1.5:8443/dl', 'https://10.0.0.2/dl', 'https://172.20.1.1/dl',
                    'https://mirror.lan/dl', 'https://nas.local/dl', 'https://nas/dl', 'https://127.0.0.1:8000/dl'):
            conf.write_text(f'F50_DOWNLOAD_URL={url}\n')
            conf.chmod(0o600)
            self.stub('curl', 'for u in "$@"; do :; done\nprintf "%s\\n" "$u" >> "$STUBLOG/urls"\n'
                              f'case $u in {url}*) exit 7 ;; *) exit 0 ;; esac')
            for shell in self.each_shell():
                (self.tmp / 'urls').unlink(missing_ok=True)
                r = self.up(shell, code, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0)
                self.assertNotIn('rc=0', r.stdout, url)
                self.assertEqual(r.stderr.count('the rest comes from github.com'), 0, (url, r.stderr))
                self.assertEqual(r.stderr.count('a plain-http or LAN mirror; F50_MIRROR_FALLBACK=1'), 1, (url, r.stderr))
                urls = (self.tmp / 'urls').read_text().split()
                self.assertTrue(all(u.startswith(url) for u in urls), (url, urls))      # github.com never asked
            # the line in the file allows it
            conf.write_text(f'F50_DOWNLOAD_URL={url}\nF50_MIRROR_FALLBACK=1\n')
            for shell in self.each_shell():
                r = self.up(shell, code, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0)
                self.assertEqual(r.stderr.count('mirror unreachable: the rest comes from github.com'), 1, (url, r.stderr))
        # apply's first fetch is its self-update check, with its stderr thrown away: the line is said there all the
        # same (fd 7, the run's stderr), once, and the run's next fetch does not say it again (F50-B, 2026-10-17)
        url = 'http://192.168.79.22:8799'
        two = ('source_conf; fetch_verified v2026.11.02 f50-update 2>/dev/null; '
               'fetch_verified v2026.11.02 f50-ubuntu-rootfs.tar.gz; echo "rc=$?"')
        self.stub('curl', 'exit 7')
        for extra, line in (('', 'a plain-http or LAN mirror; F50_MIRROR_FALLBACK=1'),
                            ('F50_MIRROR_FALLBACK=1\n', 'mirror unreachable: the rest comes from github.com')):
            conf.write_text(f'F50_DOWNLOAD_URL={url}\n' + extra)
            conf.chmod(0o600)
            for shell in self.each_shell():
                r = self.up(shell, two, F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0)
                self.assertNotIn('rc=0', r.stdout)
                self.assertEqual(r.stderr.count(line), 1, (extra, r.stderr))
        # which addresses count as local
        for shell in self.each_shell():
            for url, want in (('http://mirror.example/x', 0), ('https://mirror.example/x', 1), ('https://MIRROR.LAN/x', 0),
                              ('https://172.15.0.1/x', 1), ('https://172.32.0.1/x', 1), ('https://172.16.0.1/x', 0),
                              ('https://169.254.1.1/x', 0), ('https://router.home.arpa/x', 0), ('https://localhost:8000/x', 0),
                              ('https://192.169.0.1/x', 1), ('https://8.8.8.8/x', 1)):
                r = self.up(shell, f'mirror_local {url}; echo "rc=$?"')
                self.assertEqual(r.stdout, f'rc={want}\n', url)

    def test_the_fallback_state_goes_with_a_self_update(self):
        # apply execs the updater of the release it carries, in this very environment: it must not read the source
        # file again, wait for the same mirror and say the line a second time
        conf = self.tmp / 'update-source'
        conf.write_text('F50_DOWNLOAD_URL=https://mirror.example/f50/releases/download\n')
        conf.chmod(0o600)
        for shell in self.each_shell():
            r = self.up(shell, 'source_conf; mirror_fell_back; printf "child: [%s]\\n" "$(sh -c \'echo ${F50_MIRROR_FELL_BACK:-}\')"',
                        F50_SOURCE_CONF=conf)
            self.assertIn('child: [1]', r.stdout)
            r = self.up(shell, 'source_conf; echo "mirror: [${F50_DOWNLOAD_URL:-}]"; echo "base: $(fetch_base v2026.11.02)"',
                        F50_SOURCE_CONF=conf, F50_MIRROR_FELL_BACK=1)
            self.assertIn('mirror: []', r.stdout)
            self.assertIn('base: https://github.com/dikeckaan/mu300-linux/releases/download/v2026.11.02', r.stdout)
            self.assertEqual(r.stderr.count('mirror unreachable'), 0, r.stderr)
        # and the whole download of the handed-over updater: not one request to the mirror, nothing said again
        field = 'c' * 64
        self.stub('curl', 'for u in "$@"; do :; done\nprintf "%s\\n" "$u" >> "$STUBLOG/urls"\n'
                          f'case $u in *mirror.example*) exit 7 ;; *SHA256SUMS) printf "%s\\n" "{field}  f50-update"; exit 0 ;; *) exit 0 ;; esac')
        for shell in self.each_shell():
            (self.tmp / 'urls').unlink(missing_ok=True)
            r = self.up(shell, 'source_conf; fetch_verified v2026.11.02 f50-update; echo "rc=$?"',
                        F50_SOURCE_CONF=conf, F50_FETCH_DELAY=0, F50_MIRROR_FELL_BACK=1)
            urls = (self.tmp / 'urls').read_text().split()
            self.assertTrue(urls and all(u.startswith('https://github.com/') for u in urls), urls)
            self.assertEqual(r.stderr.count('mirror unreachable'), 0, r.stderr)

    def test_the_download_address(self):
        for shell in self.each_shell():
            for env, want in (({}, 'https://github.com/dikeckaan/mu300-linux/releases/download/v2026.11.02/SHA256SUMS'),
                              ({'F50_DOWNLOAD_URL': 'http://m/dl'}, 'http://m/dl/v2026.11.02/SHA256SUMS'),
                              ({'F50_RELEASE_URL': 'http://m/flat', 'F50_DOWNLOAD_URL': 'http://m/dl'}, 'http://m/flat/SHA256SUMS')):
                r = self.up(shell, 'fetch_stdout() { echo "$1" >&2; return 1; }; fetch_verified v2026.11.02 f50-update', **env)
                self.assertIn(want, r.stderr.split('\n')[0])

    def test_a_download_names_its_file_for_whoever_follows(self):
        p = self.tmp / 'progress'
        for shell in self.each_shell():
            p.unlink(missing_ok=True)
            self.up(shell, 'fetch_stdout() { return 1; }; fetch_verified v2026.11.02 f50-kernel.tar.gz; '
                           'fetch_verified v2026.11.02 f50-openwrt-rootfs.tar.gz', F50_PROGRESS=p)
            self.assertEqual(p.read_text(), 'file f50-kernel.tar.gz\nfile f50-openwrt-rootfs.tar.gz\n')
            # and without a follower nothing is written anywhere
            r = self.up(shell, 'progress file x; echo "rc=$?"')
            self.assertEqual(r.stdout, 'rc=0\n')

    def test_the_additions_are_one_block(self):
        # what the PR says it added: every function of the block, so that a merge with other work on the updater
        # can tell at a glance what is whose
        src = UPDATE.read_text()
        block = src[src.index('# ---- for a panel: status'):src.index('# ---- for a panel: end')]
        import re
        self.assertEqual(sorted(re.findall(r'^([a-z_]+)\(\) \{', block, re.M)), sorted([
            'jv', 'jstr', 'jnum', 'mtime', 'tag_ok', 'tag_key', 'newer', 'space_needed', 'source_conf', 'key_state', 'sig_raw', 'sig_state',
            'stage_private', 'tag_sha', 'sums_trusted', 'sums_verdict', 'apply_targets', 'downgrade_ok', 'boot_downgrade_ok', 'zip_p', 'man_val', 'from_open', 'from_read', 'payload_is', 'from_close', 'from_member', 'from_fits',
            'from_installed', 'apply_options', 'from_begin', 'from_verified', 'from_handover', 'inspect_json',
            'lock_holder', 'size_kb', 'status_json', 'release_json', 'releases_json', 'progress',
            'mirror_fell_back_inherited']))


# (Status borrows Lock's holder of the lock, not its tests: those run in test_update)
for _name in dir(test_update.Lock):
    if _name.startswith('test_'):
        setattr(Status, _name, None)


if __name__ == '__main__':
    unittest.main()
