CMAKE_MINIMUM_REQUIRED(VERSION 3.6 FATAL_ERROR)
PROJECT(AnnaClient)

IF(NOT DEFINED BUILD_TEST)
  SET(BUILD_TEST OFF)
ENDIF()

SET(CMAKE_CXX_STANDARD 11)
SET(CMAKE_CXX_STANDARD_REQUIRED on)

IF(${CMAKE_CXX_COMPILER} STREQUAL "/usr/bin/clang++")
  SET(CMAKE_CXX_FLAGS_COMMON "-std=c++11 -stdlib=libc++ -pthread")
ENDIF()

IF(${CMAKE_CXX_COMPILER} STREQUAL "/usr/bin/g++")
  SET(CMAKE_CXX_FLAGS_COMMON "-std=c++11 -pthread")
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
  ../../server/protobuf/kvs.proto
  ../../server/protobuf/shared.proto
  ../../server/protobuf/metadata.proto
)
PROTOBUF_GENERATE_CPP(BPROTO_SRC BPROTO_HEADER
  ../../server/protobuf/benchmark.proto
)

ADD_LIBRARY(kvs-proto ${PROTO_HEADER} ${PROTO_SRC})
ADD_LIBRARY(anna-bench-proto ${BPROTO_HEADER} ${BPROTO_SRC})

find_package(PkgConfig)
pkg_check_modules(PC_ZeroMQ QUIET zmq)

find_path(ZeroMQ_INCLUDE_DIR
        NAMES zmq.hpp
        PATHS ${PC_ZeroMQ_INCLUDE_DIRS}
        )

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

INCLUDE_DIRECTORIES(../../server/cpp/src)

INCLUDE_DIRECTORIES(src)
ADD_SUBDIRECTORY(src)

IF(${BUILD_TEST})
  ENABLE_TESTING()
  ADD_SUBDIRECTORY(tests)
ENDIF()