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
#include "monitor/policies.hpp"
#include "mock/mock_zmq_utils.hpp"

class StoragePolicyTest : public ::testing::Test {
protected:
  void SetUp() override {
    mock_zmq_util.sent_messages.clear();
    kEnableElasticity = true;
    kEnableTiering = true;
  }
  void TearDown() override {
    mock_zmq_util.sent_messages.clear();
    kEnableElasticity = false;
    kEnableTiering = false;
  }
};

// When disk-tier required nodes exceeds current count, add_node is triggered.
TEST_F(StoragePolicyTest, DiskTierScaleOutTriggered) {
  GlobalRingMap global_hash_rings;
  TimePoint grace_start = std::chrono::system_clock::now() -
                           std::chrono::seconds(kGracePeriod + 1);
  SummaryStats ss;
  ss.required_disk_node = 3;
  unsigned memory_node_count = 1;
  unsigned disk_node_count = 1;
  unsigned new_memory_count = 0;
  unsigned new_disk_count = 0;
  bool removing_disk_node = false;
  Address management_ip = "127.0.0.1";
  MonitoringThread mt("127.0.0.1");
  map<Address, unsigned> departing_node_map;
  zmq::context_t context;
  SocketCache pushers(&context, ZMQ_PUSH);

  storage_policy(log_, global_hash_rings, grace_start, ss,
                 memory_node_count, disk_node_count, new_memory_count,
                 new_disk_count, removing_disk_node, management_ip, mt,
                 departing_node_map, pushers);

  // new_disk_count should be set by add_node.
  EXPECT_GT(new_disk_count, 0u);
}

// When disk consumption is below threshold, removal is triggered.
TEST_F(StoragePolicyTest, DiskTierScaleInTriggered) {
  GlobalRingMap global_hash_rings;
  // Insert a disk-tier node into the hash ring.
  global_hash_rings[Tier::DISK].insert("127.0.0.1", "127.0.0.1", 0, 0);

  TimePoint grace_start = std::chrono::system_clock::now() -
                           std::chrono::seconds(kGracePeriod + 1);
  SummaryStats ss;
  ss.avg_disk_consumption_percentage = 0.1; // Below kMinDiskNodeConsumption
  ss.required_disk_node = 0;
  unsigned memory_node_count = 1;
  unsigned disk_node_count = 1;
  unsigned new_memory_count = 0;
  unsigned new_disk_count = 0;
  bool removing_disk_node = false;
  Address management_ip = "127.0.0.1";
  MonitoringThread mt("127.0.0.1");
  map<Address, unsigned> departing_node_map;
  zmq::context_t context;
  SocketCache pushers(&context, ZMQ_PUSH);

  storage_policy(log_, global_hash_rings, grace_start, ss,
                 memory_node_count, disk_node_count, new_memory_count,
                 new_disk_count, removing_disk_node, management_ip, mt,
                 departing_node_map, pushers);

  EXPECT_TRUE(removing_disk_node);
}
