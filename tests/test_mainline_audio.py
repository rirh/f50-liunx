"""The unsupported call-audio stack must not return through builds or extras."""
import unittest
from helpers import TOP
from test_extra import ExtrasBase

UP = TOP / 'kernel' / 'mainline'
OVERLAY = TOP / 'systems' / 'base' / 'overlay'


class NoCallAudio(ExtrasBase):
    def test_audio_assets_are_not_accepted_as_official_modules(self):
        for shell in self.each_shell():
            r = self.up(shell, 'module_asset_ok f50-audio-7.2.tar.gz && exit 4; '
                        'module_asset_ok f50-linux-vpn-core.tar.gz')
            self.assertEqual(r.returncode, 0, (r.stdout, r.stderr))

    def test_failed_legacy_uninstall_preserves_files_for_retry(self):
        for shell in self.each_shell():
            old = self.disk / 'extra/audio'
            (old / 'hooks').mkdir(parents=True, exist_ok=True)
            (old / 'hooks/unlink').write_text('exit 1\n')
            r = self.up(shell, 'retire_call_audio')
            self.assertNotEqual(r.returncode, 0)
            self.assertTrue(old.exists())
            (old / 'hooks/unlink').write_text('exit 0\n')
            self.assertEqual(self.up(shell, 'retire_call_audio').returncode, 0)
            self.assertFalse(old.exists())

    def test_no_call_audio_kernel_reservations_or_build_hooks(self):
        text = (TOP / 'kernel/vendor/f50-linux.fragment').read_text()
        self.assertNotIn('CONFIG_OF_RESERVED_MEM_ADD=', text)
        self.assertNotIn('CONFIG_SND_SOC_SPRD=m', text)
        self.assertFalse(UP.exists())
        self.assertFalse((TOP / 'kernel/vendor/patches/of-reserved-mem-add.patch').exists())
        self.assertNotIn('of-reserved-mem-add', (TOP / 'kernel/vendor/build-all.sh').read_text())
        # External USB sound cards are independent of the removed DSP stack.
        self.assertIn('CONFIG_SND_USB_AUDIO=y', text)


class NotHereAnyMore(unittest.TestCase):
    def test_no_driver_of_the_stack_is_built(self):
        self.assertFalse((TOP / 'kernel' / 'vendor' / 'build-audio.sh').exists())
        for p in ('audio-*.patch', 'mcdt-enable-clock.patch', 'sprd-card-dummy-on-defer.patch'):
            self.assertEqual(list((TOP / 'kernel' / 'vendor' / 'patches').glob(p)), [], p)

    def test_no_program_or_service_of_the_stack_is_in_the_images(self):
        for f in ('opt/f50/bin/f50-audio-dsp', 'opt/f50/bin/f50-voice', 'etc/systemd/system/f50-audio.service',
                  'etc/systemd/system/f50-voice.service', 'etc/modprobe.d/f50-audio.conf'):
            self.assertFalse((OVERLAY / f).exists(), f)
        for f in ('etc/init.d/f50-audio', 'etc/init.d/f50-voice'):
            self.assertFalse((TOP / 'systems' / 'openwrt' / 'overlay' / f).exists(), f)
        for t in ('pcmhost', 'ctlset', 'hfp'):
            self.assertFalse((TOP / 'device-tools' / t).exists(), t)


if __name__ == '__main__':
    unittest.main()
