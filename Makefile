APTGET := $(shell command -v apt-get 2> /dev/null)
BREW := $(shell command -v brew 2> /dev/null)
DNF := $(shell command -v dnf 2> /dev/null)
YUM := $(shell command -v yum 2> /dev/null)
CLANG := $(shell command -v clang 2> /dev/null)
all: clean-start clippy build test coverage docs cleanup
	@echo "SUCCESS!!"

.PHONY: clean-start
clean-start:
	@rm -rf target/profraw # Rust coverage profraw files, now directed to a single directory
	@find . -maxdepth 1 -name "*.profraw" -exec rm -f {} + # stray profraw in workspace root

# Dependencies not installed
# clang on mac
# make
# rust toolchain
# python tooling

.PHONY: setup-multinode
setup-multinode:
	@echo "Setting up loopback aliases for multi-node testing"
ifeq ($(UNAME), Darwin)
	sudo ifconfig lo0 alias 127.0.0.2
else
	@echo "Linux: 127.0.0.0/8 loopback range available by default"
endif

.PHONY: dependencies
dependencies: clang
	@echo "Installing build-tools"
ifneq ($(BREW),)
	brew install autoconf automake libtool pkg-config cmake protobuf curl lcov zmq cppzmq spdlog yaml-cpp googletest llvm
endif
ifneq ($(APTGET),)
	sudo apt-get -y install build-essential autoconf automake libtool curl unzip pkg-config cmake libc++-dev libc++abi-dev protobuf-compiler libprotobuf-dev lcov llvm libzmq3-dev cppzmq-dev libspdlog-dev libfmt-dev libyaml-cpp-dev libgtest-dev
endif
ifneq ($(YUM),)
	sudo yum install -y build-essential autoconf automake libtool curl cmake protobuf-compiler lcov llvm zeromq zeromq-devel
endif
	cargo install cargo-llvm-cov
	rustup component add llvm-tools-preview
	go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
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
	@rm -rf build
	@cargo --quiet clean
	@rm -f clients/python/anna/*_pb2.py
	@rm -rf coverage
	@rm -rf target/clients target/server # stale duplicate C++ builds under target/

.PHONY: clippy
clippy:
	@echo "Running 'clippy' on rust code"
	@$(CARGO_ENV) cargo clippy --quiet --tests # -- -D warnings # for now, don't fail on warnings

.PHONY: fmt
fmt:
	@echo "Running 'cargo fmt' on rust code"
	@cargo fmt

# Debug build, use "-DCMAKE_BUILD_TYPE=Release" for a Release build
.PHONY: build
build: client-cpp server-cpp client-rust client-python client-go

UNAME := $(shell uname)

# zeromq-src vendor crate fails to build on Linux due to strlcpy conflict
# between glibc (extern) and the vendored ZMQ (static). -fpermissive
# downgrades the C++ error to a warning with GCC. --allow-multiple-definition
# accepts the resulting duplicate symbol at link time.
ifeq ($(UNAME), Linux)
CARGO_ENV := CXXFLAGS="-fpermissive"
RUST_LINK_ALLOW := -C link-args=-Wl,--allow-multiple-definition
else
CARGO_ENV :=
RUST_LINK_ALLOW :=
endif

.PHONY: client-cpp
client-cpp:
	@mkdir -p clients/cpp/build
	@echo "Building client C++ project into ./clients/cpp/build"
ifeq ($(UNAME), Darwin)
	@cd clients/cpp/build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Debug -DCMAKE_CXX_COMPILER="/usr/bin/clang++" -DBUILD_TEST=ON .. 2>&1 > /dev/null && LD_LIBRARY_PATH="/usr/local/lib" make -s -j8 2>&1 > /dev/null
else
	@cd clients/cpp/build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Debug -DBUILD_TEST=ON .. 2>&1 > /dev/null && make -s -j8 2>&1 > /dev/null
endif

.PHONY: server-cpp
server-cpp:
	@mkdir -p server/cpp/build
	@echo "Building server C++ project into ./server/cpp/build"
ifeq ($(UNAME), Darwin)
	@cd server/cpp/build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Debug -DCMAKE_CXX_COMPILER="/usr/bin/clang++" -DBUILD_TEST=ON .. 2>&1 > /dev/null && LD_LIBRARY_PATH="/usr/local/lib" make -s -j8 2>&1 > /dev/null
else
	@cd server/cpp/build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Debug -DBUILD_TEST=ON .. 2>&1 > /dev/null && make -s -j8 2>&1 > /dev/null
endif

.PHONY: client-rust
client-rust:
	@echo "Building rust code in workspace into ./target"
	@$(CARGO_ENV) RUSTFLAGS="$(RUST_LINK_ALLOW)" cargo build --quiet

.PHONY: client-python
client-python:
	@echo "Compiling python client"
	@cd clients/python/anna && protoc -I=../../../server/protobuf/ --python_out=. kvs.proto shared.proto causal.proto
	@cd clients/python/anna && sed -i.bak 's/^import shared_pb2/from . import shared_pb2/;s/^import kvs_pb2/from . import kvs_pb2/' causal_pb2.py kvs_pb2.py && rm -f causal_pb2.py.bak kvs_pb2.py.bak

.PHONY: client-go
client-go:
	@echo "Building Go client library"
	@cd clients/go/annalib && go build ./...
	@echo "Building Go CLI"
	@mkdir -p target
	@cd clients/go/cmd/anna-go && go build -o ../../../../target/anna-go .

.PHONY: client-go-tests
client-go-tests:
	@echo "Running Go client tests with coverage"
	@cd clients/go/annalib && go test -v -coverprofile=coverage.out -coverpkg=github.com/andrewdavidmackenzie/anna/clients/go/annalib ./... 2>&1

.PHONY: coverage
coverage: test
	@echo "Generating coverage report in ./coverage/index.html"
	@genhtml -o coverage --quiet rust_workspace.info server/cpp/build/server.info clients/cpp/build/client.info || true

.PHONY: test
test: client-cpp-tests client-python-tests workspace-rust-tests client-go-tests server-system-coverage server-cpp-tests merge-server-coverage

.PHONY: server-system-coverage
server-system-coverage:
	@echo "Verifying server coverage data was generated by system tests"
	@test -f server/cpp/build/src/kvs/CMakeFiles/anna-kvs.dir/server.cpp.gcda || \
		(echo "ERROR: server.cpp.gcda not found — anna-kvs did not exit cleanly during system tests" && exit 1)
	@test -f server/cpp/build/src/route/CMakeFiles/anna-route.dir/routing.cpp.gcda || \
		(echo "ERROR: routing.cpp.gcda not found — anna-route did not exit cleanly during system tests" && exit 1)
	@test -f server/cpp/build/src/monitor/CMakeFiles/anna-monitor.dir/monitoring.cpp.gcda || \
		(echo "ERROR: monitoring.cpp.gcda not found — anna-monitor did not exit cleanly during system tests" && exit 1)
	@echo "Capturing server coverage from client system tests"
	@cd server/cpp/build && lcov --directory . --capture --output-file server-system.info --ignore-errors inconsistent,format,unused 2>/dev/null || true
	@cd server/cpp/build && lcov --remove server-system.info '/Applications/*' '/usr*' '*/build/*' '*/gtest/*' '*/tests/*' -o server-system.info --ignore-errors inconsistent,format,unused 2>/dev/null || true

.PHONY: server-cpp-tests
server-cpp-tests:
	@echo "Running C++ server tests with coverage"
	@cd server/cpp/build && make --no-print-directory -s server-test-coverage
	@find server/cpp -name "*.profraw" | xargs rm -f

.PHONY: merge-server-coverage
merge-server-coverage:
	@echo "Merging server unit test and system test coverage"
	@cd server/cpp/build && lcov --add-tracefile server.info --add-tracefile server-system.info --output-file server.info --ignore-errors inconsistent,format,unused 2>/dev/null || true

.PHONY: client-cpp-tests
client-cpp-tests:
	@echo "Running C++ client tests with coverage"
	@cd clients/cpp/build && make --no-print-directory -s client-test-coverage
	# C++ client tests use gcov-style (.gcda) not profraw — no cleanup needed

.PHONY: client-python-dependencies
client-python-dependencies:
	@pip3 install --quiet --user protobuf pytest pytest-cov pyzmq pyyaml 2>/dev/null || pip3 install --quiet --break-system-packages protobuf pytest pytest-cov pyzmq pyyaml 2>/dev/null || true

.PHONY: client-python-tests
client-python-tests: client-python-dependencies
	@echo "Running Python client tests with coverage"
	@cd clients/python && python3 -m pytest tests/ --quiet --cov=anna --cov-report=xml:coverage.xml 2>&1

.PHONY: workspace-rust-tests
workspace-rust-tests:
	@echo "Running rust tests with coverage"
	@find clients/cpp/build -name "*.gcda" -delete 2>/dev/null || true
	@$(CARGO_ENV) cargo llvm-cov test --lcov --output-path rust_workspace.info
	@lcov --remove rust_workspace.info '/Applications/*' '/usr*' '*/build/*' '**/build.rs' '*/cpp/hash_ring/*' '*/cpp/zmq/*' '**/errors.rs' '**/*.pb.*' '*tests/*' '*/protobuf/*' '*/incremental/*' -o rust_workspace.info --ignore-errors inconsistent,format,unused

.PHONY: docs
docs:
	@echo "Generating docs with cargo doc"
	@cargo doc --quiet --no-deps --target-dir=target/html/code 2>&1 > /dev/null
	@echo "Generating book with mdbook"
	@mdbook build > /dev/null 2>&1
	@rm -f target/html/*.profraw target/html/client_log.txt target/html/log.txt target/html/log_0.txt target/html/*.info
	@rm -f target/html/Makefile
	@rm -f target/html/Cargo.toml target/html/Cargo.lock target/html/CMakeLists.txt
	@rm -rf target/html/build target/html/conf target/html/dockerfiles
	@rm -rf target/html/src target/html/tests target/html/protobuf
	@rm -rf target/html/cli target/html/clients target/html/server
	@rm -rf target/html/coverage target/html/venv
	@echo "Checking links"
	@lychee --offline --root-dir target/html 'target/html/**/*.html' 2>&1
	@echo "Cleaned up extra files in docs folder"

.PHONY: cleanup
cleanup: test-cleanup coverage-cleanup
	@echo "Cleaned up files left over from build and test"

.PHONY: test-cleanup
test-cleanup:
	@rm -f log.txt log_0.txt pids client_log.txt

.PHONY: coverage-cleanup
coverage-cleanup:
	@rm -f rust_workspace.info server/cpp/build/server.info clients/cpp/build/client.info