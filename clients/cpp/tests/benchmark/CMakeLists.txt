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

FIND_PACKAGE(yaml-cpp REQUIRED)

# The benchmark is a server-side tool that uses lattice serialization
# and hash ring code. It needs the server include path.
INCLUDE_DIRECTORIES(${CMAKE_SOURCE_DIR}/../../server/cpp/src)

SET(BENCHMARK_DEPENDENCIES
    protobuf::libprotobuf
    kvs-proto
    yaml-cpp
    anna-zmq
    anna-hash-ring
)

SET(EXECUTABLE_OUTPUT_PATH ${CMAKE_BINARY_DIR}/target/benchmark)

ADD_EXECUTABLE(anna-bench benchmark.cpp)
TARGET_LINK_LIBRARIES(anna-bench spdlog::spdlog fmt::fmt ${ZeroMQ_LIBRARY} ${KV_LIBRARY_DEPENDENCIES} ${BENCHMARK_DEPENDENCIES})

ADD_EXECUTABLE(anna-bench-trigger trigger.cpp)
TARGET_LINK_LIBRARIES(anna-bench-trigger spdlog::spdlog fmt::fmt ${ZeroMQ_LIBRARY} ${KV_LIBRARY_DEPENDENCIES} ${BENCHMARK_DEPENDENCIES})