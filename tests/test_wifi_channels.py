"""The hotspot never goes onto a DFS channel (FINDINGS 42: the SC2355 driver has no radar detection, hostapd's
start_dfs_cac() fails and the AP stays down): `systems/openwrt/patches/wifi-scripts-no-dfs.patch`, how the build applies
it, and - with F50_WRT_IMAGE naming an OpenWrt 25.12 docker image (e.g. f50-openwrt-base:25.12.6, as
systems/openwrt/build-rootfs.sh imports it) - the hostapd configuration the patched wifi-scripts generate for a fake
wiphy with and without radar detection.
"""
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from helpers import TOP

PATCH = TOP / 'systems' / 'openwrt' / 'patches' / 'wifi-scripts-no-dfs.patch'
FIXTURES = TOP / 'tests' / 'fixtures' / 'wifi-scripts'
FIVE = [36, 40, 44, 48, 52, 56, 60, 64, 100, 104, 108, 112, 116, 120, 124, 128, 132, 136, 140, 144,
        149, 153, 157, 161, 165]


class WifiScriptsPatch(unittest.TestCase):
    def test_build_applies_it(self):
        text = (TOP / 'systems' / 'openwrt' / 'build-rootfs.sh').read_text()
        self.assertIn('-v "$NODFSPATCH":/in/wifi-scripts-no-dfs.patch:ro', text)
        apply = 'patch --batch --fuzz=0 -d $R -p1 -i /in/wifi-scripts-no-dfs.patch'
        self.assertIn(apply, text)
        self.assertIn('grep -q "device_no_dfs(config);" $R/usr/share/ucode/wifi/hostapd.uc || {', text)
        # after the copy into $R, before the patch tool goes
        self.assertLess(text.index('cp -a /in/overlay/. $R/'), text.index(apply))
        self.assertLess(text.index(apply), text.index('apk del patch'))

    def test_patch(self):
        p = PATCH.read_text()
        self.assertTrue(p.startswith('--- a/usr/share/ucode/wifi/hostapd.uc\n+++ b/usr/share/ucode/wifi/hostapd.uc\n'))
        added = '\n'.join(l[1:] for l in p.splitlines() if l.startswith('+') and not l.startswith('+++'))
        # the capability is read from the driver, not assumed: radar_detect_widths in an interface combination
        self.assertIn('phy.interface_combinations', added)
        self.assertIn('c.radar_detect_widths', added)
        self.assertIn('config.acs_exclude_dfs = true;', added)
        self.assertIn('device_no_dfs(config);', added)
        self.assertIn('device_chanlist_band(config);', added)


@unittest.skipUnless(os.environ.get('F50_WRT_IMAGE') and shutil.which('docker'),
                     'F50_WRT_IMAGE (an OpenWrt 25.12 arm64 docker image) not set')
class WifiScriptsInOpenWrt(unittest.TestCase):
    """The patched wifi-scripts of the OpenWrt feed, for a fake wiphy: the hostapd configuration they write."""

    @staticmethod
    def wiphy(radar, flags):
        comb = {'maxnum': 3, 'num_channels': 2}
        if radar:
            comb['radar_detect_widths'] = 63
        freqs = []
        for c in FIVE:
            e = {'freq': 5000 + 5 * c}
            e.update(flags.get(c, {'radar': True, 'no_ir': True} if 52 <= c <= 144 else {}))
            freqs.append(e)
        return {'extended_features': [0] * 16, 'cipher_suites': [], 'interface_combinations': [comb],
                'wiphy_bands': [{'freqs': [{'freq': 2407 + 5 * c} for c in range(1, 14)], 'ht_capa': 0x1fe},
                                {'freqs': freqs, 'ht_capa': 0x1fe,
                                                               'vht_capa': 0x339071b2}]}

    @staticmethod
    def data(channel, htmode, extra=None):
        cfg = {'band': '5g', 'channel': str(channel), 'htmode': htmode, 'country': 'CN'}
        cfg.update(extra or {})
        return {'phy': 'phy0', 'phy_suffix': '', 'vif_phy_suffix': '', 'config': cfg,
                'interfaces': {'ap0': {'config': {'mode': 'ap', 'ssid': 'F50-TEST', 'encryption': 'psk2',
                                                  'key': '12345678', 'ifname': 'wlan0', 'network': ['lan']},
                                       'bridge': 'br-lan', 'vlans': {}, 'stas': {}}}}

    def test_hostapd_config(self):
        no36 = {c: {'disabled': True} for c in (36, 40, 44, 48)}
        cases = {
            # name: (driver detects radar, channel flags, channel, htmode, extra) -> expected
            'cn60': (False, {}, 60, 'VHT80', None),
            'cn60-radar': (True, {}, 60, 'VHT80', None),
            'cn36': (False, {}, 36, 'VHT80', None),
            'cn149': (False, {}, 149, 'VHT80', None),
            'cn52-he160': (False, {}, 52, 'HE160', None),
            'cn120-vht40': (False, {}, 120, 'VHT40', None),
            'auto': (False, {}, 'auto', 'VHT80', None),
            'auto-radar': (True, {}, 'auto', 'VHT80', None),
            'auto-list': (False, {}, 'auto', 'VHT80', {'channels': '36 40 44 48 149 153 157 161 165'}),
            'no36-60': (False, no36, 60, 'VHT80', None),
            'no5g-60': (False, {c: {'disabled': True} for c in FIVE if not 52 <= c <= 144}, 60, 'VHT80', None),
            # #424: LuCI changed the band to 2.4 GHz and left the 5 GHz list openwrt-wifi-config wrote for auto
            '2g-auto-5glist': (False, {}, 'auto', 'HT20', {'band': '2g', 'channels': '36 40 44 48'}),
            '2g-auto-mixed': (False, {}, 'auto', 'HT20', {'band': '2g', 'channels': ['1 6', '36', '1-11', '1-36']}),
            '2g-auto-list': (False, {}, 'auto', 'HT20', {'band': '2g', 'channels': '1 6 11'}),
        }
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / 'cases').mkdir()
            (d / 'out').mkdir()
            for n, (radar, flags, ch, ht, extra) in cases.items():
                (d / 'cases' / f'{n}.wiphy.json').write_text(json.dumps(self.wiphy(radar, flags)))
                (d / 'cases' / f'{n}.data.json').write_text(json.dumps(self.data(ch, ht, extra)))
            script = r'''
mkdir -p /var/lock /var/run /tmp /fake/wifi
apk update >/dev/null && apk add wifi-scripts patch >/dev/null 2>&1 || exit 1
patch --batch --fuzz=0 -p1 -d / -i /in/wifi-scripts-no-dfs.patch || exit 1
grep -q "device_no_dfs(config);" /usr/share/ucode/wifi/hostapd.uc || exit 1
sed -e "s/^export function wiphy_info(phy) {/function nl_wiphy_info(phy) {/" /usr/share/ucode/wifi/common.uc > /fake/wifi/common.uc
grep -q "^function nl_wiphy_info" /fake/wifi/common.uc || exit 1
echo "export function wiphy_info(phy) { return json(fs.readfile(getenv(\"FAKE_WIPHY\"))); };" >> /fake/wifi/common.uc
for f in /t/cases/*.data.json; do
    n=$(basename "$f" .data.json)
    FAKE_DATA=$f FAKE_WIPHY=/t/cases/$n.wiphy.json FAKE_CC=CN \
        ucode -L "/fake/*.uc" -L "/usr/share/ucode/*.uc" /fx/run.uc > /t/out/$n.conf 2>&1 || echo "rc=$?" >> /t/out/$n.conf
done
'''
            r = subprocess.run(['docker', 'run', '--rm', '--platform', 'linux/arm64',
                                '-v', f'{PATCH.parent}:/in:ro', '-v', f'{FIXTURES}:/fx:ro', '-v', f'{d}:/t',
                                os.environ['F50_WRT_IMAGE'], '/bin/sh', '-c', script],
                               capture_output=True, text=True, timeout=900)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

            def conf(n):
                text = (d / 'out' / f'{n}.conf').read_text()
                self.assertNotIn('rc=', text, n)
                return text, dict(l.split('=', 1) for l in text.splitlines() if '=' in l and not l.startswith('#'))

            text, c = conf('cn60')
            self.assertIn('wifi-scripts: channel 60 is a DFS channel and this driver cannot detect radar', text)
            self.assertEqual((c['channel'], c['vht_oper_chwidth'], c['vht_oper_centr_freq_seg0_idx']), ('36', '1', '42'))
            # a driver that can detect radar keeps the channel (hostapd does the CAC)
            text, c = conf('cn60-radar')
            self.assertEqual((c['channel'], c['vht_oper_centr_freq_seg0_idx']), ('60', '58'))
            self.assertNotIn('DFS channel', text)
            for n, ch, seg0 in (('cn36', '36', '42'), ('cn149', '149', '155')):
                text, c = conf(n)
                self.assertEqual((c['channel'], c['vht_oper_centr_freq_seg0_idx']), (ch, seg0), n)
                self.assertNotIn('DFS channel', text)
            # 160 MHz cannot fit without DFS: 80
            _, c = conf('cn52-he160')
            self.assertEqual((c['channel'], c['he_oper_chwidth'], c['he_oper_centr_freq_seg0_idx']), ('36', '1', '42'))
            _, c = conf('cn120-vht40')
            self.assertEqual((c['channel'], c['vht_oper_chwidth'], c['ht_capab'][:7]), ('36', '0', '[HT40+]'))
            # auto: ACS without the DFS channels (and the channel list as given)
            _, c = conf('auto')
            self.assertEqual((c['channel'], c['acs_exclude_dfs']), ('0', '1'))
            _, c = conf('auto-radar')
            self.assertEqual((c['channel'], c['acs_exclude_dfs']), ('0', '0'))
            _, c = conf('auto-list')
            self.assertEqual((c['acs_exclude_dfs'], c['chanlist']), ('1', '36 40 44 48 149 153 157 161 165'))
            # 36-48 not allowed: 149
            _, c = conf('no36-60')
            self.assertEqual((c['channel'], c['vht_oper_centr_freq_seg0_idx']), ('149', '155'))
            # a channel list off the radio's band is dropped (#424): ACS over the whole band instead of no channel
            text, c = conf('2g-auto-5glist')
            self.assertEqual((c['hw_mode'], c['channel']), ('g', '0'))
            self.assertNotIn('chanlist', c)
            self.assertIn('channel list: 36 40 44 48 not on the 2g band, left out; automatic channel selection uses '
                          'the whole band', text)
            text, c = conf('2g-auto-mixed')
            self.assertEqual(c['chanlist'], '1 6 1-11')
            self.assertIn('channel list: 36 1-36 not on the 2g band, left out\n', text + '\n')
            text, c = conf('2g-auto-list')
            self.assertEqual(c['chanlist'], '1 6 11')
            self.assertNotIn('channel list', text)
            # nothing to go to: left as it was, and said
            text, c = conf('no5g-60')
            self.assertEqual(c['channel'], '60')
            self.assertIn('no other 5 GHz channel is allowed', text)


if __name__ == '__main__':
    unittest.main()
