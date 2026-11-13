"""installer/android/android-install.sh: the SD card branch, cut out between its markers and run with stubs for Android's
tools (sm, mke2fs, umount) and files standing in for block devices."""
import io
import os
import re
import shutil
import tarfile
import time

from helpers import TOP, ShellTest
from test_boot_init import fake_ext4

SRC = (TOP / 'installer' / 'android' / 'android-install.sh').read_text()


def disk_block():
    """the script's lookup of the internal disk (IDISK, IPART: mmcblk0 where Android's by-name says nothing)"""
    return re.search(r'# --- android-disk begin\n(.*?)# --- android-disk end', SRC, re.S).group(1)


def stage_block():
    """the bounded-stage functions (stage_run, stall_check, ...), cut out between their markers"""
    m = re.search(r'# --- stage begin\n(.*?)# --- stage end', SRC, re.S)
    assert m, 'android-install.sh has no stage block'
    return m.group(1)


class SdCard(ShellTest):
    def setUp(self):
        super().setUp()
        m = re.search(r'# --- sd begin\n(.*?)# --- sd end', SRC, re.S)
        self.assertIsNotNone(m, 'android-install.sh has no sd block')
        self.fn = m.group(1)
        self.dev = self.tmp / 'mmcblk1p1'
        self.stub('mke2fs', 'echo "mke2fs $*" >> "$STUBLOG/calls"')
        self.stub('umount', 'echo "umount $*" >> "$STUBLOG/calls"')
        (self.tmp / 'mounts').write_text('')

    def run_fn(self, shell, call, volumes='', still_mounted=False, sm_fails=False, pre=''):
        (self.tmp / 'volumes').write_text(volumes)
        after = volumes if still_mounted else volumes.replace(' mounted ', ' unmounted ')
        (self.tmp / 'volumes.after').write_text(after)
        fail = 'echo "Error: no volume manager" >&2; exit 3' if sm_fails else 'cat "$STUBLOG/volumes"'
        self.stub('sm', f'case "$1" in list-volumes) {fail} ;; '
                        'unmount) echo "sm unmount $2" >> "$STUBLOG/calls"; cp "$STUBLOG/volumes.after" "$STUBLOG/volumes" ;; esac')
        # the stage block too: sd_prepare runs its mke2fs through stage_run (plainly: STAGE_DISK is empty here)
        code = ('say() { echo "[device] $*"; }\n' + f'F50_MOUNTS="{self.tmp}/mounts"\nF50_SYSFS="{self.tmp}/sys"\n'
                + f'T="{self.tmp}"\nF50_BY_NAME="{self.tmp}/no-by-name"\n' + disk_block() + stage_block() + self.fn
                + f'\n{pre}\n{call}\necho "rc=$?"')
        r = self.sh(shell, code)
        calls = (self.tmp / 'calls').read_text() if (self.tmp / 'calls').exists() else ''
        (self.tmp / 'calls').unlink(missing_ok=True)
        return r.stdout, calls

    def test_blank_card_is_formatted(self):
        self.dev.write_bytes(bytes(4096))
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 1')
            self.assertIn('rc=0\n', out)
            # a card whose size cannot be read gets mke2fs's own ratio
            self.assertIn(f'mke2fs -t ext4 -F -b 4096 -m 0 -i 16384 -L f50sd {self.dev}', calls)

    def test_big_card_gets_fewer_inodes(self):
        # one inode per 16 KiB on a 128 GB card is millions of inodes and a slow, failing format: the ratio grows
        # with the card, up to 1 MiB, and never below 65536 inodes (a small card keeps the default)
        self.dev.write_bytes(bytes(4096))
        size = self.tmp / 'sys' / 'class' / 'block' / 'mmcblk1p1' / 'size'
        size.parent.mkdir(parents=True)
        for sectors, ratio in ((1433600, 16384), (62333952, 262144), (249737216, 1048576), (3900000000, 1048576)):
            size.write_text(f'{sectors}\n')
            for shell in self.each_shell():
                out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 1')
                self.assertIn('rc=0\n', out)
                self.assertIn(f'-m 0 -i {ratio} -L f50sd', calls, (sectors, shell))

    def test_foreign_ext4_is_refused(self):
        fake_ext4(self.dev, 'photos')
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 1')
            self.assertIn('rc=1\n', out)
            self.assertIn('photos', out)
            self.assertNotIn('mke2fs', calls)

    def test_unlabelled_ext4_is_refused(self):
        # an ext4 without a label is still someone's Linux data
        fake_ext4(self.dev, '')
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 1')
            self.assertIn('rc=1\n', out)
            self.assertIn('refusing to format', out)
            self.assertNotIn('mke2fs', calls)

    def test_non_ext4_card_is_formatted(self):
        # a FAT/exFAT card (magic bytes not 53ef): formatted after ERASE on the host
        fake_ext4(self.dev, 'NO NAME', magic=b'\x00\x00')
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 1')
            self.assertIn('rc=0\n', out)
            self.assertIn('mke2fs', calls)

    def test_existing_installation_is_kept_or_replaced(self):
        fake_ext4(self.dev, 'f50sd')
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 0')
            self.assertIn('rc=0\n', out); self.assertNotIn('mke2fs', calls)
            out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 1')
            self.assertIn('rc=0\n', out); self.assertIn('mke2fs', calls)

    def test_card_of_the_mu300_installer_is_an_installation(self):
        # the label before the rename: kept, or formatted over, never "foreign"
        fake_ext4(self.dev, 'mu300sd')
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 0')
            self.assertIn('rc=0\n', out); self.assertNotIn('mke2fs', calls)
            out, calls = self.run_fn(shell, f'sd_prepare {self.dev} 1')
            self.assertIn('rc=0\n', out); self.assertIn('mke2fs', calls)
            self.assertNotIn('foreign', out)

    def test_keep_without_installation_fails(self):
        self.dev.write_bytes(bytes(4096))
        for shell in self.each_shell():
            out, _ = self.run_fn(shell, f'sd_prepare {self.dev} 0')
            self.assertIn('rc=1\n', out)

    def test_android_releases_the_card(self):
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_release {self.dev}', volumes='public:179,1 mounted ABCD-1234\n')
            self.assertIn('rc=0\n', out)
            self.assertIn('sm unmount public:179,1', calls)

    def test_card_android_keeps_is_refused(self):
        for shell in self.each_shell():
            out, _ = self.run_fn(shell, f'sd_release {self.dev}', volumes='public:179,1 mounted ABCD-1234\n',
                                 still_mounted=True)
            self.assertIn('rc=1\n', out)

    def test_adopted_card_is_refused(self):
        # a card adopted as internal storage is encrypted and part of Android's data: never ours to take
        for shell in self.each_shell():
            for vols in ('private:179,3 mounted 1234-uuid\n', 'private:179,2 unmounted 1234-uuid\n'):
                out, calls = self.run_fn(shell, f'sd_release {self.dev}', volumes=vols)
                self.assertIn('rc=1\n', out)
                self.assertNotIn('sm unmount', calls)

    def test_failing_volume_manager_is_a_refusal(self):
        # no answer from sm is no proof that Android has let go of the card
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_release {self.dev}', sm_fails=True)
            self.assertIn('rc=1\n', out)
            self.assertNotIn('umount', calls)

    def test_usb_stick_is_left_alone(self):
        # public:8,1 is a USB disk (major 8): neither unmounted nor a reason to refuse; only the card (179) counts
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_release {self.dev}', still_mounted=True,
                                     volumes='public:8,1 mounted 5555-6666\n')
            self.assertIn('rc=0\n', out)
            self.assertNotIn('sm unmount', calls)

    def test_builtin_private_storage_is_not_an_adopted_card(self):
        # Android lists its own data partition as "private mounted null" (no colon): that is no reason to refuse
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_release {self.dev}',
                                     volumes='private mounted null\nemulated;0 mounted null\n'
                                             'public:179,1 mounted ABCD-1234\n')
            self.assertIn('rc=0\n', out)
            self.assertIn('sm unmount public:179,1', calls)

    def test_leftover_mounts_of_the_card_only(self):
        # vold mounts the card through /dev/block/vold/public:179,N, which sm unmount takes care of; these are
        # mounts of the card's own nodes made by hand. They go; the eMMC (mmcblk0) and another card (mmcblk10) stay
        t = self.tmp
        (t / 'mounts').write_text(f'{t}/mmcblk0p40 /data ext4 rw 0 0\n'
                                  f'{t}/mmcblk1p1 /mnt/media_rw/ABCD vfat rw 0 0\n'
                                  f'{t}/mmcblk1p2 /mnt/other ext4 rw 0 0\n'
                                  f'{t}/mmcblk10p1 /mnt/ten vfat rw 0 0\n')
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_release {self.dev}')
            self.assertIn('rc=0\n', out)
            self.assertIn('umount /mnt/media_rw/ABCD', calls)
            self.assertIn('umount /mnt/other', calls)
            self.assertNotIn('/data', calls); self.assertNotIn('/mnt/ten', calls)
            self.assertNotIn('mmcblk0', calls); self.assertNotIn('mmcblk10', calls)

    def test_whole_card_never_reaches_the_emmc(self):
        # a card without a partition table: SD_DEV is mmcblk1 itself, and mmcblk0 must stay mounted
        t = self.tmp
        (t / 'mounts').write_text(f'{t}/mmcblk0 /emmc ext4 rw 0 0\n'
                                  f'{t}/mmcblk0p40 /data ext4 rw 0 0\n'
                                  f'{t}/mmcblk1 /mnt/media_rw/ABCD vfat rw 0 0\n')
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, f'sd_release {t}/mmcblk1')
            self.assertIn('rc=0\n', out)
            self.assertIn('umount /mnt/media_rw/ABCD', calls)
            self.assertNotIn('/data', calls); self.assertNotIn('/emmc', calls)
            self.assertNotIn('mmcblk0', calls)

    def sys_type(self, disk, kind):
        d = self.tmp / 'sys' / 'block' / disk / 'device'
        d.mkdir(parents=True, exist_ok=True)
        (d / 'type').write_text(kind + '\n')

    def test_emmc_is_never_the_card(self):
        self.sys_type('mmcblk0', 'SD')   # even when sysfs would claim it
        for shell in self.each_shell():
            for dev in ('/dev/block/mmcblk0p40', '/dev/block/mmcblk0'):
                out, _ = self.run_fn(shell, f'sd_check {dev}')
                self.assertIn('rc=1\n', out, dev)

    def test_non_sd_device_is_refused(self):
        self.sys_type('mmcblk1', 'SDIO')
        for shell in self.each_shell():
            out, _ = self.run_fn(shell, 'sd_check /dev/block/mmcblk1p1')
            self.assertIn('rc=1\n', out)
            out, _ = self.run_fn(shell, 'sd_check /dev/block/mmcblk2p1')   # no sysfs entry at all
            self.assertIn('rc=1\n', out)

    def test_sd_card_is_accepted(self):
        self.sys_type('mmcblk1', 'SD')
        for shell in self.each_shell():
            for dev in ('/dev/block/mmcblk1p1', '/dev/block/mmcblk1'):
                out, _ = self.run_fn(shell, f'sd_check {dev}')
                self.assertIn('rc=0\n', out, dev)

    def fake_mount_helper(self, body):
        (self.tmp / 'android-mount-f50root.sh').write_text(
            'echo "helper $*" >> "$STUBLOG/calls"\n' + body + '\n')

    def test_internal_installation_is_marked(self):
        self.fake_mount_helper('[ "$1" = -u ] || mkdir -p "$1"')
        mark = self.tmp / 'f50root-internal' / '.f50' / 'root-on-sd'
        for shell in self.each_shell():
            out, calls = self.run_fn(shell, 'sd_mark_internal', pre='set -e; SD_MODE=1 OFF=1 SIZE=2 INTERNAL_EXISTS=1')
            self.assertIn('rc=0\n', out)
            self.assertTrue(mark.exists())
            self.assertIn(f'helper -u {self.tmp}/f50root-internal', calls)
            mark.unlink()

    def test_marker_needs_an_internal_installation(self):
        self.fake_mount_helper('[ "$1" = -u ] || mkdir -p "$1"')
        for shell in self.each_shell():
            for pre in ('SD_MODE=1 OFF=1 SIZE=2 INTERNAL_EXISTS=0', 'SD_MODE=1 INTERNAL_EXISTS=1',
                        'SD_MODE=0 OFF=1 SIZE=2 INTERNAL_EXISTS=1'):
                out, calls = self.run_fn(shell, 'sd_mark_internal', pre='set -e; ' + pre)
                self.assertIn('rc=0\n', out, pre)
                self.assertNotIn('helper', calls, pre)

    def test_marker_never_fails_the_install(self):
        for shell in self.each_shell():
            self.fake_mount_helper('exit 1')   # the internal region does not mount
            out, _ = self.run_fn(shell, 'sd_mark_internal', pre='set -e; SD_MODE=1 OFF=1 SIZE=2 INTERNAL_EXISTS=1')
            self.assertIn('rc=0\n', out)
            # it mounts, but the marker cannot be written (the mount point is a file)
            self.fake_mount_helper('[ "$1" = -u ] || : > "$1"')
            out, calls = self.run_fn(shell, 'sd_mark_internal', pre='set -e; SD_MODE=1 OFF=1 SIZE=2 INTERNAL_EXISTS=1')
            self.assertIn('rc=0\n', out)
            self.assertIn('helper -u', calls)
            (self.tmp / 'f50root-internal').unlink()
            # the directory is there but the marker is not writable (a directory in its place)
            self.fake_mount_helper('[ "$1" = -u ] || mkdir -p "$1/.f50/root-on-sd"')
            out, calls = self.run_fn(shell, 'sd_mark_internal', pre='set -e; SD_MODE=1 OFF=1 SIZE=2 INTERNAL_EXISTS=1')
            self.assertIn('rc=0\n', out)
            self.assertIn('helper -u', calls)
            shutil.rmtree(self.tmp / 'f50root-internal')

    def test_internal_install_removes_a_stale_marker(self):
        # SD_MODE=0 installs into the internal filesystem; a root-on-sd left there from an earlier card
        # installation would make init wait for a card that is no longer what should boot
        m = self.tmp / 'root'
        for shell in self.each_shell():
            (m / '.f50').mkdir(parents=True, exist_ok=True)
            (m / '.f50' / 'root-on-sd').touch()
            out, _ = self.run_fn(shell, f'sd_unmark {m}', pre='set -e; SD_MODE=1')
            self.assertIn('rc=0\n', out)
            self.assertTrue((m / '.f50' / 'root-on-sd').exists())
            out, _ = self.run_fn(shell, f'sd_unmark {m}', pre='set -e; SD_MODE=0')
            self.assertIn('rc=0\n', out)
            self.assertFalse((m / '.f50' / 'root-on-sd').exists())
            out, _ = self.run_fn(shell, f'sd_unmark {m}', pre='set -e; unset SD_MODE')   # older env files
            self.assertIn('rc=0\n', out)
            # the marker of the installer from before the rename goes too
            (m / '.mu300').mkdir(parents=True, exist_ok=True)
            (m / '.mu300' / 'root-on-sd').touch()
            out, _ = self.run_fn(shell, f'sd_unmark {m}', pre='set -e; SD_MODE=0')
            self.assertIn('rc=0\n', out)
            self.assertFalse((m / '.mu300' / 'root-on-sd').exists())

    def test_unmark_runs_on_the_installed_filesystem(self):
        self.assertIn('\nsd_unmark $M\n', SRC)
        self.assertLess(SRC.index('mkdir -p $M/.f50\n'), SRC.index('\nsd_unmark $M\n'))


def make_tar(path, files):
    with tarfile.open(path, 'w:gz') as t:
        for name, text in files.items():
            data = text.encode()
            ti = tarfile.TarInfo(name)
            ti.size = len(data)
            t.addfile(ti, io.BytesIO(data))


class Downgrade(ShellTest):
    """The downgrade block: an update never puts an older release over a system by itself - the computer's installers
    reach this script without the Magisk installer's own check (install.sh). Run on a fake T and M."""

    def setUp(self):
        super().setUp()
        self.T = self.tmp / 'T'
        self.M = self.T / 'f50root'
        self.M.mkdir(parents=True)
        tags = re.search(r'# --- release-tag begin\n(.*?)# --- release-tag end', SRC, re.S)
        block = re.search(r'# --- downgrade begin\n(.*?)# --- downgrade end', SRC, re.S)
        self.assertIsNotNone(tags, 'android-install.sh has no release-tag block')
        self.assertIsNotNone(block, 'android-install.sh has no downgrade block')
        self.code = tags.group(1) + block.group(1)

    def setup_systems(self, installed, image, os_='openwrt', member='./etc/f50/image-version', etc='f50'):
        shutil.rmtree(self.M, ignore_errors=True)
        self.M.mkdir(parents=True)
        if installed is not None:
            (self.M / os_ / 'etc' / etc).mkdir(parents=True)
            (self.M / os_ / 'etc' / etc / 'image-version').write_text(installed + '\n')
        files = {'./etc/shadow': 'root:*:19000:0:99999:7:::\n'}
        if image is not None:
            files[member] = image + '\n'
        make_tar(self.T / f'f50-{os_}.tar.gz', files)

    def check(self, shell, **env):
        e = dict(OSES='openwrt', UPDATE='1')
        e.update(env)
        pre = f'say() {{ echo "[device] $*"; }}\nT="{self.T}"; M="{self.M}"\n'
        r = self.sh(shell, pre + self.code + '\ndowngrade_check; echo "rc=$?"', **e)
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout

    def test_an_older_release_is_refused(self):
        # the tags' order, not their text: every vYYYY.MM.DD.N is newer than every vYYYY.MM.DD, N is a number
        for installed, image in (('v2026.10.16.91', 'v2026.10.16.90'), ('v2026.10.16.1', 'v2026.10.27'),
                                 ('v2026.10.16.10', 'v2026.10.16.2'), ('v2026.10.28', 'v2026.10.27')):
            self.setup_systems(installed, image)
            for shell in self.each_shell():
                out = self.check(shell)
                self.assertIn('rc=1\n', out, (installed, image))
                self.assertIn(f'the installed openwrt is {installed}, and this installation is {image}, an older release',
                              out)
                self.assertIn('F50_DOWNGRADE=yes', out)

    def test_allowed_with_downgrade(self):
        self.setup_systems('v2026.10.16.91', 'v2026.10.16.90')
        for shell in self.each_shell():
            out = self.check(shell, DOWNGRADE='1')
            self.assertIn('rc=0\n', out)
            self.assertIn('openwrt: going back from v2026.10.16.91 to v2026.10.16.90, an older release', out)

    def test_what_is_not_held_back(self):
        # a newer or the same release, the member named without ./, a system or an image that does not say which
        # release it is (a build of the checkout), no system of that kind yet, and an installation that is no update
        for installed, image, env, member in (
                ('v2026.10.16.90', 'v2026.10.16.91', {}, './etc/f50/image-version'),
                ('v2026.10.27', 'v2026.10.16.1', {}, './etc/f50/image-version'),
                ('v2026.10.16.90', 'v2026.10.16.90', {}, './etc/f50/image-version'),
                ('v2026.10.16.90', 'v2026.10.16.91', {}, 'etc/f50/image-version'),
                ('dev', 'v2026.10.16.90', {}, './etc/f50/image-version'),
                ('v2026.10.16.91', 'dev', {}, './etc/f50/image-version'),
                ('v2026.10.16.91', None, {}, './etc/f50/image-version'),
                (None, 'v2026.10.16.90', {}, './etc/f50/image-version'),
                ('v2026.10.16.91', 'v2026.10.16.90', {'UPDATE': '0'}, './etc/f50/image-version')):
            self.setup_systems(installed, image, member=member)
            for shell in self.each_shell():
                out = self.check(shell, **env)
                self.assertIn('rc=0\n', out, (installed, image, env))
                self.assertNotIn('older release', out)
        # with the member named without ./, an older image is still found
        self.setup_systems('v2026.10.16.91', 'v2026.10.16.90', member='etc/f50/image-version')
        for shell in self.each_shell():
            self.assertIn('rc=1\n', self.check(shell))

    def test_a_system_of_the_mu300_installer(self):
        # an installation from before the rename keeps its release in etc/mu300; etc/f50 is asked first
        self.setup_systems('v2026.10.16.91', 'v2026.10.16.90', etc='mu300')
        for shell in self.each_shell():
            out = self.check(shell)
            self.assertIn('rc=1\n', out)
            self.assertIn('the installed openwrt is v2026.10.16.91', out)
        (self.M / 'openwrt' / 'etc' / 'f50').mkdir()
        (self.M / 'openwrt' / 'etc' / 'f50' / 'image-version').write_text('v2026.10.16.89\n')
        for shell in self.each_shell():
            self.assertIn('rc=0\n', self.check(shell))

    def test_wiring(self):
        # checked once the filesystem is mounted and before anything is written to it or to the boot chain; the
        # computer's installers pass F50_DOWNGRADE=yes on as DOWNGRADE=1, the Magisk installer its own decision
        call = SRC.index('\ndowngrade_check || exit 1\n')
        self.assertLess(SRC.index("trap 'sync; sh $T/android-mount-f50root.sh -u $M"), call)
        self.assertLess(call, SRC.index('# --- bootchain begin'))
        self.assertLess(call, SRC.index('# --- install-os begin'))
        self.assertIn('[ "${F50_DOWNGRADE:-}" != yes ] || echo DOWNGRADE=1 >> "$env"', (TOP / 'install.sh').read_text())


class RegionGuard(ShellTest):
    """android-install.sh's own check of where it writes, on a fake sysfs: behind the last partition and before the
    backup GPT, or wholly inside the one GPT partition named f50root (another installer's place, FINDINGS 179)."""

    def guard(self, shell, off_s, size_s, parts):
        sysb = self.tmp / 'sys/block/mmcblk0'
        shutil.rmtree(sysb, ignore_errors=True)
        sysb.mkdir(parents=True)
        (sysb / 'size').write_text(f'{16 << 21}\n')
        for n, (name, start, size) in enumerate(parts, 1):
            d = sysb / f'mmcblk0p{n}'
            d.mkdir()
            (d / 'start').write_text(f'{start}\n'); (d / 'size').write_text(f'{size}\n')
            (d / 'uevent').write_text(f'PARTN={n}\nPARTNAME={name}\n')
        block = re.search(r'# --- region-guard begin\n(.*?)# --- region-guard end', SRC, re.S).group(1)
        block = block.replace('/sys/block/', f'{self.tmp}/sys/block/')
        return self.sh(shell, f'say() {{ echo "[device] $*"; }}\nOFF_S={off_s}; SIZE_S={size_s}\n'
                              f'F50_BY_NAME={self.tmp}/no-by-name; F50_SYSFS={self.tmp}/sys\n{disk_block()}{block}echo PASSED')

    def test_free_space_or_the_partition_named_f50root_nothing_else(self):
        ud = ('userdata', 2048, 8 << 21)
        part = ('f50root', 2048 + (8 << 21) + 2048, 2 << 21)
        for shell in self.each_shell():
            free = (2048 + (8 << 21)) // 4096 * 4096 + 4096
            self.assertIn('PASSED', self.guard(shell, free, 1 << 21, [ud]).stdout)
            out = self.guard(shell, part[1], part[2], [ud, part]).stdout
            self.assertIn('PASSED', out)
            self.assertIn('GPT partition named f50root', out)
            # one sector past the partition, a start inside userdata, another partition's name, two of the name
            for off, size, parts in ((part[1], part[2] + 1, [ud, part]), (2048 + 4096, 4096, [ud, part]),
                                     (part[1], part[2], [ud, ('f50data', part[1], part[2])]),
                                     (part[1], part[2], [ud, part, ('f50root', part[1] + part[2], 4096)]),
                                     (part[1], part[2], [ud, ('f50root', 2048 + (8 << 21) + 2048 + (2 << 21), 4096),
                                                         ('mu300root', part[1], part[2])])):
                out = self.guard(shell, off, size, parts).stdout
                self.assertIn('region overlaps partitions', out, (off, size, parts))
                self.assertNotIn('PASSED', out)
            # the partition of the installer from before the rename, when none is named f50root
            out = self.guard(shell, part[1], part[2], [ud, ('mu300root', part[1], part[2])]).stdout
            self.assertIn('PASSED', out)
            self.assertIn('GPT partition named mu300root', out)
            out = self.guard(shell, part[1], part[2] + 1, [ud, ('mu300root', part[1], part[2])]).stdout
            self.assertNotIn('PASSED', out)


class Systems(ShellTest):
    """The per-system install loop (OpenWrt), run on a fake /data/local/tmp (T) and a fake mounted filesystem (M)."""

    def setUp(self):
        super().setUp()
        self.T = self.tmp / 'T'
        self.M = self.T / 'f50root'
        (self.M / '.f50').mkdir(parents=True)
        # Android's sed has -i like GNU's; the Mac's wants an argument
        real = shutil.which('sed')   # resolved before the stubs are on PATH; /usr/bin/sed is not there on Alpine
        self.stub('sed', f'S={real}\nif [ "$1" = -i ]; then shift; if "$S" --version >/dev/null 2>&1; then exec "$S" -i "$@"; '
                         'else exec "$S" -i "" "$@"; fi; fi; exec "$S" "$@"')
        self.install = self.block('install-os')
        self.upk = self.block('user-packages')

    def block(self, name):
        m = re.search(rf'# --- {name} begin\n(.*?)# --- {name} end', SRC, re.S)
        self.assertIsNotNone(m, f'android-install.sh has no {name} block')
        return m.group(1)

    def tarball(self, os, extra=None):
        files = {'etc/shadow': 'root:*:19000:0:99999:7:::\nnetwork:*:19000:0:99999:7:::\n', 'etc/config/network': 'new\n'}
        files.update(extra or {})
        make_tar(self.T / f'f50-{os}.tar.gz', files)

    def run_install(self, shell, fails=False, **env):
        e = dict(OSES='openwrt', UPDATE='0', PWHASH='', DEFAULT_LINUX='0', BOOT_OS='openwrt')
        e.update(env)
        # sd_unmark is another block of the script (SdCard tests it)
        pre = ('set -e\nsay() { echo "[device] $*"; }\nsd_unmark() { :; }\nssid=; psk=\n'
               + f'T="{self.T}"; M="{self.M}"\n' + stage_block() + self.upk)
        r = self.sh(shell, pre + self.install, **e)
        if fails:
            self.assertNotEqual(r.returncode, 0, r.stderr + r.stdout)
        else:
            self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
        return r

    def test_installs_openwrt(self):
        for shell in self.each_shell():
            shutil.rmtree(self.M / 'openwrt', ignore_errors=True)
            self.tarball('openwrt')
            self.run_install(shell, PWHASH='$6$salt$hash')
            self.assertTrue((self.M / 'openwrt' / 'etc' / 'config' / 'network').exists())
            shadow = (self.M / 'openwrt' / 'etc' / 'shadow').read_text()
            self.assertIn('root:$6$salt$hash:', shadow)
            self.assertIn('network:*:', shadow)
            self.assertEqual((self.M / '.f50' / 'boot-os').read_text().strip(), 'openwrt')
            self.assertFalse((self.T / 'f50-openwrt.tar.gz').exists())

    def test_update_keeps_openwrt_config(self):
        for shell in self.each_shell():
            old = self.M / 'openwrt' / 'etc' / 'config'
            old.mkdir(parents=True, exist_ok=True)
            (old / 'network').write_text('mine\n')
            self.tarball('openwrt')
            self.run_install(shell, UPDATE='1', PWHASH='$6$salt$hash')
            self.assertEqual((self.M / 'openwrt' / 'etc' / 'config' / 'network').read_text(), 'mine\n')

    def test_update_over_the_mu300_installers_system_keeps_its_settings(self):
        # an image without f50-carry keeps a list of places: a system from before the rename has its etc/f50 as
        # etc/mu300, and that is what the new system gets as etc/f50
        for shell in self.each_shell():
            shutil.rmtree(self.M / 'openwrt', ignore_errors=True)
            old = self.M / 'openwrt' / 'etc'
            (old / 'mu300').mkdir(parents=True)
            (old / 'mu300' / 'hotspot.conf').write_text('SSID=mine\n')
            (old / 'config').mkdir()
            (old / 'config' / 'network').write_text('mine\n')
            self.tarball('openwrt', {'etc/f50/image-version': 'v2\n'})
            r = self.run_install(shell, UPDATE='1')
            new = self.M / 'openwrt' / 'etc'
            self.assertEqual((new / 'f50' / 'hotspot.conf').read_text(), 'SSID=mine\n')
            self.assertEqual((new / 'f50' / 'image-version').read_text(), 'v2\n')
            self.assertEqual((new / 'config' / 'network').read_text(), 'mine\n')
            self.assertIn('etc/mu300', r.stdout)
            # with an etc/f50 there, etc/f50 is what is kept
            (self.M / 'openwrt' / 'etc' / 'mu300').mkdir()
            (self.M / 'openwrt' / 'etc' / 'mu300' / 'hotspot.conf').write_text('SSID=stale\n')
            self.tarball('openwrt')
            self.run_install(shell, UPDATE='1')
            self.assertEqual((new / 'f50' / 'hotspot.conf').read_text(), 'SSID=mine\n')

    def test_only_openwrt_and_the_vendor_kernel(self):
        # refused before anything is touched: right after the env file is read
        head = SRC[:SRC.index('# --- android-disk begin')]
        self.assertIn('case $_os in openwrt) ;; *)', head)
        self.assertIn('case ${KERNEL:-5.4} in 5.4) ;; *)', head)
        self.assertIn('echo 5.4 > $M/boot/kernel; echo 5.4 > $M/boot/installed.kernel', SRC)
        for gone in ('WIPE_LEGACY', 'extra_from_push', 'f50-extra-', 'sd_fstab', '$M/$os/etc/systemd'):
            self.assertNotIn(gone, SRC)

    def test_adb_for_the_installing_computer(self):
        key = 'QAAAA' + 'A' * 694 + '= me@my-pc'
        etc = self.M / 'openwrt' / 'etc' / 'f50'
        for shell in self.each_shell():
            shutil.rmtree(self.M / 'openwrt', ignore_errors=True)
            self.tarball('openwrt')
            self.run_install(shell, PWHASH='$6$salt$hash')
            # nothing set: the system has adb on for every computer (f50-adbd's default)
            self.assertFalse((etc / 'adb').exists())
            self.assertFalse((etc / 'adb_auth').exists())
            self.assertFalse((etc / 'adb_keys').exists())
            self.tarball('openwrt')
            r = self.run_install(shell, PWHASH='$6$salt$hash', ADB='1', ADB_KEY=key)
            self.assertEqual('on\n', (etc / 'adb').read_text())
            self.assertEqual('keys\n', (etc / 'adb_auth').read_text())      # only this computer, from the first boot
            self.assertEqual(key + '\n', (etc / 'adb_keys').read_text())
            self.assertEqual(0o600, (etc / 'adb_keys').stat().st_mode & 0o777)
            self.assertIn('adb over USB on, only for me@my-pc', r.stdout)
            # an update: the key once, and ADB=0 takes nothing away
            self.tarball('openwrt')
            self.run_install(shell, UPDATE='1', PWHASH='$6$salt$hash', ADB='1', ADB_KEY=key)
            self.assertEqual(key + '\n', (etc / 'adb_keys').read_text())
            self.tarball('openwrt')
            self.run_install(shell, UPDATE='1', PWHASH='$6$salt$hash', ADB='0', ADB_KEY='')
            self.assertEqual('on\n', (etc / 'adb').read_text())
            self.assertEqual('keys\n', (etc / 'adb_auth').read_text())
            self.assertEqual(key + '\n', (etc / 'adb_keys').read_text())

    def test_update_adds_the_config_files_of_the_images_new_packages(self):
        # the old settings stay; a package the image brings keeps its own config file (the old etc/config copied
        # whole dropped /etc/config/sqm, and LuCI's SQM page failed)
        for shell in self.each_shell():
            old = self.M / 'openwrt' / 'etc' / 'config'
            old.mkdir(parents=True, exist_ok=True)
            (old / 'network').write_text('mine\n')
            (old / 'sqm').unlink(missing_ok=True)
            self.tarball('openwrt', {'etc/config/sqm': 'image sqm\n'})
            self.run_install(shell, UPDATE='1', PWHASH='$6$salt$hash')
            cfg = self.M / 'openwrt' / 'etc' / 'config'
            self.assertEqual((cfg / 'network').read_text(), 'mine\n')
            self.assertEqual((cfg / 'sqm').read_text(), 'image sqm\n')
            self.assertFalse((self.M / 'openwrt' / 'etc' / 'config.image').exists())

    IMAGE = {'etc/passwd': 'root:x:0:0:root:/root:/bin/ash\nnetwork:x:101:101::/var:/bin/false\n'
                           'newsvc:x:120:120::/:/usr/sbin/nologin\n',
             'etc/group': 'root:x:0:\nwheel:x:10:root\nnetwork:x:101:\nnewsvc:x:120:\n',
             'etc/shadow': 'root:$6$image$root:19000:0:99999:7:::\nnetwork:*:19000:0:99999:7:::\nnewsvc:!:19000::::::\n',
             'etc/gshadow': 'root:*::\nwheel:*::root\nnetwork:!::\nnewsvc:!::\n',
             'etc/.f50-accounts-from-image': ''}
    OLD = {'passwd': 'root:x:0:0:root:/root:/bin/ash\nnetwork:x:101:101::/var:/bin/false\n'
                     'kaan:x:1001:1001::/home/kaan:/bin/ash\n',
           'group': 'root:x:0:\nwheel:x:10:root,kaan\nnetwork:x:101:\nkaan:x:1001:\n',
           'shadow': 'root:$6$old$root:19000:0:99999:7:::\nnetwork:*:19000:0:99999:7:::\n'
                     'kaan:$6$old$kaan:19000:0:99999:7:::\n',
           'gshadow': 'root:*::\nwheel:*::root,kaan\nnetwork:!::\nkaan:!::\n'}

    def old_openwrt(self):
        shutil.rmtree(self.M / 'openwrt', ignore_errors=True)
        etc = self.M / 'openwrt' / 'etc'
        etc.mkdir(parents=True)
        for f, text in self.OLD.items():
            (etc / f).write_text(text)
        self.tarball('openwrt', self.IMAGE)

    def test_update_keeps_the_accounts(self):
        # update (keep) without a new password: the accounts are the device's, as f50-update carries them over -
        # the old hashes, the users and group memberships added on the device; the image's new system user stays
        for shell in self.each_shell():
            self.old_openwrt()
            self.run_install(shell, UPDATE='1')
            etc = self.M / 'openwrt' / 'etc'
            shadow = (etc / 'shadow').read_text()
            for line in ('root:$6$old$root:', 'kaan:$6$old$kaan:', 'newsvc:!:'):
                self.assertIn(line, shadow)
            self.assertNotIn('$6$image$', shadow)
            self.assertIn('kaan:x:1001:1001:', (etc / 'passwd').read_text())
            self.assertIn('newsvc:x:120:', (etc / 'passwd').read_text())
            self.assertIn('wheel:x:10:root,kaan\n', (etc / 'group').read_text())
            self.assertIn('kaan:!::\n', (etc / 'gshadow').read_text())
            self.assertFalse((etc / '.f50-accounts-from-image').exists())

    def test_update_with_a_new_password_keeps_the_other_accounts(self):
        for shell in self.each_shell():
            self.old_openwrt()
            self.run_install(shell, UPDATE='1', PWHASH='$6$new$hash')
            shadow = (self.M / 'openwrt' / 'etc' / 'shadow').read_text()
            self.assertIn('root:$6$new$hash:', shadow)
            self.assertIn('kaan:$6$old$kaan:', shadow)
            self.assertFalse((self.M / 'openwrt' / 'etc' / '.f50-accounts-from-image').exists())

    def test_fresh_install_takes_the_hash(self):
        for shell in self.each_shell():
            shutil.rmtree(self.M / 'openwrt', ignore_errors=True)
            self.tarball('openwrt', self.IMAGE)
            self.run_install(shell, UPDATE='0', PWHASH='$6$new$hash')
            shadow = (self.M / 'openwrt' / 'etc' / 'shadow').read_text()
            self.assertIn('root:$6$new$hash:', shadow)
            self.assertNotIn('kaan', shadow)

    def test_the_images_accounts_stand_without_a_hash(self):
        # issue #167: the images' passwords are the well-known ones (OpenWrt's root without one), and
        # a system is never refused for having them. Without a hash a new system keeps the image's shadow, an update
        # the accounts carried over (whatever they hold), and the mark that has f50-update carry some <os>.old's
        # accounts over at boot never stays. PW_SET=1 puts PWHASH in even when it is empty: OpenWrt's root without a
        # password, also over one an update would carry over (F50_PASSWORD_RESET=yes).
        # The device log says what the shadow holds (#178 follow-up): with the accounts carried over and PW_SET=1 (or
        # a hash) the system's own account gets the images' password again, so "kept the accounts and passwords" would
        # be untrue; only the other accounts keep theirs.
        KEPT_ALL = 'kept the accounts and passwords of the previous {os}'
        KEPT_OTHERS_NONE = "kept the other accounts of the previous {os}; {pwu}'s password is the images' own again: none, until one is set"
        KEPT_OTHERS_HASH = "kept the other accounts of the previous {os}; {pwu}'s password is the one the installer chose (the images' own, or F50_PASSWORD)"
        cases = (  # name, os, the old system's shadow (None: no old system; '': no shadow file), PW_SET, what root keeps, the log line
            ('fresh, no hash given', 'openwrt', None, '0', 'root::', None),
            ('fresh OpenWrt, the Magisk zip', 'openwrt', None, '1', 'root::', None),
            ('old system without a shadow', 'openwrt', '', '0', 'root::', None),
            ('OpenWrt root with an empty password', 'openwrt', 'root::19000:0:99999:7:::\n', '0', 'root::', KEPT_ALL),
            ('OpenWrt root reset to none', 'openwrt', 'root:$6$old$root:19000:0:99999:7:::\n', '1', 'root::', KEPT_OTHERS_NONE),
            ('locked account', 'openwrt', 'root:!:19000::::::\n', '0', 'root:!:', KEPT_ALL),
            ('no entry for the account', 'openwrt', 'kaan:$6$a$b:1::\n', '0', 'root::', KEPT_ALL))
        for name, os_, old, pw_set, keeps, logged in cases:
            with self.subTest(name):
                for shell in self.each_shell():
                    shutil.rmtree(self.M / os_, ignore_errors=True)
                    image = dict(self.IMAGE)
                    image['etc/shadow'] = 'root::19000:0:99999:7:::\n'
                    image['etc/.mu300-accounts-from-image'] = ''
                    self.tarball(os_, image)
                    if old is not None:
                        (self.M / os_ / 'etc').mkdir(parents=True)
                        (self.M / os_ / 'etc' / 'precious').write_text('old\n')
                        if old:
                            (self.M / os_ / 'etc' / 'shadow').write_text(old)
                    r = self.run_install(shell, OSES=os_, BOOT_OS=os_, UPDATE='1' if old is not None else '0', PW_SET=pw_set)
                    self.assertFalse((self.M / f'{os_}.new').exists())
                    self.assertIn(keeps, (self.M / os_ / 'etc' / 'shadow').read_text())
                    self.assertFalse((self.M / os_ / 'etc' / '.f50-accounts-from-image').exists())
                    self.assertFalse((self.M / os_ / 'etc' / '.mu300-accounts-from-image').exists())
                    self.assertFalse((self.M / os_ / 'etc' / 'precious').exists())
                    kept = [l for l in r.stdout.splitlines() if 'kept the' in l and 'accounts' in l]
                    pwu = 'root'
                    self.assertEqual(kept, [] if logged is None else ['[device] ' + logged.format(os=os_, pwu=pwu)], name)
        # with a new hash: installed, the hash in place; on an update the log line says so
        for shell in self.each_shell():
            shutil.rmtree(self.M / 'openwrt', ignore_errors=True)
            self.tarball('openwrt', {'etc/shadow': 'root::19000:0:99999:7:::\n'})
            self.run_install(shell, OSES='openwrt', BOOT_OS='openwrt', PWHASH='$6$new$hash')
            self.assertIn('root:$6$new$hash:', (self.M / 'openwrt' / 'etc' / 'shadow').read_text())
            (self.M / 'openwrt' / 'etc' / 'shadow').write_text('root:$6$old$root:19000:0:99999:7:::\nkaan:$6$k$k:19000::::::\n')
            (self.M / 'openwrt' / 'etc' / 'passwd').write_text('root:x:0:0:root:/root:/bin/ash\nkaan:x:1000:1000::/home/kaan:/bin/ash\n')
            self.tarball('openwrt', {'etc/shadow': 'root::19000:0:99999:7:::\n', 'etc/passwd': 'root:x:0:0:root:/root:/bin/ash\n'})
            r = self.run_install(shell, OSES='openwrt', BOOT_OS='openwrt', UPDATE='1', PWHASH='$6$new$hash', PW_SET='1')
            shadow = (self.M / 'openwrt' / 'etc' / 'shadow').read_text()
            self.assertIn('root:$6$new$hash:', shadow)
            self.assertIn('kaan:$6$k$k:', shadow)
            self.assertIn('[device] ' + KEPT_OTHERS_HASH.format(os='openwrt', pwu='root'), r.stdout)
            self.assertNotIn(KEPT_ALL.format(os='openwrt'), r.stdout)

    def test_accounts_merge_is_f50_updates(self):
        # one merge, two copies: android-install.sh runs on Android and cannot source the system's f50-update
        upd = (TOP / 'systems/base/overlay/opt/f50/bin/f50-update').read_text()
        fn = re.compile(r'^merge_accounts\(\) \{.*?^\}\n', re.S | re.M)
        self.assertEqual(fn.search(self.install).group(0), fn.search(upd).group(0))


class Stages(ShellTest):
    """The bounded stages of an SD-card installation (FINDINGS 50): a stage runs in a process group of its own and is
    ended at its deadline, or after STAGE_STALL seconds without I/O progress on the card (its completion counters in
    /sys/class/block/DISK/stat); a timeout stops the group, writes nothing more (no sync, no unmount) and leaves a
    lock keyed by Android's boot id, which refuses every run until Android has rebooted; a clean run leaves none."""

    def setUp(self):
        super().setUp()
        m = re.search(r'# --- sd begin\n(.*?)# --- sd end', SRC, re.S)
        self.sd = m.group(1)
        self.T = self.tmp / 'T'
        self.M = self.T / 'f50root'
        self.M.mkdir(parents=True)
        self.stat = self.tmp / 'sys' / 'class' / 'block' / 'mmcblk1' / 'stat'
        self.stat.parent.mkdir(parents=True)
        self.stat.write_text('100 0 800 50 200 0 1600 90 0 140 140 0 0 0 0 0 0\n')
        self.boot_id = self.tmp / 'boot_id'
        self.boot_id.write_text('11111111-2222-3333-4444-555555555555\n')
        self.lock = self.T / 'f50-install.stalled'
        # macOS has no setsid: a Python one stands in (the real one is used where it exists)
        if not shutil.which('setsid'):
            self.stub('setsid', 'exec python3 -c \'import os, sys; os.setsid(); os.execvp(sys.argv[1], sys.argv[1:])\' "$@"')

    def run_fn(self, shell, call, pre='', disk='mmcblk1', stall=2):
        code = ('say() { echo "[device] $*"; }\n' + f'T="{self.T}"; M="{self.M}"\nF50_SYSFS="{self.tmp}/sys"\n'
                f'F50_BOOT_ID="{self.boot_id}"\nF50_STAGE_STALL={stall}\nF50_STAGE_POLL=1\n'
                + stage_block() + self.sd + f'\nSTAGE_DISK={disk}\n{pre}\n{call}\necho "rc=$?"')
        return self.sh(shell, code)

    def alive(self, pid):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        try:  # a zombie is dead: its parent went with the group, and in a container nothing may reap it
            with open(f'/proc/{pid}/status') as f:
                return not any(l.startswith('State:') and 'Z' in l.split()[1] for l in f)
        except OSError:
            return True

    def test_stall_stops_the_group_and_locks(self):
        # the card's counters never move: the stage ends after STAGE_STALL seconds, its whole group (a grandchild too)
        # is gone, the lock holds the boot id and names the stage, and the run is told to reboot Android
        child = self.tmp / 'child'
        for shell in self.each_shell():
            child.unlink(missing_ok=True); self.lock.unlink(missing_ok=True)
            r = self.run_fn(shell, f'stage_run "unpack openwrt" 600 sh -c \'sleep 60 & echo $! > "{child}"; wait\'')
            self.assertIn('rc=1\n', r.stdout, r.stderr)
            self.assertIn('the unpack openwrt stage timed out (no I/O progress on mmcblk1 for 2 s)', r.stdout)
            self.assertIn('reboot Android before running the installer again', r.stdout)
            self.assertIn(f'the card stays mounted at {self.M}', r.stdout)
            self.assertIn(str(self.lock), r.stdout)
            lines = self.lock.read_text().splitlines()
            self.assertEqual(lines[0], '11111111-2222-3333-4444-555555555555')
            self.assertIn('stage unpack openwrt', lines[1]); self.assertIn('no I/O progress', lines[1])
            pid = int(child.read_text())
            for _ in range(20):
                if not self.alive(pid):
                    break
                time.sleep(0.1)
            self.assertFalse(self.alive(pid), "the stage's grandchild survived the stop")
            self.assertEqual(list(self.T.glob('f50-stage.*')), [])

    def test_deadline_ends_a_stage_that_makes_progress(self):
        # counters moving, but the stage runs longer than its limit
        bump = f'i=0; while :; do i=$((i + 1)); echo "$i 0 $i 0 $i 0 $i 0 0 0 0" > "{self.stat}"; sleep 1; done'
        for shell in self.each_shell():
            self.lock.unlink(missing_ok=True)
            r = self.run_fn(shell, f"stage_run format 2 sh -c '{bump}'", stall=60)
            self.assertIn('rc=1\n', r.stdout, r.stderr)
            self.assertIn('the format stage timed out (it ran longer than 2 s)', r.stdout)
            self.assertTrue(self.lock.exists())

    def test_progress_keeps_a_stage_alive(self):
        # a stage of 4 s with counters moving every second, STAGE_STALL 2 s: it finishes, no lock, its status kept
        bump = f'for i in 1 2 3 4; do echo "$i 0 $i 0 $i 0 $i 0 0 0 0" > "{self.stat}"; sleep 1; done; echo done; exit 3'
        for shell in self.each_shell():
            r = self.run_fn(shell, f"stage_run \"unpack openwrt\" 600 sh -c '{bump}'")
            self.assertIn('done\nrc=3\n', r.stdout, r.stderr)
            self.assertNotIn('timed out', r.stdout)
            self.assertFalse(self.lock.exists())
            r = self.run_fn(shell, 'stage_run x 600 true; stage_run y 600 false')
            self.assertIn('rc=1\n', r.stdout)
            self.assertNotIn('timed out', r.stdout)

    def test_without_counters_only_the_deadline_counts(self):
        # no stat file (another sysfs layout): the stall rule is off, said so, and the deadline still applies
        self.stat.unlink()
        for shell in self.each_shell():
            self.lock.unlink(missing_ok=True)
            r = self.run_fn(shell, "stage_run flush 600 sh -c 'sleep 3; echo flushed'", stall=1)
            self.assertIn('flushed\nrc=0\n', r.stdout, r.stderr)
            self.assertIn('cannot read the I/O counters of mmcblk1: the flush stage is bounded by its limit of 600 s only', r.stdout)
            r = self.run_fn(shell, "stage_run flush 1 sh -c 'sleep 4'", stall=1)
            self.assertIn('rc=1\n', r.stdout)
            self.assertIn('it ran longer than 1 s', r.stdout)

    def test_on_the_emmc_a_stage_runs_as_it_is(self):
        # STAGE_DISK empty (an internal installation): the command runs in this shell, no group, no deadline
        for shell in self.each_shell():
            r = self.run_fn(shell, "stage_run x 1 sh -c 'sleep 2; echo ran; exit 4'", disk='')
            self.assertIn('ran\nrc=4\n', r.stdout, r.stderr)
            self.assertNotIn('timed out', r.stdout)
            self.assertFalse(self.lock.exists())

    def test_a_rerun_in_the_same_android_boot_is_refused(self):
        for shell in self.each_shell():
            self.lock.write_text('11111111-2222-3333-4444-555555555555\n'
                                 'stage unpack openwrt, 2026-10-12 13:00: no I/O progress on mmcblk1 for 120 s\n')
            r = self.run_fn(shell, 'stall_check')
            self.assertIn('rc=1\n', r.stdout, r.stderr)
            self.assertIn('an earlier run of the installer timed out in this Android boot (stage unpack openwrt, 2026-10-12 13:00: '
                          'no I/O progress on mmcblk1 for 120 s)', r.stdout)
            self.assertIn('reboot Android, then run the installer again', r.stdout)
            self.assertTrue(self.lock.exists())
            # Android rebooted since: the lock is stale and goes
            self.lock.write_text('99999999-0000-0000-0000-000000000000\nstage format\n')
            r = self.run_fn(shell, 'stall_check')
            self.assertIn('rc=0\n', r.stdout)
            self.assertFalse(self.lock.exists())
            r = self.run_fn(shell, 'stall_check')
            self.assertIn('rc=0\n', r.stdout)

    def test_a_clean_run_leaves_no_lock(self):
        for shell in self.each_shell():
            r = self.run_fn(shell, 'stage_run x 600 true; stall_clear; ls "$T"')
            self.assertIn('rc=0\n', r.stdout, r.stderr)
            self.assertFalse(self.lock.exists())
            self.assertNotIn('f50-stage', r.stdout)

    def test_exit_flushes_nothing_after_a_timeout(self):
        # the EXIT trap of SD mode: a sync and an unmount after a timeout would wait on the stuck request
        self.stub('sync', 'echo sync >> "$STUBLOG/calls"')
        (self.T / 'android-mount-f50root.sh').write_text('echo "helper $*" >> "$STUBLOG/calls"\n')
        for shell in self.each_shell():
            (self.tmp / 'calls').unlink(missing_ok=True)
            r = self.run_fn(shell, 'sd_exit', pre='STALLED=1')
            self.assertIn('rc=0\n', r.stdout, r.stderr)
            self.assertFalse((self.tmp / 'calls').exists())
            r = self.run_fn(shell, 'sd_exit', pre='STALLED=')
            self.assertIn('rc=0\n', r.stdout)
            self.assertEqual((self.tmp / 'calls').read_text(), f'sync\nhelper -u {self.M}\n')

    def test_a_timeout_ends_the_script_before_the_ok_line(self):
        # under the script's set -e a timed-out stage is the end: the trap runs, the OK line never comes
        self.stub('sync', 'echo sync >> "$STUBLOG/calls"')
        (self.T / 'android-mount-f50root.sh').write_text('echo "helper $*" >> "$STUBLOG/calls"\n')
        for shell in self.each_shell():
            (self.tmp / 'calls').unlink(missing_ok=True); self.lock.unlink(missing_ok=True)
            r = self.run_fn(shell, "stage_run flush 600 sleep 30\nstall_clear\necho F50-INSTALL-OK",
                            pre='set -e; trap sd_exit EXIT')
            self.assertNotEqual(r.returncode, 0)
            self.assertNotIn('F50-INSTALL-OK', r.stdout)
            self.assertIn('timed out', r.stdout)
            self.assertTrue(self.lock.exists())
            self.assertFalse((self.tmp / 'calls').exists())

    def test_wiring(self):
        # the lock is checked before anything, the card's disk names the counters, every long card stage is bounded,
        # the flush comes before the OK line and the lock goes with it, and the trap is sd_exit
        self.assertLess(SRC.index('\nstall_check || exit 1\n'), SRC.index('if [ "${SD_MODE:-0}" = 1 ]; then'))
        self.assertIn('STAGE_DISK=$(sd_disk "$SD_DEV"); STAGE_DISK=${STAGE_DISK##*/}', SRC)
        self.assertIn('stage_run format 900 sh -c \'exec "$@" >/dev/null\' sh $mk -t ext4 -F -b 4096 -m 0 -i "$i" -L f50sd "$1"', SRC)
        self.assertIn('stage_run "unpack $os" 3600 tar -xzpf $tarball -C $M/$os.new', SRC)
        self.assertIn('stage_run "unpack $os vendor" 900 tar -xzpf $T/f50-vendor-$os.tar.gz -C $M/$os.new', SRC)
        self.assertIn('stage_run "clean $os" 900 rm -rf $M/$os.new\n', SRC)
        # the old system goes aside and is removed, bounded, once the new one is in place
        self.assertIn('    if [ -d $M/$os ]; then mv $M/$os $M/$os.broken; fi\n    mv $M/$os.new $M/$os\n'
                      '    stage_run "replace $os" 900 rm -rf $M/$os.broken\n', SRC)
        self.assertIn('[ "${SD_MODE:-0}" != 1 ] || stage_run flush 1800 sync\nstall_clear\n', SRC)
        self.assertLess(SRC.index('stage_run flush 1800 sync'), SRC.index('echo F50-INSTALL-OK'))
        self.assertIn('trap sd_exit EXIT', SRC)
        self.assertNotIn("trap 'sync; sh $T/android-mount-f50root.sh -u $M", SRC[:SRC.index('# --- the region must')])
