.PHONY: test

lint:
	@cargo fmt
	@cargo clippy

test: lint
	@cargo tarpaulin --out lcov