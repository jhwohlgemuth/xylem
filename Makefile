#!make
-include .env

setup:
	@cargo install $(DEV_DEPENDENCIES)

audit:
	@cargo xtask audit $(if $(OFFLINE),--offline,)

book:
	@mdbook clean docs
	@mdbook build docs

book-write:
	@mdbook serve --open docs

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

changelog-release:
	@git pull origin main --tags
	@git-cliff --output CHANGELOG.md --gitlab-token ${GITLAB_TOKEN} --latest

check-release-crates:
	@cargo xtask check-release-crates

check-version:
	@cargo xtask check-version

cover:
	@cargo tarpaulin --out lcov --line --skip-clean --no-dead-code --engine llvm --profile coverage --locked

coverage: cover

lint:
	@cargo fmt --verbose
	@cargo clippy --no-deps --all-targets --fix --allow-dirty
	@hadolint sandbox/runner/Containerfile sandbox/runner/geospatial/Containerfile
	@shellcheck scripts/install-appimage-tools.sh scripts/prepare-acquisition-ci.sh scripts/install-zigbuild.sh scripts/package-linux.sh scripts/upload-binaries.sh
	@sh -n scripts/check-process-spawns.sh
	@sh scripts/check-process-spawns.sh
	@sh -n scripts/check-rootless-sandbox.sh
	@sh -n scripts/install-appimage-tools.sh scripts/prepare-acquisition-ci.sh scripts/install-zigbuild.sh scripts/package-linux.sh scripts/upload-binaries.sh

linux-packages:
	@$(MUSL_BUILD) --release --package xylem-cli --target $(DEFAULT_TARGET)
	@$(MUSL_BUILD) --release --package xylem-cli --target $(DEFAULT_ARM64_TARGET)
	@./scripts/package-linux.sh $(VERSION) $(DEFAULT_TARGET) amd64 x86_64
	@./scripts/package-linux.sh $(VERSION) $(DEFAULT_ARM64_TARGET) arm64 aarch64

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

dependency-policy:
	@cargo xtask dependency-policy

help:
	@mkdir -p docs/src/generated
	@cargo run --quiet --package xylem-cli -- --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-help.txt
	@cargo run --quiet --package xylem-cli -- export --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-export-help.txt
	@cargo run --quiet --package xylem-cli -- run --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-run-help.txt
	@cargo run --quiet --package xylem-cli -- tui --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-tui-help.txt

homebrew-formula:
	@cargo xtask homebrew-formula $(VERSION) ./Formula/xylem.rb

publish-book: book
	@git clone "https://user:${ACCESS_TOKEN}@code-int.ornl.gov/sites/xylem.git" site
	@rm -rf ./site/html
	@mv docs/book ./site/html
	@cd ./site && git add html && (git diff --cached --quiet || (git commit -m "Publish Xylem Book" && git push))

publish-crates:
	@cargo xtask publish-crates

release-version:
	@cargo xtask release-version $(VERSION)
	@$(MAKE) check-release-crates check-version

release-checksums:
	@$(call WRITE_SHA256,./bin/xylem-v$(VERSION))
	@$(call WRITE_SHA256,./bin/xylem-v$(VERSION).exe)

version: 
	@cargo xtask version
#
# Variables
#
DEFAULT_ARM64_TARGET = aarch64-unknown-linux-musl
VERSION = `awk -F ' = ' '$$1 == "version" { gsub(/["\r]/, "", $$2); print $$2; exit }' Cargo.toml`
DEFAULT_TARGET = `awk -F ' = ' '$$1 ~ /default-target/ { gsub(/["]/, "", $$2); printf("%s",$$2) }' Cargo.toml | tr . _`
DEFAULT_WINDOWS_TARGET = x86_64-pc-windows-gnu
MUSL_BUILD ?= cargo build
DEV_DEPENDENCIES = \
	cargo-audit@0.22.2 \
	cargo-deny@0.20.2 \
	cargo-nextest \
	cargo-shear@1.14.0 \
	cargo-tarpaulin \
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
	book-write \
	build \
	changelog \
	changelog-release \
	check-release-crates \
	check-version \
	clean \
	cover \
	coverage \
	dependency-policy \
	doc \
	doctest \
	help \
	homebrew-formula \
	lint \
	linux-packages \
	publish-book \
	publish-crates \
	release-checksums \
	release-version \
	setup \
	test \
	test-ci \
	version
WRITE_SHA256 = file="$(1)"; directory="$$(dirname -- "$$file")"; name="$$(basename -- "$$file")"; if command -v sha256sum >/dev/null 2>&1; then (cd "$$directory" && sha256sum "$$name" > "$$name.sha256"); elif command -v shasum >/dev/null 2>&1; then (cd "$$directory" && shasum -a 256 "$$name" > "$$name.sha256"); else echo "A SHA-256 utility (sha256sum or shasum) is required" >&2; exit 1; fi

.PHONY: $(TASKS)
