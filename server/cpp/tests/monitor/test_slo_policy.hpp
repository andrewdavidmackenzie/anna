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

class SloPolicyTest : public ::testing::Test {
protected:
  void SetUp() override {
    mock_zmq_util.sent_messages.clear();
    kEnableElasticity = true;
    kEnableTiering = false;
    kEnableSelectiveRep = false;
  }
  void TearDown() override {
    mock_zmq_util.sent_messages.clear();
    kEnableElasticity = false;
    kEnableTiering = false;
    kEnableSelectiveRep = false;
  }
};

// When latency exceeds SLO and occupancy is high, scale-up alert is emitted.
TEST_F(SloPolicyTest, LatencyViolationTriggersScaleUp) {
  GlobalRingMap global_hash_rings;
  LocalRingMap local_hash_rings;
  TimePoint grace_start = std::chrono::system_clock::now() -
                           std::chrono::seconds(kGracePeriod + 1);
  SummaryStats ss;
  // Latency must exceed kSloWorst (default 3000)
  ss.avg_latency = 6000;
  // Occupancy must exceed kSloOccupancyUpper (default 0.15)
  ss.min_memory_occupancy = 0.5;

  unsigned memory_node_count = 2;
  unsigned new_memory_count = 0;
  bool removing_memory_node = false;
  Address scaling_alert_ip = "127.0.0.1";
  map<Key, KeyReplication> key_replication_map;
  map<Key, unsigned> key_access_summary;
  MonitoringThread mt("127.0.0.1");
  map<Address, unsigned> departing_node_map;
  zmq::context_t context;
  SocketCache pushers(&context, ZMQ_PUSH);
  zmq::socket_t response_puller(context, ZMQ_PULL);
  vector<Address> routing_ips;
  unsigned rid = 0;
  map<Key, std::pair<double, unsigned>> latency_miss_ratio_map;

  slo_policy(log_, global_hash_rings, local_hash_rings, grace_start, ss,
             memory_node_count, new_memory_count, removing_memory_node,
             scaling_alert_ip, key_replication_map, key_access_summary, mt,
             departing_node_map, pushers, response_puller, routing_ips, rid,
             latency_miss_ratio_map);

  // new_memory_count should be set by emit_scale_up_alert.
  EXPECT_GT(new_memory_count, 0u);
}
