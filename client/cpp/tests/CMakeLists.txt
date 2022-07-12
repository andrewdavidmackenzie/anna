CMAKE_MINIMUM_REQUIRED(VERSION 3.6 FATAL_ERROR)

IF(${CMAKE_BUILD_TYPE} STREQUAL "Debug")
  INCLUDE(CodeCoverage.cmake)
  SETUP_TARGET_FOR_COVERAGE(client-test-coverage client-tests client)
ENDIF()

find_library(googletest_LIBRARY
        NAMES gtest
        PATHS ${PC_googletest_LIBRARY_DIRS}
        )

find_library(gmock_LIBRARY
        NAMES gmock
        PATHS ${PC_gmock_LIBRARY_DIRS}
        )

SET(EXECUTABLE_OUTPUT_PATH ${CMAKE_BINARY_DIR}/tests)

INCLUDE_DIRECTORIES(.)

ADD_CUSTOM_TARGET(client-tests ${CMAKE_CTEST_COMMAND})


#ADD_EXECUTABLE(run_client_tests test_all.cpp)

#TARGET_LINK_LIBRARIES(run_client_tests ${googletest_LIBRARY} ${gmock_LIBRARY}
#  anna-hash-ring ${spdlog_LIBRARY} ${fmt_LIBRARY} ${ZeroMQ_LIBRARY} anna-mock)

#ADD_TEST(NAME ClientTests COMMAND run_client_tests)