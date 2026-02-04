.PHONY: lint test build clean

lint:
	pre-commit run --all-files

test:
	cargo nextest run

build:
	cargo build --release

clean:
	cargo clean
