# Copies dist/ to the device beside the live copy, then swaps it in with one rename: a half-copied console is never
# what the browser loads. scp -O: OpenWrt's dropbear has no SFTP server.
define push
	ssh $(1) 'rm -rf $(REMOTE_DIR).new && mkdir -p $(REMOTE_DIR).new'
	scp -O -r dist/. $(1):$(REMOTE_DIR).new/
	ssh $(1) 'rm -rf $(REMOTE_DIR).old; if [ -d $(REMOTE_DIR) ]; then mv $(REMOTE_DIR) $(REMOTE_DIR).old; fi; mv $(REMOTE_DIR).new $(REMOTE_DIR) && rm -rf $(REMOTE_DIR).old'
	@echo "deployed to $(1):$(REMOTE_DIR)"
endef

.PHONY: deploy deploy-stage
deploy: update-version build
	$(call push,$(DEVICE))

deploy-stage: update-version build-stage
	$(call push,$(STAGE_DEVICE))
