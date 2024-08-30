.PHONY: doc lint test

clean:
	@rm -rf ./bin
	@cargo clean

lint:
	@cargo fmt --verbose
	@cargo clippy --no-deps --all-targets --fix --allow-dirty

test: lint
	@cargo test
	@cargo tarpaulin --out lcov --ignore-tests

test-cross: clean
	@for target in $(TARGETS) ; do \
        cross test --target $$target; \
    done

doc:
	@cargo clean --doc
	@cargo doc --no-deps --all-features

build: clean
	@mkdir -p ./bin
	@for target in $(TARGETS) ; do \
		export DISPLAY=:0; \
        cross build --release --target $$target; \
		mv target/$${target}/release/pipe-cli ./bin/pipe_cli_$${target}; \
    done

release: clean
	@echo "Under Construction"

#
# Variables
#
TARGETS = \
	i586-unknown-linux-musl \
	i686-unknown-linux-gnu \
	aarch64-unknown-linux-musl \
	arm-unknown-linux-gnueabi \
	arm-unknown-linux-musleabi \
	riscv64gc-unknown-linux-gnu \
	x86_64-unknown-linux-gnu \
	x86_64-unknown-linux-musl