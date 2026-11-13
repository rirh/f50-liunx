"""Small device scripts the installers, the boot and the image still use, against a fake / (F50_SYSROOT) and stub
commands: f50-device, f50-lan-ip, android-vendor-start, f50-usb, lan-start, f50-next-boot, the sysupgrade wrapper."""
import json
import os
import re
import shutil
import stat
import struct
import subprocess
import unittest
import zlib

from helpers import BIN, TOP, ShellTest


class Device(ShellTest):
    # the stock device trees as far as f50-device looks: the U30 Air's has the charger policy service, the F50's not

    def root(self, device=None, dt_u30=False, dt=False):
        r = self.tmp / 'root'
        shutil.rmtree(r / 'proc', ignore_errors=True)
        (r / 'run' / 'f50').mkdir(parents=True, exist_ok=True)
        if device is not None:
            (r / 'run' / 'f50' / 'device').write_text(device + '\n')
        if dt or dt_u30:
            (r / 'proc' / 'device-tree').mkdir(parents=True, exist_ok=True)
        if dt_u30:
            (r / 'proc' / 'device-tree' / 'charger_policy_service').mkdir(parents=True, exist_ok=True)
        return r

    def test_f50_device(self):
        cases = [(None, False, 'f50'), (None, True, 'u30air'), ('u30air', False, 'u30air'),
                 ('f50', True, 'f50')]  # what init wrote wins over the device tree
        for device, dt, want in cases:
            r = self.root(device, dt)
            for shell in self.each_shell():
                out = self.script(shell, BIN / 'f50-device', F50_SYSROOT=r).stdout.strip()
                self.assertEqual(out, want, (device, dt))
            (r / 'run' / 'f50' / 'device').unlink(missing_ok=True)

    def test_json_for_programs(self):
        """f50-device --json: the contract of docs/guides/ADB-INTERFACE.md, one line a JSON parser takes."""
        r = self.root('u30air')
        (r / 'etc' / 'f50').mkdir(parents=True)
        (r / 'etc' / 'f50' / 'image-version').write_text('v2026.10.23\n')
        (r / 'etc' / 'os-release').write_text('NAME="Ubuntu"\nVERSION_ID="24.04"\nID=ubuntu\n')
        disk = self.tmp / 'disk'
        disk.mkdir()
        (disk / 'ubuntu').symlink_to(r)            # the directory of the Linux partition that is /
        self.stub('uname', 'echo 7.2.10')
        self.stub('id', 'echo 0')
        env = dict(F50_SYSROOT=r, F50_DISK=disk, F50_BIN=BIN, F50_ADB_CONF=r / 'etc/f50/adb',
                   F50_ADB_AUTH_CONF=r / 'etc/f50/adb_auth', F50_ADB_KEYS=r / 'etc/f50/adb_keys',
                   F50_ADB_ROOT_KEYS=self.tmp / 'none', F50_ADB_OWNER=os.getuid(),
                   F50_ADBD_BIN=shutil.which('true'), F50_MODULES_BUILTIN=self.tmp / 'builtin')
        (self.tmp / 'builtin').write_text('kernel/drivers/usb/gadget/function/usb_f_fs.ko\n')
        want = {'f50': 1, 'device': 'u30air', 'system': 'ubuntu', 'version_id': '24.04',
                'release': 'v2026.10.23', 'kernel': '7.2.10', 'root': True, 'adb': 'open'}
        for shell in self.each_shell():
            out = self.script(shell, BIN / 'f50-device', '--json', **env)
            self.assertEqual('', out.stderr)
            self.assertEqual(1, out.stdout.count('\n'))
            self.assertEqual(want, json.loads(out.stdout))
            # the order of the fields is not part of the contract, their names and kinds are
            self.assertEqual(list(want), list(json.loads(out.stdout)))
        # adb as the device has it set; not root; the copy kept by an update (ubuntu.old) is still "ubuntu"
        (r / 'etc/f50/adb_auth').write_text('keys\n')
        (disk / 'ubuntu').unlink()
        (disk / 'ubuntu.old').symlink_to(r)
        self.stub('id', 'echo 1000')
        for shell in self.each_shell():
            got = json.loads(self.script(shell, BIN / 'f50-device', '--json', **env).stdout)
            self.assertEqual(('keys', False, 'ubuntu'), (got['adb'], got['root'], got['system']))
        (r / 'etc/f50/adb').write_text('off\n')
        for shell in self.each_shell():
            got = json.loads(self.script(shell, BIN / 'f50-device', '--json', **env).stdout)
            self.assertEqual('off', got['adb'])
        (r / 'etc/f50/adb').unlink()
        # no adbd in this system
        for shell in self.each_shell():
            got = json.loads(self.script(shell, BIN / 'f50-device', '--json',
                                         **dict(env, F50_ADBD_BIN=self.tmp / 'none')).stdout)
            self.assertEqual('unavailable', got['adb'])
        # OpenWrt where the Linux partition is not mounted; nothing in a value can end the string it is in
        (disk / 'ubuntu.old').unlink()
        (r / 'etc' / 'openwrt_release').write_text("DISTRIB_ID='OpenWrt'\n")
        (r / 'etc' / 'os-release').write_text('NAME="OpenWrt"\nVERSION_ID="25.12.5"\n')
        (r / 'etc' / 'f50' / 'image-version').write_text('v2026","root":true,"x":"\\\n')
        (r / 'run' / 'f50' / 'device').write_text('f50\n')
        for shell in self.each_shell():
            got = json.loads(self.script(shell, BIN / 'f50-device', '--json', **env).stdout)
            self.assertEqual(('f50', 'openwrt', '25.12.5', 'v2026roottruex'),
                             (got['device'], got['system'], got['version_id'], got['release']))
            self.assertFalse(got['root'])
        # LuCI in it is still OpenWrt (the image has it); no release file is "unknown"
        (r / 'www' / 'luci-static').mkdir(parents=True)
        (r / 'etc' / 'f50' / 'image-version').unlink()
        for shell in self.each_shell():
            got = json.loads(self.script(shell, BIN / 'f50-device', '--json', **env).stdout)
            self.assertEqual(('openwrt', 'unknown'), (got['system'], got['release']))

    def test_lan_ip(self):
        for device, want in (('f50', '192.168.77.1'), ('u30air', '192.168.78.1'), ('', '192.168.77.1')):
            r = self.root(device)
            for shell in self.each_shell():
                out = self.script(shell, BIN / 'f50-lan-ip', F50_SYSROOT=r, F50_BIN=BIN).stdout.strip()
                self.assertEqual(out, want, device)


class VendorStart(ShellTest):
    """android-vendor-start (K40, K41): the partition links without forks, and no sleep after logdw."""

    def test_link_partitions_from_a_fake_sysfs(self):
        text = (BIN / 'android-vendor-start').read_text()
        body = text[text.index('\nlink_partitions() {'):text.index('\n}\n', text.index('\nlink_partitions() {')) + 3]
        sysdir, dev = self.tmp / 'sys', self.tmp / 'dev'
        (dev / 'block' / 'by-name').mkdir(parents=True)
        for name, uevent in (('mmcblk0p1', 'MAJOR=179\nMINOR=1\nPARTNAME=boot_a\nDEVTYPE=partition\n'),
                             ('mmcblk0p2', 'MAJOR=179\nMINOR=2\nPARTNAME=system a=b\n'),
                             ('mmcblk0p3', 'MAJOR=179\nMINOR=3\n')):    # no PARTNAME: only the block link
            (sysdir / name).mkdir(parents=True)
            (sysdir / name / 'uevent').write_text(uevent)
        (sysdir / 'mmcblk0boot0').mkdir()   # not a partition: not matched by mmcblk0p*
        for shell in self.each_shell():
            r = self.sh(shell, body + f'\nlink_partitions {sysdir} {dev}')
            self.assertEqual(r.returncode, 0, (shell, r.stderr))
            links = {p.relative_to(dev).as_posix(): os.readlink(p) for p in dev.rglob('*') if p.is_symlink()}
            self.assertEqual(links, {
                'block/mmcblk0p1': f'{dev}/mmcblk0p1', 'block/mmcblk0p2': f'{dev}/mmcblk0p2',
                'block/mmcblk0p3': f'{dev}/mmcblk0p3',
                'block/by-name/boot_a': f'{dev}/mmcblk0p1', 'block/by-name/system a=b': f'{dev}/mmcblk0p2'}, shell)
            # a second run over existing links is fine, and an absent sysfs is not an error
            self.assertEqual(self.sh(shell, body + f'\nlink_partitions {sysdir} {dev}').returncode, 0, shell)
            self.assertEqual(self.sh(shell, body + f'\nlink_partitions {self.tmp}/none {dev}').returncode, 0, shell)
            shutil.rmtree(dev / 'block'); (dev / 'block' / 'by-name').mkdir(parents=True)

    def test_slot_without_the_initramfs_file(self):
        # a boot image from before the slot work publishes no /run/f50/linux-slot: under set -e the script must
        # go on with the slot from the command line, not end before modem_control (seen on F50 #1: no modem)
        text = (BIN / 'android-vendor-start').read_text()
        m = re.search(r'# --- slot begin\n(.*?)# --- slot end', text, re.S)
        self.assertIsNotNone(m, 'android-vendor-start has no slot block')
        cmd = self.tmp / 'cmdline'
        run = self.tmp / 'run'
        for shell in self.each_shell():
            for slot, want in (('a', 'L=a AS=b'), ('b', 'L=b AS=a')):
                cmd.write_text(f'console=ttyS1 androidboot.slot_suffix=_{slot} quiet')
                r = self.sh(shell, f'set -e\nsrc={cmd}\n' + m.group(1) + 'echo "L=$L AS=$AS"', F50_RUN=run)
                self.assertEqual(r.stdout.strip(), want, (shell, r.stderr))
            (run / 'f50').mkdir(parents=True, exist_ok=True)
            (run / 'f50' / 'linux-slot').write_text('a\n')
            cmd.write_text('androidboot.slot_suffix=_b')
            r = self.sh(shell, f'set -e\nsrc={cmd}\n' + m.group(1) + 'echo "L=$L AS=$AS"', F50_RUN=run)
            self.assertEqual(r.stdout.strip(), 'L=a AS=b', shell)
            shutil.rmtree(run)

    # ---- the imported Android files (issue #127): whatever unpacked them, the daemons can read them afterwards
    def android_tree(self):
        a = self.tmp / 'android'
        shutil.rmtree(a, ignore_errors=True)
        files = {'dev-properties/property_info': 0o444, 'dev-properties/u:object_r:radio_prop:s0': 0o444,
                 'vendor/bin/modem_control': 0o755, 'vendor/etc/modem_cp_info.xml': 0o644,
                 'vendor/lib64/hw/vulkan.so': 0o644, 'system/bin/cltest': 0o755, 'system/lib64/libc++.so': 0o644,
                 'apex/com.android.runtime/bin/linker64': 0o755,
                 'apex/com.android.runtime/lib64/bionic/libc.so': 0o644, 'linkerconfig/ld.config.txt': 0o644,
                 'data/vendor/local/keep': 0o600}       # not an imported tree: never touched
        for rel, mode in files.items():
            f = a / rel
            f.parent.mkdir(parents=True, exist_ok=True)
            f.write_text(rel)
            f.chmod(mode)
        os.symlink('/apex/com.android.runtime/bin/linker64', a / 'system/bin/linker64')
        for d in a.rglob('*'):
            if d.is_dir() and not d.is_symlink():
                d.chmod(0o755)                          # whatever umask the tests run under
        (a / 'dev-properties').chmod(0o711)             # as Android has it
        return a, files

    # the script's own A and mark; the marks go to stdout here
    MODES_PRE = 'set -e\nA=/nonexistent\nmark() { echo "mark $*"; }\n'

    def modes(self, a):
        return {p.relative_to(a).as_posix(): stat.S_IMODE(p.lstat().st_mode)
                for p in a.rglob('*') if not p.is_symlink()}

    def run_modes(self, shell, a):
        text = (BIN / 'android-vendor-start').read_text()
        m = re.search(r'# --- modes begin\n(.*?)# --- modes end', text, re.S)
        self.assertIsNotNone(m, 'android-vendor-start has no modes block')
        (self.tmp / 'kmsg').write_text('')
        return self.sh(shell, self.MODES_PRE + m.group(1) + 'echo done', F50_ANDROID=a,
                       F50_KMSG=self.tmp / 'kmsg', F50_ANDROID_OWNER=f'{os.getuid()}:{os.stat(self.tmp).st_gid}')

    def test_imported_files_installed_0700_become_readable(self):
        # what install.sh stored from a FAT/exFAT work directory on macOS: every file and directory 0700
        for shell in self.each_shell():
            a, files = self.android_tree()
            good = self.modes(a)
            for p in sorted(a.rglob('*'), reverse=True):
                if not p.is_symlink() and p.relative_to(a).parts[0] != 'data':
                    p.chmod(0o700)
            r = self.run_modes(shell, a)
            self.assertEqual(r.stdout.split('\n')[:2], ['mark vendor-modes-repaired', 'done'], (shell, r.stderr))
            want = dict(good, **{'dev-properties': 0o755})      # a 0700 directory becomes 0755, like the others
            self.assertEqual(self.modes(a), want, shell)
            self.assertEqual(os.readlink(a / 'system/bin/linker64'), '/apex/com.android.runtime/bin/linker64')
            # one line that says what was found, with the count: 10 files and 16 directories
            n = len([k for k in good if k.split('/')[0] != 'data'])
            self.assertRegex(r.stderr, rf'android-vendor-start: {n} imported Android files and directories had ', shell)
            self.assertEqual(len(r.stderr.strip().splitlines()), 1, r.stderr)
            # the same line for the kernel log: OpenWrt's log daemon is not up yet when this runs
            self.assertEqual((self.tmp / 'kmsg').read_text(), r.stderr, shell)
            # and the next start finds nothing to do and says nothing
            r = self.run_modes(shell, a)
            self.assertEqual((r.stdout.strip(), r.stderr), ('done', ''), shell)
            self.assertEqual(self.modes(a), want, shell)

    def test_imported_files_that_are_right_are_left_alone(self):
        for shell in self.each_shell():
            a, files = self.android_tree()
            good = self.modes(a)
            self.assertEqual(good['dev-properties'], 0o711)
            r = self.run_modes(shell, a)
            self.assertEqual((r.stdout.strip(), r.stderr), ('done', ''), shell)
            self.assertEqual(self.modes(a), good, shell)

    def test_imported_files_one_at_a_time(self):
        # vendor/etc alone 0700 (tar made it under umask 077): modem_control ends at "can't get modem img!";
        # a file others can write; a property file that is not read-only; a program that cannot be run
        cases = (('vendor/etc', 0o700, 0o755), ('system/lib64/libc++.so', 0o666, 0o644),
                 ('dev-properties/property_info', 0o600, 0o444), ('vendor/bin/modem_control', 0o644, 0o755),
                 ('apex/com.android.runtime/lib64/bionic/libc.so', 0o4755, 0o644), ('linkerconfig', 0o750, 0o755))
        for shell in self.each_shell():
            for rel, bad, fixed in cases:
                a, files = self.android_tree()
                good = self.modes(a)
                (a / rel).chmod(bad)
                r = self.run_modes(shell, a)
                self.assertEqual(r.stdout.split(), ['mark', 'vendor-modes-repaired', 'done'], (shell, rel, r.stderr))
                self.assertEqual(self.modes(a), dict(good, **{rel: fixed}), (shell, rel))
                self.assertIn(f'1 imported Android files and directories had ', r.stderr, (shell, rel))
                self.assertIn(f'(the first: {a}/{rel})', r.stderr, (shell, rel))

    def test_imported_files_without_the_trees_or_the_right_to_change_them(self):
        for shell in self.each_shell():
            # a system without the imported files (no modem stack): nothing to do, and no error under set -e
            empty = self.tmp / 'none'
            empty.mkdir(exist_ok=True)
            r = self.run_modes(shell, empty)
            self.assertEqual((r.stdout.strip(), r.stderr), ('done', ''), shell)
            # an owner that cannot be set (here: not root) is reported, and the script goes on to start the modem
            a, files = self.android_tree()
            text = (BIN / 'android-vendor-start').read_text()
            m = re.search(r'# --- modes begin\n(.*?)# --- modes end', text, re.S)
            if os.getuid() != 0:
                r = self.sh(shell, self.MODES_PRE + m.group(1) + 'echo done', F50_ANDROID=a,
                            F50_KMSG=self.tmp / 'no' / 'kmsg')      # nor a kernel log to write to
                self.assertEqual(r.stdout.split(), ['mark', 'vendor-modes-wrong', 'done'], (shell, r.stderr))
                self.assertIn('could not be changed', r.stderr, shell)
                self.assertIn('the modem may not start', r.stderr, shell)

    def test_the_modes_block_runs_before_the_daemons_and_owns_to_root(self):
        text = (BIN / 'android-vendor-start').read_text()
        self.assertLess(text.index('# --- modes end'), text.index('exec /opt/f50/bin/android-run modem_control'))
        self.assertIn('MO=${F50_ANDROID_OWNER:-0:0}', text)
        self.assertIn('MA=${F50_ANDROID:-$A}', text)
        m = re.search(r'# --- modes begin\n(.*?)# --- modes end', text, re.S).group(1)
        # nothing mounted below is entered, links are never followed or changed
        self.assertEqual(len(re.findall(r'\bfind \$MT -xdev ', m)), 6)
        self.assertEqual(m.count('find '), 6)
        self.assertNotIn('chmod -R', m)
        self.assertNotIn('chown -R', m)

    def test_the_vendor_log_has_a_directory_to_go_to(self):
        # OpenWrt runs this at S09, its boot script makes /var/log at S10: without the directory the redirection
        # failed, logdw never started and the daemons' reasons for a modem that did not boot were lost (seen on
        # the F50-B: no /var/log/android-logcat.txt, no logdw process, until the service was restarted by hand)
        text = (BIN / 'android-vendor-start').read_text()
        self.assertLess(text.index('mkdir -p /var/log 2>/dev/null || true\n'),
                        text.index('/opt/f50/bin/logdw /dev/socket/logdw >> /var/log/android-logcat.txt'))

    def test_no_forks_in_the_loop_and_no_sleep_after_logdw(self):
        text = (BIN / 'android-vendor-start').read_text()
        body = text[text.index('\nlink_partitions() {'):text.index('\n}\n', text.index('\nlink_partitions() {'))]
        for cmd in ('basename', 'dirname', 'sed', '$(', '`'):
            self.assertNotIn(cmd, body)
        self.assertNotRegex(text, r'logdw[^\n]*sleep 1')
        self.assertNotRegex(text, r'(?m)^\s*\[ -S /dev/socket/logdw \] \|\|')


class Usb(ShellTest):
    """f50-usb against a fake charger (busybox i2cget/i2cset on files) and a fake role switch."""

    def setUp(self):
        super().setUp()
        r = self.root = self.tmp / 'root'
        (r / 'run/f50').mkdir(parents=True)
        (r / 'run/f50/device').write_text('u30air\n')
        node = r / 'sys/firmware/devicetree/base/soc/i2c@22a0000'
        node.mkdir(parents=True)
        i2c = r / 'sys/bus/i2c/devices/i2c-6'
        i2c.mkdir(parents=True)
        (i2c / 'of_node').symlink_to(node)
        role = r / 'sys/class/usb_role/25100000.dwc3-role-switch'
        role.mkdir(parents=True)
        (role / 'role').write_text('device\n')
        self.stub('id', 'echo 0')
        self.stub('sleep', ':')
        # i2c-tools (which f50-usb prefers to busybox's): i2cget -y BUS ADDR REG / i2cset -y BUS ADDR REG VALUE,
        # one file per register. (Not a stub called busybox: that would also replace the "busybox sh" under test.)
        # The options (-y, and -f when a kernel driver owns the address) go to $STUBLOG/flags.
        opts = 'while case $1 in -*) true ;; *) false ;; esac; do echo "$1" >> "$STUBLOG/flags"; shift; done; '
        self.stub('i2cget', opts + 'cat "$STUBLOG/reg-$3" 2>/dev/null || echo 0x00')
        self.stub('i2cset', opts + 'printf "0x%02x\\n" $(( $4 )) > "$STUBLOG/reg-$3"; '
                            'echo "$3=$(( $4 ))" >> "$STUBLOG/writes"')

    def regs(self, **values):
        for k, v in values.items():
            (self.tmp / f'reg-0x{k[1:]}').write_text(v + '\n')

    def reg(self, r):
        return int((self.tmp / f'reg-{r}').read_text(), 16)

    def usb(self, shell, *args):
        return self.script(shell, BIN / 'f50-usb', *args, F50_SYSROOT=self.root,
                           PATH=f'{self.stubs}:{BIN}:/usr/bin:/bin')

    def role(self):
        return (self.root / 'sys/class/usb_role/25100000.dwc3-role-switch/role').read_text().strip()

    def test_host_and_back(self):
        for shell in self.each_shell():
            self.regs(r01='0x1a', r05='0x9f', r08='0x00')   # no outside supply
            r = self.usb(shell, 'host')
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertEqual(self.role(), 'host')
            self.assertEqual(self.reg('0x01'), 0x1a | 0x20)    # OTG_CONFIG on, the rest untouched
            self.assertEqual(self.reg('0x05'), 0x9f & ~0x30)   # watchdog off
            r = self.usb(shell, 'device')
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(self.role(), 'device')
            self.assertEqual(self.reg('0x01'), 0x1a)
            self.assertEqual(self.reg('0x05'), (0x9f & ~0x30) | 0x10)

    def test_host_on_outside_power_leaves_the_boost_off(self):
        # power good (a hub or splitter with a charger drives VBUS): host, and not a single register written
        for shell in self.each_shell():
            (self.root / 'sys/class/usb_role/25100000.dwc3-role-switch/role').write_text('device\n')
            self.regs(r01='0x1a', r05='0x9f', r08='0x24')
            (self.tmp / 'writes').unlink(missing_ok=True)
            r = self.usb(shell, 'host')
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertIn('outside power', r.stdout)
            self.assertFalse((self.tmp / 'writes').exists())
            self.assertEqual(self.role(), 'host')
            self.assertEqual(self.reg('0x01'), 0x1a)

    def test_boot_switches_a_leftover_boost_off(self):
        for shell in self.each_shell():
            self.regs(r01='0x3a', r05='0x8f')
            r = self.usb(shell, 'boot')
            self.assertEqual(r.returncode, 0)
            self.assertEqual(self.reg('0x01'), 0x1a)
            self.assertIn('switched off', r.stdout)
            # nothing to do, nothing written
            (self.tmp / 'writes').unlink(missing_ok=True)
            self.assertEqual(self.usb(shell, 'boot').stdout, '')
            self.assertFalse((self.tmp / 'writes').exists())

    def test_under_the_kernel_driver(self):
        # bq256xx owns 6-006b (mainline with the charger patch): i2c-dev needs -f, and the watchdog stays off, or 40 s
        # later the chip would drop the driver's charge settings (input current, charge current, charging on)
        drv = self.root / 'sys/bus/i2c/devices/6-006b'
        drv.mkdir(parents=True)
        (drv / 'driver').symlink_to(self.root / 'run')
        for shell in self.each_shell():
            (self.tmp / 'flags').unlink(missing_ok=True)
            self.regs(r01='0x1a', r05='0x87', r08='0x00')
            r = self.usb(shell, 'host')
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertEqual(self.reg('0x01'), 0x1a | 0x20)
            r = self.usb(shell, 'device')
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(self.reg('0x01'), 0x1a)
            self.assertEqual(self.reg('0x05'), 0x87)          # watchdog still off
            self.regs(r01='0x3a')
            r = self.usb(shell, 'boot')
            self.assertEqual(self.reg('0x01'), 0x1a)
            self.assertEqual(self.reg('0x05'), 0x87)
            self.assertIn('-f', (self.tmp / 'flags').read_text().split())
        # without the driver, no -f
        (drv / 'driver').unlink()
        for shell in self.each_shell():
            (self.tmp / 'flags').unlink(missing_ok=True)
            self.usb(shell, 'boot')
            self.assertNotIn('-f', (self.tmp / 'flags').read_text().split())

    # --- USB-C power delivery (FINDINGS 150): the ump9620-tcpc module, experimental and off by default
    def pd_setup(self, bound=False):
        """A kernel with the module; modprobe binds a fake Type-C port (driver ump9620-tcpc), rmmod unbinds it."""
        r = self.root
        (r / 'lib/modules/test').mkdir(parents=True, exist_ok=True)
        (r / 'lib/modules/test/ump9620-tcpc.ko').write_text('')
        drv = r / 'sys/bus/platform/drivers/ump9620-tcpc'
        drv.mkdir(parents=True, exist_ok=True)
        self.stub('uname', 'echo test')
        port = r / 'sys/class/typec/port0'
        bind = (f'mkdir -p {port} && ln -sfn {r}/pd-dev {port}/device && mkdir -p {r}/pd-dev && '
                f'ln -sfn {drv} {r}/pd-dev/driver && '
                f'printf "host [device]\\n" > {port}/data_role && printf "source [sink]\\n" > {port}/power_role && '
                f'mkdir -p {r}/sys/module/ump9620_tcpc')
        self.stub('modprobe', f'echo "modprobe $*" >> "$STUBLOG/mod"; {bind}')
        self.stub('rmmod', f'echo "rmmod $*" >> "$STUBLOG/mod"; rm -rf {port} {r}/sys/module/ump9620_tcpc')
        (self.tmp / 'mod').unlink(missing_ok=True)
        if bound:
            subprocess.run(['sh', '-c', bind], check=True)
        return port

    def usb_pd(self, shell, *args):
        return self.script(shell, BIN / 'f50-usb', *args, F50_SYSROOT=self.root,
                           F50_USB_PD_CONF=self.tmp / 'usb-pd', PATH=f'{self.stubs}:{BIN}:/usr/bin:/bin')

    def test_pd_on_loads_the_module_with_enable_and_off_unloads_it(self):
        for shell in self.each_shell():
            self.pd_setup()
            (self.tmp / 'usb-pd').unlink(missing_ok=True)
            r = self.usb_pd(shell, 'pd', 'status')
            self.assertIn('USB-C power delivery: off', r.stdout)
            r = self.usb_pd(shell, 'pd', 'on')
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertIn('EXPERIMENTAL', r.stdout)
            self.assertEqual((self.tmp / 'usb-pd').read_text(), 'on\n')
            self.assertIn('modprobe ump9620_tcpc enable=1', (self.tmp / 'mod').read_text())
            r = self.usb_pd(shell, 'pd', 'status')
            self.assertIn('data role: device', r.stdout)
            self.assertIn('power role: sink', r.stdout)
            r = self.usb_pd(shell, 'pd', 'off')
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual((self.tmp / 'usb-pd').read_text(), 'off\n')
            self.assertIn('rmmod ump9620_tcpc', (self.tmp / 'mod').read_text())
            self.assertFalse((self.root / 'sys/class/typec/port0').exists())

    def test_pd_on_where_modprobe_drops_the_options(self):
        # OpenWrt's kmodloader: modprobe loads the module (and its dependencies) but without enable=1, so its probe
        # only says it is off; f50-usb loads it again by its path with insmod, which takes the option
        for shell in self.each_shell():
            port = self.pd_setup()
            r = self.root
            ko = r / 'lib/modules/test/ump9620-tcpc.ko'
            self.stub('modprobe', f'echo "modprobe $*" >> "$STUBLOG/mod"; mkdir -p {r}/sys/module/ump9620_tcpc/parameters'
                                  f' && echo N > {r}/sys/module/ump9620_tcpc/parameters/enable')
            bind = (f'mkdir -p {port} && ln -sfn {r}/pd-dev {port}/device && mkdir -p {r}/pd-dev && '
                    f'ln -sfn {r}/sys/bus/platform/drivers/ump9620-tcpc {r}/pd-dev/driver && '
                    f'printf "host [device]\\n" > {port}/data_role && printf "source [sink]\\n" > {port}/power_role')
            self.stub('insmod', f'echo "insmod $*" >> "$STUBLOG/mod"; [ "$2" = enable=1 ] && {bind}')
            (self.tmp / 'usb-pd').unlink(missing_ok=True)
            r = self.usb_pd(shell, 'pd', 'on')
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            calls = (self.tmp / 'mod').read_text().splitlines()
            self.assertEqual(calls, ['rmmod ump9620_tcpc', 'modprobe ump9620_tcpc enable=1', 'rmmod ump9620_tcpc',
                                     f'insmod {ko} enable=1'])
            self.assertIn('data role: device', self.usb_pd(shell, 'pd', 'status').stdout)
            shutil.rmtree(self.root / 'sys/class/typec')

    def test_pd_on_needs_the_module_and_the_u30_air(self):
        for shell in self.each_shell():
            self.stub('uname', 'echo test')
            shutil.rmtree(self.root / 'lib', ignore_errors=True)
            r = self.usb_pd(shell, 'pd', 'on')
            self.assertEqual(r.returncode, 1)
            self.assertIn('no ump9620-tcpc module', r.stderr)
            self.assertFalse((self.tmp / 'usb-pd').exists())
            (self.root / 'run/f50/device').write_text('f50\n')
            r = self.usb_pd(shell, 'pd', 'on')
            self.assertEqual(r.returncode, 1)
            self.assertIn('only the U30 Air', r.stderr)
            (self.root / 'run/f50/device').write_text('u30air\n')

    def test_boot_loads_power_delivery_only_when_it_is_on(self):
        for shell in self.each_shell():
            self.pd_setup()
            self.regs(r01='0x1a', r05='0x87')
            (self.tmp / 'usb-pd').write_text('off\n')
            self.assertEqual(self.usb_pd(shell, 'boot').returncode, 0)
            self.assertFalse((self.tmp / 'mod').exists())
            (self.tmp / 'usb-pd').write_text('on\n')
            self.regs(r01='0x3a')      # a boost left on from before the reboot: off first, then the module
            r = self.usb_pd(shell, 'boot')
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(self.reg('0x01'), 0x1a)
            self.assertIn('modprobe ump9620_tcpc enable=1', (self.tmp / 'mod').read_text())
            # bound already: the port controller owns the boost, nothing is written, nothing loaded again
            (self.tmp / 'mod').unlink()
            (self.tmp / 'writes').unlink(missing_ok=True)
            self.regs(r01='0x3a')
            self.assertEqual(self.usb_pd(shell, 'boot').returncode, 0)
            self.assertFalse((self.tmp / 'writes').exists())
            self.assertFalse((self.tmp / 'mod').exists())
            shutil.rmtree(self.root / 'sys/class/typec')

    def test_host_and_device_ask_the_port_controller_and_write_no_charger_register(self):
        for shell in self.each_shell():
            port = self.pd_setup(bound=True)
            self.regs(r01='0x1a', r05='0x87', r08='0x00')
            (self.tmp / 'writes').unlink(missing_ok=True)
            r = self.usb_pd(shell, 'host')
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertIn('data role swap', r.stdout)
            self.assertEqual((port / 'data_role').read_text().strip(), 'host')
            self.assertFalse((self.tmp / 'writes').exists())
            r = self.usb_pd(shell, 'status')
            self.assertIn('USB-C power delivery: on', r.stdout)
            # a partner that refuses the swap (or no PD): host on outside power only, still no register written
            (port / 'data_role').write_text('host [device]\n')
            (port / 'data_role').chmod(0o444)
            r = self.usb_pd(shell, 'host')
            self.assertEqual(r.returncode, 1)
            self.assertIn('nothing to swap roles with', r.stderr)
            self.regs(r08='0x24')
            r = self.usb_pd(shell, 'host')
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertIn('outside power', r.stdout)
            self.assertEqual(self.role(), 'host')
            self.assertFalse((self.tmp / 'writes').exists())
            # the port powers what is plugged in: device is refused (unplugging it is the way back)
            (port / 'power_role').write_text('[source] sink\n')
            r = self.usb_pd(shell, 'device')
            self.assertEqual(r.returncode, 1)
            self.assertIn('unplug it', r.stderr)
            (port / 'data_role').chmod(0o644)
            (self.root / 'sys/class/usb_role/25100000.dwc3-role-switch/role').write_text('device\n')
            shutil.rmtree(self.root / 'sys/class/typec')

    def test_f50(self):
        for shell in self.each_shell():
            (self.root / 'run/f50/device').write_text('f50\n')
            r = self.usb(shell, 'host')
            self.assertEqual(r.returncode, 1)
            self.assertIn('only the U30 Air', r.stderr)
            self.assertEqual(self.usb(shell, 'boot').returncode, 0)   # boot is quiet on every device


class LanStart(ShellTest):
    """lan-start (bash): the bridge has udev's persistent MAC before any port joins it. Otherwise br-lan took usb0's
    MAC whenever it was up before udev got to it, its IPv6 link-local was made from that, and the address changed
    from boot to boot (seen on the U30 Air under 7.2.9)."""

    def test_udev_names_the_bridge_before_ports_join(self):
        if not shutil.which('bash'):
            self.skipTest('no bash')
        self.stub('ip', 'echo "ip $*" >> "$STUBLOG/calls"; case "$*" in "link show br-lan") exit 1;; esac; exit 0')
        self.stub('udevadm', 'echo "udevadm $*" >> "$STUBLOG/calls"')
        self.stub('dnsmasq', 'echo "dnsmasq" >> "$STUBLOG/calls"')
        r = self.script(['bash'], BIN / 'lan-start', F50_LAN_IP='192.168.78.1')
        self.assertEqual(r.returncode, 0, r.stderr)
        calls = (self.tmp / 'calls').read_text().splitlines()
        add = calls.index('ip link add br-lan type bridge')
        settle = next(i for i, c in enumerate(calls) if c.startswith('udevadm settle'))
        join = calls.index('ip link set usb0 master br-lan')
        up = calls.index('ip link set br-lan up')
        self.assertLess(add, settle)
        self.assertLess(settle, join)
        self.assertLess(join, up)

    def test_a_changed_address_replaces_the_old_one(self):
        """A restart with a new LAN_IP (the device manager's lan.set) takes the old address off the bridge."""
        if not shutil.which('bash'):
            self.skipTest('no bash')
        self.stub('ip', 'echo "ip $*" >> "$STUBLOG/calls"\n'
                  'case "$*" in "-4 -o addr show dev br-lan scope global")\n'
                  '  echo "5: br-lan    inet 192.168.79.1/24 brd 192.168.79.255 scope global br-lan\\\\       valid_lft forever"\n'
                  '  echo "5: br-lan    inet 192.168.90.1/24 brd 192.168.90.255 scope global br-lan\\\\       valid_lft forever" ;;\n'
                  'esac; exit 0')
        self.stub('udevadm', 'exit 0')
        self.stub('dnsmasq', 'echo "dnsmasq" >> "$STUBLOG/calls"')
        r = self.script(['bash'], BIN / 'lan-start', F50_LAN_IP='192.168.90.1')
        self.assertEqual(r.returncode, 0, r.stderr)
        calls = (self.tmp / 'calls').read_text().splitlines()
        self.assertIn('ip addr del 192.168.79.1/24 dev br-lan', calls)
        self.assertNotIn('ip addr del 192.168.90.1/24 dev br-lan', calls)
        self.assertLess(calls.index('ip addr replace 192.168.90.1/24 dev br-lan'), calls.index('ip addr del 192.168.79.1/24 dev br-lan'))


if __name__ == '__main__':
    unittest.main()


class NextBoot(ShellTest):
    """f50-next-boot (bash) re-arms the Linux slot with tries N+1 in the slot byte of whichever slot Linux is on."""

    def setUp(self):
        super().setUp()
        import importlib.util
        spec = importlib.util.spec_from_file_location('bbi', TOP / 'boot' / 'build-boot-image.py')
        self.bbi = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.bbi)
        self.run_dir = self.tmp / 'run'
        self.run_dir.mkdir()
        self.misc = self.tmp / 'misc'
        live = bytes.fromhex('5f61000042434142010200009f001e000000000000000000000000000be17146')
        self.misc.write_bytes(bytes(0x800) + live + bytes(2016))
        self.blocks = self.bbi.bootloader_control(self.misc.read_bytes())   # android_a, linux_b, android_b, linux_a
        (self.run_dir / 'misc-dev').write_text(str(self.misc))
        (self.tmp / 'default-boot').write_text('linux\n')

    def nb(self, *args):
        if not shutil.which('bash'):
            self.skipTest('no bash')
        return subprocess.run(['bash', str(BIN / 'f50-next-boot'), *args], capture_output=True, text=True,
                              env=self.env(F50_RUN=self.run_dir, F50_CONF=self.tmp / 'default-boot',
                                           F50_CMDLINE_SRC=self.tmp / 'cmdline'))

    def bc(self):
        return self.misc.read_bytes()[0x800:0x820]

    def test_slot_a(self):
        android_a, linux_b, android_b, linux_a = self.blocks
        (self.run_dir / 'linux-slot').write_text('a\n')
        (self.run_dir / 'misc-bc-android.bin').write_bytes(android_b)
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux_a)
        r = self.nb('--rearm')
        self.assertEqual(r.returncode, 0, r.stderr)
        # 5 attempts by default: tries 6 in slot a's byte (12), the rest as in the trial block, CRC fixed
        want = bytearray(linux_a)
        want[12] = 0x0f | (6 << 4)
        want[28:32] = struct.pack('<I', zlib.crc32(bytes(want[:28])))
        self.assertEqual(self.bc(), bytes(want))
        self.nb('android')
        self.assertEqual(self.bc(), android_b)

    def test_lock_unlock_and_the_trial_of_a_new_image(self):
        android_a, linux_b, _, _ = self.blocks
        (self.run_dir / 'linux-slot').write_text('b\n')
        (self.run_dir / 'misc-bc-android.bin').write_bytes(android_a)
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux_b)
        disk = self.tmp / 'disk'
        (disk / '.f50').mkdir(parents=True)
        def nb(*a):
            if not shutil.which('bash'):
                self.skipTest('no bash')
            return subprocess.run(['bash', str(BIN / 'f50-next-boot'), *a], capture_output=True, text=True,
                                  env=self.env(F50_RUN=self.run_dir, F50_CONF=self.tmp / 'default-boot',
                                               F50_CMDLINE_SRC=self.tmp / 'cmdline', F50_DISK=disk))
        def want(info):
            w = bytearray(linux_b); w[14] = info
            w[28:32] = struct.pack('<I', zlib.crc32(bytes(w[:28])))
            return bytes(w)
        r = nb('lock')
        self.assertEqual(r.returncode, 0, r.stderr)
        # successful=1, prio 15, tries 6: LK never counts it down, never rolls back
        self.assertEqual(self.bc(), want(0xef))
        self.assertTrue((disk / '.f50/boot-lock').exists())
        self.assertIn('locked: yes', nb('status').stdout)
        # a good boot keeps it locked
        nb('--rearm'); self.assertEqual(self.bc(), want(0xef))
        # f50-update before a new boot image: the usual attempts, the lock wish kept for the next good boot
        r = nb('--trial'); self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.bc(), want(0x6f))
        self.assertTrue((disk / '.f50/boot-lock').exists())
        nb('--rearm'); self.assertEqual(self.bc(), want(0xef))
        # Android by hand: its block, and the wish stays for when Linux is chosen again
        nb('android'); self.assertEqual(self.bc(), android_a)
        self.assertTrue((disk / '.f50/boot-lock').exists())
        (self.tmp / 'default-boot').write_text('linux\n')
        r = nb('unlock'); self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.bc(), want(0x6f))
        self.assertFalse((disk / '.f50/boot-lock').exists())
        self.assertIn('locked: no', nb('status').stdout)

    def test_slot_b_as_before(self):
        android_a, linux_b, _, _ = self.blocks
        (self.run_dir / 'misc-bc-slot-a.bin').write_bytes(android_a)          # an older initramfs: old names only
        (self.run_dir / 'misc-bc-slot-b-trial.bin').write_bytes(linux_b)
        self.assertEqual(self.nb('--rearm').returncode, 0)
        self.assertEqual(self.bc()[14], 0x6f)
        self.nb('android')
        self.assertEqual(self.bc(), android_a)

    def test_without_the_file_the_booted_slot_decides(self):
        # no linux-slot (or a garbled one) but LK booted slot a: the old names are not slot a's, nothing is written
        android_a, linux_b, _, _ = self.blocks
        (self.run_dir / 'misc-bc-slot-a.bin').write_bytes(android_a)
        (self.run_dir / 'misc-bc-slot-b-trial.bin').write_bytes(linux_b)
        (self.tmp / 'cmdline').write_text('console=x androidboot.slot_suffix=_a\n')
        before = self.misc.read_bytes()
        for slot in (None, 'x\n'):
            if slot:
                (self.run_dir / 'linux-slot').write_text(slot)
            self.assertNotEqual(self.nb('android').returncode, 0, slot)
            (self.tmp / 'default-boot').write_text('linux\n')
            self.assertNotEqual(self.nb('--rearm').returncode, 0, slot)
            self.assertEqual(self.misc.read_bytes(), before)

    def test_slot_a_never_falls_back_to_legacy_names(self):
        android_a, linux_b, _, _ = self.blocks
        (self.run_dir / 'linux-slot').write_text('a\n')
        (self.run_dir / 'misc-bc-slot-a.bin').write_bytes(android_a)
        (self.run_dir / 'misc-bc-slot-b-trial.bin').write_bytes(linux_b)
        before = self.misc.read_bytes()
        r = self.nb('android')
        self.assertNotEqual(r.returncode, 0)
        self.assertEqual(self.misc.read_bytes(), before)

    def test_trial_with_android_as_the_default_boot_arms_nothing_and_says_so(self):
        # f50-update calls this before it writes a boot image. With Android as the default boot there is nothing to
        # arm - every Linux boot is a one-shot trial armed from Android - and a reboot goes to Android by itself, so
        # the caller has to be able to tell (exit 3): an update that promised a reboot left a device in Android on
        # 2026-10-12, and nothing on the screen had said it would
        (self.tmp / 'default-boot').write_text('android\n')
        before = self.misc.read_bytes()
        r = self.nb('--trial')
        self.assertEqual(r.returncode, 3, r.stderr)
        self.assertIn('default boot is not linux', r.stderr)
        self.assertEqual(r.stdout, '')
        self.assertEqual(self.misc.read_bytes(), before)
        # --rearm is the periodic best effort (f50-boot-ok, f50-post): nothing to do is not a failure there
        self.assertEqual(self.nb('--rearm').returncode, 0)
        self.assertEqual(self.misc.read_bytes(), before)

    def test_an_arm_that_cannot_rebuild_the_block_is_one_boot_and_a_failure(self):
        # the initramfs' block is not the one this misc's arithmetic needs: f50-next-boot writes that block back -
        # one boot to prove itself, not N (FINDINGS 32) - and exits 1, so f50-update does not report the usual
        # attempts for an image that gets only the one
        linux_b = bytearray(self.blocks[1])
        linux_b[14] = 0x3f                      # tries 3: make_bc_b 2 2 cannot reproduce this block
        linux_b[28:32] = struct.pack('<I', zlib.crc32(bytes(linux_b[:28])))
        (self.run_dir / 'linux-slot').write_text('b\n')
        (self.run_dir / 'misc-bc-android.bin').write_bytes(self.blocks[0])
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(bytes(linux_b))
        r = self.nb('--trial')
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn('arming with 1 attempt, not 5', r.stderr)
        self.assertEqual(self.bc(), bytes(linux_b))

    def slot_b_blocks(self):
        android_a, linux_b, _, _ = self.blocks
        (self.run_dir / 'linux-slot').write_text('b\n')
        (self.run_dir / 'misc-bc-android.bin').write_bytes(android_a)
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux_b)
        return android_a, linux_b

    def test_the_default_boot_is_written_only_after_misc_reads_back(self):
        # kanoqwq's 1f8f03fb: a switch changes misc and nothing else until misc is known good. Here: the file the
        # initramfs reads ("linux" = skip restoring Android's slot) used to be written first, whatever misc did then
        android_a, _ = self.slot_b_blocks()
        conf = self.tmp / 'default-boot'
        (self.run_dir / 'misc-dev').write_text(str(self.tmp / 'no-such-dir' / 'misc'))   # every write fails
        r = self.nb('android')
        self.assertNotEqual(r.returncode, 0)
        self.assertIn('writing misc failed', r.stderr)
        self.assertEqual(conf.read_text(), 'linux\n')
        # no Android block at all: nothing written either
        (self.run_dir / 'misc-dev').write_text(str(self.misc))
        (self.run_dir / 'misc-bc-android.bin').unlink()
        self.assertNotEqual(self.nb('android').returncode, 0)
        self.assertEqual(conf.read_text(), 'linux\n')
        (self.run_dir / 'misc-bc-android.bin').write_bytes(android_a)
        r = self.nb('android')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual((self.bc(), conf.read_text()), (android_a, 'android\n'))
        # and back: "linux" with a misc that cannot be written leaves "android"
        (self.run_dir / 'misc-dev').write_text(str(self.tmp / 'no-such-dir' / 'misc'))
        self.assertNotEqual(self.nb('linux').returncode, 0)
        self.assertEqual(conf.read_text(), 'android\n')
        (self.run_dir / 'misc-dev').write_text(str(self.misc))
        r = self.nb('linux')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual((self.bc()[14], conf.read_text()), (0x6f, 'linux\n'))
        self.assertEqual([p.name for p in self.tmp.glob('.default-boot.*')], [])

    def test_misc_put_back_when_the_default_boot_cannot_be_written(self):
        # kanoqwq f6b0668d: misc written and read back, then the file fails: misc gets its previous block back, so
        # the file and misc never disagree (here: misc on Android's block with the file still saying linux)
        android_a, _ = self.slot_b_blocks()
        conf = self.tmp / 'default-boot'
        self.assertEqual(self.nb('--rearm').returncode, 0)
        before = self.misc.read_bytes()
        self.assertNotEqual(self.bc(), android_a)
        os.chmod(self.tmp, 0o555)                   # the file's directory cannot take the new file
        try:
            if os.access(self.tmp, os.W_OK):
                self.skipTest('running as root: a read-only directory is still writable')
            r = self.nb('android')
        finally:
            os.chmod(self.tmp, 0o755)
        self.assertEqual(r.returncode, 1, r.stderr)
        self.assertIn('misc put back as it was', r.stderr)
        self.assertEqual(self.misc.read_bytes(), before)
        self.assertEqual(conf.read_text(), 'linux\n')
        r = self.nb('android')                       # and once it can be written, as usual
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual((self.bc(), conf.read_text()), (android_a, 'android\n'))

    def test_the_degraded_arm_still_records_linux(self):
        # FINDINGS 32: a slot block that cannot be rebuilt here gets the initramfs' one-shot block - written and read
        # back, so "linux" is recorded with it as before (exit 1 says the arm is degraded)
        linux_b = bytearray(self.blocks[1])
        linux_b[14] = 0x3f
        linux_b[28:32] = struct.pack('<I', zlib.crc32(bytes(linux_b[:28])))
        (self.run_dir / 'linux-slot').write_text('b\n')
        (self.run_dir / 'misc-bc-android.bin').write_bytes(self.blocks[0])
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(bytes(linux_b))
        (self.tmp / 'default-boot').write_text('android\n')
        r = self.nb('linux')
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertEqual((self.bc(), (self.tmp / 'default-boot').read_text()), (bytes(linux_b), 'linux\n'))

    def test_the_default_boot_is_never_written_through_a_symlink(self):
        android_a, _ = self.slot_b_blocks()
        victim = self.tmp / 'victim'
        victim.write_text('keep\n')
        conf = self.tmp / 'default-boot'
        conf.unlink()
        conf.symlink_to(victim)
        r = self.nb('android')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(victim.read_text(), 'keep\n')
        self.assertFalse(conf.is_symlink())
        self.assertEqual(conf.read_text(), 'android\n')

    def test_a_running_change_is_waited_for_and_never_overwritten(self):
        # --rearm (f50-boot-ok) reads the default boot, then writes the Linux block: an "android" in between was
        # lost. The script holds a lock from its first read to its last write; a stub flock stands for the real one
        # here (macOS has none) and says busy while the marker exists.
        android_a, _ = self.slot_b_blocks()
        busy = self.tmp / 'busy'
        flock = self.stubs / 'flock'
        flock.write_text(f'#!/bin/sh\necho "$*" >> "{self.tmp}/flock.log"\n[ ! -e "{busy}" ]\n')
        flock.chmod(0o755)
        sleep = self.stubs / 'sleep'
        sleep.write_text('#!/bin/sh\nexit 0\n')
        sleep.chmod(0o755)
        busy.touch()
        before = self.misc.read_bytes()
        if not shutil.which('bash'):
            self.skipTest('no bash')
        env = self.env(F50_RUN=self.run_dir, F50_CONF=self.tmp / 'default-boot',
                       F50_CMDLINE_SRC=self.tmp / 'cmdline', F50_NEXT_BOOT_WAIT=3)
        run = lambda *a: subprocess.run(['bash', str(BIN / 'f50-next-boot'), *a], capture_output=True, text=True,
                                        env=env)
        r = run('android')
        self.assertEqual(r.returncode, 1)
        self.assertIn('another f50-next-boot is still running', r.stderr)
        self.assertEqual(self.misc.read_bytes(), before)
        self.assertEqual((self.tmp / 'default-boot').read_text(), 'linux\n')
        self.assertTrue((self.run_dir / 'next-boot.lock').exists())
        # status reads only: it never waits
        self.assertEqual(run('status').returncode, 0)
        self.assertEqual((self.tmp / 'flock.log').read_text().split('\n').count('-n 9'), 3)
        busy.unlink()
        r = run('android')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.bc(), android_a)
        # a lock file that is a symlink is not opened (and nothing is serialised then, as without flock)
        (self.run_dir / 'next-boot.lock').unlink()
        (self.run_dir / 'next-boot.lock').symlink_to(self.tmp / 'elsewhere')
        busy.touch()
        self.assertEqual(run('android').returncode, 0)
        self.assertFalse((self.tmp / 'elsewhere').exists())

    @unittest.skipUnless(shutil.which('flock') and shutil.which('bash'), 'needs flock(1) and bash')
    def test_the_real_flock_holds_off_a_second_writer(self):
        import fcntl
        android_a, _ = self.slot_b_blocks()
        before = self.misc.read_bytes()
        with open(self.run_dir / 'next-boot.lock', 'a') as held:
            fcntl.flock(held, fcntl.LOCK_EX)
            r = subprocess.run(['bash', str(BIN / 'f50-next-boot'), 'android'], capture_output=True, text=True,
                               env=self.env(F50_RUN=self.run_dir, F50_CONF=self.tmp / 'default-boot',
                                            F50_CMDLINE_SRC=self.tmp / 'cmdline', F50_NEXT_BOOT_WAIT=2))
            self.assertEqual(r.returncode, 1, r.stderr)
            self.assertEqual(self.misc.read_bytes(), before)
        r = self.nb('android')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.bc(), android_a)

    def test_without_the_slot_file_the_published_block_names_the_slot(self):
        # no linux-slot, but the initramfs' Linux block (its first bytes are the suffix LK reads): that slot, before
        # the command line and before the b of old images (kanoqwq's b8d4f222)
        android_a, linux_b, android_b, linux_a = self.blocks
        (self.run_dir / 'misc-bc-android.bin').write_bytes(android_b)
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux_a)
        (self.tmp / 'cmdline').write_text('console=x androidboot.slot_suffix=_b\n')
        r = self.nb('--rearm')
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn('slot a armed', r.stdout)
        self.assertEqual(self.bc()[12], 0x0f | (6 << 4))
        self.assertEqual(self.nb('android').returncode, 0)
        self.assertEqual(self.bc(), android_b)

    def test_blocks_that_name_another_slot_are_never_written(self):
        # the slot file says one thing, a block another: nothing is written, whatever the command (kanoqwq's b8d4f222)
        android_a, linux_b, android_b, linux_a = self.blocks
        before = self.misc.read_bytes()
        for slot, android, linux in (('a', android_b, linux_b),     # Linux's block names b
                                     ('a', android_a, linux_a),     # Android's block names a, Linux's slot
                                     ('b', android_a, linux_a),
                                     ('b', android_b, linux_b)):
            (self.run_dir / 'linux-slot').write_text(slot + '\n')
            (self.run_dir / 'misc-bc-android.bin').write_bytes(android)
            (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux)
            for cmd in (('android',), ('--rearm',), ('--trial',), ('linux',), ('lock',)):
                (self.tmp / 'default-boot').write_text('linux\n')
                r = self.nb(*cmd)
                self.assertEqual(r.returncode, 1, (slot, cmd, r.stdout))
                self.assertIn('do not match', r.stderr)
                self.assertEqual(self.misc.read_bytes(), before)
            self.assertEqual(self.nb('status').returncode, 0)          # reading is still allowed
        # a matching pair still works
        (self.run_dir / 'linux-slot').write_text('a\n')
        (self.run_dir / 'misc-bc-android.bin').write_bytes(android_b)
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux_a)
        self.assertEqual(self.nb('android').returncode, 0)
        self.assertEqual(self.bc(), android_b)

    def test_status_says_whether_boot_control_can_be_written(self):
        """The panel's boot card (unisoc-modem/boot, kanoqwq's f6b0668d) reads this line."""
        android_a, linux_b, android_b, linux_a = self.blocks
        line = lambda: [l for l in self.nb('status').stdout.splitlines() if l.startswith('boot control:')]
        (self.run_dir / 'linux-slot').write_text('b\n')
        self.assertEqual(line(), ['boot control: not writable'])            # no blocks published
        (self.run_dir / 'misc-bc-android.bin').write_bytes(android_a)
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux_b)
        self.assertEqual(line(), ['boot control: writable'])
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux_a)       # the pair of another slot
        self.assertEqual(line(), ['boot control: not writable'])
        (self.run_dir / 'misc-bc-linux-trial.bin').write_bytes(linux_b)
        (self.run_dir / 'misc-dev').write_text(str(self.tmp / 'no-misc'))
        self.assertEqual(line(), ['boot control: not writable'])


class SysupgradeWrapper(ShellTest):
    """OpenWrt's /sbin/sysupgrade here: a firmware image is refused, the configuration backup tools pass (kanoqwq's
    b8d4f222 added restore: LuCI's System > Backup "Upload archive" runs sysupgrade --restore-backup)."""
    WRAP = TOP / 'systems/openwrt/overlay/usr/libexec/f50-sysupgrade'

    def run_it(self, shell, *args):
        real = self.stubs / 'sysupgrade.openwrt'
        real.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$STUBLOG/real"\n')
        real.chmod(0o755)
        (self.tmp / 'real').unlink(missing_ok=True)
        r = self.script(shell, self.WRAP, *args, F50_SYSUPGRADE=real)
        called = (self.tmp / 'real').read_text() if (self.tmp / 'real').exists() else None
        return r, called

    def test_backup_restore_and_list_pass(self):
        for shell in self.each_shell():
            for args in (['-b', '/tmp/backup.tar.gz'], ['--create-backup', '-'], ['-k', '-b', '/tmp/x.tgz'],
                         ['--restore-backup', '/tmp/backup.tar.gz'], ['-r', '/tmp/backup.tar.gz'],
                         ['-l'], ['--list-backup'], ['-u', '-c', '-l'], ['-v', '-b', 'x'], ['-q', '-r', 'x']):
                r, called = self.run_it(shell, *args)
                self.assertEqual(r.returncode, 0, (args, r.stderr))
                self.assertEqual(called, ' '.join(args) + '\n')

    def test_firmware_and_mixed_or_incomplete_requests_are_refused(self):
        for shell in self.each_shell():
            for args in ([], ['firmware.bin'], ['-n', 'firmware.bin'], ['-F', 'firmware.bin'], ['-T', 'image'],
                         ['-b'], ['-r'], ['-b', ''], ['-b', 'x', '-r', 'y'], ['-l', '-b', 'x'], ['-b', 'x', 'image'],
                         ['-l', 'image'], ['--test', 'image'], ['-r', 'x', '-n']):
                r, called = self.run_it(shell, *args)
                self.assertEqual(r.returncode, 1, args)
                self.assertIsNone(called, args)
                self.assertIn('sysupgrade is disabled', r.stderr)
