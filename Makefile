APTGET := $(shell command -v apt-get 2> /dev/null)
BREW := $(shell command -v brew 2> /dev/null)

all: clippy build test docs

.PHONY: dependencies
dependencies:
ifneq ($(BREW),)
	@echo "Installing Mac OS X specific dependencies using $(BREW)"
	brew install zmq graphviz autoconf automake libtool make unzip pkg-config wget cmake
endif
ifneq ($(APTGET),)
	@echo "Installing Linux specific dependencies using $(APTGET)"
	sudo apt-get -y install libzmq3-dev graphviz
endif
	@echo "You might be prompted for your password to install the protobuf headers and set ldconfig."
	wget https://github.com/google/protobuf/releases/download/v3.9.1/protobuf-all-3.9.1.zip > /dev/null
	unzip protobuf-all-3.9.1 > /dev/null
	cd protobuf-3.9.1 && ./autogen.sh && ./configure CXX=clang++ CXXFLAGS='-std=c++11 -stdlib=libc++ -O3 -g' && make -j4 && sudo make install
ifneq ($(BREW),)
	sudo update_dyld_shared_cache
endif
	rm -rf protobuf-*
	@echo "You might be asked for your password to install lcov..."
	wget http://downloads.sourceforge.net/ltp/lcov-1.13.tar.gz
	tar xvzf lcov-1.13.tar.gz > /dev/null 2>&1
	rm -rf lcov-1.13.tar.gz
	cd lcov-1.13 && sudo make install
	which lcov
	lcov -v
	rm -rf lcov-1.13

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