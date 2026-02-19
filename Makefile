#!make
.PHONY: $(TASKS)
-include .env

setup:
	@cargo install $(DEV_DEPENDENCIES)

audit:
	@cargo udeps
	@cargo deny check
	@cargo audit \
		--ignore RUSTSEC-2020-0071 \
		--ignore RUSTSEC-2025-0055 \
		--ignore RUSTSEC-2025-0067 \
		--ignore RUSTSEC-2025-0068 \
		--ignore RUSTSEC-2025-0056 \
		--ignore RUSTSEC-2024-0388 \
		--ignore RUSTSEC-2024-0436

build: clean
	@mkdir -p ./bin
	@cargo build --release --package xylem-cli --target $(DEFAULT_TARGET)
	@mv target/$(DEFAULT_TARGET)/release/xylem ./bin/xylem-v$(VERSION)
	@pwsh -Command New-Item -ItemType Directory -Path ".\bin" -Force
	@cargo build --release --package xylem-cli --target $(DEFAULT_WINDOWS_TARGET)
	@pwsh -Command Move-Item -Path "target\$(DEFAULT_WINDOWS_TARGET)\release\xylem.exe" -Destination ".\bin\xylem-v${VERSION}.exe"

clean:
	@rm -rf ./bin
	@cargo clean

changelog:
	@git pull origin main --tags
	@git-cliff --output CHANGELOG.md --gitlab-token ${GITLAB_TOKEN}

coverage: clean lint
	@cargo tarpaulin --out lcov --line --skip-clean --no-dead-code --engine llvm --lib

lint:
	@cargo fmt --verbose
	@cargo clippy --no-deps --all-targets --fix --allow-dirty

test:
	@cargo nextest run

test-all:
	@cargo nextest run --run-ignored all

test-ci: clean lint
	@cargo test --lib --bins --tests

doc:
	@cargo clean --doc
	@cargo doc --no-deps --all-features

doctest:
	@cargo test --doc

version: 
	@echo $(VERSION)
#
# Variables
#
VERSION = `awk -F ' = ' '$$1 ~ /version/ { gsub(/["]/, "", $$2); printf("%s",$$2) }' Cargo.toml`
DEFAULT_TARGET = `awk -F ' = ' '$$1 ~ /default-target/ { gsub(/["]/, "", $$2); printf("%s",$$2) }' Cargo.toml | tr . _`
DEFAULT_WINDOWS_TARGET = x86_64-pc-windows-gnu
DEV_DEPENDENCIES = \
	cargo-audit \
	cargo-auditable \
	cargo-deny \
	cargo-insta \
	cargo-nextest \
	cargo-shear \
	cargo-tarpaulin \
	cargo-udeps \
	mdbook \
	mdbook-mermaid \
	mdbook-pikchr
TARGETS = \
	i586-unknown-linux-musl \
	i686-unknown-linux-gnu \
	aarch64-unknown-linux-musl \
	arm-unknown-linux-gnueabi \
	arm-unknown-linux-musleabi \
	riscv64gc-unknown-linux-gnu \
	x86_64-unknown-linux-gnu \
	x86_64-unknown-linux-musl \
	x86_64-pc-windows-gnu
TASKS = \
	audit \
	book \
	build \
	changelog \
	clean \
	coverage \
	doc \
	lint \
	setup \
	test \
	test-ci \
	version