# Deploys to a device that runs OpenWrt with f50d already (make install did the first one): over SSH, no reflash.
.PHONY: deploy deploy-f50d deploy-proxy deploy-web deploy-reboot

deploy: deploy-f50d deploy-proxy deploy-web

# The new binary streamed beside the old one (one connection, no scp), then one rename and the daemon restarted. The AT keeper (f50d-at) keeps running
# the old copy until the next boot: a modem channel it closes is not given back (FINDINGS 13b). make deploy-reboot
# runs everything new at once.
deploy-f50d: f50d
	$(SSH) 'cat > /usr/sbin/f50d.new && chmod 755 /usr/sbin/f50d.new && mv /usr/sbin/f50d.new /usr/sbin/f50d && /etc/init.d/f50d restart && echo "f50d deployed"' < $(F50D_BIN)

# the proxy engine, the same way, with its procd service (a device installed before the proxy has none: it is put in
# and enabled first); f50d sends it the start request again once it is back (its supervisor, every 5 s)
deploy-proxy: proxy
	$(SSH) 'cat > /etc/init.d/f50-proxy.new && chmod 755 /etc/init.d/f50-proxy.new && mv /etc/init.d/f50-proxy.new /etc/init.d/f50-proxy && /etc/init.d/f50-proxy enable' < f50d/openwrt/init.d/f50-proxy
	$(SSH) 'mkdir -p /opt/f50/bin && cat > /opt/f50/bin/f50-proxy.new && chmod 755 /opt/f50/bin/f50-proxy.new && mv /opt/f50/bin/f50-proxy.new /opt/f50/bin/f50-proxy && /etc/init.d/f50-proxy restart && echo "f50-proxy deployed"' < $(F50_PROXY_BIN)

# f50-web's own deploy: built, copied beside /www/f50 and swapped in with one rename
deploy-web: web-deps
	$(MAKE) -C f50-web deploy DEVICE=$(SSH_TARGET) SSH_OPTS="$(SSH_OPTS)"

deploy-reboot: deploy reboot
