# Tests and checks. `make test` is what a change must pass; test-installer runs the installer's and images' Python tests.
.PHONY: test test-f50d test-web lint lint-f50d lint-web test-installer test-all

test: test-f50d lint test-web

test-f50d:
	cd f50d && $(CARGO) test --workspace

lint: lint-f50d lint-web

lint-f50d:
	cd f50d && $(CARGO) fmt --all --check
	cd f50d && $(CARGO) clippy --workspace --all-targets -- -D warnings
	cd f50d && $(CARGO) clippy --release --target aarch64-unknown-linux-musl -p f50d -- -D warnings

lint-web: web-deps
	cd f50-web && $(PM) lint

# the type check of the production build
test-web: web-deps
	cd f50-web && $(PM) exec tsc -b

test-installer:
	$(PYTHON) -m unittest discover -s tests

test-all: test test-installer
