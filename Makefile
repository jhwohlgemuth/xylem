.PHONY: test

lint:
	@cargo fmt
	@cargo clippy

test: lint
	@cargo test
	@cargo tarpaulin --out lcov --ignore-tests