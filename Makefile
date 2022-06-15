APTGET := $(shell command -v apt-get 2> /dev/null)
BREW := $(shell command -v brew 2> /dev/null)
DNF := $(shell command -v dnf 2> /dev/null)
YUM := $(shell command -v yum 2> /dev/null)
CMAKE := $(shell command -v cmake 2> /dev/null)
WGET := $(shell command -v wget 2> /dev/null)
LCOV := $(shell command -v lcov 2> /dev/null)
CLANG := $(shell command -v clang++ 2> /dev/null)
PROTOBUF := $(shell command -v protoc 2> /dev/null)
mdbook := $(shell command -v mdbook 2> /dev/null)

all: clippy build test docs

.PHONY: dependencies
dependencies: build-tools lcov protobuf
ifneq ($(BREW),)
	@echo "Installing Mac OS X specific dependencies using $(BREW)"
	brew install zmq graphviz
endif
ifneq ($(APTGET),)
	@echo "Installing Linux specific dependencies using $(APTGET)"
	sudo apt-get -y install libzmq3-dev graphviz
endif
ifneq ($(YUM),)
	@echo "Installing Linux specific dependencies using $(YUM)"
	sudo yum install -y zeromq zeromq-devel graphviz
endif

.PHONY: build-tools
build-tools: clang cmake
ifneq ($(BREW),)
	@echo "Installing Mac OS X specific dependencies using $(BREW)"
	brew install autoconf automake libtool unzip pkg-config
endif
ifneq ($(APTGET),)
	@echo "Installing Linux specific dependencies using $(APTGET)"
	sudo apt-get -y install build-essential autoconf automake libtool unzip pkg-config libc++-dev libc++abi-dev
endif
ifneq ($(YUM),)
	sudo yum install -y build-essential autoconf automake libtool
endif

.PHONY: protobuf
protobuf: wget build-tools
ifeq ($(PROTOBUF),)
	@echo "You might be prompted for your password to install the protobuf headers and set ldconfig."
	wget https://github.com/google/protobuf/releases/download/v3.9.1/protobuf-all-3.9.1.zip > /dev/null
	unzip protobuf-all-3.9.1 > /dev/null
	cd protobuf-3.9.1 && ./autogen.sh && ./configure CXX=clang++ CXXFLAGS='-std=c++11 -stdlib=libc++ -O3 -g' && make -j4 && sudo make install
ifneq ($(YUM),)
	sudo ldconfig
endif
ifneq ($(YUM),)
	# this is probably useless inside a Makefile
	export LD_LIBRARY_PATH=/usr/local/lib
	echo "export LD_LIBRARY_PATH=/usr/local/lib" >> ~/.bashrc
	source ~/.bashrc
endif
	rm -rf protobuf-*
endif

.PHONY: clang
clang:
ifeq ($(CLANG),)
ifneq ($(BREW),)
	# Leave mac Xcode clang install to the user
endif
ifneq ($(APTGET),)
	echo "Installing clang..."
	sudo apt-add-repository "deb http://apt.llvm.org/trusty/ llvm-toolchain-trusty-5.0 main"
	sudo apt-get install -y --force-yes --allow-unauthenticated clang clang++ lldb clang-format
endif
endif

.PHONY: cmake
cmake:
ifeq ($(CMAKE),)
ifneq ($(BREW),)
	brew install cmake
else
	sudo apt-get install -y cmake
endif
endif

.PHONY: wget
wget:
ifeq ($(WGET),)
ifneq ($(BREW),)
	brew install wget
else
	sudo apt-get install -y wget
endif
endif

.PHONY: lcov
lcov: wget cmake
ifeq ($(LCOV),)
	@echo "You might be asked for your password to install lcov..."
	wget http://downloads.sourceforge.net/ltp/lcov-1.13.tar.gz
	tar xvzf lcov-1.13.tar.gz > /dev/null 2>&1
	rm -rf lcov-1.13.tar.gz
	cd lcov-1.13 && sudo make install
	which lcov
	lcov -v
	rm -rf lcov-1.13
endif

.PHONY: clippy
clippy:
	cargo clippy --tests # -- -D warnings # for now, don't fail on warnings

.PHONY: build
build: build-tools
	./scripts/build.sh -bDebug   # Debug build
	# ./scripts/build.sh -bRelease   # Release build
	cargo build

.PHONY: test
test: test-simple
	cargo test
	rm -f log.txt log_0.txt pids client_log.txt

# This target replaces the ./tests/simple/test-simple.sh script with Makefile steps
# "Usage: $0 <build>"
.PHONY: test-simple
test-simple: build
	./tests/simple/test-simple.sh y

.PHONY: mdbook
mdbook:
ifeq ($(MDBOOK),)
	cargo install mdbook
	cargo install mdbook-linkcheck
endif

.PHONY: docs
docs: mdbook
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