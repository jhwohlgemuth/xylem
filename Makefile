#!make
.PHONY: $(TASKS)
-include .env

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
	@cargo nextest run --run-ignored all

test-ci: clean lint
	@cargo test --lib --bins --tests

test-cross: clean
	@for target in $(TARGETS) ; do \
        cross test --target $$target; \
    done

doc:
	@cargo clean --doc
	@cargo doc --no-deps --all-features

build: clean
	@mkdir -p ./bin
	@cargo build --release --target $(DEFAULT_TARGET)
	@mv target/$(DEFAULT_TARGET)/release/xylem ./bin/xylem-v$(VERSION)

build-cross: clean
	@mkdir -p ./bin
	@for target in $(TARGETS) ; do \
		export DISPLAY=:0; \
        cross build --release --target $$target; \
		mv target/$${target}/release/xylem ./bin/xylem_v$(VERSION)_$${target}; \
    done

release: clean
	@echo "Under Construction"

version: 
	@echo $(VERSION)
#
# Variables
#
VERSION = `awk -F ' = ' '$$1 ~ /version/ { gsub(/["]/, "", $$2); printf("%s",$$2) }' Cargo.toml`
DEFAULT_TARGET = `awk -F ' = ' '$$1 ~ /default-target/ { gsub(/["]/, "", $$2); printf("%s",$$2) }' Cargo.toml | tr . _`
TARGETS = \
	i586-unknown-linux-musl \
	i686-unknown-linux-gnu \
	aarch64-unknown-linux-musl \
	arm-unknown-linux-gnueabi \
	arm-unknown-linux-musleabi \
	riscv64gc-unknown-linux-gnu \
	x86_64-unknown-linux-gnu \
	x86_64-unknown-linux-musl
TASKS = \
	clean \
	lint \
	test \
	test-ci \
	test-cross \
	doc \
	build \
	build-cross \
	release \
	version