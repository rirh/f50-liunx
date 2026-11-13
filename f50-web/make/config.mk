# Shared settings. The device is reached over SSH as root (OpenWrt's dropbear).
PM ?= pnpm
DEVICE ?= root@192.168.77.1
STAGE_DEVICE ?= $(DEVICE)
REMOTE_DIR ?= /www/f50
SSH_OPTS ?= -o ConnectTimeout=8
