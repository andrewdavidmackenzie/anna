APTGET := $(shell command -v apt-get 2> /dev/null)
BREW := $(shell command -v brew 2> /dev/null)
DNF := $(shell command -v dnf 2> /dev/null)
YUM := $(shell command -v yum 2> /dev/null)
CLANG := $(shell command -v clang 2> /dev/null)
MDBOOK := $(shell command -v mdbook 2> /dev/null)
GRCOV := $(shell command -v grcov 2> /dev/null)

all: clean clippy build test docs cleanup

# Dependencies not installed
# clang on mac
# make

.PHONY: dependencies
dependencies: clang
	@echo "Installing build-tools"
ifneq ($(BREW),)
	brew install autoconf automake libtool unzip pkg-config cmake protobuf curl lcov zmq graphviz llvm
endif
ifneq ($(APTGET),)
	sudo apt-get -y install build-essential autoconf automake libtool curl unzip pkg-config cmake libc++-dev libc++abi-dev protobuf-compiler lcov llvm libzmq3-dev graphviz
endif
ifneq ($(YUM),)
	sudo yum install -y build-essential autoconf automake libtool curl cmake protobuf-compiler lcov llvm zeromq zeromq-devel graphviz
endif
	cargo install mdbook
	cargo install mdbook-linkcheck
	cargo install grcov
	rustup component add llvm-tools-preview
	# Skipping installing Python pre-requisites for now
	# sudo apt-get install -y python3-pip
	# brew install python
	# sudo pip3 install pycodestyle coverage codecov
	# awscli jq

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

.PHONY: clean
clean:
	rm -rf build
	rm -f *.profraw
	rm -f cli/*.profraw

.PHONY: clippy
clippy:
	cargo clippy --tests # -- -D warnings # for now, don't fail on warnings

.PHONY: build
build:  # Debug build, use "Release" for a Release build
	mkdir build
	LD_LIBRARY_PATH="/usr/local/lib" cd build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Debug -DCMAKE_CXX_COMPILER="/usr/bin/clang++" -DBUILD_TEST=ON .. && make -j8
	cargo build

.PHONY: test
test:
	./tests/simple/test-simple.sh y
	cd build && make test
	cd build && make test-coverage && lcov --list coverage.info
	RUSTFLAGS="-C instrument-coverage" LLVM_PROFILE_FILE="anna-%p-%m.profraw" cargo test
	grcov . --binary-path target/debug/ -s . -t lcov --branch --ignore-not-existing --ignore "/*" -o lcov.info

.PHONY: docs
docs:
	cargo doc --no-deps --target-dir=target/html/code
	mdbook build

.PHONY: cleanup
cleanup: test-cleanup coverage-cleanup

.PHONY: test-cleanup
test-cleanup:
	rm -f log.txt log_0.txt pids client_log.txt

.PHONY: coverage-cleanup
coverage-cleanup:
	rm -f lcov.info build/coverage.info
	find . -name \*.profraw | xargs rm -f
