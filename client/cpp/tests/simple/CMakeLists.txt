CMAKE_MINIMUM_REQUIRED(VERSION 3.6 FATAL_ERROR)

ADD_EXECUTABLE(run_simple_tests run_simple_tests.cpp)
TARGET_LINK_LIBRARIES(run_simple_tests ${spdlog_LIBRARY} ${fmt_LIBRARY} ${googletest_LIBRARY} ${gmock_LIBRARY})
ADD_TEST(NAME SimpleTest COMMAND run_simple_tests)
