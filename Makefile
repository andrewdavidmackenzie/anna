all: clippy build test docs

.PHONY: linux_dependencies
linux_dependencies:
	sudo apt-get -y install libzmq3-dev
	sudo apt-get -y install graphviz

.PHONY: mac_dependencies
mac_dependencies:
	brew install zmq graphviz

.PHONY: clippy
clippy:
	cargo clippy --tests # -- -D warnings # for now, don't fail on warnings

.PHONY: build
build:
	./scripts/build.sh -bDebug -t   # Debug build, build tests, default number of build threads
	cargo build

.PHONY: test
test: test-simple
	cargo test
	rm -f log.txt log_0.txt pids client_log.txt

# This target replaces the ./tests/simple/test-simple.sh script with Makefile steps
# "Usage: $0 <build>"
.PHONY: test-simple
test-simple:
	./tests/simple/test-simple.sh y

.PHONY: docs
docs:
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