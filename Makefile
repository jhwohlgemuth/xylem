#!make
-include .env

setup:
	@cargo install $(DEV_DEPENDENCIES)

audit:
	@./scripts/check-dependency-policy.sh
	@cargo shear --locked
	@cargo deny check
	@cargo audit `awk -F '\t' 'NR > 1 { printf "--ignore %s ", $$1 }' security/advisory-exceptions.tsv`

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

cover:
	@cargo tarpaulin --out lcov --line --skip-clean --no-dead-code --engine llvm --profile coverage --locked

coverage: cover

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

help:
	@mkdir -p docs/src/generated
	@cargo run --quiet --package xylem-cli -- --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-help.txt
	@cargo run --quiet --package xylem-cli -- run --help | sed 's/[[:space:]]*$$//' > docs/src/generated/xylem-run-help.txt

homebrew-formula:
	@./scripts/homebrew/generate-formula.sh $(VERSION) ./Formula/xylem.rb

publish-book: book
	@git clone "https://user:${ACCESS_TOKEN}@code-int.ornl.gov/sites/xylem.git" site
	@rm -rf ./site/html
	@mv docs/book ./site/html
	@cd ./site && git add html && (git diff --cached --quiet || (git commit -m "Publish Xylem Book" && git push))

version: 
	@echo $(VERSION)
#
# Variables
#
VERSION = `awk -F ' = ' '$$1 == "version" { gsub(/["\r]/, "", $$2); print $$2; exit }' Cargo.toml`
DEFAULT_TARGET = `awk -F ' = ' '$$1 ~ /default-target/ { gsub(/["]/, "", $$2); printf("%s",$$2) }' Cargo.toml | tr . _`
DEFAULT_WINDOWS_TARGET = x86_64-pc-windows-gnu
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
	clean \
	cover \
	coverage \
	doc \
	doctest \
	help \
	homebrew-formula \
	lint \
	publish-book \
	setup \
	test \
	test-ci \
	version

.PHONY: $(TASKS)
