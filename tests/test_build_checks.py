"""Build checks that keep an old binary out of a kernel bundle (after kanoqwq 265f4de3, f6b0668d, f0926715):
make-bundle.sh refuses a vendor module whose source changed after it was built, build-wlan.sh a Wi-Fi tree patched
with another patch set, and build-all.sh's patched_copy removes only its own copies."""
import os
import re
import time

from helpers import TOP, ShellTest

BUNDLE = TOP / 'kernel' / 'mainline' / 'make-bundle.sh'


class WlanPatchMarker(ShellTest):
    def test_build_wlan_refuses_a_tree_of_another_patch_set(self):
        s = (TOP / 'kernel' / 'vendor' / 'build-wlan.sh').read_text()
        self.assertIn('[ "$(cat .f50-patches)" = "$sum" ] ||', s)
        # the marker is the same sum build-all.sh's patched_copy writes
        self.assertIn('sum=$(cat /work/patches/wlan_combo-*.patch | sha256sum | cut -d" " -f1)', s)
        self.assertLess(s.index('if [ -f .f50-patches ]'), s.index('make O=/src/out-linux'))

    def test_patched_copy_removes_only_its_own_copies(self):
        s = (TOP / 'kernel' / 'vendor' / 'build-all.sh').read_text()
        body = s[s.index('patched_copy() {'):s.index('\n}\n', s.index('patched_copy() {'))]
        self.assertLess(body.index('case $1 in /src/ext-wlan_combo|/src/ext-sprdbt) ;;'), body.index('rm -rf "$1"'))
