#!make
-include .env

setup: ## Install the Rust toolchain and project development dependencies.
	@rustup update stable
	@cargo install $(DEV_DEPENDENCIES) --locked
	@rustup target add $(TARGETS)

help: ## Show the available development tasks.
	@awk 'BEGIN {FS = ":.*## "; printf "Usage: make <target> [VERSION=x.y.z] [CI_JOB=<id|name>] [CI_PIPELINE=<id>] [CI_REF=<branch|tag>] [RELEASE_REF=<branch|tag>] [TAG=vX.Y.Z]\n\nTargets:\n"} /^[a-zA-Z0-9_-]+:.*## / {printf "  %-22s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

audit: ## Run dependency and security audits for the project.
	@cargo xtask audit $(if $(OFFLINE),--offline,)

book: ## Build the documentation site with mdBook.
	@mdbook clean docs
	@mdbook build docs

book-write: ## Serve the documentation site locally for preview.
	@mdbook serve --open docs

build: ## Build release binaries for the CLI.
ifeq ($(OS),Windows_NT)
	@$(CLEAN_BIN_CMD)
	@pwsh -Command New-Item -ItemType Directory -Path ".\bin" -Force
	@cargo build --release --package xylem-cli --target $(DEFAULT_WINDOWS_TARGET)
	@pwsh -NoProfile -File scripts/check-windows-runtime.ps1 -Binary "$(ARTIFACT_TARGET_DIR)/$(DEFAULT_WINDOWS_TARGET)/release/xylem.exe"
	@pwsh -Command Move-Item -Path "$(ARTIFACT_TARGET_DIR)/$(DEFAULT_WINDOWS_TARGET)/release/xylem.exe" -Destination "./bin/xylem-v${VERSION}.exe" -Force
else
	@$(CLEAN_BIN_CMD)
	@mkdir -p ./bin
	@$(MUSL_BUILD) --release --package xylem-cli --target $(DEFAULT_TARGET)
	@cp $(ARTIFACT_TARGET_DIR)/$(DEFAULT_TARGET)/release/xylem ./bin/xylem-v$(VERSION)
	@cargo build --release --package xylem-cli --target $(DEFAULT_WINDOWS_TARGET)
	@pwsh -NoProfile -File scripts/check-windows-runtime.ps1 -Binary "$(ARTIFACT_TARGET_DIR)/$(DEFAULT_WINDOWS_TARGET)/release/xylem.exe" -Objdump x86_64-w64-mingw32-objdump
	@cp $(ARTIFACT_TARGET_DIR)/$(DEFAULT_WINDOWS_TARGET)/release/xylem.exe ./bin/xylem-v$(VERSION).exe
endif

clean: ## Remove build artifacts and generated binaries.
	@cargo clean
	@$(CLEAN_BIN_CMD)

changelog: ## Generate the changelog from git tags and metadata.
	@git pull origin main --tags
	@git-cliff --output CHANGELOG.md --gitlab-token ${GITLAB_TOKEN}

changelog-release: ## Generate the latest release changelog entry.
	@git pull origin main --tags
	@git-cliff --output CHANGELOG.md --gitlab-token ${GITLAB_TOKEN} --latest

ci-retry: ## Retry a GitLab CI job, selecting interactively unless CI_JOB is set.
	@glab ci retry $(if $(CI_JOB),"$(CI_JOB)") $(if $(CI_PIPELINE),--pipeline-id "$(CI_PIPELINE)",$(if $(CI_REF),--branch "$(CI_REF)"))

ci-watch: ## Watch the latest GitLab CI pipeline for CI_REF or the current branch.
	@glab ci status $(if $(CI_REF),--branch "$(CI_REF)") --live

check-release-crates: ## Verify that every crates.io workspace crate is in the release list.
	@cargo xtask check-release-crates

check-version: ## Validate the workspace version metadata.
	@cargo xtask check-version

cover: ## Run coverage analysis with tarpaulin.
	@cargo tarpaulin $(COVER_JOBS) --out lcov --line --force-clean --no-dead-code --engine llvm --profile coverage --locked --timeout 300

coverage: cover ## Run coverage analysis with tarpaulin.

lint: ## Format and lint the entire workspace.
	@cargo fmt --verbose
	@cargo clippy --workspace --exclude xylem-memory-spike --exclude xtask --no-deps --all-targets --fix --allow-dirty
	@hadolint sandbox/runner/Containerfile sandbox/runner/geospatial/Containerfile
	@shellcheck scripts/install-appimage-tools.sh scripts/prepare-acquisition-ci.sh scripts/install-zigbuild.sh scripts/package-linux.sh scripts/upload-binaries.sh
	@sh -n scripts/check-process-spawns.sh
	@sh scripts/check-process-spawns.sh
	@sh -n scripts/check-rootless-sandbox.sh
	@sh -n scripts/install-appimage-tools.sh scripts/prepare-acquisition-ci.sh scripts/install-zigbuild.sh scripts/package-linux.sh scripts/upload-binaries.sh

linux-packages: ## Build x86_64 and ARM64 AppImage and Debian release packages.
	@$(MUSL_BUILD) --release --package xylem-cli --target $(DEFAULT_TARGET)
	@$(MUSL_BUILD) --release --package xylem-cli --target $(DEFAULT_ARM64_TARGET)
	@sh scripts/package-linux.sh $(VERSION) $(DEFAULT_TARGET) amd64 x86_64
	@sh scripts/package-linux.sh $(VERSION) $(DEFAULT_ARM64_TARGET) arm64 aarch64

test: ## Run the main test suite with nextest.
	@cargo nextest run

test-all: ## Run the full suite including ignored tests.
	@cargo nextest run --run-ignored all

test-ci: lint ## Run the CI-style test workflow.
	@cargo test --lib --bins --tests

doc: ## Build the Rust documentation for the workspace.
	@cargo clean --doc
	@cargo doc --no-deps --all-features

doctest: ## Run the project doctests.
	@cargo test --doc

dependency-policy: ## Validate workspace dependency policy.
	@cargo xtask dependency-policy

help-cli: ## Regenerate the checked-in Xylem CLI help reference.
	@mkdir -p docs/src/generated
	@cargo run --quiet --package xylem-cli -- --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-help.txt
	@cargo run --quiet --package xylem-cli -- export --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-export-help.txt
	@cargo run --quiet --package xylem-cli -- run --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-run-help.txt
	@cargo run --quiet --package xylem-cli -- tui --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-tui-help.txt

homebrew-formula: ## Generate the Homebrew formula for the CLI.
	@cargo xtask homebrew-formula $(VERSION) ./Formula/xylem.rb

publish-book: book ## Publish the built docs site to the configured destination.
	@git clone "https://user:${ACCESS_TOKEN}@code-int.ornl.gov/sites/xylem.git" site
	@rm -rf ./site/html
	@mv docs/book ./site/html
	@cd ./site && git add html && (git diff --cached --quiet || (git commit -m "Publish Xylem Book" && git push))

publish-crates: ## Publish the workspace crates to crates.io.
	@cargo xtask publish-crates

release-version: ## Bump the release version and verify it.
	@cargo xtask release-version $(VERSION)
	@$(MAKE) check-release-crates check-version

release-checksums: ## Generate SHA256 checksums for release artifacts.
	@$(call WRITE_SHA256,./bin/xylem-v$(VERSION))
	@$(call WRITE_SHA256,./bin/xylem-v$(VERSION).exe)

release-download: ## Download TAG release assets into the release-artifacts directory.
	@glab release download "$(TAG)" --dir release-artifacts

release-list: ## List recent GitLab releases.
	@glab release list --per-page 10

release-retry: ## Retry a job in the latest RELEASE_REF pipeline.
	@$(MAKE) ci-retry CI_JOB="$(CI_JOB)" CI_PIPELINE="$(CI_PIPELINE)" CI_REF="$(RELEASE_REF)"

release-view: ## Show the GitLab release and assets for TAG.
	@glab release view "$(TAG)"

release-watch: ## Watch the latest RELEASE_REF pipeline used to publish a release.
	@$(MAKE) ci-watch CI_REF="$(RELEASE_REF)"

version: ## Print the project version string.
	@cargo xtask version

rust-version: ## Print the Rust toolchain version string.
	@echo $(RUST_VERSION)
#
# Variables
#
ifeq ($(OS),Windows_NT)
CLEAN_BIN_CMD = pwsh -NoProfile -Command "if (Test-Path ./bin) { Remove-Item -Recurse -Force ./bin }"
COVER_JOBS ?= --jobs 1
WRITE_SHA256 = pwsh -NoProfile -Command "(Get-FileHash -Algorithm SHA256 -LiteralPath '$(1)').Hash.ToLowerInvariant() + '  $(1)' | Set-Content -Encoding ascii -LiteralPath '$(1).sha256'"
else
CLEAN_BIN_CMD = rm -rf ./bin
COVER_JOBS ?=
WRITE_SHA256 = if command -v sha256sum >/dev/null 2>&1; then sha256sum "$(1)" > "$(1).sha256"; elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$(1)" > "$(1).sha256"; else echo "A SHA-256 utility (sha256sum or shasum) is required" >&2; exit 1; fi
endif
DEFAULT_ARM64_TARGET = aarch64-unknown-linux-musl
DEFAULT_TARGET = `awk -F ' = ' '$$1 ~ /default-target/ { gsub(/["\r]/, "", $$2); printf("%s",$$2) }' Cargo.toml | tr . _`
DEFAULT_WINDOWS_TARGET = x86_64-pc-windows-gnu
MUSL_BUILD ?= cargo build
ARTIFACT_TARGET_DIR = $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)
RUST_VERSION = `awk -F ' = ' '$$1 ~ /rust-version/ { gsub(/["\r]/, "", $$2); printf("%s",$$2) }' Cargo.toml`
VERSION = `awk -F ' = ' '$$1 ~ /^version/ { gsub(/["\r]/, "", $$2); printf("%s",$$2) }' Cargo.toml`
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
CI_JOB =
CI_PIPELINE =
CI_REF =
RELEASE_REF ?= main
TAG ?= v$(VERSION)
TASKS = \
	audit \
	book \
	book-write \
	build \
	changelog \
	changelog-release \
	ci-retry \
	ci-watch \
	check-release-crates \
	check-version \
	clean \
	cover \
	coverage \
	dependency-policy \
	doc \
	doctest \
	help \
	help-cli \
	homebrew-formula \
	lint \
	linux-packages \
	publish-book \
	publish-crates \
	release-checksums \
	release-download \
	release-list \
	release-retry \
	release-version \
	release-view \
	release-watch \
	rust-version \
	setup \
	test \
	test-all \
	test-ci \
	version

.PHONY: $(TASKS)
