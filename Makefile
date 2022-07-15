APTGET := $(shell command -v apt-get 2> /dev/null)
BREW := $(shell command -v brew 2> /dev/null)
DNF := $(shell command -v dnf 2> /dev/null)
YUM := $(shell command -v yum 2> /dev/null)
CLANG := $(shell command -v clang 2> /dev/null)
MDBOOK := $(shell command -v mdbook 2> /dev/null)
GRCOV := $(shell command -v grcov 2> /dev/null)

all: clippy build test coverage docs cleanup
	@echo "SUCCESS!!"

# Dependencies not installed
# clang on mac
# make
# rust toolchain
# python tooling

.PHONY: dependencies
dependencies: clang
	@echo "Installing build-tools"
ifneq ($(BREW),)
	brew install autoconf automake libtool pkg-config cmake protobuf curl lcov zmq cppzmq spdlog yaml-cpp googletest llvm
endif
ifneq ($(APTGET),)
	sudo apt-get -y install build-essential autoconf automake libtool curl unzip pkg-config cmake libc++-dev libc++abi-dev protobuf-compiler lcov llvm libzmq3-dev
endif
ifneq ($(YUM),)
	sudo yum install -y build-essential autoconf automake libtool curl cmake protobuf-compiler lcov llvm zeromq zeromq-devel
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

.PHONY: rustup
rustup:
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
	@. ${HOME}/.cargo/env

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
clean: cleanup
	@echo "Deleting all build artifacts"
	@rm -rf clients/cpp/build
	@rm -rf server/cpp/build
	@cargo --quiet clean
	@rm -f clients/python/anna/*_pb2.py
	@rm -rf coverage

.PHONY: clippy
clippy:
	@echo "Running 'clippy' on rust code"
	@cargo clippy --quiet --tests # -- -D warnings # for now, don't fail on warnings

# Debug build, use "-DCMAKE_BUILD_TYPE=Release" for a Release build
.PHONY: build
build: client-cpp server-cpp client-rust client-python

.PHONY: client-cpp
client-cpp:
	@mkdir -p clients/cpp/build
	@echo "Building client C++ project into ./clients/cpp/build"
	@LD_LIBRARY_PATH="/usr/local/lib" cd clients/cpp/build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Debug -DCMAKE_CXX_COMPILER="/usr/bin/clang++" -DBUILD_TEST=ON .. 2>&1 > /dev/null && make -s -j8 2>&1 > /dev/null

.PHONY: server-cpp
server-cpp:
	@mkdir -p server/cpp/build
	@echo "Building server C++ project into ./server/cpp/build"
	@LD_LIBRARY_PATH="/usr/local/lib" cd server/cpp/build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Debug -DCMAKE_CXX_COMPILER="/usr/bin/clang++" -DBUILD_TEST=ON .. 2>&1 > /dev/null && make -s -j8 2>&1 > /dev/null

.PHONY: client-rust
client-rust:
	@echo "Building rust code in workspace into ./target"
	@cargo build --quiet

.PHONY: client-python
client-python:
	@echo "Compiling python client"
	@cd clients/python/anna && protoc -I=../../../server/protobuf/ --python_out=. kvs.proto shared.proto causal.proto

.PHONY: test
test: server-cpp-tests client-cpp-tests workspace-rust-tests

.PHONY: coverage
coverage: test
	@echo "Generating coverage report in ./coverage/index.html"
	@genhtml -o coverage --quiet rust_workspace.info server/cpp/build/server.info #clients/cpp/build/client.info

.PHONY: server-cpp-tests
server-cpp-tests:
	@echo "Running C++ server tests with coverage"
	@cd server/cpp/build && make --no-print-directory -s server-test-coverage > /dev/null 2>&1

.PHONY: client-cpp-tests
client-cpp-tests:
	@echo "Running C++ client tests with coverage"
	@cd clients/cpp/build && make --no-print-directory -s client-test-coverage > /dev/null 2>&1

.PHONY: workspace-rust-tests
workspace-rust-tests:
	@echo "Running rust tests with coverage"
	@RUSTFLAGS="-C instrument-coverage" LLVM_PROFILE_FILE="anna-%p-%m.profraw" cargo --quiet test 2>&1 > /dev/null
	@echo "Gathering covering information"
	@grcov . --binary-path target/debug/ -s . -t lcov --branch --ignore-not-existing --ignore "/*" -o rust_workspace.info 2>&1 > /dev/null
	@lcov  --remove rust_workspace.info '/Applications/*' '/usr*' '*/build/*' '**/build.rs' '*/cpp/hash_ring/*' '*/cpp/zmq/*' '**/errors.rs' '**/*.pb.*' '*tests/*' '*/protobuf/*' -o rust_workspace.info 2>&1 > /dev/null

.PHONY: docs
docs:
	@echo "Generating docs with cargo doc"
	@cargo doc --quiet --no-deps --target-dir=target/html/code 2>&1 > /dev/null
	@echo "Generating book with mdbook"
	@mdbook build > /dev/null 2>&1
	@rm -f target/html/*.profraw target/html/client_log.txt target/html/log.txt target/html/log_0.txt target/html/*.info
	@rm -f target/html/Makefile
	@rm -f target/html/LICENSE target/html/Cargo.toml target/html/Cargo.lock target/html/CMakeLists.txt
	@rm -rf target/html/build target/html/conf target/html/common target/html/dockerfiles target/html/include
	@rm -rf target/html/src target/html/tests target/html/protobuf
	@rm -rf target/html/cli target/html/client
	@echo "Cleaned up extra files in docs folder"

.PHONY: cleanup
cleanup: test-cleanup coverage-cleanup
	@echo "Cleaned up files left over from build and test"

.PHONY: test-cleanup
test-cleanup:
	@rm -f log.txt log_0.txt pids client_log.txt

.PHONY: coverage-cleanup
coverage-cleanup:
	@rm -f rust_workspace.info build/server.info build/client.info
	@find . -name \*.profraw | xargs rm -f
