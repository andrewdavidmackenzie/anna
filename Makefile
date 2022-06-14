all: clippy build test docs

.PHONY: linux_dependencies
linux_dependencies:
	sudo apt-get -y install libzmq3-dev

.PHONY: mac_dependencies
mac_dependencies:
	brew install zmq

.PHONY: clippy
clippy:
	cargo clippy --tests # -- -D warnings

.PHONY: build
build:
	./scripts/build.sh -bDebug -t -j2
	cargo build

.PHONY: test
test:
	./tests/simple/test-simple.sh
	cargo test
	rm -f log.txt log_0.txt pids client_log.txt

.phony: docs
docs:
	sudo apt-get -y install graphviz
	cargo install mdbook
	cargo install mdbook-linkcheck
	cargo doc --no-deps --target-dir=target/html/code
	mdbook build

.PHONY: configure_coverage
configure_coverage:
	cargo install grcov
	rustup component add llvm-tools-preview
	export RUSTFLAGS="-C instrument-coverage"
	export LLVM_PROFILE_FILE="flow-%p-%m.profraw"

.PHONY: upload_coverage
upload_coverage:
	grcov . --binary-path target/debug/ -s . -t lcov --branch --ignore-not-existing --ignore "/*" -o lcov.info
	bash <(curl -s https://codecov.io/bash) -f lcov.info
	rm -f lcov.info