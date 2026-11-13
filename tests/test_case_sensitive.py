"""Kernel modules whose names differ only in case (FINDINGS 212): xt_HL.ko and xt_hl.ko, xt_TCPMSS.ko and
xt_tcpmss.ko, xt_DSCP.ko and xt_dscp.ko, xt_RATEEST.ko and xt_rateest.ko. A tree on a file system that ignores case
keeps one file of each pair; the release scripts refuse such a directory, the audits find a bundle that lost one,
and the installers take a mainline kernel's modules from its tarball, never from a tree unpacked on the host."""
import importlib.util
import io
import os
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

TOP = Path(__file__).resolve().parents[1]
PAIRS = ('xt_HL.ko', 'xt_hl.ko', 'xt_TCPMSS.ko', 'xt_tcpmss.ko', 'xt_DSCP.ko', 'xt_dscp.ko', 'xt_RATEEST.ko',
         'xt_rateest.ko')


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, TOP / path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def folds(d):
    """whether d ignores case, asked the same way case_sensitive does"""
    probe = Path(tempfile.mkdtemp(dir=d))
    try:
        (probe / 'f50-case').write_text('')
        (probe / 'F50-CASE').write_text('')
        return len(os.listdir(probe)) != 2
    finally:
        for p in probe.iterdir():
            p.unlink()
        probe.rmdir()


def add(t, name, data=b''):
    info = tarfile.TarInfo(name)
    info.size = len(data)
    t.addfile(info, io.BytesIO(data))


@unittest.skipIf(os.name == 'nt', 'the release scripts run on a Unix host')
class CaseSensitive(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.d = Path(self.tmp.name)

    def run_check(self, path=None):
        env = dict(os.environ)
        if path:
            env['PATH'] = path + os.pathsep + env['PATH']
        return subprocess.run(['sh', '-c', '. "$1" && case_sensitive "$2"', 'sh', str(TOP / 'kernel/mainline/case-sensitive.sh'),
                               str(self.d)], capture_output=True, text=True, env=env)

    def test_the_host_directory(self):
        # whatever this host's temporary directory is, the check says the same as Python does, and leaves nothing
        r = self.run_check()
        self.assertEqual(r.returncode, 1 if folds(self.d) else 0, r.stderr)
        self.assertEqual(os.listdir(self.d), [])

    def test_a_directory_that_ignores_case(self):
        # a file system that keeps one of the two names, made here with an ls that shows one
        bin_ = self.d / 'bin'
        bin_.mkdir()
        (bin_ / 'ls').write_text('#!/bin/sh\necho f50-case\n')
        (bin_ / 'ls').chmod(0o755)
        self.d = self.d / 'stage'
        self.d.mkdir()
        r = self.run_check(str(bin_))
        self.assertEqual(r.returncode, 1)
        self.assertIn(f'{self.d} is on a file system that ignores case', r.stderr)
        self.assertIn("-fs 'Case-sensitive APFS'", r.stderr)
        self.assertIn('FINDINGS.md 212', r.stderr)
        self.assertEqual(os.listdir(self.d), [])

    def test_a_directory_it_cannot_write(self):
        self.d = self.d / 'missing'
        r = self.run_check()
        self.assertEqual(r.returncode, 1)
        self.assertIn('cannot make a directory in', r.stderr)


class VendorOverlay(unittest.TestCase):
    def test_both_modules_of_a_pair_from_the_tarball(self):
        vo = load('vendor_overlay', 'installer/lib/vendor-overlay.py')
        with tempfile.TemporaryDirectory() as d:
            bundle = Path(d) / 'f50-kernel-6.18.tar.gz'
            with tarfile.open(bundle, 'w:gz') as t:
                add(t, './kernel.release', b'6.18.57-f50\n')
                add(t, './modules.builtin', b'kernel/x.ko\n')
                add(t, './Image', b'kernel')
                for m in PAIRS + ('vhost_net.ko',):
                    add(t, f'./modules/{m}', m.encode())
            for os_name, sub in (('ubuntu', 'usr/lib/modules/6.18.57-f50/extra'), ('openwrt', 'lib/modules/6.18.57-f50')):
                out = io.BytesIO()
                with tarfile.open(fileobj=out, mode='w') as t:
                    krel, n = vo.add_kernel_bundle(t, bundle, os_name, set())
                self.assertEqual((krel, n), ('6.18.57-f50', len(PAIRS) + 1))
                out.seek(0)
                with tarfile.open(fileobj=out) as t:
                    for m in PAIRS:
                        self.assertEqual(t.extractfile(f'{sub}/{m}').read(), m.encode(), m)
                    names = t.getnames()
                self.assertNotIn(f'{sub}/Image', names)
                self.assertEqual(os_name == 'ubuntu', 'usr/lib/modules/6.18.57-f50/modules.builtin' in names)


if __name__ == '__main__':
    unittest.main()
