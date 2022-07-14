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

CMAKE_MINIMUM_REQUIRED(VERSION 3.6 FATAL_ERROR)
PROJECT(Anna)

SET(ANNA_VERSION_MAJOR 0)
SET(ANNA_VERSION_MINOR 1)
SET(ANNA_VERSION_PATCH 0)

IF(NOT DEFINED BUILD_TEST)
  SET(BUILD_TEST OFF)
ENDIF()

IF(${BUILD_TEST})
  ENABLE_TESTING()
ENDIF()

SET(CMAKE_CXX_STANDARD 11)
SET(CMAKE_CXX_STANDARD_REQUIRED on)

IF(${CMAKE_CXX_COMPILER} STREQUAL "/usr/bin/clang++") 
  SET(CMAKE_CXX_FLAGS_COMMON
      "-std=c++11 \
      -stdlib=libc++ -pthread")
ENDIF()

IF(${CMAKE_CXX_COMPILER} STREQUAL "/usr/bin/g++") 
  SET(CMAKE_CXX_FLAGS_COMMON
      "-std=c++11 -pthread")
ENDIF()

SET(CMAKE_CXX_FLAGS_DEBUG
    "${CMAKE_CXX_FLAGS_DEBUG} \
    ${CMAKE_CXX_FLAGS_COMMON} \
    -g -O0 -fprofile-arcs -ftest-coverage")

SET(CMAKE_CXX_FLAGS_RELEASE
    "${CMAKE_CXX_FLAGS_RELEASE} \
    ${CMAKE_CXX_FLAGS_COMMON} \
    -O3")

INCLUDE_DIRECTORIES(${CMAKE_CURRENT_BINARY_DIR})

INCLUDE(FindProtobuf)
FIND_PACKAGE(Protobuf REQUIRED)
INCLUDE_DIRECTORIES(${PROTOBUF_INCLUDE_DIR})
PROTOBUF_GENERATE_CPP(PROTO_SRC PROTO_HEADER
  ./protobuf/kvs.proto
  ./protobuf/shared.proto
  ./protobuf/metadata.proto
)

PROTOBUF_GENERATE_CPP(BPROTO_SRC BPROTO_HEADER
  ./protobuf/benchmark.proto
)

# load in pkg-config support
find_package(PkgConfig)
# use pkg-config to get hints for 0mq locations
pkg_check_modules(PC_ZeroMQ QUIET zmq)

# use the hint from above to find where 'zmq.hpp' is located
find_path(ZeroMQ_INCLUDE_DIR
        NAMES zmq.hpp
        PATHS ${PC_ZeroMQ_INCLUDE_DIRS}
        )

# use the hint from above to find the location of libzmq
find_library(ZeroMQ_LIBRARY
        NAMES zmq
        PATHS ${PC_ZeroMQ_LIBRARY_DIRS}
        )

find_path(spdlog_INCLUDE_DIR
        NAMES spdlog.h
        PATHS ${PC_spdlog_INCLUDE_DIRS}
        )

find_library(spdlog_LIBRARY
        NAMES spdlog
        PATHS ${PC_spdlog_LIBRARY_DIRS}
        )

find_library(fmt_LIBRARY
        NAMES fmt
        PATHS ${PC_fmt_LIBRARY_DIRS}
        )

FIND_PACKAGE(cppzmq REQUIRED)
FIND_PACKAGE(spdlog REQUIRED)
FIND_PACKAGE(yaml-cpp REQUIRED)

ADD_LIBRARY(kvs-proto ${PROTO_HEADER} ${PROTO_SRC})
ADD_LIBRARY(anna-bench-proto ${BPROTO_HEADER} ${BPROTO_SRC})

INCLUDE_DIRECTORIES(src)
ADD_SUBDIRECTORY(src)

ADD_SUBDIRECTORY(clients/cpp)

IF(${BUILD_TEST})
  ADD_SUBDIRECTORY(tests)
ENDIF()
