# The first installation, from macOS or Linux, on an F50 running its stock Android. Every target that talks to Android turns
# on ADB first (scripts/device/f50-enable-adb.sh returns at once when it is on already).
.PHONY: adb install install-check uninstall backup backup-full collect-logs reset-password

ADB_ENABLE = scripts/device/f50-enable-adb.sh --ip "$(F50_IP)" $(if $(F50_SERIAL),--serial "$(F50_SERIAL)")

# After `make adb` the F50 answers over USB: pin it so the scripts do not ask.
# A device running Linux (serial F50LINUX-...) is left to the installer, which reboots it into Android.
WITH_USB = adb=$$(./install.sh --adb-path 2>/dev/null) && export PATH="$${adb%/*}:$$PATH" && \
	serial=$$(adb -d get-serialno 2>/dev/null) && \
	case $$serial in F50LINUX*|'') ;; *) export ANDROID_SERIAL=$${ANDROID_SERIAL:-$$serial} ;; esac;

# The installer builds everything here, nothing downloaded from another project: the kernel of out/ (make kernel),
# OpenWrt's own rootfs with f50d and the web console (systems/openwrt/build-rootfs.sh, docker), Android's vendor files
# from this device. It installs OpenWrt on the 5.4 kernel, the only system and kernel it knows.

adb:
	$(ADB_ENABLE)

install: build adb
	@[ -f out/Image ] && ls out/modules/*.ko >/dev/null 2>&1 || { echo "no kernel in out/: run make kernel first" >&2; exit 1; }
	@$(WITH_USB) ./install.sh $(INSTALL_ARGS)

install-check: adb
	@$(WITH_USB) ./install.sh --check

uninstall: adb
	@$(WITH_USB) ./uninstall.sh

backup: adb
	@$(WITH_USB) scripts/device/backup-device.sh $(BACKUP_DIR)

backup-full: adb
	@$(WITH_USB) scripts/device/backup-full.sh "$(or $(BACKUP_DIR),backup/full-$(shell date +%Y%m%d-%H%M%S))"

collect-logs: adb
	@$(WITH_USB) scripts/device/collect-logs.sh $(LOG_DIR)

reset-password: adb
	@$(WITH_USB) scripts/device/reset-password.sh $(or $(SYSTEM),openwrt)
