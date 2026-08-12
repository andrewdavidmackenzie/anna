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
	sudo apt-get update
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
build: client-cpp server-cpp server-rust client-rust client-python client-go

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
server-cpp: server-rust
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

.PHONY: server-rust
server-rust:
	@echo "Building Rust server binaries"
	@$(CARGO_ENV) RUSTFLAGS="$(RUST_LINK_ALLOW)" cargo build --quiet -p anna-monitor -p anna-hashring

.PHONY: client-python
client-python:
	@echo "Compiling python client"
	@cd clients/python/anna && protoc -I=../../../server/protobuf/ --python_out=. kvs.proto shared.proto causal.proto benchmark.proto metadata.proto
	@cd clients/python/anna && sed -i.bak 's/^import shared_pb2/from . import shared_pb2/;s/^import kvs_pb2/from . import kvs_pb2/' causal_pb2.py kvs_pb2.py benchmark_pb2.py && rm -f causal_pb2.py.bak kvs_pb2.py.bak benchmark_pb2.py.bak

.PHONY: client-go
client-go:
	@echo "Building Go client library"
	@cd clients/go/annalib && go build ./...
	@echo "Building Go CLI"
	@mkdir -p target
	@cd clients/go/cmd/anna-go && go build -o ../../../../target/anna-go .

.PHONY: client-go-tests
client-go-tests:
	@echo "Running Go client unit tests with coverage"
	@cd clients/go/annalib && go test -v -coverprofile=coverage.out -coverpkg=github.com/andrewdavidmackenzie/anna/clients/go/annalib ./... 2>&1
	@echo "Running Go client system tests"
	@cd clients/go/tests && go test -run TestSystem -count=1 -timeout 60s

.PHONY: coverage
coverage: test
	@echo "Generating coverage report in ./coverage/index.html"
	@genhtml -o coverage --quiet rust_workspace.info server/cpp/build/server.info clients/cpp/build/client.info || true

.PHONY: test
test: client-cpp-tests client-python-tests workspace-rust-tests client-go-tests server-system-coverage server-cpp-tests merge-server-coverage rust-monitor-tests rust-kvs-tests rust-coverage-report docs

# Generate combined Rust coverage report from all accumulated profraw files
# (unit tests + Rust KVS subprocess + Rust monitor subprocess).
.PHONY: rust-coverage-report
rust-coverage-report:
	@echo "Generating combined Rust coverage report"
	@cargo llvm-cov report --lcov --output-path rust_workspace.info
	@lcov --remove rust_workspace.info '/Applications/*' '/usr*' '*/build/*' '**/build.rs' '*/cpp/hash_ring/*' '*/cpp/zmq/*' '**/errors.rs' '**/*.pb.*' '*tests/*' '*/protobuf/*' '*/incremental/*' -o rust_workspace.info --ignore-errors inconsistent,format,unused

.PHONY: rust-monitor-tests
rust-monitor-tests:
	@echo "Running monitor integration tests with Rust monitor (instrumented)"
	@ANNA_MONITOR_BIN=$(shell pwd)/target/llvm-cov-target/debug/anna-monitor-rs \
		CARGO_LLVM_COV=1 \
		$(CARGO_ENV) cargo test --target-dir target/llvm-cov-target --test monitor

# Dual-KVS testing: run all black-box / client tests against the Rust KVS.
# These are the same tests that workspace-rust-tests, client-cpp-tests, etc.
# run against the C++ KVS — duplicated here to ensure compatibility.
# Disk-tier tests are excluded because the Rust KVS only has memory serializers.
.PHONY: rust-kvs-tests
rust-kvs-tests:
	@echo "=== Rust client tests with Rust KVS (instrumented) ==="
	@ANNA_KVS_BIN=$(shell pwd)/target/llvm-cov-target/debug/anna-kvs-rs \
		CARGO_LLVM_COV=1 \
		$(CARGO_ENV) cargo test --target-dir target/llvm-cov-target --test lattice_types -- --skip disk_tier
	@pkill -9 -f anna-kvs 2>/dev/null; pkill -9 -f anna-monitor 2>/dev/null; pkill -9 -f anna-route 2>/dev/null; sleep 1; true
	@ANNA_KVS_BIN=$(shell pwd)/target/llvm-cov-target/debug/anna-kvs-rs \
		CARGO_LLVM_COV=1 \
		$(CARGO_ENV) cargo test --target-dir target/llvm-cov-target --test consistency -- --skip disk
	@pkill -9 -f anna-kvs 2>/dev/null; pkill -9 -f anna-monitor 2>/dev/null; pkill -9 -f anna-route 2>/dev/null; sleep 1; true
	@echo "=== C++ client system tests with Rust KVS ==="
	@cd clients/cpp/build && ANNA_KVS_BIN=$(shell pwd)/target/llvm-cov-target/debug/anna-kvs-rs ctest -R system_tests --output-on-failure
	@pkill -9 -f anna-kvs 2>/dev/null; pkill -9 -f anna-monitor 2>/dev/null; pkill -9 -f anna-route 2>/dev/null; sleep 1; true
	@echo "=== C++ CLI smoke test with Rust KVS ==="
	@cd clients/cpp/build && ANNA_KVS_BIN=$(shell pwd)/target/llvm-cov-target/debug/anna-kvs-rs ctest -R CliSmokeTest --output-on-failure
	@pkill -9 -f anna-kvs 2>/dev/null; pkill -9 -f anna-monitor 2>/dev/null; pkill -9 -f anna-route 2>/dev/null; sleep 1; true
	@echo "=== Python client system tests with Rust KVS ==="
	@cd clients/python && ANNA_KVS_BIN=$(shell pwd)/target/llvm-cov-target/debug/anna-kvs-rs python3 -m pytest tests/test_system.py -x
	@pkill -9 -f anna-kvs 2>/dev/null; pkill -9 -f anna-monitor 2>/dev/null; pkill -9 -f anna-route 2>/dev/null; sleep 1; true
	@echo "=== Go client system tests with Rust KVS ==="
	@cd clients/go/tests && ANNA_KVS_BIN=$(shell pwd)/target/llvm-cov-target/debug/anna-kvs-rs go test -run TestSystem -count=1 -timeout 60s

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
	@cp server/cpp/build/server.info server/cpp/build/server-unit.info 2>/dev/null || true

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
	@echo "Building instrumented Rust server binaries for subprocess coverage"
	@cargo llvm-cov run --no-report -p anna-kvs -- --help 2>/dev/null
	@cargo llvm-cov run --no-report -p anna-monitor -- --help 2>/dev/null
	@echo "Running workspace tests with C++ KVS (profraw accumulated)"
	@$(CARGO_ENV) cargo llvm-cov test --workspace --no-report -- --skip docker

MDBOOK := $(shell command -v mdbook 2> /dev/null)
LYCHEE := $(shell command -v lychee 2> /dev/null)

.PHONY: docs
docs:
ifeq ($(MDBOOK),)
	@echo "Skipping docs: mdbook not found (install with 'cargo binstall mdbook')"
else ifeq ($(LYCHEE),)
	@echo "Skipping docs: lychee not found (install with 'cargo binstall lychee')"
else
	@echo "Generating docs with cargo doc"
	@cargo doc --quiet --no-deps --target-dir=target/html/code 2>&1 > /dev/null
	@echo "Generating book with mdbook"
	@mdbook build > /dev/null 2>&1
	@rm -f target/html/*.profraw target/html/client_log.txt target/html/log.txt target/html/log_0.txt target/html/*.info
	@rm -f target/html/Makefile
	@rm -f target/html/Cargo.toml target/html/Cargo.lock target/html/CMakeLists.txt
	@rm -rf target/html/build target/html/conf
	@rm -rf target/html/src target/html/tests target/html/protobuf
	@rm -rf target/html/cli target/html/clients target/html/server
	@rm -rf target/html/coverage target/html/venv
	@echo "Checking links"
	@lychee --offline --root-dir target/html 'target/html/**/*.html' 2>&1
	@echo "Cleaned up extra files in docs folder"
endif

.PHONY: cleanup
cleanup: test-cleanup coverage-cleanup
	@echo "Cleaned up files left over from build and test"

.PHONY: test-cleanup
test-cleanup:
	@rm -f log.txt log_0.txt pids client_log.txt

.PHONY: coverage-cleanup
coverage-cleanup:
	@rm -f rust_workspace.info server/cpp/build/server.info clients/cpp/build/client.info

.PHONY: bench-build
bench-build:
	@echo "Building C++ server (Release)..."
	@mkdir -p server/cpp/release-build
ifeq ($(UNAME), Darwin)
	@cd server/cpp/release-build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_COMPILER="/usr/bin/clang++" -DBUILD_TEST=OFF .. 2>&1 > /dev/null && LD_LIBRARY_PATH="/usr/local/lib" make -s -j8 2>&1 > /dev/null
else
	@cd server/cpp/release-build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Release -DBUILD_TEST=OFF .. 2>&1 > /dev/null && make -s -j8 2>&1 > /dev/null
endif
	@echo "Building C++ client (Release)..."
	@mkdir -p clients/cpp/release-build
ifeq ($(UNAME), Darwin)
	@cd clients/cpp/release-build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_COMPILER="/usr/bin/clang++" -DBUILD_TEST=OFF .. 2>&1 > /dev/null && LD_LIBRARY_PATH="/usr/local/lib" make -s -j8 2>&1 > /dev/null
else
	@cd clients/cpp/release-build && cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Release -DBUILD_TEST=OFF .. 2>&1 > /dev/null && make -s -j8 2>&1 > /dev/null
endif
	@echo "Building Rust client (Release)..."
	@$(CARGO_ENV) RUSTFLAGS="$(RUST_LINK_ALLOW)" cargo build --release --quiet
	@echo "Building Go client..."
	@cd clients/go/annalib && go build ./...
	@mkdir -p target
	@cd clients/go/cmd/anna-go && go build -o ../../../../target/anna-go .
	@echo "All release builds complete"

.PHONY: bench
bench: bench-build
	@scripts/bench.sh

.PHONY: docker
docker:
	docker build -t anna .

.PHONY: docker-run
docker-run: docker
ifeq ($(UNAME), Linux)
	docker run --rm --network host anna
else
	@echo "Note: On macOS/Windows, use --network host on a Linux host or provide a custom config."
	@echo "Docker Desktop's VM networking prevents ZMQ from working with port mapping."
	@echo "Run: docker run --rm --network host anna  (Linux only)"
endif