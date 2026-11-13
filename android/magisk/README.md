# Magisk module: switch to Linux from Android

A one-tap way to start the Linux system from Android. It replaces having a computer plugged in just to change the
boot slot. `./install.sh` puts it on the device at the end of an installation (installer/lib/install-magisk-module.sh
copies `f50-linux-switch/` of this checkout into `/data/adb/modules/f50_linux_switch`; nothing is downloaded). It
installs nothing and only changes the boot slot. To install it by hand, build the zip:

```sh
android/magisk/build.sh          # builds f50-linux-switch.zip
```

Install the zip in the Magisk app (Modules → Install from storage), or from a computer:

```sh
adb push android/magisk/f50-linux-switch.zip /data/local/tmp/
adb shell "su -c 'magisk --install-module /data/local/tmp/f50-linux-switch.zip'"
adb reboot                      # Magisk applies a new module on the next boot
```

## Using it

* **Magisk app → Modules → F50 Linux switch → Action.** The device arms the slot Android is not on and reboots
  into Linux.
* **From a terminal or adb:**

```sh
su -c f50-linux               # switch to Linux now
su -c 'f50-linux status'      # what is on each slot; changes nothing
su -c 'f50-linux --dry-run'   # print the block it would write, write nothing
su -c 'f50-linux bootchain'   # does the boot chain differ between the slots? changes nothing
su -c 'f50-linux bootchain sync'   # copy Android's slot's chain to the Linux slot (backed up first)
```

`bootchain sync` is the installers' step on its own (`bootchain.sh`, a copy of `installer/android/android-bootchain.sh`; FINDINGS
40): where `uboot`, `sml`, `teecfg`, `trustos` or `vbmeta` differ between the slots - after an Android update, say -
both slots' images go to `/data/adb/f50-bootchain/<date>/` with a `SHA256SUMS`, then Android's are written to the
Linux slot and read back; a mismatch writes the old ones back. It refuses when Android's slot is not marked as booted
successfully, when the other slot is set to boot next without Linux on it (an update waiting for its restart), when
any of the ten partitions is missing or the two slots of one differ in size, and on any other model.

Going back needs nothing: Linux restores Android's block during its own boot, so the next reboot returns to
Android. From inside Linux, `f50-next-boot linux` arms another Linux boot and `f50-os <name>` picks which
system starts.

## What it writes

Switching writes exactly 32 bytes: the AOSP `bootloader_control` block at offset `0x800` of `misc`, with the slot Android is not on
at the highest priority and `tries_remaining = 2` (LK treats 1 as an already failed boot), Android's slot still
bootable and marked successful. `boot_a`, `boot_b`, the GPT, `userdata` and every other partition are untouched, so
the worst case of a Linux that does not boot is that LK falls back to Android by itself.

The block is not a canned blob: the module reads the live one, changes the slot suffix and the two metadata bytes,
and recomputes the CRC32 (zlib, over the first 28 bytes) in `awk`, so it stays correct on a device whose block
differs. It refuses to do anything when the magic is not `BCAB`, when Android's slot is unknown, or when the other
boot partition is missing, is not a boot image, or is just a copy of Android's (no Linux installed).

## Checked on hardware

ZTE F50 / F50, Magisk 30.7, Android 13: `status` and `--dry-run` produce exactly the trial block that
`boot/flash-trial.sh` computes on a computer, and the Action/`f50-linux` path rebooted the device into Linux.
