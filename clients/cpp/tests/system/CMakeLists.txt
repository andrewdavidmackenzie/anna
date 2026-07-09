# clients/cpp/tests/system/CMakeLists.txt
cmake_minimum_required(VERSION 3.14 FATAL_ERROR)

find_package(GTest QUIET)
if(NOT GTest_FOUND OR NOT TARGET GTest::gmock)
  include(FetchContent)
  FetchContent_Declare(googletest
    URL https://github.com/google/googletest/archive/refs/tags/v1.14.0.tar.gz
    URL_HASH SHA256=8ad598c73ad796e0d8280b082cebd82a630d73e73cd3c70057938a6501bba5d7
  )
  FetchContent_MakeAvailable(googletest)
endif()

add_executable(system_tests test_system.cpp)

# Link against the library under test
target_link_libraries(system_tests
  GTest::gtest
  GTest::gmock
  annalib
  protobuf::libprotobuf
  zmq
  spdlog::spdlog
  yaml-cpp
)

# Add to ctest
add_test(NAME system_tests COMMAND system_tests)
