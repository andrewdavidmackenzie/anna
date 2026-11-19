# clients/cpp/tests/system/CMakeLists.txt
cmake_minimum_required(VERSION 3.14 FATAL_ERROR)

# gtest/gmock are provided by the parent tests/CMakeLists.txt (anna-gtest,
# anna-gtest-main, anna-gmock wrapper targets).

add_executable(system_tests test_system.cpp)

# Link against the library under test
target_link_libraries(system_tests
  anna-gtest
  anna-gmock
  anna-client-lib
)

# Add to ctest
add_test(NAME system_tests COMMAND python3 ${CMAKE_CURRENT_SOURCE_DIR}/test_system_runner.py)
