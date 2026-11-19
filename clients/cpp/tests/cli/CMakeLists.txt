cmake_minimum_required(VERSION 3.14 FATAL_ERROR)

# gtest/gmock are provided by the parent tests/CMakeLists.txt (anna-gtest,
# anna-gtest-main, anna-gmock wrapper targets).

add_executable(cli_tests test_cli_invocation.cpp)

target_compile_definitions(cli_tests PRIVATE
  ANNA_CLI_PATH="${CMAKE_BINARY_DIR}/cli/anna-cli"
)

target_link_libraries(cli_tests
  anna-gtest
)

add_test(NAME CliInvocationTests COMMAND cli_tests)
add_test(NAME CliSmokeTest COMMAND python3
  ${CMAKE_SOURCE_DIR}/../../tests/shared/cli/run_smoke_test.py
  ${CMAKE_BINARY_DIR}/cli/anna-cli --routing 127.0.0.1 --client-ip 127.0.0.1 cli
)
