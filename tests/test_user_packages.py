"""Packages installed on the device across an update (#116): f50-update's upk_record writes down what was added to
a system (OpenWrt's world file, dpkg's manual packages) less the image's own set and sets the UCI files of those
packages aside, and the image build lists the image's own set with upk_manual. apk, apt-get, dpkg-query and uci are
stubs."""
import os
import re
import shutil
import subprocess

from helpers import BIN, TOP, ShellTest

UPD = BIN / 'f50-update'


def write(root, files):
    for f, data in files.items():
        p = root / f
        p.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(data, bytes):
            p.write_bytes(data)
        else:
            p.write_text(data)


def apk_db(pkgs):
    """lib/apk/db/installed: {name: [paths]} in apk's text format (P:, F: directory, R: file)."""
    out = []
    for name, paths in pkgs.items():
        out += ['C:Q1x=', f'P:{name}', 'V:1.0-r1']
        dirs = {}
        for p in paths:
            d, f = p.rsplit('/', 1)
            dirs.setdefault(d, []).append(f)
        for d, fs in dirs.items():
            out.append(f'F:{d}')
            out += [f'R:{f}' for f in fs]
        out.append('')
    return '\n'.join(out) + '\n'


IMAGE_WORLD = 'base-files=1711~f5dae5ece4\nbusybox\ndnsmasq\nluci\nluci-theme-aurora><Q1abc=\n'
IMAGE_DB = {'base-files': ['etc/config/system'], 'busybox': ['bin/busybox'], 'dnsmasq': ['etc/config/dhcp', 'etc/init.d/dnsmasq'],
            'luci': ['etc/config/luci'], 'luci-theme-aurora': ['www/luci-static/aurora/main.css']}


def openwrt_old(root, image_list=True):
    """An OpenWrt the user added OpenClash (with an init script and a UCI file), the Argon theme from a file and a
    kmod to."""
    db = dict(IMAGE_DB)
    db['luci-app-openclash'] = ['etc/config/openclash', 'etc/init.d/openclash', 'usr/share/openclash/x.sh']
    db['luci-theme-argon'] = ['etc/config/argon', 'www/luci-static/argon/x.css']
    db['kmod-tun'] = ['lib/modules/6.12/tun.ko']
    db['coreutils'] = ['usr/bin/coreutils']      # a dependency, not in the world file
    files = {'etc/apk/world': IMAGE_WORLD + 'luci-app-openclash\nluci-theme-argon><Q1xyz=\nkmod-tun\n',
             'lib/apk/db/installed': apk_db(db),
             'etc/config/openclash': "config openclash 'config'\n\toption enable '1'\n",
             'etc/config/argon': 'config global\n', 'etc/config/dhcp': 'old dhcp\n', 'etc/config/luci': 'old luci\n',
             'etc/config/system': 'old system\n', 'etc/config/mine': 'by hand\n',
             'etc/f50/image-version': 'v1\n', 'etc/f50/vpn.conf': 'ENABLE=1\n'}
    if image_list:
        files['etc/f50/image-packages'] = 'base-files\nbusybox\ndnsmasq\nluci\nluci-theme-aurora\n'
    write(root, files)
    (root / 'etc/rc.d').mkdir(parents=True, exist_ok=True)
    for n in ('S99openclash', 'S19dnsmasq'):
        os.symlink(f'../init.d/{n[3:]}', root / 'etc/rc.d' / n)


def openwrt_new(root, extra_db=None):
    """The new image as unpacked: its own world, package database, etc/config and lists."""
    db = dict(IMAGE_DB)
    db.update(extra_db or {})
    write(root, {'etc/apk/world': IMAGE_WORLD + ''.join(f'{n}\n' for n in (extra_db or {})),
                 'lib/apk/db/installed': apk_db(db),
                 'etc/config/dhcp': 'image dhcp\n', 'etc/config/luci': 'image luci\n', 'etc/config/system': 'image\n',
                 'etc/f50/image-version': 'v2\n',
                 'etc/f50/image-packages': 'base-files\nbusybox\ndnsmasq\nluci\nluci-theme-aurora\n'})


class Record(ShellTest):
    """upk_record: what was added, written into the new system; the UCI files of those packages set aside."""

    def setUp(self):
        super().setUp()
        self.old, self.new = self.tmp / 'old', self.tmp / 'new'

    def fresh(self, **kw):
        for d in (self.old, self.new):
            shutil.rmtree(d, ignore_errors=True)
        openwrt_old(self.old, **{k: v for k, v in kw.items() if k == 'image_list'})
        openwrt_new(self.new, kw.get('new_db'))

    def record(self, shell, os_name='openwrt-luci', pre='', **env):
        code = (f'. "{UPD}"; {pre} image_own_save "{self.new}" && '
                f'keep() {{ for k in etc/config etc/f50; do rm -rf "{self.new}/$k"; cp -a "{self.old}/$k" "{self.new}/$k"; done; }} && keep && '
                f'image_own_restore "{self.new}" && upk_record "{self.old}" "{self.new}" {os_name}; echo "rc=$?"')
        return self.sh(shell, code, F50_LIB=1, **env)

    def rec(self, name):
        p = self.new / 'etc/f50/user-packages' / name
        return p.read_text() if p.exists() else None

    def test_openwrt_added_packages_are_written_down_and_their_config_set_aside(self):
        for shell in self.each_shell():
            self.fresh()
            r = self.record(shell)
            self.assertIn('rc=0', r.stdout, r.stderr)
            # by hand, less the image's set, the kmod (another kernel's) and a dependency; versions and pins off
            self.assertEqual(self.rec('wanted'), 'luci-app-openclash\nluci-theme-argon\n')
            self.assertEqual(self.rec('state'), 'pending\n')
            self.assertEqual(self.rec('from'), 'v1\n')
            self.assertEqual(self.rec('done'), '')
            self.assertIn('luci-app-openclash etc/config/openclash\n', self.rec('files'))
            self.assertIn('luci-app-openclash etc/init.d/openclash\n', self.rec('files'))
            self.assertNotIn('usr/share', self.rec('files'))   # only etc/
            self.assertIn('S99openclash\n', self.rec('rc.d'))
            orph = self.new / 'etc/f50/orphaned-config/etc/config'
            self.assertEqual((orph / 'openclash').read_text(), "config openclash 'config'\n\toption enable '1'\n")
            self.assertEqual((orph / 'argon').read_text(), 'config global\n')
            self.assertFalse((self.new / 'etc/config/openclash').exists())
            # the image's own files and one made by hand stay where they are, as the device had them
            self.assertEqual((self.new / 'etc/config/dhcp').read_text(), 'old dhcp\n')
            self.assertEqual((self.new / 'etc/config/mine').read_text(), 'by hand\n')
            self.assertEqual((self.new / 'etc/f50/vpn.conf').read_text(), 'ENABLE=1\n')
            # the new image's version and list, not the old ones that came with etc/f50
            self.assertEqual((self.new / 'etc/f50/image-version').read_text(), 'v2\n')
            self.assertFalse((self.new / '.f50-image').exists())
            self.assertIn('luci-app-openclash luci-theme-argon', r.stdout)
            self.assertIn('/etc/config/openclash -> /etc/f50/orphaned-config/etc/config/openclash', r.stdout)
            self.assertIn('first boot installs them again', r.stdout)

    def test_a_file_the_new_image_has_is_never_set_aside(self):
        # a package that owned (or claimed) dhcp: the image has its own, so it stays
        for shell in self.each_shell():
            self.fresh()
            db = (self.old / 'lib/apk/db/installed').read_text().replace('F:etc/config\nR:openclash',
                                                                       'F:etc/config\nR:openclash\nR:dhcp')
            (self.old / 'lib/apk/db/installed').write_text(db)
            r = self.record(shell)
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertEqual((self.new / 'etc/config/dhcp').read_text(), 'old dhcp\n')
            self.assertFalse((self.new / 'etc/f50/orphaned-config/etc/config/dhcp').exists())

    def test_a_package_the_new_image_has_is_neither_wanted_nor_set_aside(self):
        for shell in self.each_shell():
            self.fresh(new_db={'luci-app-openclash': ['etc/config/openclash', 'etc/init.d/openclash']})
            r = self.record(shell)
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertEqual(self.rec('wanted'), 'luci-theme-argon\n')
            self.assertEqual((self.new / 'etc/config/openclash').read_text(), "config openclash 'config'\n\toption enable '1'\n")

    def test_a_wanted_package_kept_from_its_file_is_named_in_the_report(self):
        for shell in self.each_shell():
            self.fresh()
            write(self.old, {'etc/f50/user-apks/index':
                             'luci-app-openclash 0.48.002 luci-app-openclash-0.48.002.apk\n'
                             'luci-theme-argon 2.0 luci-theme-argon-2.0.apk\n'   # wanted, but its file is gone
                             'nano 8.0 nano-8.0.apk\n',                         # not wanted: not installed again
                             'etc/f50/user-apks/luci-app-openclash-0.48.002.apk': 'x\n',
                             'etc/f50/user-apks/nano-8.0.apk': 'x\n'})
            r = self.record(shell)
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertIn('  from a kept file: /etc/f50/user-apks/luci-app-openclash-0.48.002.apk', r.stdout)
            self.assertNotIn('luci-theme-argon-2.0.apk', r.stdout)
            self.assertNotIn('nano-8.0.apk', r.stdout)

    def test_without_the_old_images_list_the_new_ones_and_packages_txt_stand_in(self):
        for shell in self.each_shell():
            self.fresh(image_list=False)
            # a package the old image had and the new one dropped on purpose: in its packages.txt, not added by hand
            w = self.old / 'etc/apk/world'
            w.write_text(w.read_text() + 'oldthing\n')
            (self.old / 'etc/f50/packages.txt').write_text(
                'oldthing-2024.10.04~abc-r1 aarch64_generic {feeds/x} (GPL) [installed]\n'
                'busybox-1.37.0-r5 aarch64_generic {feeds/base} (GPL) [installed]\n')
            r = self.record(shell)
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertEqual(self.rec('wanted'), 'luci-app-openclash\nluci-theme-argon\n')

    def test_nothing_added_leaves_no_record(self):
        for shell in self.each_shell():
            self.fresh()
            (self.old / 'etc/apk/world').write_text(IMAGE_WORLD + 'kmod-tun\n')
            r = self.record(shell)
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertIn('none', r.stdout)
            self.assertFalse((self.new / 'etc/f50/user-packages').exists())
            self.assertTrue((self.new / 'etc/config/openclash').exists())

    def test_an_old_record_is_replaced_and_what_it_never_got_back_stays_wanted(self):
        for shell in self.each_shell():
            self.fresh()
            write(self.old, {'etc/f50/user-packages/wanted': 'adguardhome\nluci-app-sqm\nnano\n',
                             'etc/f50/user-packages/done': 'nano\n',
                             'etc/f50/user-packages/failed': 'luci-app-sqm ERROR: unable to select\n',
                             'etc/f50/user-packages/files': 'adguardhome etc/config/adguardhome\n',
                             'etc/f50/user-packages/log': 'old log\n'})
            r = self.record(shell)
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertEqual(self.rec('wanted'), 'adguardhome\nluci-app-openclash\nluci-theme-argon\n')
            self.assertIn('adguardhome etc/config/adguardhome\n', self.rec('files'))
            self.assertIsNone(self.rec('log'))
            self.assertEqual(self.rec('failed'), '')

    def test_skipped_still_sets_the_settings_aside(self):
        for shell in self.each_shell():
            self.fresh()
            r = self.record(shell, F50_KEEP_PACKAGES='0')
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertEqual(self.rec('state'), 'skipped\n')
            self.assertIn('not installed again', r.stdout)
            self.assertTrue((self.new / 'etc/f50/orphaned-config/etc/config/openclash').exists())

    def test_safe_under_set_e(self):
        # installer/android/android-install.sh runs with set -e
        for shell in self.each_shell():
            self.fresh()
            r = self.record(shell, pre='set -e;')
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertEqual(self.rec('wanted'), 'luci-app-openclash\nluci-theme-argon\n')

    def test_ubuntu_manual_packages_less_the_images(self):
        def dpkg(names, auto=()):
            st = ''.join(f'Package: {n}\nStatus: install ok installed\nVersion: 1\n\n' for n in names)
            st += 'Package: removed\nStatus: deinstall ok config-files\n\n'
            es = ''.join(f'Package: {n}\nArchitecture: arm64\nAuto-Installed: 1\n\n' for n in auto)
            return {'var/lib/dpkg/status': st, 'var/lib/apt/extended_states': es}
        for shell in self.each_shell():
            for d in (self.old, self.new):
                shutil.rmtree(d, ignore_errors=True)
            write(self.old, dpkg(['bash', 'openssh-server', 'htop', 'libfoo', 'linux-image-6.8'], auto=['libfoo']))
            write(self.old, {'etc/f50/image-packages': 'bash\nopenssh-server\n', 'etc/f50/image-version': 'v1\n',
                             'etc/config/x': ''})
            write(self.new, dpkg(['bash', 'openssh-server']))
            write(self.new, {'etc/f50/image-version': 'v2\n', 'etc/f50/image-packages': 'bash\nopenssh-server\n'})
            r = self.record(shell, 'ubuntu')
            self.assertIn('rc=0', r.stdout, r.stderr)
            self.assertEqual(self.rec('wanted'), 'htop\n')
            self.assertEqual(self.rec('files'), '')
            # without the old image's list: the new image's manual set
            (self.old / 'etc/f50/image-packages').unlink()
            shutil.rmtree(self.new / 'etc/f50/user-packages')
            r = self.record(shell, 'ubuntu')
            self.assertEqual(self.rec('wanted'), 'htop\n', r.stderr)

    def ubuntu_roots(self, old_codename='noble', new_codename='noble', manual=('bash', 'htop')):
        def dpkg(names):
            return {'var/lib/dpkg/status': ''.join(f'Package: {n}\nStatus: install ok installed\n\n' for n in names),
                    'var/lib/apt/extended_states': ''}
        for d in (self.old, self.new):
            shutil.rmtree(d, ignore_errors=True)
        write(self.old, dpkg(manual))
        write(self.old, {
            'usr/lib/os-release': f'NAME="Ubuntu"\nVERSION_CODENAME={old_codename}\n',
            'etc/f50/image-packages': 'bash\n', 'etc/f50/image-version': 'v1\n',
            # the image's own source (the new image has it too) and two the user added, one deb822 with a key
            'etc/apt/sources.list.d/ubuntu.sources': 'old\n',
            'etc/apt/sources.list.d/ppa-htop.list': 'deb [signed-by=/etc/apt/keyrings/htop.gpg] http://ppa noble main\n',
            'etc/apt/sources.list.d/vendor.sources': 'Types: deb\nSigned-By: /usr/share/keyrings/vendor.gpg\n',
            'etc/apt/keyrings/htop.gpg': 'k1', 'usr/share/keyrings/vendor.gpg': 'k2',
            'usr/share/keyrings/unrelated.gpg': 'k3', 'usr/share/keyrings/ubuntu-archive-keyring.gpg': 'old key',
            'etc/apt/trusted.gpg.d/legacy.gpg': 'k4',
            # pinning: the user's own, and one the new image ships itself (never replaced)
            'etc/apt/preferences.d/htop-ppa': 'Package: *\nPin: release o=LP-PPA-htop\nPin-Priority: 600\n',
            'etc/apt/preferences.d/image-pin': 'old pin\n',
            # htop's units: one enabled, one masked, one disabled, a template (skipped)
            'var/lib/dpkg/info/htop:arm64.list': '/.\n/usr/lib/systemd/system/htopd.service\n'
                                                 '/lib/systemd/system/htop.timer\n/usr/lib/systemd/system/htop-x.socket\n'
                                                 '/usr/lib/systemd/system/htop@.service\n/usr/bin/htop\n'
                                                 '/usr/lib/systemd/system/htop-alias.service\n'})
        os.makedirs(self.old / 'etc/systemd/system/multi-user.target.wants')
        os.symlink('/usr/lib/systemd/system/htopd.service', self.old / 'etc/systemd/system/multi-user.target.wants/htopd.service')
        os.symlink('/dev/null', self.old / 'etc/systemd/system/htop.timer')
        # an alias of htopd.service (smartd.service of smartmontools): it follows its unit, never recorded itself
        os.makedirs(self.old / 'usr/lib/systemd/system', exist_ok=True)
        os.symlink('htopd.service', self.old / 'usr/lib/systemd/system/htop-alias.service')
        write(self.new, dpkg(['bash']))
        write(self.new, {'usr/lib/os-release': f'NAME="Ubuntu"\nVERSION_CODENAME={new_codename}\n',
                         'etc/f50/image-packages': 'bash\n', 'etc/f50/image-version': 'v2\n',
                         'etc/apt/sources.list.d/ubuntu.sources': 'image\n',
                         'etc/apt/preferences.d/image-pin': 'image pin\n',
                         'usr/share/keyrings/ubuntu-archive-keyring.gpg': 'image key'})

    def test_ubuntu_apt_sources_and_units(self):
        for shell in self.each_shell():
            self.ubuntu_roots()
            r = self.record(shell, 'ubuntu')
            self.assertIn('rc=0', r.stdout, r.stderr)
            src = self.new / 'etc/apt/sources.list.d'
            # the image's own never replaced; the user's kept, with the keys they name and trusted.gpg.d's
            self.assertEqual((src / 'ubuntu.sources').read_text(), 'image\n')
            self.assertTrue((src / 'ppa-htop.list').exists())
            self.assertTrue((src / 'vendor.sources').exists())
            self.assertEqual((self.new / 'etc/apt/keyrings/htop.gpg').read_text(), 'k1')
            self.assertEqual((self.new / 'usr/share/keyrings/vendor.gpg').read_text(), 'k2')
            self.assertEqual((self.new / 'etc/apt/trusted.gpg.d/legacy.gpg').read_text(), 'k4')
            self.assertFalse((self.new / 'usr/share/keyrings/unrelated.gpg').exists())
            self.assertEqual((self.new / 'usr/share/keyrings/ubuntu-archive-keyring.gpg').read_text(), 'image key')
            self.assertEqual(self.rec('apt-sources'),
                             'etc/apt/sources.list.d/ppa-htop.list\netc/apt/sources.list.d/vendor.sources\n')
            self.assertEqual(self.rec('units'), 'htop htop-x.socket disabled\nhtop htop.timer masked\n'
                                                'htop htopd.service enabled\n')
            self.assertIn('apt: /etc/apt/sources.list.d/ppa-htop.list kept', r.stdout)
            pref = self.new / 'etc/apt/preferences.d'
            self.assertIn('LP-PPA-htop', (pref / 'htop-ppa').read_text())
            self.assertEqual((pref / 'image-pin').read_text(), 'image pin\n')
            self.assertEqual(self.rec('apt-prefs'), 'etc/apt/preferences.d/htop-ppa\n')

    def test_ubuntu_sources_of_another_release_with_nothing_to_install_stay_off(self):
        # 24.04 -> 26.04 and no package to bring back: no first-boot check, so they are kept disabled
        for shell in self.each_shell():
            self.ubuntu_roots('noble', 'resolute', manual=('bash',))
            r = self.record(shell, 'ubuntu')
            self.assertIn('rc=0', r.stdout, r.stderr)
            src = self.new / 'etc/apt/sources.list.d'
            self.assertFalse((src / 'ppa-htop.list').exists())
            self.assertTrue((src / 'ppa-htop.list.disabled').exists())
            self.assertIn('is for Ubuntu noble, this is resolute: kept disabled', r.stdout)
            self.assertFalse((self.new / 'etc/f50/user-packages').exists())
            # with a package to bring back they stay on: the first boot tries them (f50-user-packages)
            self.ubuntu_roots('noble', 'resolute')
            self.record(shell, 'ubuntu')
            self.assertTrue((src / 'ppa-htop.list').exists())

    def test_android_install_has_the_same_block(self):
        upd = UPD.read_text()
        ai = (TOP / 'installer/android/android-install.sh').read_text()
        blk = re.compile(r'# --- user-packages begin\n.*?# --- user-packages end\n', re.S)
        self.assertEqual(blk.search(ai).group(0), blk.search(upd).group(0))
        # and it runs where f50-update's does: after the settings, before the switch
        self.assertLess(ai.index('image_own_save "$M/$os.new"'), ai.index('for k in $keep'))
        self.assertLess(ai.index('upk_record "$M/$os"'), ai.index('mv $M/$os.new $M/$os'))


class ApplyOne(ShellTest):
    """apply_one with an OpenWrt that has OpenClash: the record, the set-aside UCI file, its rc.d link not carried
    over to a system without the service, the new image's version."""

    def test_apply_one(self):
        disk = self.tmp / 'disk'
        img = self.tmp / 'img'
        openwrt_new(img)
        write(img, {'sbin/init': '#!/bin/sh\n', 'etc/init.d/dnsmasq': '#!/bin/sh\n'})
        (img / 'sbin/init').chmod(0o755)
        (img / 'etc/init.d/dnsmasq').chmod(0o755)
        (img / 'etc/rc.d').mkdir()
        stage = disk / '.f50-update'
        for shell in self.each_shell():
            shutil.rmtree(disk, ignore_errors=True)
            openwrt_old(disk / 'openwrt')
            write(disk / 'openwrt', {'sbin/init': '#!/bin/sh\n'})
            stage.mkdir(parents=True)
            subprocess.run(['tar', '-czf', str(stage / 'f50-openwrt-rootfs.tar.gz'), '-C', str(img), '.'], check=True)
            r = self.sh(shell, f'. "{UPD}"; is_root() {{ false; }}; apply_one openwrt v2', F50_LIB=1, F50_DISK=disk,
                        F50_BIN=BIN)
            self.assertEqual(r.returncode, 0, r.stderr)
            new = disk / 'openwrt'
            self.assertEqual((new / 'etc/f50/user-packages/wanted').read_text(), 'luci-app-openclash\nluci-theme-argon\n')
            self.assertTrue((new / 'etc/f50/orphaned-config/etc/config/openclash').exists())
            self.assertFalse((new / 'etc/config/openclash').exists())
            self.assertEqual((new / 'etc/f50/image-version').read_text(), 'v2\n')
            self.assertFalse((new / '.f50-image').exists())
            # the service of a package the new system does not have: no dangling link; dnsmasq's comes along
            self.assertFalse(os.path.lexists(new / 'etc/rc.d/S99openclash'))
            self.assertTrue(os.path.islink(new / 'etc/rc.d/S19dnsmasq'))
            self.assertIn('packages installed on the device', r.stdout)
            self.assertIn('luci-app-openclash luci-theme-argon', r.stdout)

    def test_stray_config_of_v2026_10_18_goes(self):
        # the old system (a device updated to v2026.10.18) and the image both carry etc/config/attendedsysupgrade
        # and uhttpd.apk-new; neither stays, except attendedsysupgrade with its package installed in the new image
        disk = self.tmp / 'disk'
        stage = disk / '.f50-update'
        for shell in self.each_shell():
            for extra_db, keeps in ((None, False), ({'attendedsysupgrade-common': ['etc/config/attendedsysupgrade']}, True)):
                shutil.rmtree(disk, ignore_errors=True)
                img = self.tmp / 'img'
                shutil.rmtree(img, ignore_errors=True)
                openwrt_new(img, extra_db)
                write(img, {'sbin/init': '#!/bin/sh\n', 'etc/config/attendedsysupgrade': 'image\n',
                            'etc/config/uhttpd.apk-new': 'image\n'})
                (img / 'sbin/init').chmod(0o755)
                (img / 'etc/rc.d').mkdir()
                openwrt_old(disk / 'openwrt')
                write(disk / 'openwrt', {'sbin/init': '#!/bin/sh\n', 'etc/config/attendedsysupgrade': 'stray\n',
                                         'etc/config/uhttpd.apk-new': 'stray\n', 'etc/config/uhttpd': 'mine\n'})
                stage.mkdir(parents=True)
                subprocess.run(['tar', '-czf', str(stage / 'f50-openwrt-rootfs.tar.gz'), '-C', str(img), '.'], check=True)
                r = self.sh(shell, f'. "{UPD}"; is_root() {{ false; }}; apply_one openwrt v2', F50_LIB=1, F50_DISK=disk,
                            F50_BIN=BIN)
                self.assertEqual(r.returncode, 0, r.stderr)
                new = disk / 'openwrt'
                self.assertFalse((new / 'etc/config/uhttpd.apk-new').exists(), keeps)
                self.assertEqual((new / 'etc/config/attendedsysupgrade').exists(), keeps)
                self.assertEqual((new / 'etc/config/uhttpd').read_text(), 'mine\n')   # the device's own stays
                # deleted, not set aside (the fixture's OpenClash config is, as always)
                orph = new / 'etc/f50/orphaned-config/etc/config'
                self.assertFalse((orph / 'attendedsysupgrade').exists())
                self.assertFalse((orph / 'uhttpd.apk-new').exists())

    def test_with_the_image_config_files_added(self):
        # (#121) the update adds the config files of the image's packages the old system did not have, then sets the
        # orphaned ones aside: sqm-scripts, a package of the image the user had removed, gets the image's
        # /etc/config/sqm back, never set aside; OpenClash's (the user's own package) still goes aside
        disk = self.tmp / 'disk'
        img = self.tmp / 'img'
        openwrt_new(img, {'sqm-scripts': ['etc/config/sqm']})
        write(img, {'sbin/init': '#!/bin/sh\n', 'etc/config/sqm': 'image sqm\n',
                    'etc/f50/image-packages': 'base-files\nbusybox\ndnsmasq\nluci\nluci-theme-aurora\nsqm-scripts\n'})
        (img / 'sbin/init').chmod(0o755)
        (img / 'etc/rc.d').mkdir()
        stage = disk / '.f50-update'
        for shell in self.each_shell():
            shutil.rmtree(disk, ignore_errors=True)
            openwrt_old(disk / 'openwrt')
            old = disk / 'openwrt'
            write(old, {'sbin/init': '#!/bin/sh\n',
                        'etc/f50/image-packages': 'base-files\nbusybox\ndnsmasq\nluci\nluci-theme-aurora\nsqm-scripts\n'})
            stage.mkdir(parents=True)
            subprocess.run(['tar', '-czf', str(stage / 'f50-openwrt-rootfs.tar.gz'), '-C', str(img), '.'], check=True)
            r = self.sh(shell, f'. "{UPD}"; is_root() {{ false; }}; apply_one openwrt v2', F50_LIB=1, F50_DISK=disk,
                        F50_BIN=BIN)
            self.assertEqual(r.returncode, 0, r.stderr)
            new = disk / 'openwrt'
            self.assertEqual((new / 'etc/config/sqm').read_text(), 'image sqm\n')
            self.assertFalse((new / 'etc/f50/orphaned-config/etc/config/sqm').exists())
            self.assertNotIn('sqm-scripts', (new / 'etc/f50/user-packages/wanted').read_text())
            self.assertTrue((new / 'etc/f50/orphaned-config/etc/config/openclash').exists())
            self.assertEqual((new / 'etc/config/dhcp').read_text(), 'old dhcp\n')     # the device's, not the image's

    def test_no_reinstall_flag(self):
        src = UPD.read_text()
        self.assertIn('--no-reinstall) F50_KEEP_PACKAGES=0', src)


class Wiring(ShellTest):
    def test_the_image_lists_its_packages(self):
        b = (TOP / 'systems/openwrt/build-rootfs.sh').read_text()
        self.assertIn('upk_manual $R openwrt) > $R/etc/f50/image-packages', b)
        # no stray config file in the image itself (an update would add it to every device): removed from $R, checked
        self.assertIn('rm -f $R/etc/config/attendedsysupgrade $R/etc/config/*.apk-new', b)
        self.assertLess(b.index('rm -f $R/etc/config/attendedsysupgrade'), b.index('cd $R && tar -czf'))

    def test_build_shell_can_source_the_updater(self):
        # the OpenWrt build sources f50-update under sh -eu
        r = self.tmp / 'r'
        write(r, {'etc/apk/world': 'a\nb=1\nc><Q1x=\n'})
        for shell in self.each_shell():
            out = self.sh(shell, f'set -eu; (F50_LIB=1; . "{UPD}"; upk_manual "{r}" openwrt)')
            self.assertEqual((out.returncode, out.stdout), (0, 'a\nb\nc\n'), out.stderr)
