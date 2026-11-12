.PHONY: ssh status cache-clear restart reboot

ssh status cache-clear restart reboot:
	$(DEVICE_TOOL) $@
