SHELL := /bin/bash

CARGO ?= cargo
WB    ?= wasm-bindgen

TARGET   := wasm32-unknown-unknown
WEB_BIN  := gitrust-web
WEB_DIR  := crates/gitrust-web
WEB_OUT  := $(WEB_DIR)/dist
WEB_WASM := target/$(TARGET)/release/$(WEB_BIN).wasm
ADDR     ?= 127.0.0.1:3737

WBG_VERSION ?= 0.2.121

.DEFAULT_GOAL := help
.PHONY: help setup web run serve check check-native check-wasm fmt lint clean

help:
	@echo "Targets:"
	@echo "  make setup       — add $(TARGET) target + install wasm-bindgen-cli $(WBG_VERSION)"
	@echo "  make web         — build WASM bundle into $(WEB_OUT)/"
	@echo "  make run         — build bundle, then start server on $(ADDR)"
	@echo "  make serve       — start server on $(ADDR) (skip bundle rebuild)"
	@echo "  make check       — cargo check on native + wasm32 targets"
	@echo "  make fmt         — cargo fmt --all"
	@echo "  make lint        — cargo clippy --all-targets -- -D warnings"
	@echo "  make clean       — cargo clean + remove $(WEB_OUT)"
	@echo
	@echo "Override ADDR (e.g. ADDR=0.0.0.0:8080 make run) to bind elsewhere."

setup:
	rustup target add $(TARGET)
	$(CARGO) install wasm-bindgen-cli --version $(WBG_VERSION) --locked

web:
	$(CARGO) build -p $(WEB_BIN) --target $(TARGET) --release
	mkdir -p $(WEB_OUT)
	$(WB) --target web --no-typescript \
	      --out-dir $(WEB_OUT) --out-name gitrust_web $(WEB_WASM)
	cp $(WEB_DIR)/index.html $(WEB_OUT)/index.html

run: web
	$(CARGO) run -p gitrust -- serve --addr $(ADDR) --web-dist $(WEB_OUT)

serve:
	$(CARGO) run -p gitrust -- serve --addr $(ADDR) --web-dist $(WEB_OUT)

check: check-native check-wasm

check-native:
	$(CARGO) check

check-wasm:
	$(CARGO) check -p $(WEB_BIN) --target $(TARGET)

fmt:
	$(CARGO) fmt --all

lint:
	$(CARGO) clippy --all-targets -- -D warnings

clean:
	$(CARGO) clean
	rm -rf $(WEB_OUT)
