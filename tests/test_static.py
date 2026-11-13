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

    def test_init_finds_partitions_after_the_modules(self):
        # the eMMC driver is one of the vendor modules: misc and boot_b cannot be found before they are loaded
        init = (TOP / 'boot' / 'init').read_text()
        calls = [l.strip() for l in init.splitlines() if l.strip() in ('load_vendor_modules', 'find_partitions')]
        self.assertEqual(calls, ['load_vendor_modules', 'find_partitions'])

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
