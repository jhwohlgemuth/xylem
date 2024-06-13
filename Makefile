.PHONY: test

lint:
	@cargo fmt
	@cargo clippy

test: lint
	@cargo test
	@cargo tarpaulin --out lcov --ignore-tests

doc:
	@cargo clean --doc
	@cargo doc --no-deps --all-features --document-private-items