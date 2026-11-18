# First installation on stock Android, from macOS or Linux. Targets that talk to Android
# prepare a root channel explicitly; plain `make adb` exits when USB is already connected.
.PHONY: adb adb-root _prepare-adb _probe-adb install install-check uninstall backup backup-full collect-logs reset-password

ADB_ENABLE = $(PYTHON) scripts/device/f50-enable-adb.py --ip "$(F50_IP)" $(if $(F50_SERIAL),--serial "$(F50_SERIAL)")

# After `make adb` the F50 answers over USB: pin it so the scripts do not ask.
# A device running Linux (serial F50LINUX-...) is left to the installer, which reboots it into Android.
WITH_USB = adb=$$(./install.sh --adb-path 2>/dev/null) && export PATH="$${adb%/*}:$$PATH" && \
	export F50_IP="$(F50_IP)" F50_KEEP_ADB=1 && \
	serial=$$(adb $(if $(F50_SERIAL),-s "$(F50_SERIAL)",-d) get-serialno 2>/dev/null) && \
	case $$serial in F50LINUX*|'') ;; *) export ANDROID_SERIAL="$$serial" ;; esac &&

# The installer builds everything here, nothing downloaded from another project: the kernel of out/ (make kernel),
# OpenWrt's own rootfs with f50d and the web console (systems/openwrt/build-rootfs.sh, docker), Android's vendor files
# from this device. It installs OpenWrt on the 5.4 kernel, the only system and kernel it knows.

adb:
	$(ADB_ENABLE)

# Installation and maintenance require root even when USB ADB is already connected. This step changes Android
# (ADB, the TCP 5555 fallback, the root channel): only the targets that write to the device take it.
_prepare-adb:
	$(ADB_ENABLE) --prepare-root

# the same, by hand: what install-check needs when it finds no root shell
adb-root: _prepare-adb

# Read only: is a USB ADB device there? Nothing on the device is changed (no ADB switch, no root channel, no reboot).
_probe-adb:
	@adb=$$(./install.sh --adb-path 2>/dev/null) || { echo "no adb found (see README)" >&2; exit 1; }; \
	"$$adb" $(if $(F50_SERIAL),-s "$(F50_SERIAL)",-d) get-serialno >/dev/null 2>&1 || \
	{ echo "no USB ADB device: install-check changes nothing on the device, so it does not turn ADB on;" >&2; \
	  echo "run make adb (or make adb-root for the root channel) first" >&2; exit 1; }

# make install owns the f50d and proxy builds (build, above); build-rootfs.sh takes them as built (F50_PREBUILT=1).
# Order: the builds, then Android is prepared, then the installer - never side by side under make -j.
install: build
	@[ -f out/Image ] && ls out/modules/*.ko >/dev/null 2>&1 || { echo "no kernel in out/: run make kernel first" >&2; exit 1; }
	@$(MAKE) --no-print-directory _prepare-adb
	@$(WITH_USB) F50_PREBUILT=1 ./install.sh $(INSTALL_ARGS)

# Read only, as documented: the probe, then install.sh --check (which writes nothing, its USB functions included).
# Without a root shell it says so and stops; make adb-root prepares one.
install-check: _probe-adb
	@$(WITH_USB) ANDROID_SERIAL="$$serial" ./install.sh --check

uninstall: _prepare-adb
	@$(WITH_USB) ./uninstall.sh

backup: _prepare-adb
	@$(WITH_USB) scripts/device/backup-device.sh $(BACKUP_DIR)

backup-full: _prepare-adb
	@$(WITH_USB) scripts/device/backup-full.sh "$(or $(BACKUP_DIR),backup/full-$(shell date +%Y%m%d-%H%M%S))"

collect-logs: _prepare-adb
	@$(WITH_USB) scripts/device/collect-logs.sh $(LOG_DIR)

reset-password: _prepare-adb
	@$(WITH_USB) scripts/device/reset-password.sh $(or $(SYSTEM),openwrt)
