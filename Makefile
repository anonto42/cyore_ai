.DEFAULT_GOAL := help

# Crate whose binary `run`/`dev` launch (one workspace member per learning milestone)
CRATE ?= xor
IMAGE := cyore-$(CRATE)
CARGO := $(shell which cargo 2>/dev/null || echo $(HOME)/.cargo/bin/cargo)
OPENSPEC := $(shell which openspec 2>/dev/null || echo npx --yes @fission-ai/openspec@1.14.2)

# MNIST dataset: mirror URL, local directory (git-ignored), and the four IDX files
MNIST_URL ?= https://storage.googleapis.com/cvdf-datasets/mnist
MNIST_DIR := data/mnist
MNIST_FILES := train-images-idx3-ubyte train-labels-idx1-ubyte t10k-images-idx3-ubyte t10k-labels-idx1-ubyte
# Extra options for `make mnist`, e.g. make mnist ARGS="--epochs 2"
ARGS ?=

.PHONY: help build run dev test bench lint fmt fmt-check clean check spec size docker-build docker-run mnist-data mnist commands-data intent-train intent-eval intent-predict intent-quantize intent-eval-human agent agent-run release cross-arm64

help:
	@printf '%s\n' 'Cyore commands (CRATE=$(CRATE); override with CRATE=<member>):'
	@printf '%s\n' '  make help          Show available commands'
	@printf '%s\n' '  make build         Build the optimised release binary for CRATE'
	@printf '%s\n' '  make run           Build and run the release binary for CRATE'
	@printf '%s\n' '  make dev           Run CRATE with the debug profile (fast compile)'
	@printf '%s\n' '  make test          Run all workspace tests'
	@printf '%s\n' '  make bench         Run benchmarks (cargo bench)'
	@printf '%s\n' '  make lint          Run clippy with warnings as errors'
	@printf '%s\n' '  make fmt           Format all Rust code'
	@printf '%s\n' '  make size          Show the release binary size (Stage 4 target: 5-30 MB)'
	@printf '%s\n' '  make spec          Validate OpenSpec specs and changes (strict)'
	@printf '%s\n' '  make mnist-data    Download and unpack the MNIST dataset into data/mnist/'
	@printf '%s\n' '  make mnist         Train the MNIST classifier (release); pass options via ARGS="..."'
	@printf '%s\n' '  make commands-data Generate the Stage 2 command dataset into data/commands/'
	@printf '%s\n' '  make intent-train  Train the intent + slot model into data/models/intent/'
	@printf '%s\n' '  make intent-eval   Evaluate the trained model on the hand-written test set'
	@printf '%s\n' '  make intent-predict ARGS="show me the files"  Turn one request into a command'
	@printf '%s\n' '  make intent-quantize Write the int8 copy of the model and compare its accuracy'
	@printf '%s\n' '  make intent-eval-human Score the model on dataset/human_test.txt (totals only)'
	@printf '%s\n' '  make agent         Build the cyore agent (release): ./target/release/cyore'
	@printf '%s\n' '  make agent-run     Run cyore; one request via ARGS="show me the files", else interactive'
	@printf '%s\n' '  make release       Dataset → train → int8 → one-file ./target/dist/cyore, model built in'
	@printf '%s\n' '  make cross-arm64   Cross-compile a static Linux ARM64 cyore (e.g. Raspberry Pi)'
	@printf '%s\n' '  make docker-build  Build the container image for CRATE'
	@printf '%s\n' '  make docker-run    Run the container image for CRATE'
	@printf '%s\n' '  make clean         Remove build artifacts'
	@printf '%s\n' '  make check         Run fmt check, clippy, tests, and release build'

build:
	@printf '%s\n' 'Compiling $(CRATE) (release)...'
	@$(CARGO) build --release -p $(CRATE)
	@printf '%s\n' 'Build complete: ./target/release/$(CRATE)'

run: build
	@./target/release/$(CRATE)

dev:
	@$(CARGO) run -p $(CRATE)

test:
	@printf '%s\n' 'Running workspace test suite...'
	@$(CARGO) test --workspace

bench:
	@$(CARGO) bench --workspace

lint:
	@$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	@$(CARGO) fmt --all

fmt-check:
	@$(CARGO) fmt --all -- --check

size: build
	@ls -lh target/release/$(CRATE) | awk '{print $$5, $$9}'

spec:
	@printf '%s\n' 'Validating OpenSpec specs and changes...'
	@$(OPENSPEC) validate --all --strict --no-interactive

# Download each file only if it is missing. Unpack to a temp file and rename it, so an
# interrupted run never leaves a half-written IDX file behind.
mnist-data:
	@mkdir -p $(MNIST_DIR)
	@for f in $(MNIST_FILES); do \
		if [ -f "$(MNIST_DIR)/$$f" ]; then \
			printf '%s\n' "  $$f already present, skipping"; \
		else \
			printf '%s\n' "  downloading $$f"; \
			curl -fsSL "$(MNIST_URL)/$$f.gz" -o "$(MNIST_DIR)/$$f.gz" && \
			gunzip -c "$(MNIST_DIR)/$$f.gz" > "$(MNIST_DIR)/$$f.tmp" && \
			mv "$(MNIST_DIR)/$$f.tmp" "$(MNIST_DIR)/$$f" && \
			rm -f "$(MNIST_DIR)/$$f.gz" || exit 1; \
		fi; \
	done
	@printf '%s\n' 'MNIST dataset ready in $(MNIST_DIR)/'

mnist:
	@$(CARGO) run --release -p mnist -- $(ARGS)

commands-data:
	@$(CARGO) run --release -q -p dataset -- $(ARGS)

intent-train:
	@$(CARGO) run --release -q -p intent -- train $(ARGS)

intent-eval:
	@$(CARGO) run --release -q -p intent -- eval $(ARGS)

intent-predict:
	@$(CARGO) run --release -q -p intent -- predict $(ARGS)

intent-quantize:
	@$(CARGO) run --release -q -p intent -- quantize $(ARGS)

# Converts dataset/human_test.txt (validating it) and scores the current model on it.
intent-eval-human:
	@$(CARGO) run --release -q -p dataset > /dev/null
	@$(CARGO) run --release -q -p intent -- eval --set human $(ARGS)

agent:
	@$(CARGO) build --release -p agent
	@printf '%s\n' 'Build complete: ./target/release/cyore'

agent-run:
	@$(CARGO) run --release -q -p agent -- $(ARGS)

# Stage 4: everything from scratch into one file. The build script embeds model-q8.bpk.
release: commands-data intent-train intent-quantize
	@$(CARGO) build --profile dist -p agent
	@printf 'Single-file binary: ./target/dist/cyore (%s bytes)\n' "$$(wc -c < target/dist/cyore)"
	@./target/dist/cyore --help | tail -n 1

# Static ARM64 Linux binary without a cross C toolchain: musl ships its own C runtime objects and
# Rust's bundled linker (rust-lld) links them. Needs `rustup target add aarch64-unknown-linux-musl`.
ARM64_TARGET := aarch64-unknown-linux-musl
cross-arm64:
	@rustup target list --installed 2>/dev/null | grep -qx '$(ARM64_TARGET)' || { printf '%s\n' 'Missing target: run rustup target add $(ARM64_TARGET)'; exit 1; }
	@CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld $(CARGO) build --profile dist -p agent --target $(ARM64_TARGET)
	@file target/$(ARM64_TARGET)/dist/cyore

docker-build:
	@docker build --build-arg CRATE=$(CRATE) -t $(IMAGE):local .

docker-run: docker-build
	@docker run --rm $(IMAGE):local

clean:
	@printf '%s\n' 'Cleaning up build artifacts...'
	@$(CARGO) clean
	@printf '%s\n' 'Clean complete.'

check: fmt-check lint test build
	@printf '%s\n' 'All quality checks passed.'
