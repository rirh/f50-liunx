"""The VPN is a module of its own (dikeckaan/mu300-linux-vpn, installed as the vpn extra): the image carries no
f50-vpn, and the places that work with it must do nothing harmful without it - and, with the VPN on and its kill
switch on, fail closed. Here the orphan check (lib/vpn-orphan.sh) and the copies f50-update and the installer carry."""
import shutil
import unittest

from helpers import BIN, TOP, ShellTest

ORPHAN_LIB = BIN.parent / 'lib' / 'vpn-orphan.sh'


class Orphaned(ShellTest):
    """vpn_killswitch_orphaned: the VPN wanted with its kill switch, read from the files as f50-vpn reads them, and
    no f50-vpn"""

    def check(self, shell, conf=None, settings=None, store=True, cmd=None):
        root = self.tmp / 'root'
        shutil.rmtree(root, ignore_errors=True)
        (root / 'etc/f50').mkdir(parents=True)
        if conf is not None:
            (root / 'etc/f50/vpn.conf').write_text(conf)
        if settings is not None:
            (root / 'etc/f50/vpn').mkdir()
            (root / 'etc/f50/vpn/settings').write_text(settings)
            if store:
                (root / 'etc/f50/vpn/profiles').mkdir()
        r = self.sh(shell, f'. "{ORPHAN_LIB}"; vpn_killswitch_orphaned "{cmd or self.tmp / "no-f50-vpn"}" "{root}" '
                           '&& echo yes || echo no')
        return r.stdout.strip()

    def test_the_matrix(self):
        self.stub('f50-vpn', 'exit 0')
        cases = [
            # (vpn.conf, store settings, store in use, orphaned)
            (None, None, True, 'no'),
            ('ENABLE=0\n', None, True, 'no'),
            ('ENABLE=1\n', None, True, 'yes'),                       # no KILL_SWITCH line: on, f50-vpn's default
            ('ENABLE=1\nKILL_SWITCH=1\n', None, True, 'yes'),
            ('ENABLE=1\nKILL_SWITCH=0\n', None, True, 'no'),
            ("ENABLE='1'\nKILL_SWITCH='0'\n", None, True, 'no'),
            ('ENABLE="1"\nKILL_SWITCH="1"\n', None, True, 'yes'),
            ('ENABLE=1  # on\n', None, True, 'yes'),
            ('ENABLE=0\nENABLE=1\n', None, True, 'yes'),             # the last line wins
            ('ENABLE=1\nENABLE=0\n', None, True, 'no'),
            ('ENABLE=yes\n', None, True, 'no'),                      # only 1 is on
            ('ENABLE=1\nKILL_SWITCH=yes\n', None, True, 'yes'),       # only 0 is off
            # the store: its settings, not vpn.conf's legacy line
            ('ENABLE=1\nKILL_SWITCH=1\n', "KILL_SWITCH='0'\n", True, 'no'),
            ('ENABLE=1\nKILL_SWITCH=0\n', "KILL_SWITCH='1'\n", True, 'yes'),
            ('ENABLE=1\nKILL_SWITCH=0\n', 'TAILSCALE=1\n', True, 'yes'),  # not in the settings: the default, on
            ('ENABLE=1\nKILL_SWITCH=0\n', "KILL_SWITCH='1'\n", False, 'no'),  # no profiles: not the store yet
            ('ENABLE=0\n', "KILL_SWITCH='1'\n", True, 'no'),
        ]
        for shell in self.each_shell():
            for conf, settings, store, want in cases:
                self.assertEqual(self.check(shell, conf, settings, store), want, (shell, conf, settings, store))
            # f50-vpn there: never orphaned
            self.assertEqual(self.check(shell, 'ENABLE=1\n', cmd=self.stubs / 'f50-vpn'), 'no')

    def test_the_copies_are_the_same(self):
        # f50-update (runs before the new system exists) carries its own copy of the check: the same two functions,
        # word for word (installer/android/android-install.sh has no VPN check any more: the VPN module is gone there)
        import re
        lib = ORPHAN_LIB.read_text()

        def fns(text):
            return [re.search(rf'\n{n}\(\) \{{\n.*?\n\}}\n', '\n' + text, re.S).group(0)
                    for n in ('vpn_conf_value', 'vpn_killswitch_wanted')]
        for f in (BIN / 'f50-update',):
            self.assertEqual(fns(f.read_text()), fns(lib), f)

    def test_the_engine_missing(self):
        """vpn_engine_missing: the VPN on, and its active profile's engine extra not installed (module v2: each
        engine is an extra; module v1 and the bundle carry xray and sing-box in the vpn extra)"""
        root = self.tmp / 'root'
        disk = self.tmp / 'disk'

        def case(shell, typ, enable='1', have=()):
            shutil.rmtree(root, ignore_errors=True)
            shutil.rmtree(disk, ignore_errors=True)
            (root / 'etc/f50/vpn/profiles/p').mkdir(parents=True)
            (root / 'etc/f50/vpn.conf').write_text(f'ENABLE={enable}\n')
            (root / 'etc/f50/vpn/active').write_text('p\n')
            (root / 'etc/f50/vpn/profiles/p/meta').write_text(f"NAME='p'\nTYPE='{typ}'\n")
            for path in have:
                (disk / 'extra' / path).parent.mkdir(parents=True, exist_ok=True)
                (disk / 'extra' / path).write_text('#!/bin/sh\n')
                (disk / 'extra' / path).chmod(0o755)
            r = self.sh(shell, f'. "{ORPHAN_LIB}"; vpn_engine_missing "{root}" || echo none', F50_DISK=disk)
            return r.stdout.strip()

        for shell in self.each_shell():
            self.assertEqual(case(shell, 'xray'), 'vpn-xray')
            self.assertEqual(case(shell, 'xray', enable='0'), 'none')
            self.assertEqual(case(shell, 'xray', have=('vpn-xray/bin/xray',)), 'none')
            self.assertEqual(case(shell, 'xray', have=('vpn/bin/xray',)), 'none')          # v1, the bundle
            self.assertEqual(case(shell, 'sing-box', have=('vpn/bin/sing-box',)), 'none')
            self.assertEqual(case(shell, 'dae', have=('vpn/bin/xray',)), 'vpn-dae')
            self.assertEqual(case(shell, 'mihomo', have=('vpn-mihomo/bin/mihomo',)), 'none')
            self.assertEqual(case(shell, 'wireguard'), 'none')                           # a package, not an extra

    def test_nothing_is_sourced(self):
        for shell in self.each_shell():
            self.check(shell, 'ENABLE=1\nKILL_SWITCH=$(touch "$STUBLOG/ran")\n',
                       "KILL_SWITCH='$(touch \"$STUBLOG/ran\")'\n")
            self.assertFalse((self.tmp / 'ran').exists())


if __name__ == '__main__':
    unittest.main()
