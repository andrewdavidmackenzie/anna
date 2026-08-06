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

find_package(GTest QUIET)
if(NOT GTest_FOUND OR NOT TARGET GTest::gmock)
  include(FetchContent)
  FetchContent_Declare(googletest
    URL https://github.com/google/googletest/archive/refs/tags/v1.14.0.tar.gz
    URL_HASH SHA256=8ad598c73ad796e0d8280b082cebd82a630d73e73cd3c70057938a6501bba5d7
  )
  FetchContent_MakeAvailable(googletest)
endif()

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

# Hash ring FFI test (validates Rust C library from C++).
IF(EXISTS ${ANNA_HASHRING_LIB})
  ADD_EXECUTABLE(hashring-ffi-test test_hashring_ffi.cpp)
  TARGET_LINK_LIBRARIES(hashring-ffi-test GTest::gtest_main ${ANNA_HASHRING_LIB})
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