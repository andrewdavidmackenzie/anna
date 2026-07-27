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
#include "monitor/monitoring_utils.hpp"

// Test compute_summary_stats with disk-tier data populated.
// This exercises the disk-tier branches in stats_helpers.cpp.
TEST(ComputeSummaryStats, DiskTierStatsAreComputed) {
  map<Key, map<Address, unsigned>> key_access_frequency;
  StorageStats memory_storage;
  StorageStats disk_storage;
  OccupancyStats memory_occupancy;
  OccupancyStats disk_occupancy;
  AccessStats memory_access;
  AccessStats disk_access;
  map<Key, unsigned> key_access_summary;
  SummaryStats ss;
  unsigned epoch = 1;

  // Populate memory-tier stats (1 node, 1 thread).
  memory_storage["127.0.0.1/127.0.0.1"][0] = 500000;
  memory_occupancy["127.0.0.1/127.0.0.1"][0] = {0.5, 1};
  memory_access["127.0.0.1/127.0.0.1"][0] = 100;

  // Populate disk-tier stats (1 node, 1 thread).
  disk_storage["127.0.0.2/127.0.0.2"][0] = 300000;
  disk_occupancy["127.0.0.2/127.0.0.2"][0] = {0.3, 1};
  disk_access["127.0.0.2/127.0.0.2"][0] = 50;

  // Add key access data.
  key_access_frequency["key1"]["127.0.0.1/127.0.0.1:0"] = 10;
  key_access_frequency["key2"]["127.0.0.2/127.0.0.2:0"] = 5;

  compute_summary_stats(key_access_frequency, memory_storage, disk_storage,
                        memory_occupancy, disk_occupancy, memory_access,
                        disk_access, key_access_summary, ss, log_, epoch);

  // Verify disk-tier stats were computed.
  EXPECT_EQ(ss.total_disk_access, 50u);
  EXPECT_GT(ss.total_disk_consumption, 0u);
  EXPECT_GT(ss.max_disk_consumption_percentage, 0.0);
  EXPECT_GT(ss.avg_disk_consumption_percentage, 0.0);
  EXPECT_GT(ss.max_disk_occupancy, 0.0);
  EXPECT_LE(ss.min_disk_occupancy, 1.0);
  EXPECT_GT(ss.avg_disk_occupancy, 0.0);
  EXPECT_GT(ss.required_disk_node, 0u);

  // Verify memory-tier stats also computed.
  EXPECT_EQ(ss.total_memory_access, 100u);
  EXPECT_GT(ss.total_memory_consumption, 0u);
}

// Test that summary stats clear() resets all disk fields.
TEST(ComputeSummaryStats, ClearResetsDiskFields) {
  SummaryStats ss;
  ss.total_disk_access = 42;
  ss.total_disk_consumption = 99999;
  ss.max_disk_consumption_percentage = 0.8;
  ss.avg_disk_consumption_percentage = 0.4;
  ss.required_disk_node = 3;
  ss.max_disk_occupancy = 0.9;
  ss.min_disk_occupancy = 0.1;
  ss.avg_disk_occupancy = 0.5;

  ss.clear();

  EXPECT_EQ(ss.total_disk_access, 0u);
  EXPECT_EQ(ss.total_disk_consumption, 0u);
  EXPECT_DOUBLE_EQ(ss.max_disk_consumption_percentage, 0.0);
  EXPECT_DOUBLE_EQ(ss.avg_disk_consumption_percentage, 0.0);
  EXPECT_EQ(ss.required_disk_node, 0u);
  EXPECT_DOUBLE_EQ(ss.max_disk_occupancy, 0.0);
  EXPECT_DOUBLE_EQ(ss.min_disk_occupancy, 1.0);
  EXPECT_DOUBLE_EQ(ss.avg_disk_occupancy, 0.0);
}
