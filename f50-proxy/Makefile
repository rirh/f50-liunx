# f50-proxy: the proxy engine (Go, Xray-core). `make device` builds the router binary.
.DEFAULT_GOAL := build

VERSION ?= 0.1.0
GO      ?= go
DIST    := dist
LDFLAGS := -s -w -X f50/proxy/internal/version.Version=$(VERSION)

.PHONY: build device test vet clean

## build: host binary (dist/f50-proxy), for `check` and trying the API locally
build:
	$(GO) build -trimpath -ldflags "$(LDFLAGS)" -o $(DIST)/f50-proxy ./cmd/f50-proxy

## device: static linux/arm64 binary for the F50 (installed as /opt/f50/bin/f50-proxy)
device:
	CGO_ENABLED=0 GOOS=linux GOARCH=arm64 $(GO) build -trimpath -ldflags "$(LDFLAGS)" -o $(DIST)/f50-proxy-linux-arm64 ./cmd/f50-proxy
	@ls -l $(DIST)/f50-proxy-linux-arm64 | awk '{printf "%s: %.1f MB\n", $$NF, $$5/1048576}'

## test: vet and unit tests (they bind loopback ports)
test: vet
	$(GO) test ./...

vet:
	$(GO) vet ./...

clean:
	rm -rf $(DIST)
