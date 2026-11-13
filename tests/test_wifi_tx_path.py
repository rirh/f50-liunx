"""The SC2355 Wi-Fi TX path of the mainline kernels (kernel/mainline/modules/sprd_wlan_combo, wcn_bsp): four defects of the
vendor's code that kanoqwq found and measured on an F50 (clean-tf-7.2 149c9d2a), fixed here in our own words
(FINDINGS 103). These read the source: the modules are built by the release (and were built for 6.18.56 and 7.2.10
without a new warning), and the behaviour needs a device (the checklist in the section)."""
import unittest

from helpers import TOP

WLAN = TOP / 'kernel' / 'mainline' / 'modules' / 'sprd_wlan_combo' / 'sc2355'
WCN = TOP / 'kernel' / 'mainline' / 'modules' / 'wcn_bsp'


def body(src, sig):
    b = src[src.index(sig):]
    return b[:b.index('\n}\n')]


class WifiTxPath(unittest.TestCase):
    def test_the_descriptor_fill_keeps_the_caller_s_skb(self):
        # skb_realloc_headroom() + kfree_skb() replaced only the local pointer: every caller went on with a freed skb
        for f, fn in (('pcie.c', 'int sc2355_pcie_hif_fill_msdu_dscr('), ('sdio.c', 'int sc2355_hif_fill_msdu_dscr(')):
            with self.subTest(file=f):
                b = body((WLAN / f).read_text(), fn)
                self.assertNotIn('skb_realloc_headroom(skb', b)
                self.assertNotIn('kfree_skb(', b)
                self.assertIn('skb_header_cloned(skb)', b)
                self.assertIn('if (skb_cow_head(skb, DSCR_LEN + offset + hif->hif_offset + MSG_PTR_LEN + dscr_rsvd)) {', b)
                # the Ethernet header is read again after the data may have moved, before anything uses it
                self.assertLess(b.index('skb_cow_head'), b.index('ethhdr = (struct ethhdr *)skb->data;\t/*'))
                self.assertLess(b.index('ethhdr = (struct ethhdr *)skb->data;\t/*'), b.index('memcpy(hif->skb_da'))

class WifiCommandNotSent(unittest.TestCase):
    """FINDINGS 106 (after kanoqwq's clean-tf-7.2 0e19fe32): a command the driver does not send is an error, with
    nothing in the caller's reply buffer; get_station's reply starts zeroed. The 5.4 driver gets the same change as
    kernel/vendor/patches/wlan_combo-unsent-cmd-error.patch."""

    def test_the_vendor_kernel_gets_the_same_change(self):
        p = (TOP / 'kernel' / 'vendor' / 'patches' / 'wlan_combo-unsent-cmd-error.patch').read_text()
        self.assertIn('--- a/sc2355/cmdevt.c\n+++ b/sc2355/cmdevt.c\n', p)
        for s in ('+\tif (!ret && cmdevt_lock_cmd(cmd, hif))\n+\t\tret = -EIO;', '+\t\t\tret = -EBUSY;',
                  '+\tstruct cmd_get_station get_sta = {};'):
            self.assertIn(s, p)
        self.assertEqual(p.count('+\t\t\tret = -EBUSY;'), 2)


class WifiTxPath54(unittest.TestCase):
    """FINDINGS 230: the 5.4 kernel's versions of the same fixes (kanoqwq 149c9d2a, 3cbf37fc, 0438ab01), as patches
    to the vendor trees: the kernel ones in build-all.sh's KPATCHES, the Wi-Fi ones by the wlan_combo-*.patch glob.
    The build applies them; these check what they carry and that the diagnostics stay out."""
    P = TOP / 'kernel' / 'vendor' / 'patches'

    def kpatches(self):
        import re
        return re.search(r'^KPATCHES="([^"]*)"', (TOP / 'kernel/vendor/build-all.sh').read_text(), re.M).group(1).split()

    def test_the_kernel_patches_are_applied(self):
        kp = self.kpatches()
        for name in ('sprdwcn-tx-pending-backpressure', 'sdhci-sprd-dma-ordering'):
            with self.subTest(patch=name):
                self.assertIn(name, kp)
                self.assertTrue((self.P / (name + '.patch')).is_file())
        # the backpressure patch edits what wcn-pcie-rx-push-cursor leaves
        self.assertLess(kp.index('wcn-pcie-rx-push-cursor'), kp.index('sprdwcn-tx-pending-backpressure'))
        for name in ('wlan_combo-tx-retry', 'wlan_combo-skb-headroom', 'wlan_combo-special-data-prefix',
                     'wlan_combo-ipv6-multicast-data', 'wlan_combo-stale-tx-ratelimit'):
            with self.subTest(patch=name):
                self.assertTrue((self.P / (name + '.patch')).is_file())

    def test_his_diagnostics_stay_out(self):
        for name in ('sprdwcn-edma-pressure-stats', 'wlan_combo-tx-pressure-logging', 'wlan_combo-rx-ip-multicast-endian'):
            with self.subTest(patch=name):
                self.assertFalse((self.P / (name + '.patch')).exists())
                self.assertNotIn(name, self.kpatches())

    def test_a_full_ring_is_backpressure_and_does_not_stick(self):
        p = (self.P / 'sprdwcn-tx-pending-backpressure.patch').read_text()
        self.assertIn('-\t\treturn ERROR;\n', p)
        self.assertIn('+\t\treturn -EAGAIN;\n', p)
        self.assertIn('+\t\tstatic DEFINE_RATELIMIT_STATE(full_rs, 60 * HZ, 1);', p)
        self.assertIn('+\t\tif (edma_pending_q_num(chn) <= 0)\n+\t\t\tq->status = 0;', p)
        self.assertNotIn('atomic64', p)
        w = (self.P / 'wlan_combo-tx-retry.patch').read_text()
        self.assertEqual(w.count('+\t\t\tNULL, sc2355_pcie_tx_data_complete, NULL),'), 2)
        self.assertIn('+\tif (ret && ret != -EAGAIN) {', w)
        self.assertIn('+\t\tif (hif->hw_type == SPRD_HW_SC2355_PCIE && ret == -EAGAIN)\n+\t\t\treturn ret;', w)

    def test_the_descriptor_fill_keeps_the_skb(self):
        p = (self.P / 'wlan_combo-skb-headroom.patch').read_text()
        self.assertEqual(p.count('-\t\tkfree_skb(temp_skb);'), 2)
        self.assertEqual(p.count('+\t\tif (skb_cow_head(skb, DSCR_LEN + offset + hif->hif_offset +'), 2)

    def test_sdhci_accessors_are_ordered(self):
        p = (self.P / 'sdhci-sprd-dma-ordering.patch').read_text()
        for acc in ('readl', 'writel', 'writew', 'writeb'):
            with self.subTest(accessor=acc):
                self.assertIn('-\t' + ('return ' if acc == 'readl' else '') + acc + '_relaxed(', p)


if __name__ == '__main__':
    unittest.main()
