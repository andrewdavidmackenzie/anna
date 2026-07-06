CMAKE_MINIMUM_REQUIRED(VERSION 3.6 FATAL_ERROR)

ADD_EXECUTABLE(run_simple_tests run_simple_tests.cpp)
TARGET_LINK_LIBRARIES(run_simple_tests spdlog::spdlog fmt::fmt GTest::gtest GTest::gmock)
ADD_TEST(NAME SimpleTest COMMAND run_simple_tests)
