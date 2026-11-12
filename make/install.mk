# F50 installation from macOS. Every target that talks to stock Android enables ADB first
# (scripts/device/f50-enable-adb-macos.sh returns at once when ADB is already set up).
.PHONY: adb install install-check install-prebuilt uninstall backup backup-full collect-logs reset-password

ADB_ENABLE = scripts/device/f50-enable-adb-macos.sh --ip "$(F50_IP)" $(if $(F50_SERIAL),--serial "$(F50_SERIAL)")

# After `make adb` the F50 answers over USB and at $(F50_IP):5555: pin the USB one so the scripts do not ask.
# A device in MU300 Linux (serial MU300LINUX-...) is left to the installer, which reboots it into Android.
WITH_USB = adb=$$(./install.sh --adb-path 2>/dev/null) && export PATH="$${adb%/*}:$$PATH" && \
	serial=$$(adb -d get-serialno 2>/dev/null) && \
	case $$serial in MU300LINUX*|'') ;; *) export ANDROID_SERIAL=$${ANDROID_SERIAL:-$$serial} ;; esac;

adb:
	$(ADB_ENABLE)

install: adb
	@$(WITH_USB) ./install.sh $(INSTALL_ARGS)

install-check: adb
	@$(WITH_USB) ./install.sh --check

install-prebuilt: adb
	@$(WITH_USB) ./install.sh --prebuilt $(INSTALL_ARGS)

uninstall: adb
	@$(WITH_USB) ./uninstall.sh

backup: adb
	@$(WITH_USB) scripts/device/backup-device.sh $(BACKUP_DIR)

backup-full: adb
	@$(WITH_USB) scripts/device/backup-full.sh "$(or $(BACKUP_DIR),backup/full-$(shell date +%Y%m%d-%H%M%S))"

collect-logs: adb
	@$(WITH_USB) scripts/device/collect-logs.sh $(LOG_DIR)

reset-password: adb
	@$(WITH_USB) scripts/device/reset-password.sh $(SYSTEM)
