CMAKE_MINIMUM_REQUIRED(VERSION 3.6 FATAL_ERROR)
PROJECT(AnnaClient)

IF(NOT DEFINED BUILD_TEST)
  SET(BUILD_TEST OFF)
ENDIF()

SET(CMAKE_CXX_STANDARD 17)
SET(CMAKE_CXX_STANDARD_REQUIRED on)

IF(CMAKE_CXX_COMPILER_ID STREQUAL "Clang")
  SET(CMAKE_CXX_FLAGS_COMMON "-stdlib=libc++ -pthread")
ELSEIF(CMAKE_CXX_COMPILER_ID STREQUAL "GNU")
  SET(CMAKE_CXX_FLAGS_COMMON "-pthread")
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

FIND_PACKAGE(Protobuf CONFIG REQUIRED)
INCLUDE_DIRECTORIES(${protobuf_INCLUDE_DIR})

protobuf_generate(
  LANGUAGE cpp
  PROTOS ../../server/protobuf/kvs.proto ../../server/protobuf/shared.proto ../../server/protobuf/metadata.proto
  IMPORT_DIRS ../../server/protobuf
  APPEND_PATH
  OUT_VAR PROTO_SRC
)
ADD_LIBRARY(kvs-proto ${PROTO_SRC})
TARGET_LINK_LIBRARIES(kvs-proto protobuf::libprotobuf)

protobuf_generate(
  LANGUAGE cpp
  PROTOS ../../server/protobuf/benchmark.proto
  IMPORT_DIRS ../../server/protobuf
  APPEND_PATH
  OUT_VAR BPROTO_SRC
)
ADD_LIBRARY(anna-bench-proto ${BPROTO_SRC})
TARGET_LINK_LIBRARIES(anna-bench-proto protobuf::libprotobuf)

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

FIND_PACKAGE(fmt CONFIG QUIET)

FIND_PACKAGE(cppzmq REQUIRED)

FIND_PACKAGE(spdlog CONFIG REQUIRED)
FIND_PACKAGE(yaml-cpp REQUIRED)

INCLUDE_DIRECTORIES(../../server/cpp/src)

INCLUDE_DIRECTORIES(src)
ADD_SUBDIRECTORY(src)

IF(${BUILD_TEST})
  ENABLE_TESTING()
  ADD_SUBDIRECTORY(tests)
ENDIF()