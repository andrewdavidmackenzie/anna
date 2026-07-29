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

class MovementPolicyTest : public ::testing::Test {
protected:
  void SetUp() override {
    mock_zmq_util.sent_messages.clear();
    kEnableElasticity = true;
    kEnableTiering = true;
    kEnableSelectiveRep = false;
  }
  void TearDown() override {
    mock_zmq_util.sent_messages.clear();
    kEnableElasticity = false;
    kEnableTiering = false;
    kEnableSelectiveRep = false;
  }
};

// When promoting keys overflows memory capacity, scale-up alert for memory
// tier is emitted.
TEST_F(MovementPolicyTest, PromotionOverflowTriggersMemoryScaleUp) {
  GlobalRingMap global_hash_rings;
  LocalRingMap local_hash_rings;
  TimePoint grace_start = std::chrono::system_clock::now() -
                           std::chrono::seconds(kGracePeriod + 1);
  SummaryStats ss;
  // Set memory consumption near capacity to force overflow during promotion
  ss.total_memory_consumption =
      kMaxMemoryNodeConsumption * kTierMetadata[Tier::MEMORY].node_capacity_;

  unsigned memory_node_count = 1;
  unsigned disk_node_count = 1;
  unsigned new_memory_count = 0;
  unsigned new_disk_count = 0;
  Address scaling_alert_ip = "127.0.0.1";
  map<Key, KeyReplication> key_replication_map;
  map<Key, unsigned> key_access_summary;
  map<Key, unsigned> key_size;

  // Create a key that is on disk only (memory rep = 0) and has high access
  // count to trigger promotion, but the promotion will overflow memory.
  Key key = "hot_key";
  key_replication_map[key] = create_new_replication_vector(0, 1, 1, 1);
  key_access_summary[key] = kKeyPromotionThreshold + 1;
  key_size[key] = 1000;  // large enough to overflow

  MonitoringThread mt("127.0.0.1");
  zmq::context_t context;
  SocketCache pushers(&context, ZMQ_PUSH);
  zmq::socket_t response_puller(context, ZMQ_PULL);
  vector<Address> routing_ips;
  unsigned rid = 0;

  movement_policy(log_, global_hash_rings, local_hash_rings, grace_start, ss,
                  memory_node_count, disk_node_count, new_memory_count,
                  new_disk_count, scaling_alert_ip, key_replication_map,
                  key_access_summary, key_size, mt, pushers, response_puller,
                  routing_ips, rid);

  // new_memory_count should be set by emit_scale_up_alert due to overflow.
  EXPECT_GT(new_memory_count, 0u);
}

// When demoting keys overflows disk capacity, scale-up alert for disk
// tier is emitted.
TEST_F(MovementPolicyTest, DemotionOverflowTriggersDiskScaleUp) {
  GlobalRingMap global_hash_rings;
  LocalRingMap local_hash_rings;
  TimePoint grace_start = std::chrono::system_clock::now() -
                           std::chrono::seconds(kGracePeriod + 1);
  SummaryStats ss;
  // Set disk consumption near capacity to force overflow during demotion
  ss.total_disk_consumption =
      kMaxDiskNodeConsumption * kTierMetadata[Tier::DISK].node_capacity_;

  unsigned memory_node_count = 1;
  unsigned disk_node_count = 1;
  unsigned new_memory_count = 0;
  unsigned new_disk_count = 0;
  Address scaling_alert_ip = "127.0.0.1";
  map<Key, KeyReplication> key_replication_map;
  map<Key, unsigned> key_access_summary;
  map<Key, unsigned> key_size;

  // Create a key that is in memory (memory rep > 0) and has zero access
  // count (below kKeyDemotionThreshold=1) to trigger demotion, but the
  // demotion will overflow disk.
  Key key = "cold_key";
  key_replication_map[key] = create_new_replication_vector(1, 0, 1, 1);
  key_access_summary[key] = 0;  // below demotion threshold
  key_size[key] = 1000;         // large enough to overflow

  MonitoringThread mt("127.0.0.1");
  zmq::context_t context;
  SocketCache pushers(&context, ZMQ_PUSH);
  zmq::socket_t response_puller(context, ZMQ_PULL);
  vector<Address> routing_ips;
  unsigned rid = 0;

  movement_policy(log_, global_hash_rings, local_hash_rings, grace_start, ss,
                  memory_node_count, disk_node_count, new_memory_count,
                  new_disk_count, scaling_alert_ip, key_replication_map,
                  key_access_summary, key_size, mt, pushers, response_puller,
                  routing_ips, rid);

  // new_disk_count should be set by emit_scale_up_alert due to overflow.
  EXPECT_GT(new_disk_count, 0u);
}
