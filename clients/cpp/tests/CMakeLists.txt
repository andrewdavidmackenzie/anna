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
  SETUP_TARGET_FOR_COVERAGE(client-test-coverage ctest client)
ENDIF()

SET(EXECUTABLE_OUTPUT_PATH ${CMAKE_BINARY_DIR}/tests)

INCLUDE_DIRECTORIES(.)

ADD_SUBDIRECTORY(unit)
ADD_SUBDIRECTORY(zmq)
ADD_SUBDIRECTORY(system)
ADD_SUBDIRECTORY(cli)

# Benchmarking is now a CLI command (anna-cli bench). See issue #418.

ADD_CUSTOM_TARGET(ctest ${CMAKE_CTEST_COMMAND})