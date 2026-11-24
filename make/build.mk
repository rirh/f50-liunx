# Builds: f50d for the device, the web console, the vendor kernel.
.PHONY: build f50d proxy web web-deps kernel clean

build: f50d proxy web

# a static aarch64 binary (rust-lld links it: no cross C toolchain needed)
f50d:
	cd f50d && $(CARGO) device

# the proxy engine: One Node's client core (one-node/app/client, see f50-proxy/README.md), a static linux/arm64 binary
proxy:
	$(MAKE) -C f50-proxy device

web-deps:
	@[ -d f50-web/node_modules ] || (cd f50-web && $(PM) install --frozen-lockfile)

web: web-deps
	$(MAKE) -C f50-web build

# the 5.4 vendor kernel and its modules into out/ (docker; the sources are fetched on the first run)
kernel:
	sh kernel/vendor/build-all.sh

clean:
	cd f50d && $(CARGO) clean
	cd f50-web && rm -rf dist
	$(MAKE) -C f50-proxy clean
