# macOS reports arm64; Alpine, QEMU and Rust all call it aarch64.
ARCH ?= $(shell uname -m | sed 's/^arm64$$/aarch64/')
BUILDER := alpymist-builder
# The container is the architecture being built for, named explicitly: a
# DOCKER_DEFAULT_PLATFORM set for other work otherwise decides it, and an
# aarch64 image built in an emulated x86_64 container is built under emulation
# from start to finish.
PLATFORM := linux/$(if $(filter aarch64,$(ARCH)),arm64,amd64)

.PHONY: help check test lint builder shell iso smoke clean
help: ## Show this help
	@grep -hE '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | sed 's/:.*## /\t/' | expand -t20

check: ## Type-check the Rust workspace
	cargo check --workspace --all-targets

test: ## Run the Rust test suite
	cargo test --workspace

lint: ## Format check + clippy, warnings are errors
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings

builder: ## Build the Alpine build container
	docker build --platform $(PLATFORM) -t $(BUILDER) builder

shell: builder ## Interactive shell in the build container
	docker run --rm -it --platform $(PLATFORM) -v "$(PWD)":/src $(BUILDER) bash

iso: builder ## Build the Alpymist ISO (ARCH=aarch64|x86_64)
	mkdir -p out && chmod 777 out
	docker run --rm --platform $(PLATFORM) -v "$(PWD)":/src -v "$(PWD)/out":/out $(BUILDER) \
		bash /src/ci/build-iso.sh $(ARCH)

smoke: ## Boot the built ISO in QEMU and assert it reports a tier
	cargo run -q -p xtask -- smoke --iso out/alpymist-0.0.1-$(ARCH).iso --arch $(ARCH)

clean:
	cargo clean
	rm -rf out work
