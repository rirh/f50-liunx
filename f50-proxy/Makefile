# f50-proxy: the F50's proxy engine is One Node's client core (one-node/app/client), built for the router.
# `make device` builds the router binary; nothing here is Go code of its own.
.DEFAULT_GOAL := build

VERSION  ?= 0.1.0
GO       ?= go
ONE_NODE ?= /Users/zh/Documents/workspaces/voh/one/one-node
CLIENT   := $(ONE_NODE)/app/client
DIST     := $(CURDIR)/dist
APP      := github.com/voh/one-node/app/client/internal
# main.appVersion: the release (UA F50Proxy/<VERSION> <arch>); app.Product: the gateway's client name
# (One Node F50); device.Model: X-Device-Model.
LDFLAGS  := -s -w -X main.appVersion=$(VERSION) -X $(APP)/app.Product=F50Proxy -X '$(APP)/device.Model=ZTE F50'

.PHONY: build device test vet clean check-client

check-client:
	@test -f $(CLIENT)/go.mod || { echo "one-node client not found at $(CLIENT) (set ONE_NODE=…)" >&2; exit 1; }

## build: host binary (dist/f50-proxy), for `check` and trying the API locally
build: check-client
	cd $(CLIENT) && $(GO) build -trimpath -ldflags "$(LDFLAGS)" -o $(DIST)/f50-proxy .

## device: static linux/arm64 binary for the F50 (installed as /opt/f50/bin/f50-proxy)
device: check-client
	cd $(CLIENT) && CGO_ENABLED=0 GOOS=linux GOARCH=arm64 $(GO) build -trimpath -ldflags "$(LDFLAGS)" -o $(DIST)/f50-proxy-linux-arm64 .
	@ls -l $(DIST)/f50-proxy-linux-arm64 | awk '{printf "%s: %.1f MB\n", $$NF, $$5/1048576}'

## test: the client core's vet and tests (they bind loopback ports)
test: vet
	cd $(CLIENT) && $(GO) test ./...

vet: check-client
	cd $(CLIENT) && $(GO) vet ./...

clean:
	rm -rf $(DIST)
