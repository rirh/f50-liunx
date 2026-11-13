# ZTE F50: OpenWrt with f50d and the f50-web console. `make` lists every command; the targets live in make/*.mk.
.DEFAULT_GOAL := help

include make/config.mk
include make/build.mk
include make/install.mk
include make/test.mk
include make/deploy.mk
include make/device.mk
include make/dev.mk
include make/help.mk
