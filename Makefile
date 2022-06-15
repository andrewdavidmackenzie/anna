APTGET := $(shell command -v apt-get 2> /dev/null)
BREW := $(shell command -v brew 2> /dev/null)
DNF := $(shell command -v dnf 2> /dev/null)
YUM := $(shell command -v yum 2> /dev/null)
CMAKE := $(shell command -v cmake 2> /dev/null)
WGET := $(shell command -v wget 2> /dev/null)
LCOV := $(shell command -v lcov 2> /dev/null)
CLANG := $(shell command -v clang++ 2> /dev/null)

all: clippy build test docs

.PHONY: dependencies
dependencies: cmake clang lcov
ifneq ($(BREW),)
	@echo "Installing Mac OS X specific dependencies using $(BREW)"
	brew install zmq graphviz autoconf automake libtool unzip pkg-config
endif
ifneq ($(APTGET),)
	@echo "Installing Linux specific dependencies using $(APTGET)"
	sudo apt-get -y install libzmq3-dev graphviz build-essential autoconf automake libtool unzip pkg-config libc++-dev libc++abi-dev
endif
ifneq ($(YUM),)
	sudo yum install -y zeromq zeromq-devel graphviz build-essential autoconf automake libtool
endif
	@echo "You might be prompted for your password to install the protobuf headers and set ldconfig."
	wget https://github.com/google/protobuf/releases/download/v3.9.1/protobuf-all-3.9.1.zip > /dev/null
	unzip protobuf-all-3.9.1 > /dev/null
	cd protobuf-3.9.1 && ./autogen.sh && ./configure CXX=clang++ CXXFLAGS='-std=c++11 -stdlib=libc++ -O3 -g' && make -j4 && sudo make install
ifneq ($(YUM),)
	sudo ldconfig
endif
ifneq ($(YUM),)
	export LD_LIBRARY_PATH=/usr/local/lib
	echo "export LD_LIBRARY_PATH=/usr/local/lib" >> ~/.bashrc
	source ~/.bashrc
endif
	rm -rf protobuf-*

.PHONY: clang
clang:
ifeq ($(CLANG),)
ifneq ($(APTGET),)
	echo "Installing clang..."
	sudo apt-add-repository "deb http://apt.llvm.org/trusty/ llvm-toolchain-trusty-5.0 main"
	sudo apt-get install -y --force-yes clang-5.0 lldb-5.0 clang-format-5.0
	sudo update-alternatives --install /usr/bin/clang clang /usr/bin/clang-5.0 1
	sudo update-alternatives --install /usr/bin/clang++ clang++ /usr/bin/clang++-5.0 1
	sudo update-alternatives --install /usr/bin/clang-format clang-format /usr/bin/clang-format-5.0 1
endif
endif

.PHONY: cmake
cmake: wget
ifeq ($(CMAKE),)
ifneq ($(BREW),)
	brew install cmake
	echo "Installing cmake..."
	echo "You might be prompted for your password to add CMake to /usr/bin."
	wget https://cmake.org/files/v3.11/cmake-3.11.4-Linux-x86_64.tar.gz
	tar xvzf cmake-3.11.4-Linux-x86_64.tar.gz > /dev/null 2>&1
	sudo mkdir /usr/cmake
	sudo mv cmake-3.11.4-Linux-x86_64/* /usr/cmake/
	sudo ln -s /usr/cmake/bin/cmake /usr/bin/cmake
	rm -rf cmake-3.11.4-Linux-x86_64*
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