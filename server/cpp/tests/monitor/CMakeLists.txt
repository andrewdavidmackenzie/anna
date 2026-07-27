#  Copyright 2019 U.C. Berkeley RISE Lab
#
#  Licensed under the Apache License, Version 2.0 (the "License");
#  you may not use this file except in compliance with the License.
#  You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.

CMAKE_MINIMUM_REQUIRED(VERSION 3.6 FATAL_ERROR)

ADD_EXECUTABLE(run_config_defaults_tests
         run_config_defaults_tests.cpp)

TARGET_LINK_LIBRARIES(run_config_defaults_tests GTest::gtest
  anna-hash-ring spdlog::spdlog fmt::fmt ${ZeroMQ_LIBRARY} yaml-cpp)

ADD_TEST(NAME ConfigDefaultsTests COMMAND run_config_defaults_tests)

FILE(GLOB MONITOR_SOURCES "${CMAKE_SOURCE_DIR}/src/monitor/*_handler.cpp"
                          "${CMAKE_SOURCE_DIR}/src/monitor/*_policy.cpp"
                          "${CMAKE_SOURCE_DIR}/src/monitor/stats_helpers.cpp"
                          "${CMAKE_SOURCE_DIR}/src/monitor/elasticity.cpp"
                          "${CMAKE_SOURCE_DIR}/src/monitor/replication_helpers.cpp")

ADD_EXECUTABLE(run_monitor_handler_tests
         run_monitor_handler_tests.cpp
         ${MONITOR_SOURCES})

TARGET_LINK_LIBRARIES(run_monitor_handler_tests GTest::gtest GTest::gmock
  anna-hash-ring anna-mock anna-bench-proto spdlog::spdlog fmt::fmt ${ZeroMQ_LIBRARY})

ADD_TEST(NAME MonitorHandlerTests COMMAND run_monitor_handler_tests)
