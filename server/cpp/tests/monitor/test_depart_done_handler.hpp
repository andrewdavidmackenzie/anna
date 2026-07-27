//  Copyright 2019 U.C. Berkeley RISE Lab
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.

#include "gtest/gtest.h"
#include "monitor/monitoring_handlers.hpp"
#include "mock/mock_zmq_utils.hpp"

// Test that depart_done_handler handles disk-tier node departure.
TEST(DepartDoneHandler, DiskTierDepartClearsFlag) {
  map<Address, unsigned> departing_node_map;
  departing_node_map["127.0.0.2"] = 1;  // 1 thread remaining
  bool removing_memory_node = false;
  bool removing_disk_node = true;
  Address management_ip = "127.0.0.1";
  zmq::context_t context;
  SocketCache pushers(&context, ZMQ_PUSH);
  TimePoint grace_start = std::chrono::system_clock::now();

  // Format: public_ip_private_ip_tier_id  (tier_id 2 = DISK)
  string serialized = "127.0.0.2_127.0.0.2_2";

  depart_done_handler(log_, serialized, departing_node_map, management_ip,
                      removing_memory_node, removing_disk_node, pushers,
                      grace_start);

  EXPECT_FALSE(removing_disk_node);
  EXPECT_EQ(departing_node_map.count("127.0.0.2"), 0u);
}
