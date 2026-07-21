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

SET(LIBRARY_DEPENDENCIES
    protobuf::libprotobuf
    kvs-proto
    anna-zmq
)

# The annalib client library: KVS operations (get/put/...) and process
# management (start/stop/status), wrapping KvsClientInterface with no
# dependency on stdin/stdout/argv. See issue #75.
ADD_LIBRARY(anna-client-lib STATIC client_lib.cpp latency_reporter.cpp)
TARGET_LINK_LIBRARIES(anna-client-lib spdlog::spdlog fmt::fmt ${ZeroMQ_LIBRARY} ${LIBRARY_DEPENDENCIES})
TARGET_INCLUDE_DIRECTORIES(anna-client-lib PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

# anna-cli is an example CLI built on top of anna-client-lib.
ADD_EXECUTABLE(anna-cli cli.cpp)
TARGET_LINK_LIBRARIES(anna-cli anna-client-lib spdlog::spdlog fmt::fmt ${ZeroMQ_LIBRARY} ${LIBRARY_DEPENDENCIES})

SET(EXECUTABLE_OUTPUT_PATH ${CMAKE_BINARY_DIR}/cli)
