#!/bin/bash

rm -rf build
mkdir build
cd build

cmake "-GUnix Makefiles" -DCMAKE_BUILD_TYPE=Debug -DCMAKE_CXX_COMPILER="/usr/bin/clang++" -DBUILD_TEST=ON ..
make -j8