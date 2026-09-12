ARCH ?= $(shell uname -m)
BUILDER := alpy-builder

.PHONY: help check test lint builder shell iso clean
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
	docker build -t $(BUILDER) builder

shell: builder ## Interactive shell in the build container
	docker run --rm -it -v "$(PWD)":/src $(BUILDER) bash

clean:
	cargo clean
	rm -rf out work
