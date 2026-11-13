"""The U30 Air's firmware SAE offload is disabled, in every layer (FINDINGS 80, docs/research/U30AIR-SAE.md): the hotspot is
WPA2-PSK only (the SC2355).

* the driver (mainline tree and the 5.4 patch): the firmware's PMK record never reaches cfg80211 - the strip function
  compiled from the source (with UBSan when F50_TEST_UBSAN=1) against records, malformed lists and random input;
  the vendor command that gave the firmware the SAE password is refused;
* no hostapd with the import is built or installed;
* the wifi-scripts refuse an SAE access point (static, and in the OpenWrt feed's scripts with F50_WRT_IMAGE);
* the LuCI notice module of a hotspot f50d moved off SAE (/etc/f50/wpa3-disabled), under Node, and its ACL."""
import json
import os
import random
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from helpers import BIN, TOP, ShellTest

WLAN = TOP / 'kernel/mainline/modules/sprd_wlan_combo'
PATCH54 = TOP / 'kernel/vendor/patches/wlan_combo-softap-no-sae.patch'
WIFI_PATCH = TOP / 'systems/openwrt/patches/wifi-scripts-wpa2-only.patch'
NOTICE_JS = TOP / 'systems/openwrt/overlay/www/luci-static/resources/view/status/include/05_f50_wpa2.js'
RECORD_PREFIX = bytes([0xdd, 0x34, 0x40, 0x45, 0xda, 0x04])


def c_function(source, name):
    """the text of C function NAME in SOURCE, from its 'static' line to its closing brace"""
    start = source.index('static u16 ' + name + '(')
    i = source.index('{', start)
    depth = 0
    while True:
        if source[i] == '{':
            depth += 1
        elif source[i] == '}':
            depth -= 1
            if depth == 0:
                return source[start:i + 1]
        i += 1


def added_lines(patch, path):
    """the '+' lines of the hunks of PATH in a unified diff"""
    out, on = [], False
    for line in patch.splitlines():
        if line.startswith('+++ '):
            on = line[4:].strip() == 'b/' + path
            continue
        if line.startswith('--- '):
            continue
        if on and line.startswith('+'):
            out.append(line[1:])
    return '\n'.join(out) + '\n'


def strip_model(ies):
    """what the driver must leave: the well-formed elements, in order, without any dd ..40:45:da:04 element"""
    out, i = b'', 0
    while i + 2 <= len(ies):
        n = ies[i + 1]
        if i + 2 + n > len(ies):
            break
        e = ies[i:i + 2 + n]
        if not (e[0] == 221 and n >= 4 and e[2:6] == RECORD_PREFIX[2:]):
            out += e
        i += 2 + n
    return out


class DriverPrivacy(unittest.TestCase):
    """the driver never logs the passphrase, a station key or raw command/event payloads (kept from the offload work)"""

    def body(self, path, name):
        source = (TOP / 'kernel/mainline/modules/sprd_wlan_combo' / path).read_text()
        start = source.index('{', source.index(name + '('))
        depth = 1
        end = start + 1
        while depth:
            if source[end] == '{': depth += 1
            if source[end] == '}': depth -= 1
            end += 1
        return source[start:end]

    def test_transport_command_payloads_never_reach_hex_dump(self):
        for path, name in [('sc2355/pcie.c', 'pcie_tx_one'),
                           ('sc2355/sdio.c', 'sdio_tx_one'),
                           ('sc2355/cmdevt.c', 'cmdevt_send_cmd'),
                           ('sc2355/cmdevt.c', 'sc2355_rx_rsp_process')]:
            with self.subTest(path=path, name=name):
                self.assertNotIn('hex_dump', self.body(path, name))

    def test_transport_rx_cannot_dump_a_new_station_pmk(self):
        for path, name in [('sc2355/pcie.c', 'sc2355_pcie_rx_work_queue'),
                           ('sc2355/sdio.c', 'sc2355_rx_work_queue')]:
            with self.subTest(path=path):
                self.assertNotIn('hex_dump', self.body(path, name))

class NoImportIsBuilt(ShellTest):
    def test_the_hostapd_patch_and_its_recipe_are_gone(self):
        self.assertFalse((TOP / 'systems/openwrt/patches/hostapd').exists())
        self.assertFalse((TOP / 'systems/openwrt/build-u30-sae.sh').exists())
        self.assertFalse((BIN / 'openwrt-wifi-sae-guard').exists())
        build = (TOP / 'systems/openwrt/build-rootfs.sh').read_text()
        for gone in ('/in/u30-sae', 'wpad-basic-openssl', 'SAE_APKS'):
            self.assertNotIn(gone, build)
        self.assertIn('apk add wpad-basic-mbedtls wifi-scripts', build)

    def test_no_code_turns_it_on(self):
        """u30_sae_offload is named only where it is refused or removed (and in the docs and tests)"""
        allowed = {'systems/openwrt/patches/wifi-scripts-wpa2-only.patch'}
        out = subprocess.run(['git', '-C', str(TOP), 'grep', '-l', 'u30_sae_offload', '--', '.', ':!docs', ':!tests',
                              ':!wiki', ':!site', ':!README.md'], capture_output=True, text=True)
        if out.returncode not in (0, 1):
            self.skipTest('not a git checkout')
        self.assertEqual(set(out.stdout.split()) - allowed, set())


class WifiScriptsRefuseSae(unittest.TestCase):
    def test_patch(self):
        p = WIFI_PATCH.read_text()
        self.assertTrue(p.startswith('--- a/usr/share/ucode/wifi/hostapd.uc\n+++ b/usr/share/ucode/wifi/hostapd.uc\n'))
        self.assertIn("[ 'sae', 'psk3', 'sae-mixed', 'psk3-mixed', 'sae-compat' ]", p)
        self.assertIn("'u30_sae_offload*'", p)
        self.assertIn('+\t\tif (f50_sae_refused(k, interface.config))\n+\t\t\tcontinue;', p)
        self.assertIn('WPA2-PSK only', p)
        build = (TOP / 'systems/openwrt/build-rootfs.sh').read_text()
        self.assertIn('-v "$WPA2PATCH":/in/wifi-scripts-wpa2-only.patch:ro', build)
        # after the DFS patch, whose log import it uses, and checked like it
        self.assertLess(build.index('-i /in/wifi-scripts-no-dfs.patch'), build.index('-i /in/wifi-scripts-wpa2-only.patch'))
        self.assertIn('grep -q "f50_sae_refused(k, interface.config)" $R/usr/share/ucode/wifi/hostapd.uc ||', build)

    @unittest.skipUnless(os.environ.get('F50_WRT_IMAGE') and shutil.which('docker'),
                         'F50_WRT_IMAGE (an OpenWrt 25.12 arm64 docker image) not set')
    def test_feed_scripts_start_wpa2_and_refuse_sae(self):
        import test_wifi_channels as channels
        fake = channels.WifiScriptsInOpenWrt
        cases = {'psk2': {}, 'psk2-ccmp': {'encryption': 'psk2+ccmp'}, 'sae': {'encryption': 'sae'},
                 'sae-mixed': {'encryption': 'sae-mixed'}, 'sae-compat': {'encryption': 'sae-compat'},
                 'psk3': {'encryption': 'psk3'},
                 'offload': {'encryption': 'psk2', 'hostapd_bss_options': ['u30_sae_offload=1']}}
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / 'cases').mkdir()
            (d / 'out').mkdir()
            for name, iface in cases.items():
                data = fake.data(36, 'VHT80')
                data['interfaces']['ap0']['config'].update(iface)
                (d / 'cases' / f'{name}.wiphy.json').write_text(json.dumps(fake.wiphy(False, {})))
                (d / 'cases' / f'{name}.data.json').write_text(json.dumps(data))
            script = r"""
mkdir -p /var/lock /var/run /tmp /fake/wifi
apk update >/dev/null && apk add wifi-scripts patch >/dev/null 2>&1 || exit 1
patch --batch --fuzz=0 -p1 -d / -i /in/wifi-scripts-no-dfs.patch || exit 1
patch --batch --fuzz=0 -p1 -d / -i /in/wifi-scripts-wpa2-only.patch || exit 1
sed -e "s/^export function wiphy_info(phy) {/function nl_wiphy_info(phy) {/" /usr/share/ucode/wifi/common.uc > /fake/wifi/common.uc
echo "export function wiphy_info(phy) { return json(fs.readfile(getenv(\"FAKE_WIPHY\"))); };" >> /fake/wifi/common.uc
for f in /t/cases/*.data.json; do
    n=$(basename "$f" .data.json)
    rm -f /var/run/hostapd-phy0.conf
    FAKE_DATA=$f FAKE_WIPHY=/t/cases/$n.wiphy.json FAKE_CC=CN \
        ucode -L "/fake/*.uc" -L "/usr/share/ucode/*.uc" /fx/run.uc > /t/out/$n.conf 2>&1 || echo "rc=$?" >> /t/out/$n.conf
done
"""
            r = subprocess.run(['docker', 'run', '--rm', '--platform', 'linux/arm64',
                                '-v', f'{WIFI_PATCH.parent}:/in:ro', '-v', f'{channels.FIXTURES}:/fx:ro', '-v', f'{d}:/t',
                                os.environ['F50_WRT_IMAGE'], '/bin/sh', '-c', script],
                               capture_output=True, text=True, timeout=900)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            for name in cases:
                text = (d / 'out' / f'{name}.conf').read_text()
                with self.subTest(case=name):
                    self.assertNotIn('rc=', text)
                    conf = dict(l.split('=', 1) for l in text.splitlines() if '=' in l and not l.startswith('#'))
                    if name.startswith('psk2'):
                        self.assertEqual((conf['wpa_key_mgmt'], conf.get('wpa_pairwise', conf.get('rsn_pairwise'))), ('WPA-PSK', 'CCMP'), text)
                        self.assertNotIn('refused', text, text)
                        self.assertIn('ssid2="F50-TEST"', text)
                    else:
                        self.assertIn("wifi-scripts: ap0: encryption '", text)
                        self.assertIn('WPA2-PSK only', text)
                        self.assertNotIn('interface=wlan0', text)
                        self.assertNotIn('wpa_passphrase', text)
                        self.assertNotIn('wpa_key_mgmt', text)
                        self.assertNotIn('12345678', text)


@unittest.skipUnless(shutil.which('node'), 'no node')
class NoticeModule(unittest.TestCase):
    """the LuCI notice (Status > Overview and the panel's home): nothing without the file, the message in the
    language of the page (English, Turkish, Chinese), the dismissal removes the file"""

    def render(self, content, lang):
        harness = r"""
const fs = require('fs');
const src = fs.readFileSync(process.argv[2], 'utf8');
const content = process.argv[3] === '-' ? null : process.argv[3];
const removed = [];
function E(tag, attrs, children) {
  const kids = (Array.isArray(children) ? children : children == null ? [] : [children]);
  return { tag, attrs, style: {}, children: kids, appendChild(c) { kids.push(c); } };
}
global.E = E;
global.document = { documentElement: { getAttribute: () => process.argv[4] } };
global.L = { resolveDefault: (p, d) => Promise.resolve(p).catch(() => d) };
const baseclass = { extend: (o) => o };
const fsmod = { read: (p) => content == null ? Promise.reject(new Error('NotFound')) : Promise.resolve(content),
                remove: (p) => { removed.push(p); return Promise.resolve(0); } };
const ui = { createHandlerFn: (ctx, fn) => fn.bind(ctx) };
const body = src.split('\n').filter(l => !/^'require /.test(l) && l !== "'use strict';").join('\n');
const mod = new Function('baseclass', 'fs', 'ui', body)(baseclass, fsmod, ui);
function text(n) { return typeof n === 'string' ? n : (n.children || []).map(text).join(''); }
mod.load().then(async (n) => {
  const box = mod.render(n);
  const out = { title: mod.title, box: box ? text(box) : null, cls: box ? box.attrs['class'] : null };
  if (box) { await box.children[1].attrs.click(); out.removed = removed; out.hidden = box.style.display; }
  console.log(JSON.stringify(out));
});
"""
        with tempfile.TemporaryDirectory() as d:
            h = Path(d) / 'h.js'
            h.write_text(harness)
            r = subprocess.run(['node', str(h), str(NOTICE_JS), content if content is not None else '-', lang],
                               capture_output=True, text=True, timeout=30)
            self.assertEqual(r.returncode, 0, r.stderr)
            return json.loads(r.stdout)

    def test_no_file_no_notice(self):
        self.assertIsNone(self.render(None, 'en')['box'])
        self.assertIsNone(self.render('garbage\n', 'en')['box'])

    def test_messages(self):
        migrated = 'state=migrated\ntime=2026-10-17 10:00:00\nmigrated=ap0\ndown=\n'
        down = 'state=down\ntime=2026-10-17 10:00:00\nmigrated=\ndown=ap0\n'
        o = self.render(migrated, 'en')
        self.assertIn('WPA2-PSK (CCMP) with the same password: ap0', o['box'])
        self.assertIn('2026-10-17 10:00:00', o['box'])
        self.assertIn('warning', o['cls'])
        self.assertEqual((o['removed'], o['hidden']), (['/etc/f50/wpa3-disabled'], 'none'))
        o = self.render(down, 'en')
        self.assertIn('ap0 is off, not open', o['box'])
        self.assertIn('error', o['cls'])
        self.assertIn('WPA2-PSK\'ye (CCMP)', self.render(migrated, 'tr')['box'])
        self.assertIn('Kapat', self.render(migrated, 'tr')['box'])
        zh = self.render(migrated, 'zh-cn')
        self.assertIn('WPA2-PSK\uff08CCMP\uff09', zh['box'])
        self.assertIn('\u5b89\u5168', zh['title'])
        # any other language: English
        self.assertIn('Dismiss', self.render(migrated, 'de')['box'])

    def test_acl_allows_only_this_file(self):
        acl = json.loads((TOP / 'systems/openwrt/overlay/usr/share/rpcd/acl.d/f50-wpa2-notice.json').read_text())
        g = acl['f50-wpa2-notice']
        self.assertEqual(list(g['read']['file']), ['/etc/f50/wpa3-disabled'])
        self.assertEqual(list(g['write']['file']), ['/etc/f50/wpa3-disabled'])
        self.assertEqual(g['write']['ubus'], {'file': ['remove']})


if __name__ == '__main__':
    unittest.main()
