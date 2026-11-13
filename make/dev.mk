# Local development of the web console: Vite on 127.0.0.1:5173, /ubus proxied to the device.
.PHONY: dev

dev: web-deps
	F50_DEVICE=http://$(DEVICE_IP) $(MAKE) -C f50-web dev
