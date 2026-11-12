.PHONY: deploy deploy-ui deploy-backend deploy-all

deploy: deploy-ui

deploy-ui:
	$(DEVICE_TOOL) deploy --scope ui

deploy-backend:
	$(DEVICE_TOOL) deploy --scope backend

deploy-all:
	$(DEVICE_TOOL) deploy --scope all
