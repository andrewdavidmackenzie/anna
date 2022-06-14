# Building Anna

## Prerequisites
In order to build Anna, there are a variety of C++ and other dependencies that are required. 
Most can be installed with standard package managers like `brew` on macOS and `apt` on Linux. 

You can use the top-level Makefile to install them using `make dependencies`

## Building with `make`
You can run the standard build using the tope-level `Makefile` with just `make`

This will build, lint, run tests, generate docs etc.

KVS server executables will be in `build/target`, the CPP-based interactive CLI for Anna in 
`build/client` and the rust cli `anna` in `cli/target`.