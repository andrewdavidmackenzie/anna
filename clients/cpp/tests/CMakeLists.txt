CMAKE_MINIMUM_REQUIRED(VERSION 3.14 FATAL_ERROR)

find_package(GTest QUIET)
if(NOT GTest_FOUND OR NOT TARGET GTest::gmock)
  include(FetchContent)
  FetchContent_Declare(googletest
    URL https://github.com/google/googletest/archive/refs/tags/v1.14.0.tar.gz
  )
  FetchContent_MakeAvailable(googletest)
endif()

IF(${CMAKE_BUILD_TYPE} STREQUAL "Debug")
  INCLUDE(CodeCoverage.cmake)
  SETUP_TARGET_FOR_COVERAGE(client-test-coverage ctest client)
ENDIF()

SET(EXECUTABLE_OUTPUT_PATH ${CMAKE_BINARY_DIR}/tests)

INCLUDE_DIRECTORIES(.)

ADD_SUBDIRECTORY(simple)
ADD_SUBDIRECTORY(benchmark)
ADD_SUBDIRECTORY(zmq)
ADD_SUBDIRECTORY(hash_ring)

ADD_CUSTOM_TARGET(ctest ${CMAKE_CTEST_COMMAND})