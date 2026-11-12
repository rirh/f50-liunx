# ZTE F50 web console. `make` lists the targets.
include make/config.mk
include make/version.mk
include make/app.mk
include make/deploy.mk

.DEFAULT_GOAL := help

.PHONY: help
help:
	@echo "make dev             dev server on 127.0.0.1:5173, /ubus proxied to F50_DEVICE (default http://192.168.77.1)"
	@echo "make build           production build into dist/"
	@echo "make deploy          production build, copied to the device's /www/f50 (DEVICE=$(DEVICE))"
	@echo "make deploy-stage    stage build, copied to the test device's /www/f50 (STAGE_DEVICE=$(STAGE_DEVICE))"
	@echo "make update-version  writes the generated version ($(VERSION)) into package.json"
