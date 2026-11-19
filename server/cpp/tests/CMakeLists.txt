#  Copyright 2019 U.C. Berkeley RISE Lab
# 
#  Licensed under the Apache License, Version 2.0 (the "License");
#  you may not use this file except in compliance with the License.
#  You may obtain a copy of the License at
# 
#      http://www.apache.org/licenses/LICENSE-2.0
# 
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.

CMAKE_MINIMUM_REQUIRED(VERSION 3.14 FATAL_ERROR)

# Prefer the system GTest when it provides everything the tests need (gtest,
# gtest_main and gmock). Some distros ship an incomplete CMake package — e.g.
# Ubuntu 26.04's libgtest-dev provides GTestConfig.cmake with GTest::gtest and
# GTest::gtest_main but no gmock. Mixing that partial system package with a
# vendored gmock puts two different libgtest.a copies on the link line
# (duplicate-symbol errors), so in that case fall back to the vendored
# googletest for all of gtest/gmock.
find_package(GTest QUIET)
if(GTest_FOUND AND TARGET GTest::gmock)
  set(ANNA_GTEST GTest::gtest)
  set(ANNA_GTEST_MAIN GTest::gtest_main)
  set(ANNA_GMOCK GTest::gmock)
else()
  include(FetchContent)
  FetchContent_Declare(googletest
    URL https://github.com/google/googletest/archive/refs/tags/v1.14.0.tar.gz
    URL_HASH SHA256=8ad598c73ad796e0d8280b082cebd82a630d73e73cd3c70057938a6501bba5d7
  )
  FetchContent_MakeAvailable(googletest)
  # Link the vendored libraries by their real target names: unlike the
  # GTest:: aliases, these cannot resolve to a partial system package.
  set(ANNA_GTEST gtest)
  set(ANNA_GTEST_MAIN gtest_main)
  set(ANNA_GMOCK gmock)
endif()

# Stable link targets for the tests below, independent of the gtest source.
add_library(anna-gtest INTERFACE)
target_link_libraries(anna-gtest INTERFACE ${ANNA_GTEST})
add_library(anna-gtest-main INTERFACE)
target_link_libraries(anna-gtest-main INTERFACE ${ANNA_GTEST_MAIN})
add_library(anna-gmock INTERFACE)
target_link_libraries(anna-gmock INTERFACE ${ANNA_GMOCK})
# NOTE: test targets must link anna-gtest*/anna-gmock BEFORE other libraries
# (spdlog, fmt, protobuf, ...). In the vendored path their include
# directories have to come first so that gtest headers reachable through
# other dependencies' include directories (e.g. /usr/include, where a
# different system gtest version may be installed) cannot shadow them.

IF(${CMAKE_BUILD_TYPE} STREQUAL "Debug")
  INCLUDE(CodeCoverage.cmake)
  SETUP_TARGET_FOR_COVERAGE(server-test-coverage ctest server)
ENDIF()

SET(EXECUTABLE_OUTPUT_PATH ${CMAKE_BINARY_DIR}/tests)

INCLUDE_DIRECTORIES(.)

ADD_SUBDIRECTORY(kvs)
ADD_SUBDIRECTORY(lattices)
ADD_SUBDIRECTORY(mock)
ADD_SUBDIRECTORY(monitor)
ADD_SUBDIRECTORY(route)

# Hash ring FFI test (validates Rust C library from C++).
IF(EXISTS ${ANNA_HASHRING_LIB})
  ADD_EXECUTABLE(hashring-ffi-test test_hashring_ffi.cpp)
  TARGET_LINK_LIBRARIES(hashring-ffi-test anna-gtest-main ${ANNA_HASHRING_LIB})
  # Rust static library needs system libraries for its runtime.
  IF(APPLE)
    TARGET_LINK_LIBRARIES(hashring-ffi-test
      "-framework Security" "-framework CoreFoundation" "-framework SystemConfiguration"
      "-lSystem" "-liconv" "-lm" "-lc++")
  ELSE()
    TARGET_LINK_LIBRARIES(hashring-ffi-test dl pthread m)
  ENDIF()
  ADD_TEST(NAME HashRingFFI COMMAND hashring-ffi-test)
ENDIF()

ADD_CUSTOM_TARGET(ctest ${CMAKE_CTEST_COMMAND})