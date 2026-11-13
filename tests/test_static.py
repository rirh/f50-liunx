"""Checks over every script without running it: syntax under each shell that runs it, executable bits, and rules
that past bugs taught (see each test)."""
import json
import os
import gzip
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from helpers import BIN, TOP, shells

OPENWRT = TOP / 'systems' / 'openwrt' / 'overlay'


def shebang(p):
    try:
        with open(p, 'rb') as f:
            first = f.readline()
    except OSError:
        return ''
    return first.decode(errors='replace').strip() if first.startswith(b'#!') else ''


def shell_scripts():
    """(path, 'sh'|'bash') of every shell script the project ships or runs."""
    cands = list(BIN.iterdir()) + [p for d in ('installer/lib', 'installer/android', 'scripts/release', 'scripts/device')
                                    for p in (TOP / d).glob('*.sh')] + [
        TOP / 'install.sh', TOP / 'uninstall.sh', TOP / 'boot' / 'init', TOP / 'systems' / 'openwrt' / 'build-rootfs.sh',
        TOP / 'kernel' / 'vendor' / 'build-all.sh', TOP / 'installer' / 'lib' / 'i18n.sh',
        TOP / 'installer' / 'lib' / 'linux-mode.sh', TOP / 'installer' / 'lib' / 'storage.sh', TOP / 'android' / 'vendor' / 'ueventd-perms.sh']
    cands += list((TOP / 'android' / 'magisk' / 'f50-linux-switch').glob('*.sh'))
    cands += [p for p in OPENWRT.rglob('*') if p.is_file()]
    out = []
    for p in sorted(set(cands)):
        if not p.is_file():
            continue
        sb = shebang(p)
        if 'bash' in sb:
            out.append((p, 'bash'))
        elif sb.endswith('sh') or 'rc.common' in sb or (not sb and p.suffix == '.sh'):
            out.append((p, 'sh'))
    return out


class Syntax(unittest.TestCase):
    def test_every_script_parses(self):
        sh_shells = [s for s in shells() if s[0] != 'bash'] or shells()
        scripts = shell_scripts()
        self.assertGreater(len(scripts), 40)
        for p, kind in scripts:
            if kind == 'bash' and not shutil.which('bash'):
                continue
            for s in ([['bash']] if kind == 'bash' else sh_shells):
                with self.subTest(script=str(p.relative_to(TOP)), shell=' '.join(s)):
                    r = subprocess.run(s + ['-n', str(p)], capture_output=True, text=True)
                    self.assertEqual(r.returncode, 0, r.stderr)

    def test_device_programs_are_executable(self):
        for p in BIN.iterdir():
            if p.is_file() and shebang(p):
                with self.subTest(p=p.name):
                    self.assertTrue(p.stat().st_mode & 0o111, 'not executable')


class Rules(unittest.TestCase):
    def test_init_restores_androids_slot(self):
        # every restore goes through restore_android, so Linux on slot a returns to Android on b
        init = (TOP / 'boot' / 'init').read_text()
        self.assertNotIn('restore_slot_a', init)
        self.assertNotIn('slot_suffix=_b/androidboot.slot_suffix=_a', init)
        self.assertIn('sleep 300', init)

    def test_single_quoted_scripts_have_no_apostrophes(self):
        # A script handed to `sh -c '...'` (docker run) ends at its first apostrophe: "the U30 Air's charger" in a
        # comment there broke OpenWrt's build at release time, while every parser still saw balanced quotes.
        for name in ('systems/openwrt/build-rootfs.sh', 'scripts/release/make-release.sh'):
            lines = (TOP / name).read_text().splitlines()
            for n, line in enumerate(lines):
                if not line.rstrip().endswith("-c '"):
                    continue
                for m in range(n + 1, len(lines)):
                    if "'" in lines[m]:
                        self.assertTrue(lines[m].rstrip().endswith("'") and lines[m].count("'") == 1,
                                        f'{name}:{m + 1}: an apostrophe inside the script of line {n + 1}')
                        break

    def test_every_tool_that_mounts_the_filesystem_knows_the_card(self):
        for f in ('uninstall.sh', 'scripts/device/reset-password.sh'):
            self.assertIn('f50sd', (TOP / f).read_text(), f)

    def test_uninstallers_have_no_per_system_list(self):
        # they remove the whole Linux filesystem; a per-system case arm added later would forget the third name
        for f in ('uninstall.sh',):
            text = (TOP / f).read_text()
            self.assertNotRegex(text, r'(^|\s)(ubuntu|openwrt)\)', f)

    def test_images_carry_no_vpn(self):
        # the VPN is the module of dikeckaan/mu300-linux-vpn: no f50-vpn, no drivers, no service, no engines in
        # the image or in this repository, and no release of this repository builds or publishes it
        for f in ('systems/base/overlay/opt/f50/bin/f50-vpn', 'systems/base/overlay/opt/f50/lib/vpn',
                  'systems/openwrt/overlay/etc/init.d/f50-vpn'):
            self.assertFalse((TOP / f).exists(), f)
        for f in ('systems/base/Dockerfile', 'systems/openwrt/build-rootfs.sh'):
            src = (TOP / f).read_text()
            for e in ('xray', 'sing-box', 'hev-socks5-tunnel', 'mihomo', 'f50-vpn'):
                self.assertNotRegex(src, rf'opt/f50/bin/{e}\b', (f, e))
            self.assertNotIn('vpn.conf.example', src, f)
            self.assertNotIn('lib/vpn', src, f)
        rel = (TOP / 'scripts/release/make-release.sh').read_text()
        self.assertNotIn('make-extra.sh', rel)
        self.assertNotIn('f50-extra-vpn', rel)
        self.assertNotIn('fetch-', rel)
        self.assertIn('https://github.com/dikeckaan/mu300-linux-vpn', rel)
        # the audit refuses an image that still has any of it
        self.assertIn('opt/f50/bin/(xray|sing-box|hev-socks5-tunnel|mihomo|f50-vpn)', rel)
        for pat in ('^opt/f50/lib/vpn(/|$)', 'etc/systemd/system/f50-vpn', 'etc/init\\.d/f50-vpn',
                    'etc/f50/vpn\\.conf\\.example'):
            self.assertIn(pat, rel)

    def test_the_release_source_is_wired(self):
        # a published image must not carry a configured release source, and the image has the example
        rel = (TOP / 'scripts/release/make-release.sh').read_text()
        self.assertIn("^etc/f50/(hotspot|vpn|toolkit|mobile-data)\\.conf$", rel)
        self.assertRegex(rel, r"\^etc/f50/update-source\$")
        self.assertNotIn('update-source.example', rel)
        for f in ('systems/openwrt/overlay/etc/f50/update-source.example',):
            text = (TOP / f).read_text()
            for s in ('F50_DOWNLOAD_URL=http://mirror.lan/', 'F50_API_URL=https://', '403', 'F50_MIRROR_FALLBACK=0'):
                self.assertIn(s, text, f)
        # the fallback tells an unreachable mirror from an answering one by curl's exit codes, so the image carries
        # curl (OpenWrt has only the wget family, where every failure looks the same)
        self.assertRegex((TOP / 'systems/openwrt/build-rootfs.sh').read_text(), r'apk add[^>]*\bcurl\b')
        updater = (TOP / 'systems/base/overlay/opt/f50/bin/f50-update').read_text()
        self.assertIn('source) shift; source_cmd "$@"', updater)
        self.assertIn('"source":%s}', updater)

    def test_images_carry_docker(self):
        # FINDINGS 130: Docker from OpenWrt's feed; dockerd does not start at boot, its data is on the Linux partition
        # beside the system (an update never copies it) and it uses Docker's nftables backend (no iptables-nft
        # translation, no xt_addrtype that the 5.4 kernel lacks)
        owrt = (TOP / 'systems/openwrt/build-rootfs.sh').read_text()
        # OpenWrt: in the image with F50_DOCKER=1, ready to install (its settings and zone only) by default - 190 MB,
        # twice over during an update, do not fit OpenWrt's smallest region (800 MiB)
        self.assertIn('case ${F50_DOCKER:-0} in', owrt)
        self.assertIn('-e F50_DOCKER="$DOCKER"', owrt)
        block = owrt[owrt.index('if [ "$F50_DOCKER" = 1 ]; then'):]
        block = block[:block.index('\nfi\n')]
        self.assertIn('\n    apk add dockerd docker docker-compose >/dev/null\n', block)
        self.assertNotIn('luci-app-dockerman', owrt)
        # what its kmods put in /etc/modules.d goes; it does not start at boot
        self.assertLess(block.index('ls /etc/modules.d /etc/modules-boot.d > /tmp/modules.before'),
                        block.index('apk add dockerd'))
        self.assertIn('/etc/rc.d/S*dockerd', block)
        self.assertIn('echo "dockerd starts at boot"', owrt)
        self.assertIn('rm -f $R/etc/uhttpd.key $R/etc/uhttpd.crt', owrt)
        # the build script is one single-quoted sh -c argument: no apostrophe may be in it
        start = "f50-openwrt-base:$VER /bin/sh -eu -c '"
        body = owrt[owrt.index(start) + len(start):owrt.rindex("ls -la /out/$OUT'")]
        self.assertNotIn("'", body)
        # the package reads Docker's own file (alt_config_file), which an apk add on the device leaves as it is
        cfg = (OPENWRT / 'etc/config/dockerd').read_text()
        self.assertIn("\toption alt_config_file '/etc/docker/daemon.json'\n", cfg)
        conf = json.loads((OPENWRT / 'etc/docker/daemon.json').read_text())
        self.assertEqual(conf['data-root'], '/mnt/f50-disk/docker/openwrt')
        self.assertIn('grep -q "\\"data-root\\": \\"/mnt/f50-disk/docker/openwrt\\"," $R/etc/docker/daemon.json', owrt)
        self.assertEqual(conf['firewall-backend'], 'nftables')
        self.assertEqual(conf['storage-driver'], 'overlay2')
        self.assertIs(conf['features']['containerd-snapshotter'], False)
        self.assertEqual(conf['default-address-pools'], [{'base': '172.16.0.0/12', 'size': 20}])
        sysctl = (OPENWRT / 'etc/sysctl.d/13-f50-docker.conf').read_text()
        self.assertIn('net.bridge.bridge-nf-call-iptables=0', sysctl)
        # the firewall: the docker zone covers only Docker's range, never br-lan; out to wan, in from the LAN only
        zone = (OPENWRT / 'etc/uci-defaults/95-f50-docker').read_text()
        self.assertTrue(os.access(OPENWRT / 'etc/uci-defaults/95-f50-docker', os.X_OK))
        for line in ("set network.docker.device='docker0'", "set network.@device[-1].name='docker0'",
                     "add_list firewall.docker.network='docker'", "add_list firewall.docker.device='br-+'", "add_list firewall.docker.device='!br-lan'",
                     "add_list firewall.docker.subnet='172.16.0.0/12'", "set firewall.docker_wan.dest='wan'",
                     "set firewall.lan_docker.src='lan'"):
            self.assertIn(line, zone)
        self.assertNotRegex(zone, r"src='wan'")
        # containers never reach the router itself, nor the LAN, nor one Docker network another (security review)
        self.assertIn("set firewall.docker.input='REJECT'", zone)
        self.assertIn("set firewall.docker.forward='REJECT'", zone)
        self.assertNotIn("firewall.docker.input='ACCEPT'", zone)
        self.assertNotRegex(zone, r"src='docker'\nset firewall\.[a-z_]*\.dest='lan'")
        # the marker is a file of the device, so a zone the user deleted is not made again after an update
        self.assertIn('[ -e /etc/f50/docker-zone ] && exit 0', zone)

    def test_release_audit_refuses_the_audio_stack(self):
        # make-release.sh: no kernel bundle carries a driver of the removed call-audio stack, and no image its programs or
        # services. The audit's own lines, run against bundles made here.
        rel = (TOP / 'scripts' / 'release' / 'make-release.sh').read_text()
        audit = rel[rel.index('AUDIO_KO="'):rel.index('[ $fail = 0 ] || { echo "audit failed')]
        self.assertEqual(23, len(re.search(r'AUDIO_KO="([^"]*)"', audit).group(1).split()))
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)

            def bundle(name, *mods):
                src = d / 'src' / name
                shutil.rmtree(src, ignore_errors=True)
                (src / 'modules').mkdir(parents=True)
                for m in ('sipc-core.ko',) + mods:
                    (src / 'modules' / m).write_text('')
                subprocess.run(['tar', '-czf', str(d / f'{name}.tar.gz'), '-C', str(src), '.'], check=True)

            def manifest(name, *paths):
                with gzip.open(d / f'{name}.manifest.gz', 'wt') as f:
                    for p in ('etc/f50/image-version',) + paths:
                        f.write(f'f 755 0 0 1 {"0" * 64} {p}\n')

            def run():
                r = subprocess.run(['sh', '-c', 'fail=0\n' + audit + 'echo "fail=$fail"'], env={'D': str(d), 'PATH': os.environ['PATH']},
                                   capture_output=True, text=True)
                return r.stdout

            bundle('f50-kernel', 'sprd-dma.ko', 'virt-dma.ko')
            manifest('f50-openwrt-rootfs', 'etc/init.d/f50d')
            self.assertIn('fail=0', run())
            for name, mods in (('f50-kernel', ('audio_sipc.ko',)), ('f50-kernel', ('snd-soc-sprd-card.ko',)),
                               ('f50-kernel', ('audio_mem.ko',))):
                bundle(name, *mods)
                out = run()
                self.assertIn('fail=1', out, name)
                self.assertIn(f'{name} carries {mods[0]}', out)
                bundle(name)
            for name, path in (('f50-openwrt-rootfs', 'opt/f50/bin/f50-voice'),
                               ('f50-openwrt-rootfs', 'etc/init.d/f50-audio'),
                               ('f50-openwrt-rootfs', 'etc/modprobe.d/f50-audio.conf')):
                manifest(name, path)
                self.assertIn('fail=1', run(), path)
                manifest(name)
            self.assertIn('fail=0', run())

    def test_e2fsck_is_pinned_and_everywhere(self):
        # device-tools/e2fsck/build.sh: a pinned e2fsprogs (its signed SHA-256) in a pinned image (FINDINGS 182), and every
        # path that builds a boot ramdisk hands it over
        text = (TOP / 'device-tools' / 'e2fsck' / 'build.sh').read_text()
        self.assertRegex(text, r'(?m)^VER=1\.47\.4$')
        self.assertRegex(text, r'(?m)^SHA=[0-9a-f]{64}$')
        self.assertRegex(text, r'(?m)^IMG=alpine:[0-9.]+@sha256:[0-9a-f]{64}$')
        self.assertIn('sha256sum -c', text)
        for f, s in (('scripts/release/make-release.sh', '--e2fsck "$IN/device-tools/e2fsck/e2fsck"'),
                     ('scripts/release/make-release.sh', 'has no sbin/e2fsck'),
                     ('kernel/mainline/make-bundle.sh', '--e2fsck "$W/e2fsck"'),
                     ('install.sh', '${E2FSCK:+--e2fsck "$E2FSCK"}')):
            self.assertIn(s, (TOP / f).read_text(), f)
        # init: before every mount of the root, and never a gate (root_fsck always returns 0)
        init = (TOP / 'boot' / 'init').read_text()
        self.assertEqual(3, init.count('root_fsck "$'))
        block = re.search(r'# --- fsck begin\n(.*?)# --- fsck end', init, re.S).group(1)
        self.assertNotRegex(block, r'return [1-9]')

    def test_init_finds_partitions_after_the_modules(self):
        # the eMMC driver is one of the vendor modules: misc and boot_b cannot be found before they are loaded
        init = (TOP / 'boot' / 'init').read_text()
        calls = [l.strip() for l in init.splitlines() if l.strip() in ('load_vendor_modules', 'find_partitions')]
        self.assertEqual(calls, ['load_vendor_modules', 'find_partitions'])

    def test_every_device_has_its_files(self):
        # a device the installers know needs its module order; its modules come from kernel/build-<device>.sh
        for dev in ('u30air',):
            self.assertTrue((TOP / 'boot' / f'module-order-{dev}.txt').is_file())
            self.assertTrue((TOP / 'kernel' / 'vendor' / f'{dev}.fragment').is_file())
            self.assertIn(dev, (TOP / 'install.sh').read_text())

    def test_mainline_keeps_the_sd_host(self):
        # the SD card can hold the Linux filesystem: the port lets the card slot's host probe next to the eMMC, and
        # still keeps any other sdhci host (the stock DT's sdio_wifi; Wi-Fi is on PCIe) out
        port = (TOP / 'kernel' / 'mainline' / 'port' / 'install.py').read_text()
        self.assertIn('F50: only the eMMC and the card slot', port)
        self.assertIn('if (!of_property_read_bool(pdev->dev.of_node, "non-removable") &&', port)
        self.assertIn('of_property_match_string(pdev->dev.of_node, "sprd,name", "sdio_sd") < 0)', port)
        self.assertIn('F50: CD GPIO deferred', port)
        # a deferral that is only masked leaves the rest of mmc_of_parse() undone (UHS modes, no-sdio, no-mmc): the
        # unresolvable cd-gpios is dropped and the parse run again
        self.assertIn('of_remove_property(pdev->dev.of_node, cd)', port)
        self.assertIn("'F50: only the eMMC and the card slot', 'F50: CD GPIO deferred', "
                      "'of_remove_property(pdev->dev.of_node, cd)'", port)
        self.assertRegex((TOP / 'kernel' / 'mainline' / 'make-bundle.sh').read_text(),
                         r"printf 'sdcard\\nlinux-slot\\ngpt-root\\n' > \"\$W/b/features\"")

    def test_mainline_keeps_the_emmc_at_mmc0(self):
        # Both hosts probe asynchronously and the stock DT has no mmc aliases, so the card slot could take mmc0 and
        # leave mmcblk1 to the eMMC: init then never found the card and rebooted at 300 s (FINDINGS 31l). The card
        # slot's host waits until the eMMC's host is added.
        port = (TOP / 'kernel' / 'mainline' / 'port' / 'install.py').read_text()
        self.assertIn('F50: the eMMC is mmc0', port)
        self.assertIn('static bool sdhci_sprd_emmc_added;', port)
        self.assertIn('for_each_compatible_node(np, NULL, "sprd,sdhci-r11")', port)
        # the deferral sits right after the host filter, before sdhci_pltfm_init() allocates the host index
        self.assertIn('\\t    sdhci_sprd_emmc_pending(pdev->dev.of_node))\n\\t\\treturn -EPROBE_DEFER;\n', port)
        # and says what that costs: without a working eMMC host the card slot never binds
        self.assertIn("no SD root without a working eMMC host", port)
        self.assertLess(port.index("t.replace(filt, filt + '''"), port.index('add_old = '))
        # the flag is set only once the eMMC's host is added
        self.assertIn("add_old = '\\tret = __sdhci_add_host(host);\\n\\tif (ret)\\n\\t\\tgoto err_cleanup_host;\\n'",
                      port)
        self.assertIn('\\t\\tWRITE_ONCE(sdhci_sprd_emmc_added, true);', port)
        # and the build stops when the edit did not apply
        self.assertIn("'F50: the eMMC is mmc0', 'WRITE_ONCE(sdhci_sprd_emmc_added, true);'", port)

    def test_mainline_v50_bits_bind_only_on_a_v50(self):
        """The ZTE V50 (FINDINGS 95, kano's port): its LED driver binds only ZTE's AW9523B node, its eMMC host is kept
        awake only where that node is, and a kernel bundle names the V50 only when it has the driver. The F50's and
        the U30 Air's device trees have no such node: there the kernel does what it did."""
        cfg = (TOP / 'kernel' / 'mainline' / 'f50-mainline.config').read_text()
        self.assertRegex(cfg, r'(?m)^CONFIG_LEDS_AW9523B=y$')
        port = (TOP / 'kernel' / 'mainline' / 'port' / 'install.py').read_text()
        self.assertIn("('LEDS_AW9523B', 'leds-aw9523b',", port)
        drv = (TOP / 'kernel' / 'mainline' / 'port' / 'drivers' / 'leds' / 'leds-aw9523b.c').read_text()
        self.assertEqual(re.findall(r'\.compatible = "([^"]+)"', drv), ['awinic,aw9523b'])
        for s in ('"aw9523b,led_enable", &enabled) || enabled != 1', '"aw9523b,gpio_enable", &gpio_enabled) || gpio_enabled',
                  'if (ret != AW_ID_VALUE) {', '"v50:net1:white"', '"v50:wifi:blue"', 'devm_add_action_or_reset(dev, aw9523b_off, chip)'):
            self.assertIn(s, drv)
        p17 = (TOP / 'kernel' / 'mainline' / 'patches' / '0017-mmc-sdhci-sprd-V50-eMMC-host-kept-awake.patch').read_text()
        for s in ('+\tif (mmc_card_is_removable(host->mmc))\n+\t\treturn false;',
                  '+\tnp = of_find_compatible_node(NULL, NULL, "awinic,aw9523b");',
                  '+\tif (sdhci_sprd_keep_awake(host)) {\n+\t\tpm_runtime_forbid(&pdev->dev);',
                  '+\tif (sdhci_sprd_keep_awake(host))\n+\t\tpm_runtime_allow(&pdev->dev);\n \tpm_runtime_put_noidle(&pdev->dev);'):
            self.assertIn(s, p17)
        # the patch touches nothing else
        self.assertEqual(re.findall(r'(?m)^\+\+\+ (\S+)', p17), ['b/drivers/mmc/host/sdhci-sprd.c'])
        bundle = (TOP / 'kernel' / 'mainline' / 'make-bundle.sh').read_text()
        self.assertIn("if grep -qx 'CONFIG_LEDS_AW9523B=y' \"$UO/kernel.config\"; then\n    echo \"f50 u30air v50\"", bundle)
        self.assertIn('else\n    echo "f50 u30air" > "$W/b/devices"', bundle)
        # the 5.4 bundle does not name it
        self.assertIn('echo "f50 u30air" > "$K/devices"', (TOP / 'scripts' / 'release' / 'make-release.sh').read_text())

    def test_mainline_emmc_timing_falls_back(self):
        """FINDINGS 160 (issue #226): an eMMC whose HS400ES selection fails at a cold boot is initialised again at
        HS200, then HS/DDR52, then HS, instead of being given up after mmc_rescan's attempts at HS400ES; a boot whose
        HS400ES works drops nothing. The test hook in sdhci-sprd is off by default and touches only CMD13 while a new
        card is set up at HS400ES; a test build sets it through build.sh's F50_CMDLINE_EXTRA, never a release."""
        p = (TOP / 'kernel' / 'mainline' / 'patches' / '0021-mmc-HS400ES-failure-falls-back-to-HS200.patch').read_text()
        self.assertEqual(re.findall(r'(?m)^\+\+\+ (\S+)', p), ['b/drivers/mmc/core/mmc.c', 'b/drivers/mmc/host/sdhci-sprd.c'])
        core = p[p.index('+++ b/drivers/mmc/core/mmc.c'):p.index('--- a/drivers/mmc/host/sdhci-sprd.c')]
        # the ladder: HS400ES drops HS400 with it, then HS200, then DDR52; a removed card drops nothing
        for s in ('+\tif (err == -ENOMEDIUM)\n+\t\treturn;',
                  '+\tif (card->mmc_avail_type & EXT_CSD_CARD_TYPE_HS400ES) {\n'
                  '+\t\thost->caps2 &= ~(MMC_CAP2_HS400_ES | MMC_CAP2_HS400);',
                  '+\t\thost->caps2 &= ~(MMC_CAP2_HS200 | MMC_CAP2_HS400);',
                  '+\t} else if (card->mmc_avail_type & EXT_CSD_CARD_TYPE_DDR_52) {\n+\t\thost->caps &= ~MMC_CAP_DDR;',
                  '+\tpr_warn("%s: %s failed (%d), the next initialisation uses %s\\n",',
                  '+timing_failed:\n+\tmmc_timing_fallback(card, err);\n free_card:'):
            self.assertIn(s, core)
        # exactly the four failures of the timing selection go through it, nothing before or after
        self.assertEqual(len(re.findall(r'(?m)^-\t+goto free_card;$', core)), 4)
        self.assertEqual(len(re.findall(r'(?m)^\+\t+goto timing_failed;$', core)), 4)
        self.assertEqual(core.count('goto timing_failed;'), 4)
        # the card type is chosen again from the host caps at every initialisation, also of a known card: what the
        # caps drop relies on (kept in the context of the patch would be too far; the header says it)
        self.assertIn('"since host caps could have been changed when debugging"', p)
        hook = p[p.index('+++ b/drivers/mmc/host/sdhci-sprd.c'):]
        for s in ('+static unsigned int fail_hs400es;\n+module_param(fail_hs400es, uint, 0644);',
                  '+\tif (unlikely(sprd_host->fail_hs400es) && mmc->ios.enhanced_strobe &&\n'
                  '+\t    !mmc->card && mrq->cmd->opcode == MMC_SEND_STATUS) {\n+\t\tmrq->cmd->error = -EILSEQ;',
                  '+\tsprd_host->fail_hs400es = fail_hs400es > 0;'):
            self.assertIn(s, hook)
        self.assertNotRegex(hook, r'(?m)^\+static unsigned int fail_hs400es\s*=')
        build = (TOP / 'kernel' / 'mainline' / 'build.sh').read_text()
        self.assertIn('--set-str CMDLINE "$c $F50_CMDLINE_EXTRA"', build)
        # after the check of the config file (which would refuse a changed CMDLINE), before the build
        self.assertLess(build.index('config options not taken'), build.index('F50_CMDLINE_EXTRA'))
        self.assertLess(build.index('F50_CMDLINE_EXTRA'), build.index('make O=$O ARCH=arm64 -j"$(nproc)" Image'))
        for wf in ('release.yml', 'mainline.yml'):
            self.assertNotIn('F50_CMDLINE_EXTRA', (TOP / '.github' / 'workflows' / wf).read_text())
        self.assertNotIn('fail_hs400es', (TOP / 'kernel' / 'mainline' / 'f50-mainline.config').read_text())

    def test_mainline_drives_the_u30_air_charger(self):
        # Without a driver the charger kept what Android left in it: charging off (CHG_CONFIG 0), 500 mA input, and
        # the battery drained while plugged in. bq256xx drives the vendor node "ti,bq2560x_chg" now.
        cfg = (TOP / 'kernel' / 'mainline' / 'f50-mainline.config').read_text()
        for k in ('CONFIG_CHARGER_BQ256XX=y', 'CONFIG_REGMAP_I2C=y', 'CONFIG_I2C_SPRD=y', 'CONFIG_EXTCON_USB_GPIO=y'):
            self.assertRegex(cfg, rf'(?m)^{k}$')
        port = (TOP / 'kernel' / 'mainline' / 'port' / 'install.py').read_text()
        # the edit itself, on the two forms of the I2C table (6.18 positional, 7.x designated), twice (idempotent)
        code = port[port.index("bp = os.path.join(tree, 'drivers/power/supply/bq256xx_charger.c')"):
                    port.index('# Every edit above is a text substitution')]
        of = ('static const struct of_device_id bq256xx_of_match[] = {\n'
              '\t{ .compatible = "ti,bq25601", .data = &bq256xx_chip_info_tbl[BQ25601] },\n\t{}\n};\n')
        acpi = ('static const struct acpi_device_id bq256xx_acpi_match[] = {\n'
                '\t{ "bq25601", (kernel_ulong_t)&bq256xx_chip_info_tbl[BQ25601] },\n\t{}\n};\n')
        for entry, want in (('{ "bq25601", (kernel_ulong_t)&bq256xx_chip_info_tbl[BQ25601] }',
                             '\t{ "bq2560x_chg", (kernel_ulong_t)&bq256xx_chip_info_tbl[BQ25601] },\n'),
                            ('{ .name = "bq25601", .driver_data = (kernel_ulong_t)&bq256xx_chip_info_tbl[BQ25601] }',
                             '\t{ .name = "bq2560x_chg", .driver_data = (kernel_ulong_t)&bq256xx_chip_info_tbl[BQ25601] },\n')):
            with tempfile.TemporaryDirectory() as tree:
                f = Path(tree, 'drivers/power/supply/bq256xx_charger.c')
                f.parent.mkdir(parents=True)
                f.write_text('static const struct i2c_device_id bq256xx_i2c_ids[] = {\n\t' + entry + ',\n\t{}\n};\n'
                             'MODULE_DEVICE_TABLE(i2c, bq256xx_i2c_ids);\n' + of + acpi)
                for _ in range(2):
                    exec(code, {'os': os, 'tree': tree})
                t = f.read_text()
                self.assertEqual(t.count(want), 1, t)
                self.assertEqual(t.count('"ti,bq2560x_chg", .data = &bq256xx_chip_info_tbl[BQ25601]'), 1, t)
                self.assertNotIn('bq2560x_chg', t[t.index('acpi_device_id'):])   # the ACPI table stays as it was
        # and the build stops when it did not apply
        self.assertIn("('drivers/power/supply/bq256xx_charger.c', ['\"ti,bq2560x_chg\"", port)
        # the driver's handling of that node: no stale cache, Android's 1.98 A and 4.208 V at most, the chip's own
        # input current from the PMIC's charger detection on every VBUS change from the extcon, the chip's own only as
        # a fallback, charging on unless userspace said "N/A"
        p9 = (TOP / 'kernel' / 'mainline' / 'patches' / '0009-power-bq256xx-drive-the-U30-Air-charger.patch').read_text()
        for s in ('.cache_type = REGCACHE_NONE', 'BQ2560X_CHG_ICHG_MAX_uA\t\t1980000', 'BQ2560X_CHG_VBATREG_MAX_uV\t4208000',
                  'BQ256XX_IINDET_EN', 'devm_extcon_register_notifier(dev, bq->vbus_edev, EXTCON_USB',
                  '"linux,extcon-usb-gpio"', 'bq->init_data.iindpm = 500000;', 'WRITE_ONCE(bq->chg_off',
                  'off ? 0 : BQ256XX_CHG_CONFIG_MASK', 'case BQ256XX_VBUS_STAT_USB_CDP:\n+\t\treturn 1500000;',
                  'case BQ256XX_VBUS_STAT_USB_DCP:\n+\t\treturn 2000000;', 'case BQ256XX_VBUS_STAT_NONSTD:\n+\t\treturn 1000000;',
                  'static char *supplied_to[] = { "sc27xx-fgu" };',
                  # the PMIC's charger detection first: the charger's own (IINDET_EN) drives D+/D-, the gadget's lines
                  'sprd_pmic_detect_charger_type(bq->pmic)', 'of_find_compatible_node(NULL, NULL, "sprd,ump9620")',
                  'bus_find_device_by_of_node(&spi_bus_type, np)', 'no charger type from the PMIC'):
            self.assertIn(s, p9)
        # the fuel gauge asks the charger by the name the driver registers
        p10 = (TOP / 'kernel' / 'mainline' / 'patches' / '0010-power-sc27xx-fuel-gauge-status-from-bq256xx.patch').read_text()
        self.assertIn('+\t"bq256xx-charger",', p10)
        # the U30 Air's other charger (issue #102): when nothing answers at the node's address, ZTE's driver goes on
        # at 0x1a (an SGM41513), and so does this one, with that chip's charge and termination current steps; a
        # charger that answers at the node's address takes none of it
        p15 = (TOP / 'kernel' / 'mainline' / 'patches' / '0015-power-bq256xx-SGM41513-at-0x1a.patch').read_text()
        for s in ('#define SGM41513_I2C_ADDR\t\t0x1a', 'if (part < 0 && client->addr != SGM41513_I2C_ADDR) {',
                  'devm_i2c_new_dummy_device(dev, client->adapter, SGM41513_I2C_ADDR)',
                  'bq->regmap = devm_regmap_init_i2c(chip,', 'no charger: %d (and %d at 0x%02x)',
                  '{ 0, 40000, 130000, 300000, 540000, 1020000, 1500000, 2460000 }',
                  '{ 5000, 10000, 20000, 30000, 60000, 60000, 120000, 120000 }',
                  '80000, 100000, 120000, 140000, 160000, 180000, 200000, 240000,',
                  'info->bq256xx_set_ichg = sgm41513_set_ichg_curr;', 'info->bq256xx_set_iterm = sgm41513_set_term_curr;',
                  'info->bq256xx_set_iprechg = sgm41513_set_prechrg_curr;'):
            self.assertIn(s, p15)
        # ... but only on a board with a battery (the U30 Air's charger policy service): the F50 has a charger at
        # 0x1a too, and bound and told to charge it reset the board in a loop on a router's port (issue #225)
        p16 = (TOP / 'kernel' / 'mainline' / 'patches' / '0016-power-bq256xx-SGM41513-only-with-a-battery.patch').read_text()
        for s in ('of_find_node_by_path("/charger_policy_service")', 'of_node_put(np);',
                  '+\t\tif (part < 0 && client->addr != SGM41513_I2C_ADDR && battery) {'):
            self.assertIn(s, p16)
        # the PMIC's charger detection reads the UMP9620's own register (0x239c), not the SC2730's (0x1b9c), and a
        # tree prepared with the old one is corrected
        self.assertIn(r"#define SPRD_UMP9620_CHG_DET\t\t0x239c", port)
        self.assertIn(r"'\t.charger_det = SPRD_SC2730_CHG_DET,\n};\n', UMP9620_DATA)", port)
        self.assertIn("'.charger_det = SPRD_UMP9620_CHG_DET,'", port)
        # Android's last capacity in the FGU's user area is whole percent in bits 7:0 and tenths in bits 11:8 (575 =
        # 0x23f = 63.2 %), not 0.1 %; mainline writes whole percent; a valid value is taken as it is, only an invalid
        # one goes to the OCV (FINDINGS 181: no migration guess); the charger's Full (from a charger driver) means 100 %
        # with the battery current within the node's fullbatt-current, no OCV percentage or voltage (the 4.146 V of a
        # real Full is below fullbatt-voltage); the capacity is saved as it changes and stays 0-100
        patches = TOP / 'kernel' / 'mainline' / 'patches'
        self.assertFalse((patches / '0011-power-sc27xx-fuel-gauge-UMP9620-capacity-in-tenths.patch').exists())
        p11 = (patches / '0011-power-sc27xx-fuel-gauge-UMP9620-saved-capacity-and-full.patch').read_text()
        for s in ('+#define UMP9620_FGU_CAP_INTEGER_MASK\tGENMASK(7, 0)', '+#define UMP9620_FGU_CAP_DECIMAL_MASK\tGENMASK(11, 8)',
                  '+\t\tcap = clamp(cap, 0, 100);', '+\tvalid = whole <= 100 && tenths <= 9 && !(whole == 100 && tenths);',
                  '+\tif (valid) {',
                  '+\t*cap = power_supply_ocv2cap_simple(data->cap_table, data->table_len, ocv);',
                  '+\tif (!is_first_poweron && data->var == &ump9620_info)',
                  '+\tif (data->var == &ump9620_info && chg_sts == POWER_SUPPLY_STATUS_FULL && cap != 100 &&',
                  '+\t    sc27xx_fgu_has_charger() && ump9620_fgu_full_current(data)) {',
                  '+\treturn abs(cur) <= (int)(data->full_current_ua / 1000);',
                  '+\t\t\tof_property_read_u32(bat, "fullbatt-current", &data->full_current_ua);',
                  '+\t\tsc27xx_fgu_adjust_cap(data, 100);',
                  '+\t\tif (*cap != data->saved_cap && sc27xx_fgu_save_last_cap(data, *cap))',
                  '+\t\tval->intval = clamp(value, 0, 100);'):
            self.assertIn(s, p11)
        self.assertNotIn('* 10;', p11)
        for s in ('ambiguous', 'UMP9620_FGU_RELAXED_MA', 'UMP9620_FGU_FULL_MIN_CAP', 'fullbatt-voltage"'):
            self.assertNotIn(s, ''.join(l for l in p11.splitlines(True) if l.startswith('+')))
        # (FINDINGS 105, after kanoqwq's 2af101f8) the capacity is read and kept once more at a power-off, on UMP9620
        # with a battery only; the user area's steps stay regmap_update_bits() as in Unisoc's driver and the U30 Air's
        # stock module (FINDINGS 181), with its 200 us wait after the release too
        p18 = (patches / '0018-power-sc27xx-fuel-gauge-UMP9620-capacity-kept-at-power-off.patch').read_text()
        for s in ('+static void sc27xx_fgu_shutdown(struct platform_device *pdev)',
                  '+\tif (!data || data->var != &ump9620_info || !data->bat_present)',
                  '+\t\tret = sc27xx_fgu_save_last_cap(data, clamp(cap, 0, 100));',
                  '+\t\tret = sc27xx_fgu_save_boot_mode(data, SC27XX_FGU_NORMAIL_POWERTON);',
                  '+\t.shutdown = sc27xx_fgu_shutdown,'):
            self.assertIn(s, p18)
        self.assertNotIn('regmap_write_bits(data', p18)
        self.assertEqual(p18.count('+\tif (!ret && data->var == &ump9620_info)\n+\t\tudelay(200);'), 2)
        # f50-usb reaches the chip past the driver and leaves its watchdog off then
        usb = (BIN / 'f50-usb').read_text()
        self.assertIn('[ -e "$R/sys/bus/i2c/devices/$BUS-006b/driver" ] && FORCE=-f', usb)

    def test_mainline_can_suspend(self):
        # System suspend (PSCI SYSTEM_SUSPEND) and a tickless idle: under the periodic tick a CPU brought back online
        # was left out of the timer broadcast and locked up, which every suspend (and `cpu7 offline/online`) hit
        cfg = (TOP / 'kernel' / 'mainline' / 'f50-mainline.config').read_text()
        for k in ('CONFIG_SUSPEND', 'CONFIG_PM_SLEEP', 'CONFIG_NO_HZ_IDLE', 'CONFIG_HIGH_RES_TIMERS'):
            self.assertRegex(cfg, rf'(?m)^{k}=y$')
        drv = TOP / 'kernel' / 'mainline' / 'port' / 'drivers'
        # the PMIC watchdog keeps counting while the AP sleeps: stopped for the sleep, armed again after it
        wdt = (drv / 'watchdog' / 'ump9620-pmic-wdt.c').read_text()
        self.assertIn('NOIRQ_SYSTEM_SLEEP_PM_OPS(ump9620_wdt_suspend_noirq, ump9620_wdt_resume_noirq)', wdt)
        self.assertIn('.pm = pm_sleep_ptr(&ump9620_wdt_pm_ops)', wdt)
        self.assertIn('platform_set_drvdata(pdev, w);', wdt)
        # PCIe WAKE# is a wakeup only when power/wakeup says so, and is masked (unlazily) for the sleep otherwise
        pcie = (drv / 'pci' / 'controller' / 'dwc' / 'pcie-sprd.c').read_text()
        self.assertIn('if (device_may_wakeup(pci->dev))\n\t\tpm_wakeup_hard_event(pci->dev);', pcie)
        self.assertNotIn('IRQF_TRIGGER_FALLING | IRQF_NO_SUSPEND', pcie)
        self.assertIn('irq_set_status_flags(ctrl->wakeup_irq, IRQ_DISABLE_UNLAZY);', pcie)
        self.assertNotIn('device_init_wakeup(dev, true)', pcie)
        self.assertIn('SET_SYSTEM_SLEEP_PM_OPS(sprd_pcie_pm_suspend, sprd_pcie_pm_resume)', pcie)
        # the firmware's debug log pulled WAKE# right after every L2 entry: off by default, and a resume leaves it be
        mods = TOP / 'kernel' / 'mainline' / 'modules'
        sysfs = (mods / 'wcn_bsp' / 'platform' / 'sysfs.c').read_text()
        self.assertRegex(sysfs, r'#endif\n(\t.*\n)*\tsysfs_info\.armlog_status = 0;\n')
        ep = (mods / 'wcn_bsp' / 'pcie' / 'pcie.c').read_text()
        resume = ep[ep.index('static int sprd_ep_resume('):ep.index('const struct dev_pm_ops sprd_ep_pm_ops')]
        self.assertNotIn('wcn_set_armlog(true)', resume)
        # without a WoWLAN configuration cfg80211 closed the interfaces while the bus was going down, and the
        # firmware asserted at the next Wi-Fi open: WoWLAN "any" from the wiphy's registration on
        iface = (mods / 'sprd_wlan_combo' / 'common' / 'iface.c').read_text()
        self.assertIn('wowlan->any = true;\n\t\t\twiphy->wowlan_config = wowlan;', iface)
        self.assertLess(iface.index('ret = wiphy_register(wiphy);'), iface.index('wiphy->wowlan_config = wowlan;'))

    def test_mainline_config_has_kvm_and_the_module_set(self):
        # /dev/kvm (the CPUs start at EL2) and the router/container modules; the kernel's own modules reach the bundle
        cfg = (TOP / 'kernel' / 'mainline' / 'f50-mainline.config').read_text()
        opts = {}
        for line in cfg.splitlines():
            if line.startswith('CONFIG_'):
                k, v = line.split('=', 1)
                # an option given twice must say the same thing both times: merge_config takes the last one silently
                self.assertEqual(opts.setdefault(k, v), v, k)
        for k in ('CONFIG_VIRTUALIZATION', 'CONFIG_KVM', 'CONFIG_MODULES', 'CONFIG_WIREGUARD', 'CONFIG_NET_SCH_CAKE',
                  'CONFIG_TUN', 'CONFIG_NF_FLOW_TABLE', 'CONFIG_IKCONFIG_PROC', 'CONFIG_CHECKPOINT_RESTORE',
                  'CONFIG_MPTCP', 'CONFIG_IP_MROUTE', 'CONFIG_NFSD_V4', 'CONFIG_FS_ENCRYPTION'):
            self.assertEqual(opts.get(k), 'y', k)
        for k in ('CONFIG_VHOST_NET', 'CONFIG_NTFS3_FS', 'CONFIG_DM_CRYPT', 'CONFIG_NET_SCH_HTB', 'CONFIG_USB_NET_CDC_MBIM',
                  'CONFIG_XFRM_USER', 'CONFIG_BRIDGE_NETFILTER', 'CONFIG_BINFMT_MISC', 'CONFIG_SND_USB_AUDIO',
                  'CONFIG_VIRTIO_FS', 'CONFIG_VHOST_SCSI', 'CONFIG_9P_FS', 'CONFIG_DM_INTEGRITY',
                  'CONFIG_MPLS_ROUTING', 'CONFIG_USBIP_HOST', 'CONFIG_USB_VIDEO_CLASS'):
            self.assertEqual(opts.get(k), 'm', k)
        mods = (TOP / 'kernel' / 'mainline' / 'build-modules.sh').read_text()
        self.assertIn('INSTALL_MOD_STRIP=1 DEPMOD=true modules_install', mods)
        self.assertIn('has the name of an in-tree module', mods)

    def test_mainline_rewrites_the_ttl_in_tc(self):
        # f50-ttl's tc backend (clsact egress, matchall, pedit, csum): the flowtable transmits through egress, so
        # flow offloading stays on. The 5.4 kernel has no pedit: f50-ttl falls back to nftables there.
        cfg = (TOP / 'kernel' / 'mainline' / 'f50-mainline.config').read_text()
        for k in ('CONFIG_NET_SCH_INGRESS', 'CONFIG_NET_CLS_ACT'):
            self.assertRegex(cfg, rf'(?m)^{k}=y$')
        for k in ('CONFIG_NET_CLS_MATCHALL', 'CONFIG_NET_ACT_PEDIT', 'CONFIG_NET_ACT_CSUM'):
            self.assertRegex(cfg, rf'(?m)^{k}=[ym]$')
        # tc itself: tc-tiny comes with sqm-scripts on OpenWrt
        self.assertIn('sqm-scripts', (TOP / 'systems' / 'openwrt' / 'build-rootfs.sh').read_text())

    def test_every_release_kernel_bundle_has_the_sd_host(self):
        # 5.4 reads the card as well (FINDINGS 31j): its bundle says so, and the release audit fails when it does not
        # (f50-update refuses such a bundle for a system on the card)
        mr = (TOP / 'scripts' / 'release' / 'make-release.sh').read_text()
        self.assertIn("printf 'sdcard\\nlinux-slot\\ngpt-root\\n' > \"$K/features\"", mr)
        self.assertLess(mr.index('$K/features'), mr.index('tar -C "$K" -czf "$D/f50-kernel.tar.gz" .'))
        self.assertIn('for a in f50-kernel; do\n'
                      '    tar -xzOf "$D/$a.tar.gz" ./features 2>/dev/null | grep -qx sdcard', mr)
        # and that its init works with Linux on either slot (f50-update refuses one without it on slot a)
        self.assertIn('    tar -xzOf "$D/$a.tar.gz" ./features 2>/dev/null | grep -qx linux-slot', mr)
        # and that its init finds a system in a GPT partition named f50root (FINDINGS 178; f50-update asks)
        self.assertIn('    tar -xzOf "$D/$a.tar.gz" ./features 2>/dev/null | grep -qx gpt-root', mr)

    def test_release_can_be_a_prerelease(self):
        # a release is published as a prerelease first and promoted to "latest" only after a board ran it: the
        # option reaches `gh release create`, and an unknown option stops the script instead of being ignored
        mr = (TOP / 'scripts' / 'release' / 'make-release.sh').read_text()
        self.assertIn('--prerelease) PRERELEASE=--prerelease ;;', mr)
        self.assertIn('--publish) PUBLISH=--publish ;;', mr)
        self.assertRegex(mr, r'\*\) echo "unknown option \$a')
        self.assertRegex(mr, r'gh release create "\$TAG" [^\n]*\$PRERELEASE')
        self.assertNotIn('--latest', mr)

    def test_proc_reads_are_braced(self):
        # `tr < /proc/$pid/cmdline 2>/dev/null` reports a failed redirection (the process just went) before its own
        # 2>/dev/null applies, so "can't open /proc/..." reaches the log: the read is braced, `{ tr < ...; } 2>/dev/null`
        files = [p for p, _ in shell_scripts()]
        bad = re.compile(r'<\s*"?/proc/\$[^\s;|]*\s+2>\s*/dev/null')
        found = []
        for p in sorted(set(files)):
            for n, line in enumerate(p.read_text(errors='replace').splitlines(), 1):
                if bad.search(line):
                    found.append(f'{p.relative_to(TOP)}:{n}')
        self.assertEqual(found, [])

    def test_quiet_console_sysctl(self):
        # K24: the image carries the drop-in (procd's /etc/init.d/sysctl applies it)
        a = (TOP / 'systems' / 'openwrt' / 'overlay' / 'etc' / 'sysctl.d' / '99-f50-console.conf').read_text()
        self.assertRegex(a, r'(?m)^kernel\.printk = 1$')

    def test_cellular_downlink_in_the_software_flowtable(self):
        # K28, K29: f50cell reports sipa_eth0 as l3_device only, so fw4 leaves it out of its flowtable and the
        # downlink takes the slow forwarding path. The patch puts it in; it is applied with --fuzz=0, so a changed fw4
        # fails the build instead of shipping without it.
        patch = (TOP / 'systems' / 'openwrt' / 'patches' / 'fw4-sipa-offload.patch').read_text()
        self.assertIn('+++ b/usr/share/ucode/fw4.uc', patch)
        self.assertIn("+\t\t\tif (fs.access('/sys/class/net/sipa_eth0'))", patch)
        self.assertIn("+\t\t\t\tpush(devices, 'sipa_eth0');", patch)
        text = (TOP / 'systems' / 'openwrt' / 'build-rootfs.sh').read_text()
        self.assertIn('-v "$FW4PATCH":/in/fw4-sipa-offload.patch:ro', text)
        apply = 'patch --batch --fuzz=0 -d $R -p1 -i /in/fw4-sipa-offload.patch'
        self.assertIn(apply, text)
        self.assertLess(text.index('cp -a /in/overlay/. $R/'), text.index(apply))
        self.assertNotIn('luci-plugin', text)
        # the patch tool is installed after the copy into $R: the build container has it, the image does not
        self.assertLess(text.index('for e in /*; do'), text.index('apk add patch'))
        self.assertLess(text.index(apply), text.index('apk del patch'))
        self.assertLess(text.index('apk del patch'), text.index('apk list --installed | sort'))
        # #95: fw4 reload/restart on 5.4 drops the old flowtable first; applied the same way, after the first patch
        ft = (TOP / 'systems' / 'openwrt' / 'patches' / 'fw4-old-kernel-flowtable.patch').read_text()
        self.assertIn('+++ b/sbin/fw4', ft)
        self.assertIn('-v "$FW4FTPATCH":/in/fw4-old-kernel-flowtable.patch:ro', text)
        apply_ft = 'patch --batch --fuzz=0 -d $R -p1 -i /in/fw4-old-kernel-flowtable.patch'
        self.assertLess(text.index(apply), text.index(apply_ft))
        self.assertLess(text.index(apply_ft), text.index('apk del patch'))
        self.assertIn('grep -q drop_old_flowtable $R/sbin/fw4', text)
        # the function as patched in: only a kernel before 5.13 deletes - the rules that use the flowtable first (it
        # cannot be deleted while in use), then the flowtable - and it never fails the reload
        added = [l[1:] for l in ft.splitlines() if l.startswith('+') and not l.startswith('+++')]
        func = '\n'.join(added[added.index('drop_old_flowtable() {'):added.index('}') + 1])
        nft = r'''nft() {
    echo "nft $*" >> "$LOG"
    case "$*" in
        "list flowtables") printf 'table inet fw4 {\n\tflowtable ft {\n' ;;
        "-a list table inet fw4") printf 'table inet fw4 { # handle 1\n\tchain forward { # handle 2\n\t\tmeta l4proto { tcp, udp } flow add @ft # handle 56\n\t}\n\tchain other { # handle 3\n\t\tct state established accept # handle 7\n\t\tflow offload @ft # handle 9\n\t}\n}\n' ;;
    esac
    return 0
}'''
        for rel, deletes in (('5.4.254', True), ('5.12.19-x', True), ('4.19.1', True), ('5.13.0', False),
                             ('6.18.56', False), ('7.2.10', False)):
            with tempfile.TemporaryDirectory() as d:
                log = Path(d) / 'log'
                log.write_text('')
                r = subprocess.run(['sh', '-c', f'uname() {{ echo {rel}; }}\n{nft}\n{func}\ndrop_old_flowtable; echo rc=$?'],
                                   capture_output=True, text=True, env=dict(os.environ, LOG=str(log)))
                self.assertIn('rc=0', r.stdout, (rel, r.stderr))
                calls = log.read_text().splitlines()
                if not deletes:
                    self.assertEqual(calls, [], rel)
                    continue
                self.assertEqual(calls[-3:], ['nft delete rule inet fw4 forward handle 56',
                                              'nft delete rule inet fw4 other handle 9',
                                              'nft delete flowtable inet fw4 ft'], (rel, calls))
        # software offloading on, hardware off: the SIPA and SC2355 drivers have no nftables hardware offload
        uci = (OPENWRT / 'etc' / 'uci-defaults' / '90-f50').read_text()
        self.assertIn("uci -q set firewall.@defaults[0].flow_offloading='1'", uci)
        self.assertIn("uci -q set firewall.@defaults[0].flow_offloading_hw='0'", uci)
        self.assertIn('uci commit firewall', uci)

    def test_build_stops_without_the_cellular_protocol(self):
        # K74: without f50cell.sh (f50d's, installed by f50d/openwrt/install.sh) netifd has no wan, and the image would
        # boot without mobile data; the patches are needed as much. Checked before any download.
        def run(files, extra_env=None):
            with tempfile.TemporaryDirectory() as d:
                top = Path(d)
                (top / 'systems' / 'openwrt').mkdir(parents=True)
                shutil.copy(TOP / 'systems' / 'openwrt' / 'build-rootfs.sh', top / 'systems' / 'openwrt' / 'build-rootfs.sh')
                for rel, body in files.items():
                    (top / rel).parent.mkdir(parents=True, exist_ok=True)
                    (top / rel).write_text(body)
                env = dict(os.environ, F50_INPUTS=d, PATH='/usr/bin:/bin')
                env.pop('F50_SYSTEM', None)
                env.pop('F50_FLAVOUR', None)
                env.update(extra_env or {})
                return subprocess.run(['sh', str(top / 'systems' / 'openwrt' / 'build-rootfs.sh'), 'x.tar.gz'], env=env,
                                      capture_output=True, text=True, timeout=30)
        f50d = {'f50d/openwrt/netifd/f50cell.sh': 'x\n', 'f50d/openwrt/install.sh': 'x\n'}
        patch = {'systems/openwrt/patches/fw4-sipa-offload.patch': 'x\n',
                 'systems/openwrt/patches/fw4-old-kernel-flowtable.patch': 'x\n',
                 'systems/openwrt/patches/wifi-scripts-no-dfs.patch': 'x\n',
                 'systems/openwrt/patches/wifi-scripts-wpa2-only.patch': 'x\n'}
        with self.subTest(missing='the WPA2-only patch'):
            r = run({**f50d, **{k: v for k, v in patch.items() if 'wpa2' not in k}})
            self.assertEqual(r.returncode, 1, r.stderr)
            self.assertIn('wifi-scripts-wpa2-only.patch', r.stderr)
        for missing in f50d:
            with self.subTest(missing=missing):
                r = run({**patch, **{k: v for k, v in f50d.items() if k != missing}})
                self.assertEqual(r.returncode, 1, r.stderr)
                self.assertIn(missing, r.stderr)
        with self.subTest(missing='the patch'):
            r = run(f50d)
            self.assertEqual(r.returncode, 1, r.stderr)
            self.assertIn('fw4-sipa-offload.patch', r.stderr)
        with self.subTest(missing='the wifi-scripts patch'):
            r = run({**f50d, **{k: v for k, v in patch.items() if 'wifi-scripts' not in k}})
            self.assertEqual(r.returncode, 1, r.stderr)
            self.assertIn('wifi-scripts-no-dfs.patch', r.stderr)
        # the systems and flavours that are gone are refused, not ignored
        for var, value in (('F50_SYSTEM', 'openwrt-luci'), ('F50_FLAVOUR', 'immortalwrt')):
            with self.subTest(**{var: value}):
                r = run({**f50d, **patch}, {var: value})
                self.assertEqual(r.returncode, 1, r.stderr)
                self.assertIn('only the plain OpenWrt image is built', r.stderr)
        # with every file present no check stops the build: it stops at its download, which a curl stub refuses
        with tempfile.TemporaryDirectory() as stubs:
            curl = Path(stubs) / 'curl'
            curl.write_text('#!/bin/sh\necho "curl: stub refuses" >&2\nexit 22\n')
            curl.chmod(0o755)
            r = run({**f50d, **patch}, {'PATH': f'{stubs}:/usr/bin:/bin'})
            self.assertNotEqual(r.returncode, 0, r.stderr)
            self.assertIn('stub refuses', r.stderr)
            self.assertNotIn('required f50d file missing', r.stderr)
            self.assertNotIn('fw4-sipa-offload.patch', r.stderr)

    def test_board_json_names_the_usb_lan_port(self):
        # panel page audit: armsr's 99-default_network gave board.json a LAN on eth0, which the F50 does not have;
        # LuCI's Status > Overview showed an "eth0, no link" port and its ubus status call failed. board.d/05 names
        # usb0 first (board_detect runs board.d in name order, and 99-default_network leaves a network it finds)
        d = OPENWRT / 'etc' / 'board.d'
        self.assertEqual(sorted(p.name for p in d.iterdir()), ['05_f50_network'])
        self.assertLess('05_f50_network', '99-default_network')
        f = d / '05_f50_network'
        self.assertTrue(f.stat().st_mode & 0o111)
        stub = ('board_config_update() { echo update; }\nboard_config_flush() { echo flush; }\n'
                'ucidef_set_interface_lan() { echo "lan $*"; }\n')
        body = f.read_text().replace('. /lib/functions/uci-defaults.sh', stub)
        self.assertNotEqual(body, f.read_text())
        for s in shells():
            r = subprocess.run(s + ['-c', body], capture_output=True, text=True)
            self.assertEqual((r.returncode, r.stdout), (0, 'update\nlan usb0\nflush\n'), (s, r.stderr))

    def test_sqm_default_queue_names_the_cellular_uplink(self):
        # panel page audit: the image's /etc/config/sqm (sqm-scripts' default, disabled) named eth1, which the
        # F50 does not have; the build names sipa_eth0 instead, and stops when a queue still names eth1
        text = (TOP / 'systems' / 'openwrt' / 'build-rootfs.sh').read_text()
        start = text.index('if [ -f $R/etc/config/sqm ]; then')
        block = text[start:text.index('\nfi\n', start) + 4]
        # a uci stand-in over one file of "sqm.SECTION.OPTION='VALUE'" lines (what uci show prints)
        stub = ('uci() { while [ "${1#-}" != "$1" ]; do [ "$1" = -c ] && shift; shift; done\n'
                '  case $1 in show) cat "$CFG" ;; commit) echo "commit $2" >> "$LOG" ;;\n'
                '  set) k=${2%%=*}; v=${2#*=}; grep -v "^$k=" "$CFG" > "$CFG.n"; echo "$k=\'$v\'" >> "$CFG.n"; '
                'mv "$CFG.n" "$CFG" ;; esac; }\n')
        for s in shells():
            for queues, want in (
                    (["sqm.eth1=queue", "sqm.eth1.enabled='0'", "sqm.eth1.interface='eth1'"],
                     ["sqm.eth1.interface='sipa_eth0'"]),
                    (["sqm.mine=queue", "sqm.mine.interface='br-lan'"], ["sqm.mine.interface='br-lan'"])):
                with tempfile.TemporaryDirectory() as d, self.subTest(shell=s, queues=queues):
                    (Path(d) / 'etc' / 'config').mkdir(parents=True)
                    (Path(d) / 'etc' / 'config' / 'sqm').write_text('x\n')
                    cfg = Path(d) / 'show'
                    cfg.write_text('\n'.join(queues) + '\n')
                    r = subprocess.run(s + ['-c', f'R={d}\n{stub}{block}echo done'], capture_output=True, text=True,
                                       env=dict(os.environ, CFG=str(cfg), LOG=str(Path(d) / 'log')))
                    self.assertEqual((r.returncode, r.stdout), (0, 'done\n'), r.stderr)
                    lines = cfg.read_text().splitlines()
                    for w in want:
                        self.assertIn(w, lines)
                    self.assertNotIn("sqm.eth1.interface='eth1'", lines)
                    self.assertEqual((Path(d) / 'log').read_text(), 'commit sqm\n')

    def test_wifi_power_transitions_are_serialised(self):
        # FINDINGS 31n: the probe's power-off (outside RTNL) ran post_deinit while hostapd's open ran post_init;
        # post_deinit cleared the context post_init had just set, and the next RX interrupt read it as NULL in hard
        # IRQ. Both kernels' drivers hold a mutex across the power transition and drop RX with no context
        wlan = TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo'
        patch = (TOP / 'kernel' / 'vendor' / 'patches' / 'wlan_combo-wcn-power-serialise.patch').read_text()
        hif = (wlan / 'common' / 'hif.h').read_text()
        iface = (wlan / 'common' / 'iface.c').read_text()
        pcie = (wlan / 'sc2355' / 'pcie.c').read_text()
        self.assertIn('\tstruct mutex power_lock;', hif)
        self.assertIn('+\tstruct mutex power_lock;', patch)
        for text, pre in ((iface, ''), (patch, '+')):
            self.assertIn(pre + '\tmutex_init(&hif->power_lock);', text)
            self.assertIn(pre + '\tmutex_lock(&hif->power_lock);', text)
            self.assertIn(pre + '\tmutex_unlock(&hif->power_lock);', text)
        body = iface[iface.index('int sprd_iface_set_power('):]
        body = body[:body.index('\n}\n')]
        # no return between the lock and the unlock
        self.assertNotIn('return ret;', body[:body.index('mutex_unlock')])
        rx = pcie[pcie.index('static int pcie_rx_handle('):]
        rx = rx[:rx.index('\n}\n')]
        guard = 'rx_mgmt = hif ? (struct rx_mgmt *)READ_ONCE(hif->rx_mgmt) : NULL;'
        self.assertLess(rx.index(guard), rx.index('if (unlikely(!rx_mgmt))'))
        self.assertLess(rx.index('if (unlikely(!rx_mgmt))'), rx.index('rx_mgmt->rx_list'))
        self.assertIn('+\t' + guard, patch)
        # remove powers off before sprd_core_free frees priv (and hif, and the lock in it)
        rm = iface[iface.index('int sprd_iface_remove('):]
        self.assertLess(rm.index('sprd_iface_set_power(hif, false);'), rm.index('sprd_core_free(priv);'))
        for text, pre in ((pcie, ''), (patch, '+')):
            self.assertIn(pre + '\tsmp_store_release(&sc2355_hif.hif, (void *)hif);', text)
            self.assertIn(pre + '\tWRITE_ONCE(sc2355_hif.hif, NULL);', text)

    def test_wifi_per_frame_logs_are_bounded(self):
        # FINDINGS 92 (after kanoqwq's clean-tf-7.2 3cbf37fc): three lines the TX path wrote for every frame - a
        # group frame's LUT lookup (pr_info), a frame for a client that has left (pr_err) and a send that failed
        # (netdev_err) - are debug or rate-limited; the frames are still dropped and freed as before
        wlan = TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo'
        for name in ('pcie.c', 'sdio.c'):
            text = (wlan / 'sc2355' / name).read_text()
            self.assertNotIn('pr_info("%s, %d, group lut_index=%d\\n"', text, name)
            self.assertNotIn('pr_err("%s, %d, sta disconn, no data tx!"', text, name)
            self.assertIn('pr_debug_ratelimited("%s, %d, group lut_index=%d\\n"', text, name)
            self.assertIn('\t\tpr_debug_ratelimited("%s, %d, sta disconn, no data tx!\\n", __func__, __LINE__);\n'
                          '\t\treturn -EPERM;', text, name)
        iface = (wlan / 'common' / 'iface.c').read_text()
        body = iface[iface.index('static netdev_tx_t iface_start_xmit('):]
        body = body[:body.index('\n}\n')]
        self.assertIn('\t\tif (net_ratelimit())\n\t\t\tnetdev_err(ndev, "%s drop msg due to TX Err\\n", __func__);', body)
        after = body[body.index('drop msg due to TX Err'):]
        self.assertLess(after.index('dev_kfree_skb(skb);'), after.index('return NETDEV_TX_OK;'))
        self.assertLess(after.index('sprd_chip_free_msg(&vif->priv->chip, msg);'), after.index('return NETDEV_TX_OK;'))

    def test_wifi_notify_init_unwinds_and_registers_the_reset_notifier_last(self):
        # kanoqwq/mu300-linux@23361ce: a failed notify_init left the WCN reset notifier registered on a freed priv
        iface = (TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo' / 'common' / 'iface.c').read_text()
        body = iface[iface.index('static int iface_notify_init('):]
        body = body[:body.index('\n}\n')]
        reg = body.index('atomic_notifier_chain_register(&wcn_reset_notifier_list,')
        for step in ('misc_register(&wlan_misc_device);', 'register_inetaddr_notifier(&iface_inetaddr_cb);',
                     'register_inet6addr_notifier(&iface_inet6addr_cb);'):
            self.assertLess(body.index(step), reg, step)
        # nothing returns between the first registration and the end without undoing what it registered
        self.assertEqual(body.count('return ret;'), 2)
        self.assertIn('\t\treturn ret;\n\t}\n\n\tret = register_inetaddr_notifier', body)
        for label in ('goto err_misc;', 'goto err_inet;', 'err_inet:\n\tunregister_inetaddr_notifier',
                      'err_misc:\n\tmisc_deregister(&wlan_misc_device);\n\treturn ret;'):
            self.assertIn(label, body)

    def test_wifi_rx_teardown_stops_the_refill_producers_first(self):
        # kanoqwq/mu300-linux@23361ce: the RX work queues refills on sprd_work and a refill arms the timed retry;
        # rx_deinit drained them in an order that let one re-arm the other over a freed rx_mgmt
        wlan = TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo' / 'sc2355'
        self.assertIn('\tbool stopping;\n};', (wlan / 'rx.h').read_text())
        rx = (wlan / 'rx.c').read_text()

        def body(sig):
            b = rx[rx.index(sig):]
            return b[:b.index('\n}\n')]
        q = body('void sc2355_queue_rx_buff_work(')
        self.assertLess(q.index('READ_ONCE(rx_mgmt->stopping)'), q.index('sprd_alloc_work(0)'))
        self.assertIn('if (!READ_ONCE(rx_mgmt->stopping))\n\t\tsc2355_queue_rx_buff_work(', body('static void rx_refill_retry('))
        f = body('int sc2355_mm_fill_buffer(')
        # read under refill_lock since the bus-completion resume (FINDINGS 195)
        self.assertLess(f.index('if (rx_mgmt->stopping) {'), f.index('atomic_xchg(&mm_entry->alloc_num, 0)'))
        d = body('int sc2355_rx_deinit(')
        order = ['WRITE_ONCE(rx_mgmt->stopping, true);', 'cancel_delayed_work_sync(&rx_mgmt->refill_retry);',
                 'flush_workqueue(rx_mgmt->rx_queue);', 'flush_workqueue(rx_mgmt->rx_net_workq);',
                 'sprd_clean_work(hif->priv);']
        pos = [d.index(s) for s in order]
        self.assertEqual(pos, sorted(pos))
        self.assertEqual(d.count('cancel_delayed_work_sync(&rx_mgmt->refill_retry);'), 2)
        self.assertLess(d.index('sprd_clean_work(hif->priv);'), d.rindex('cancel_delayed_work_sync(&rx_mgmt->refill_retry);'))
        self.assertLess(d.rindex('cancel_delayed_work_sync'), d.index('kfree(rx_mgmt);'))

    def test_wifi_remove_tears_the_driver_down(self):
        # issue #146: the port's platform remove only logged (the vendor's calls sprd_iface_remove), so rmmod freed
        # the module with the recovery watchdog's delayed work still armed: __run_timers panicked within a second
        wlan = TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo'
        drv = (wlan / 'common' / 'sprd_wlan.c').read_text()
        rm = drv[drv.index('static void sprd_wlan_remove('):]
        rm = rm[:rm.index('\n}\n')]
        self.assertIn('\tif (p_match_data->hw_type == SPRD_HW_SC2355_PCIE)\n\t\tsprd_iface_remove(pdev);', rm)
        iface = (wlan / 'common' / 'iface.c').read_text()
        body = iface[iface.index('int sprd_iface_remove('):]
        body = body[:body.index('\n}\n')]
        # the watchdog stops first, and a failed power-on does not leave the teardown undone
        self.assertLess(body.index('iface_wd_stop();'), body.index('sprd_iface_set_power(hif, true);'))
        self.assertEqual(body.count('\n\treturn '), 1)
        self.assertIn('\tif (!ret)\n\t\tsprd_iface_set_power(hif, false);', body)
        wd = iface[iface.index('static void iface_wd_stop(void)'):]
        wd = wd[:wd.index('\n}\n')]
        self.assertIn('cancel_delayed_work_sync(&iface_wd.tick);', wd)
        # the timers that live in priv are shut down where priv is freed
        core_free = (wlan / 'common' / 'cfg80211.c').read_text()
        core_free = core_free[core_free.index('void sprd_core_free('):]
        core_free = core_free[:core_free.index('\n}\n')]
        self.assertLess(core_free.index('timer_shutdown_sync(&priv->scan_timer);'), core_free.index('wiphy_free(wiphy);'))
        ack = (wlan / 'sc2355' / 'tcp_ack.c').read_text()
        ack = ack[ack.index('void sc2355_tcp_ack_deinit('):]
        ack = ack[:ack.index('\n}\n')]
        self.assertLess(ack.index('sprd_chip_drop_tcp_msg('), ack.index('timer_shutdown_sync(&ack_m->ack_info[i].timer);'))

    def test_wifi_tx_backpressure_is_not_logged_as_an_error(self):
        # kanoqwq/mu300-linux@3386928: a full TX ring is flow control, hit on every scheduling pass under load;
        # the vendor printed two pr_err lines for it each time
        wlan = TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo' / 'sc2355'
        pcie = (wlan / 'pcie.c').read_text()
        fc = pcie[pcie.index('int sc2355_pcie_fc_test_send_num('):]
        fc = fc[:fc.index('\n}\n')]
        self.assertNotIn('pr_err(', fc)
        self.assertIn('pr_debug_ratelimited("%s,%d free_num=%d, data_num=%d\\n"', fc)
        tx = (wlan / 'tx.c').read_text()
        self.assertNotIn('pr_err("%s, %d: _fc_ no credit!\\n"', tx)
        self.assertIn('pr_debug_ratelimited("%s, %d: _fc_ no credit!\\n"', tx)

    def test_wifi_ap_get_station_answers_for_its_stations_only(self):
        # FINDINGS 180 (after kanoqwq's f59a296): on an access point get_station handed out the interface's own
        # counters and the station side's GET_STATION for any address; now -ENOENT unless the address is one of this
        # interface's peers, and nothing filled in for one (the firmware has no per-client figures); dump_station
        # refuses a negative index; get_channel answers for link 0 only. The 5.4 patch adds the same code
        mainline = (TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo' / 'common' / 'cfg80211.c').read_text()
        patch = (TOP / 'kernel' / 'vendor' / 'patches' / 'wlan_combo-ap-stats-scope.patch').read_text()
        lines = ('static bool sprd_ap_has_peer(struct sprd_vif *vif, const u8 *mac)',
                 '\tif (!is_valid_ether_addr(mac) || ether_addr_equal(mac, vif->ndev->dev_addr))',
                 '\t\tif (READ_ONCE(hif->peer_entry[i].ctx_id) == vif->ctx_id &&',
                 '\tif (vif->wdev.iftype == NL80211_IFTYPE_AP || vif->wdev.iftype == NL80211_IFTYPE_P2P_GO) {',
                 '\t\tif (!sprd_ap_has_peer(vif, mac))\n\t\t\treturn -ENOENT;\n\t\tsinfo->filled = 0;\n\t\treturn 0;',
                 '\tif (idx < 0 || (vif->wdev.iftype != NL80211_IFTYPE_AP && vif->wdev.iftype != NL80211_IFTYPE_P2P_GO))',
                 '\tif (link_id || (wdev->iftype != NL80211_IFTYPE_AP && wdev->iftype != NL80211_IFTYPE_P2P_GO) ||',
                 '\t*chandef = wdev->links[0].ap.chandef;')
        added = '\n'.join(l[1:] if l[:1] in '+ ' else l for l in patch.splitlines())
        for s in lines:
            self.assertIn(s, mainline)
            self.assertIn(s, added)
        self.assertNotIn('links[link_id]', mainline)
        # the AP branch comes before anything of the station side's is filled in
        f = mainline[mainline.index('int sprd_cfg80211_get_station('):]
        self.assertLess(f.index('sprd_ap_has_peer(vif, mac)'), f.index('sinfo->tx_bytes = ndev->stats.tx_bytes;'))

    def test_wifi_routine_events_are_not_logged_as_errors(self):
        # issue #140: the vendor printed per-poll survey data, the multicast default of the peer lookup, the
        # short-frame copy path, reordering drops, dup-ACK skips, EAPOL sends and DELBA for an unknown session with
        # pr_err/netdev_err. Both drivers trace them at debug level, the 5.4 one through its patch; the real errors
        # next to them ("too short", "alloc skb fail", BAR for an unknown session) keep their level
        wlan = TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo' / 'sc2355'
        patch = (TOP / 'kernel' / 'vendor' / 'patches' / 'wlan_combo-routine-not-err.patch').read_text()
        src = {f: (wlan / f).read_text() for f in ('acs.c', 'pcie.c', 'sdio.c', 'mm.c', 'defrag.c', 'tx.c', 'reorder.c',
                                                   'tcp_ack.c')}
        # all of acs.c's error lines were dump_survey's trace
        self.assertNotIn('netdev_err(', src['acs.c'])
        self.assertIn('-\t\t\tnetdev_err(vif->ndev, "Not AP mode, exit %s!\\n",', patch)
        self.assertEqual(patch.count('+\t\t\t\t\tnetdev_dbg(vif->ndev,'), 2)
        lowered = (('pcie.c', '\t\tpr_debug("%s,%d,bssid not found, multicast?\\n"', 3),
                   ('sdio.c', '\t\tpr_debug("%s,%d,bssid not found, multicast?\\n"', 3),
                   ('mm.c', '\t\t\tpr_debug_ratelimited("%s: data len is %d, skb need %d\\n",', 1),
                   ('defrag.c', '\t\tpr_debug("%s:msdu len %d\\n", __func__, node->msdu_len);', 1),
                   ('tx.c', '\t\tpr_debug("send %s frame by CMD_TX_DATA\\n",', 1),
                   ('reorder.c', '\t\tpr_debug_ratelimited("%s: seq_num: %d is less than win_start: %d\\n",', 1),
                   ('reorder.c', '\t\tpr_debug_ratelimited("%s: kfree skb %d", __func__, ret);', 1),
                   ('reorder.c', '\t\tpr_debug("%s: NOT FOUND sta_lut_index: %d, tid: %d\\n",', 1),
                   ('tcp_ack.c', '\t\t\tpr_debug_ratelimited("%s before abnormal ack: %d, %d\\n",', 1))
        def hunks(f):  # the patch's section for one file
            s = patch[patch.index('--- a/sc2355/' + f + '\n'):]
            end = s.find('\n--- a/', 1)
            return s if end < 0 else s[:end]
        for f, line, n in lowered:
            with self.subTest(file=f, line=line):
                self.assertEqual(src[f].count(line), n)
                self.assertEqual(hunks(f).count('+' + line), n)
                self.assertEqual(hunks(f).count('-' + line.replace('pr_debug_ratelimited(', 'pr_err(')
                                                .replace('pr_debug(', 'pr_err(')), n)
        # the errors next to them stay
        self.assertIn('pr_err_ratelimited("%s: data len is %d, too short\\n"', src['mm.c'])
        self.assertIn('pr_err_ratelimited("%s: alloc skb fail\\n"', src['mm.c'])
        bar = src['reorder.c'][src['reorder.c'].index('static void reorder_wlan_bar_event('):]
        self.assertIn('pr_err("%s: NOT FOUND sta_lut_index: %d, tid: %d\\n"', bar[:bar.index('\n}\n')])
        for kept in ('too short', 'alloc skb fail'):
            self.assertNotIn(kept, patch[patch.index('--- a/'):])



class ApkRetry(unittest.TestCase):
    """systems/openwrt/build-rootfs.sh: the feed's packages are fetched while the image is built, and a download that broke
    off failed the image; update, upgrade and add are tried three times (FINDINGS 66)."""

    def setUp(self):
        text = (TOP / 'systems' / 'openwrt' / 'build-rootfs.sh').read_text()
        self.block = re.search(r'# --- apk-retry begin\n(.*?)# --- apk-retry end\n', text, re.S).group(1)
        # the function stands before the first use, inside the one single-quoted script of the container
        self.assertLess(text.index('# --- apk-retry end'), text.index('\napk update >/dev/null'))
        self.assertNotIn("'", self.block)
        self.tmp = Path(tempfile.mkdtemp(prefix='f50-test-'))
        stub = self.tmp / 'apk'
        # fails FAIL times (a counter in a file), then succeeds; records every call
        stub.write_text('#!/bin/sh\necho "$*" >> "$LOG"\nn=$(cat "$COUNT" 2>/dev/null || echo 0)\n'
                        '[ "$n" -lt "$FAIL" ] || { echo "ok $1"; exit 0; }\necho $((n + 1)) > "$COUNT"\nexit 1\n')
        stub.chmod(0o755)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_apk(self, shell, args, fail):
        for f in ('log', 'count'):
            (self.tmp / f).unlink(missing_ok=True)
        env = dict(os.environ, PATH=f'{self.tmp}{os.pathsep}{os.environ["PATH"]}', LOG=str(self.tmp / 'log'),
                   COUNT=str(self.tmp / 'count'), FAIL=str(fail), F50_APK_RETRY_WAIT='0')
        # as the build runs it: sh -eu, the output thrown away
        r = subprocess.run(shell + ['-eu', '-c', self.block + f'apk {args} >/dev/null\necho done'], env=env,
                           capture_output=True, text=True, timeout=30)
        log = (self.tmp / 'log').read_text().splitlines() if (self.tmp / 'log').exists() else []
        return r, log

    def test_a_download_that_breaks_off_is_tried_again(self):
        for shell in shells():
            for cmd in ('update', 'upgrade', 'add wpad-basic-mbedtls iw', 'add --allow-untrusted /in/x.apk'):
                for fail, tries, ok in ((0, 1, True), (1, 2, True), (2, 3, True), (3, 3, False)):
                    r, log = self.run_apk(shell, cmd, fail)
                    self.assertEqual(log, [cmd] * tries, (shell, cmd, fail))     # the same arguments every time
                    self.assertEqual((r.returncode == 0, 'done' in r.stdout), (ok, ok), (shell, cmd, fail, r.stderr))
                    self.assertEqual(r.stderr.count('trying again'), tries - 1)

    def test_anything_else_runs_once(self):
        for shell in shells():
            r, log = self.run_apk(shell, 'del procd-ujail', 1)
            self.assertEqual((log, r.returncode), (['del procd-ujail'], 1), shell)
            self.assertNotIn('done', r.stdout)
            # "apk del ... || true", as the build has it
            env = dict(os.environ, PATH=f'{self.tmp}{os.pathsep}{os.environ["PATH"]}', LOG=str(self.tmp / 'log'),
                       COUNT=str(self.tmp / 'count'), FAIL='9')
            r = subprocess.run(shell + ['-eu', '-c', self.block + 'apk del x >/dev/null 2>&1 || true\necho done'],
                               env=env, capture_output=True, text=True, timeout=30)
            self.assertEqual((r.returncode, r.stdout.strip()), (0, 'done'), r.stderr)



if __name__ == '__main__':
    unittest.main()
