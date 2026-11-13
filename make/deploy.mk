# Deploys to a device that runs OpenWrt with f50d already (make install did the first one): over SSH, no reflash.
.PHONY: deploy deploy-f50d deploy-web deploy-reboot

deploy: deploy-f50d deploy-web

# The new binary streamed beside the old one (one connection, no scp), then one rename and the daemon restarted. The AT keeper (f50d-at) keeps running
# the old copy until the next boot: a modem channel it closes is not given back (FINDINGS 13b). make deploy-reboot
# runs everything new at once.
deploy-f50d: f50d
	$(SSH) 'cat > /usr/sbin/f50d.new && chmod 755 /usr/sbin/f50d.new && mv /usr/sbin/f50d.new /usr/sbin/f50d && /etc/init.d/f50d restart && echo "f50d deployed"' < $(F50D_BIN)

# f50-web's own deploy: built, copied beside /www/f50 and swapped in with one rename
deploy-web: web-deps
	$(MAKE) -C f50-web deploy DEVICE=$(SSH_TARGET) SSH_OPTS="$(SSH_OPTS)"

deploy-reboot: deploy reboot
