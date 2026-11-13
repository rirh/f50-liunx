# The running device, over SSH.
.PHONY: ssh status logs restart reboot

ssh:
	$(SSH) -t

status:
	@$(SSH) 'uname -r; cat /etc/f50/image-version 2>/dev/null || cat /etc/mu300/image-version 2>/dev/null; uptime; free -m; \
	for s in f50d f50d-at; do printf "%s: " $$s; /etc/init.d/$$s running && echo running || echo stopped; done'

logs:
	$(SSH) 'exec logread -f'

# the daemon only (the AT keeper stays: see deploy-f50d)
restart:
	$(SSH) '/etc/init.d/f50d restart && echo "f50d restarted"'

# the whole device, into whatever its boot setting says (Linux by default after make install)
reboot:
	$(SSH) 'sync; reboot' || true
	@echo "rebooting $(DEVICE_IP)"
