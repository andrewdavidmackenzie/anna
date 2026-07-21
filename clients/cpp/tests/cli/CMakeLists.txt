cmake_minimum_required(VERSION 3.14 FATAL_ERROR)

find_package(GTest QUIET)
if(NOT GTest_FOUND OR NOT TARGET GTest::gtest)
  include(FetchContent)
  FetchContent_Declare(googletest
    URL https://github.com/google/googletest/archive/refs/tags/v1.14.0.tar.gz
    URL_HASH SHA256=8ad598c73ad796e0d8280b082cebd82a630d73e73cd3c70057938a6501bba5d7
  )
  FetchContent_MakeAvailable(googletest)
endif()

add_executable(cli_tests test_cli_invocation.cpp)

target_compile_definitions(cli_tests PRIVATE
  ANNA_CLI_PATH="${CMAKE_BINARY_DIR}/cli/anna-cli"
)

target_link_libraries(cli_tests
  GTest::gtest
)

add_test(NAME CliInvocationTests COMMAND cli_tests)
add_test(NAME CliSmokeTest COMMAND python3
  ${CMAKE_SOURCE_DIR}/../../tests/shared/cli/run_smoke_test.py
  ${CMAKE_BINARY_DIR}/cli/anna-cli --routing 127.0.0.1 --client-ip 127.0.0.1 cli
)
