APTGET := $(shell command -v apt-get 2> /dev/null)
BREW := $(shell command -v brew 2> /dev/null)
DNF := $(shell command -v dnf 2> /dev/null)
YUM := $(shell command -v yum 2> /dev/null)
CLANG := $(shell command -v clang 2> /dev/null)
MDBOOK := $(shell command -v mdbook 2> /dev/null)
GRCOV := $(shell command -v grcov 2> /dev/null)

all: clippy build test docs

.PHONY: dependencies
dependencies: clang
	@echo "Installing build-tools"
ifneq ($(BREW),)
	brew install autoconf automake libtool unzip pkg-config cmake protobuf lcov zmq graphviz
endif
ifneq ($(APTGET),)
	sudo apt-get -y install build-essential autoconf automake libtool unzip pkg-config cmake libc++-dev libc++abi-dev protobuf-compiler lcov libzmq3-dev graphviz
endif
ifneq ($(YUM),)
	sudo yum install -y build-essential autoconf automake libtool cmake protobuf-compiler lcov zeromq zeromq-devel graphviz
endif
	cargo install mdbook
	cargo install mdbook-linkcheck
	cargo install grcov
	rustup component add llvm-tools-preview

.PHONY: clang
clang:
ifeq ($(CLANG),)
	@echo "Installing clang"
ifneq ($(BREW),)
	# Leave mac Xcode clang install to the user
endif
ifneq ($(APTGET),)
	echo "Installing clang..."
	#sudo apt-add-repository "deb http://apt.llvm.org/trusty/ llvm-toolchain-trusty-5.0 main"
	sudo apt-get install -y --allow-unauthenticated clang clang++ lldb clang-format
endif
endif

.PHONY: clippy
clippy:
	cargo clippy --tests # -- -D warnings # for now, don't fail on warnings

.PHONY: build
build:
	LD_LIBRARY_PATH=/usr/local/lib ./scripts/build.sh -bDebug   # Debug build, use "-bRelease" for a Release build
	cargo build

.PHONY: test
test: configure_coverage test-simple
	cargo test
	rm -f log.txt log_0.txt pids client_log.txt

# This target replaces the ./tests/simple/test-simple.sh script with Makefile steps
# "Usage: $0 <build>"
.PHONY: test-simple
test-simple: build
	./tests/simple/test-simple.sh y

.PHONY: docs
docs:
	cargo doc --no-deps --target-dir=target/html/code
	mdbook build

.PHONY: configure_coverage
configure_coverage:
	# This is probably useless in a Makefile
	export RUSTFLAGS="-C instrument-coverage"
	export LLVM_PROFILE_FILE="anna-%p-%m.profraw"

.PHONY: upload_coverage
upload_coverage:
	grcov . --binary-path target/debug/ -s . -t lcov --branch --ignore-not-existing --ignore "/*" -o lcov.info
	bash <(curl -s https://codecov.io/bash) -f lcov.info
	rm -f lcov.info