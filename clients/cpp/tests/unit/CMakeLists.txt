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

ADD_EXECUTABLE(run_unit_tests test_serialization.cpp test_client_lib.cpp test_value_change_subscriber.cpp test_latency_reporter.cpp)
TARGET_LINK_LIBRARIES(run_unit_tests anna-client-lib spdlog::spdlog fmt::fmt GTest::gtest GTest::gtest_main GTest::gmock)
ADD_TEST(NAME UnitTests COMMAND run_unit_tests)
