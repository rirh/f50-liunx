"""f50-modprobe-guard: the kernel's on-demand module loads go through it on both systems (review of #256), checked
against a sample of a build's modules.dep and modules.alias. (The mainline kernel's all-modules.py and
check-dt-modules.py, which this file tested too, went with kernel/mainline.)"""
import fnmatch
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import unittest

TOP = Path(__file__).resolve().parents[1]


class AutoloadGuard(unittest.TestCase):
    """The kernel's on-demand loads go through f50-modprobe-guard on both systems (review of #256)."""
    GUARD = TOP / 'systems/base/overlay/opt/f50/bin/f50-modprobe-guard'
    LIST = TOP / 'systems/base/overlay/opt/f50/lib/autoload-blocked'

    SAMPLE = TOP / 'tests/data/autoload-7.2.11'   # a subset of the 7.2.11 build's modules.dep and modules.alias

    def run_guard(self, name, allow=False, shell=('sh',), moddir=None):
        with tempfile.TemporaryDirectory() as d:
            log, kmsg = Path(d) / 'log', Path(d) / 'kmsg'
            fake = Path(d) / 'modprobe'
            fake.write_text('#!/bin/sh\necho "$@" > "%s"\n' % log)
            fake.chmod(0o755)
            env = {'PATH': '/usr/bin:/bin', 'F50_MODPROBE': str(fake), 'F50_AUTOLOAD_BLOCKED': str(self.LIST),
                   'F50_KMSG': str(kmsg), 'F50_AUTOLOAD_ALLOW': str(Path(d) / 'allow'), 'F50_KREL': 't',
                   'F50_MODDIR': str(moddir or Path(d) / 'none'), 'F50_GUARD_CACHE': str(Path(d) / 'cache')}
            if allow:
                (Path(d) / 'allow').write_text('')
            r = subprocess.run([*shell, str(self.GUARD), '-q', '--', name], env=env, capture_output=True, text=True)
            return r.returncode, log.read_text() if log.exists() else None, kmsg.read_text() if kmsg.exists() else ''

    def test_the_list_holds_module_names(self):
        names = [l.split('#')[0].strip() for l in self.LIST.read_text().splitlines()]
        names = [n for n in names if n]
        self.assertEqual(len(names), len(set(names)))
        for n in names:
            self.assertRegex(n, r'^[a-z0-9_]+$')
        for n in ('tipc', 'sctp', 'can', 'can_raw', 'can_bcm', 'nfc', 'ieee802154', 'slcan', 'hfs', 'jfs', 'nilfs2'):
            self.assertIn(n, names)
        for n in ('l2tp_core', 'l2tp_ppp', 'can_dev', 'hfsplus', 'wireguard', 'vhost_net'):
            self.assertNotIn(n, names)

    def test_a_refusal_is_logged_and_never_reaches_modprobe(self):
        with tempfile.TemporaryDirectory() as d:
            moddir = Path(d)
            (moddir / 'tipc.ko').write_text('')
            rc, called, kmsg = self.run_guard('tipc', moddir=moddir)
            self.assertEqual((rc, called), (1, None))
            self.assertIn('on-demand loading of tipc (tipc) refused (root: modprobe tipc', kmsg)
            # without a table (no modinfo answer) the names still hold, in either spelling
            self.assertEqual(self.run_guard('can-raw')[:2], (1, None))
            self.assertEqual(self.run_guard('wireguard')[:2], (0, '-q -- wireguard\n'))

    def sample_moddir(self, d):
        """A module directory made from the build sample: every module file carries its aliases in its .modinfo."""
        aliases = {}
        for line in (self.SAMPLE / 'modules.alias').read_text().splitlines():
            if line.startswith('alias '):
                _, a, m = line.split(None, 2)
                aliases.setdefault(m, []).append(a)
        names = {os.path.basename(l.split(':')[0]).split('.ko')[0].replace('-', '_')
                 for l in (self.SAMPLE / 'modules.dep').read_text().splitlines()}
        for m in names:
            (d / (m + '.ko')).write_bytes(b'\x7fELF\0' + '\0'.join('alias=' + a for a in aliases.get(m, [])).encode() + b'\0')
        return aliases

    # the requests the first (request-string) list refused, made concrete
    OLD_REQUESTS = ['net-pf-29', 'net-pf-29-proto-1', 'net-pf-30', 'net-pf-30-proto-0', 'net-pf-36', 'net-pf-39',
                    'net-pf-39-proto-1', 'net-pf-40', 'net-pf-2-proto-132', 'net-pf-2-proto-132-type-1',
                    'net-pf-10-proto-132', 'net-pf-10-proto-132-type-5', 'net-pf-16-proto-16-family-TIPCv2',
                    'net-pf-16-proto-16-family-nfc', 'net-pf-16-proto-16-family-nl802154',
                    'net-pf-16-proto-16-family-802.15.4 MAC', 'net-pf-16-proto-16-family-j1939', 'rtnl-family-29'] + [
                    'fs-' + f for f in ('adfs affs befs bfs coda cramfs efs gfs2 gfs2meta hfs hpfs jfs minix nilfs2 ocfs2 '
                                        'ocfs2_dlmfs omfs orangefs qnx4 qnx6 romfs sysv v7 ufs vxfs ceph').split()]

    def test_what_the_old_list_refused_is_still_refused_or_not_built(self):
        with tempfile.TemporaryDirectory() as d:
            aliases = self.sample_moddir(Path(d))
            pairs = [(a.replace('-', '_'), m) for m, al in aliases.items() for a in al]
            for req in self.OLD_REQUESTS:
                answering = {m for a, m in pairs if fnmatch.fnmatchcase(req.replace('-', '_'), a)}
                rc, called, _ = self.run_guard(req, moddir=Path(d))
                if answering:
                    self.assertEqual((rc, called), (1, None), (req, answering))
                else:
                    # nothing in the build answers it (sysv went upstream in 6.15; no genetlink family by that name):
                    # the guard hands it on, and modprobe finds nothing either
                    self.assertEqual(rc, 0, req)

    def test_the_families_the_review_named_are_not_built(self):
        gone = ('rds rds_tcp rds_rdma atm clip br2684 lec mpoa pppoatm ax25 netrom rose mkiss 6pack bpqether x25 lapb '
                'lapbether appletalk phonet cdc_phonet af_rxrpc rxkad kafs kcm smc smc_diag caif caif_socket chnl_net '
                'mISDN_core mISDN_dsp capi n_gsm n_hdlc dccp dccp_ipv4 dccp_ipv6 dccp_diag qrtr psnap p8022 llc2').split()
        listed = {l.split('#')[0].strip() for l in self.LIST.read_text().splitlines()}
        for kv in ('7.2.11', '6.18.57'):
            names = set((self.SAMPLE / ('modules.names-' + kv)).read_text().split())
            self.assertGreater(len(names), 2500)
            self.assertEqual(sorted(set(gone) & names), [], kv)
            # L2TP stays, on purpose (L2TP VPNs), and is not listed
            self.assertIn('l2tp_ppp', names)
        self.assertNotIn('l2tp_ppp', listed)
        # every line discipline that is built is listed
        ldisc = {l.split()[2] for l in (self.SAMPLE / 'modules.alias').read_text().splitlines()
                 if l.startswith('alias tty-ldisc-')}
        self.assertTrue(ldisc)
        self.assertLessEqual(ldisc, listed)

    def test_a_module_that_depends_on_a_listed_one_is_refused_too(self):
        # Ubuntu's modules.dep: the table takes in every module whose dependencies reach a listed one, whatever the list
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / 'tipc.ko').write_bytes(b'\0alias=net-pf-30\0')
            (d / 'evil-helper.ko').write_bytes(b'\0alias=net-pf-99\0alias=rtnl-link-evil\0')
            (d / 'other.ko').write_bytes(b'\0alias=net-pf-98\0')
            (d / 'modules.dep').write_text('kernel/net/tipc/tipc.ko:\nkernel/x/evil-helper.ko: kernel/net/tipc/tipc.ko\n'
                                           'kernel/x/other.ko:\n')
            for req in ('net-pf-99', 'rtnl-link-evil', 'evil_helper', 'evil-helper'):
                self.assertEqual(self.run_guard(req, moddir=d)[:2], (1, None), req)
            self.assertEqual(self.run_guard('net-pf-98', moddir=d)[:2], (0, '-q -- net-pf-98\n'))

    def test_without_a_table_it_fails_closed_for_the_families(self):
        with tempfile.TemporaryDirectory() as d:
            env_cache = Path(d) / 'elsewhere'
            env_cache.mkdir()
            link = Path(d) / 'cache-link'
            link.symlink_to(env_cache)   # not a directory of root's own making: no table
            log, kmsg = Path(d) / 'log', Path(d) / 'kmsg'
            fake = Path(d) / 'modprobe'
            fake.write_text('#!/bin/sh\necho "$@" > "%s"\n' % log)
            fake.chmod(0o755)
            env = {'PATH': '/usr/bin:/bin', 'F50_MODPROBE': str(fake), 'F50_AUTOLOAD_BLOCKED': str(self.LIST),
                   'F50_KMSG': str(kmsg), 'F50_AUTOLOAD_ALLOW': str(Path(d) / 'allow'), 'F50_KREL': 't',
                   'F50_MODDIR': str(Path(d)), 'F50_GUARD_CACHE': str(link)}
            run = lambda req: subprocess.run(['sh', str(self.GUARD), '-q', '--', req], env=env).returncode
            for req in ('net-pf-99', 'net-pf-16-proto-16-family-x', 'fs-foo', 'tty-ldisc-21', 'rtnl-link-x'):
                self.assertEqual(run(req), 1, req)
                # the hint names what root would load (on F50-B it said "root: modprobe no alias table")
                self.assertIn('on-demand loading of %s (no alias table) refused (root: modprobe %s;' % (req, req),
                              kmsg.read_text())
            self.assertEqual(run('can-raw'), 1)          # a listed name, still known without a table
            self.assertFalse(log.exists())
            for req in ('crypto-gcm(aes)', 'nft-chain-2-nat', 'char-major-10-200', 'wireguard'):
                self.assertEqual(run(req), 0, req)

    def test_the_switch_turns_it_off(self):
        self.assertEqual(self.run_guard('tipc', allow=True)[:2], (0, '-q -- tipc\n'))

    def test_prepare_makes_the_table_then_takes_over(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            helper = d / 'helper'
            helper.write_text('/sbin/modprobe\n')
            (d / 'mods').mkdir()
            (d / 'mods' / 'tipc.ko').write_bytes(b'\x7fELF\0name=tipc\0alias=net-pf-30\0alias=net-pf-16-proto-16-family-TIPCv2\0')
            env = {'PATH': '/usr/bin:/bin', 'F50_AUTOLOAD_BLOCKED': str(self.LIST), 'F50_KREL': '7.2.11',
                   'F50_MODDIR': str(d / 'mods'), 'F50_GUARD_CACHE': str(d / 'cache'),
                   'F50_HELPER_SYSCTL': str(helper), 'F50_AUTOLOAD_ALLOW': str(d / 'allow')}
            r = subprocess.run(['sh', str(self.GUARD), '--prepare'], env=env, capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(helper.read_text(), '/opt/f50/bin/f50-modprobe-guard\n')
            table = (d / 'cache' / '7.2.11').read_text().splitlines()
            self.assertIn('net_pf_30 tipc', table)
            self.assertIn('net_pf_16_proto_16_family_TIPCv2 tipc', table)
            self.assertEqual((d / 'cache').stat().st_mode & 0o777, 0o700)
            # with the switch file the helper stays the system's
            helper.write_text('/sbin/modprobe\n')
            (d / 'allow').write_text('')
            subprocess.run(['sh', str(self.GUARD), '--prepare'], env=env, capture_output=True, text=True)
            self.assertEqual(helper.read_text(), '/sbin/modprobe\n')

    def test_both_systems_install_it(self):
        for system in ('base', 'openwrt'):
            conf = (TOP / 'systems' / system / 'overlay/etc/sysctl.d/98-f50-modules.conf').read_text()
            self.assertIn('dev.tty.ldisc_autoload = 0', conf)
        unit = (TOP / 'systems/base/overlay/etc/systemd/system/f50-modprobe-guard.service').read_text()
        self.assertIn('ExecStart=/opt/f50/bin/f50-modprobe-guard --prepare', unit)
        self.assertIn('f50-modprobe-guard.service:sysinit.target', (TOP / 'systems/base/assemble.sh').read_text())
        hw = (TOP / 'systems/openwrt/overlay/etc/init.d/f50-hw').read_text()
        self.assertIn('/opt/f50/bin/f50-modprobe-guard --prepare', hw)
        # OpenWrt's image takes /opt/f50 from the Ubuntu overlay, and both copy their overlay whole
        build = (TOP / 'systems/openwrt/build-rootfs.sh').read_text()
        self.assertIn('systems/base/overlay/opt/f50":/in/opt-f50', build)
        self.assertIn('cp -a /in/overlay/. $R/', build)
        self.assertTrue(self.GUARD.stat().st_mode & 0o111)


if __name__ == '__main__':
    unittest.main()
