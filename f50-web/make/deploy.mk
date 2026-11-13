# Streams dist/ as a tar over one SSH connection into a directory beside the live copy, then swaps it in with one
# rename: a half-copied console is never what the browser loads. No scp: OpenWrt's dropbear has no SFTP server, and
# OpenSSH 10's legacy scp refuses `dist/.`. COPYFILE_DISABLE: macOS tar adds no ._ files or extended attributes.
define push
	COPYFILE_DISABLE=1 tar --no-xattrs --no-mac-metadata -C dist -cf - . | ssh $(SSH_OPTS) $(1) 'set -e; rm -rf $(REMOTE_DIR).new; mkdir -p $(REMOTE_DIR).new; tar -C $(REMOTE_DIR).new -xf -; rm -rf $(REMOTE_DIR).old; if [ -d $(REMOTE_DIR) ]; then mv $(REMOTE_DIR) $(REMOTE_DIR).old; fi; mv $(REMOTE_DIR).new $(REMOTE_DIR); rm -rf $(REMOTE_DIR).old'
	@echo "deployed to $(1):$(REMOTE_DIR)"
endef

.PHONY: deploy deploy-stage
deploy: update-version build
	$(call push,$(DEVICE))

deploy-stage: update-version build-stage
	$(call push,$(STAGE_DEVICE))
