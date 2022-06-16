# Building Anna

## Prerequisites left to the user
There are a few pre-requisites that we don't install and that we leave to the user to install first:
* rust toolchain (cargo, rustc etc). We suggest using [rustup](https://rustup.rs/)
* clang `C` and `C++` compiler on macos. The most normal way of getting this 
  would be installing [XCode from the Mac App Store](https://apps.apple.
  com/us/app/xcode/id497799835?mt=12), and [Xcode command line tools]
  (https://www.freecodecamp.org/news/install-xcode-command-line-tools/) and 
  [accept the license](https://developer.apple.com/forums/thread/91443) from the command line.

## Prerequisites
In order to build Anna, there are a variety of additional build tool dependencies.
Most can be installed with standard package managers like `brew` on macOS 
and `apt-get` on Linux.
Some require download and build locally with the build-tools previously 
installed.

You can use the top-level Makefile to install them using `make dependencies` 
from the root of the project.

## Building with `make`
Once all pre-requisites are correctly working on the development machine, you 
can run the standard build using the top-level `Makefile` with just 
`make`

This will build, lint, run tests, generate docs etc.

KVS server executables will be in `build/target`, the CPP-based interactive CLI for Anna in 
`build/client` and the rust cli `anna` in `cli/target`.