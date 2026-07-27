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

// Test that membership_handler processes a disk-tier node join.
TEST(MembershipHandler, DiskTierNodeJoinDecrementsPendingCount) {
  GlobalRingMap global_hash_rings;
  unsigned new_memory_count = 0;
  unsigned new_disk_count = 2;  // Expecting 2 disk nodes
  TimePoint grace_start = std::chrono::system_clock::now();
  vector<Address> routing_ips;
  StorageStats memory_storage;
  StorageStats disk_storage;
  OccupancyStats memory_occupancy;
  OccupancyStats disk_occupancy;
  map<Key, map<Address, unsigned>> key_access_frequency;

  // Simulate a disk-tier node join message.
  // Format: "join:TIER_NAME:PUBLIC_IP:PRIVATE_IP"
  string serialized = "join:DISK:127.0.0.2:127.0.0.2";

  membership_handler(log_, serialized, global_hash_rings,
                     new_memory_count, new_disk_count, grace_start,
                     routing_ips, memory_storage, disk_storage,
                     memory_occupancy, disk_occupancy,
                     key_access_frequency);

  EXPECT_EQ(new_disk_count, 1u);
}

// Test that membership_handler processes a disk-tier node depart.
TEST(MembershipHandler, DiskTierNodeDepartClearsStats) {
  GlobalRingMap global_hash_rings;
  // Insert a disk-tier node.
  global_hash_rings[Tier::DISK].insert("127.0.0.2", "127.0.0.2", 0, 0);

  unsigned new_memory_count = 0;
  unsigned new_disk_count = 0;
  TimePoint grace_start = std::chrono::system_clock::now();
  vector<Address> routing_ips;
  StorageStats memory_storage;
  StorageStats disk_storage;
  disk_storage["127.0.0.2"][0] = 100000;
  OccupancyStats memory_occupancy;
  OccupancyStats disk_occupancy;
  disk_occupancy["127.0.0.2"][0] = {0.5, 1};
  map<Key, map<Address, unsigned>> key_access_frequency;

  // Simulate a disk-tier node depart message.
  string serialized = "depart:DISK:127.0.0.2:127.0.0.2";

  membership_handler(log_, serialized, global_hash_rings,
                     new_memory_count, new_disk_count, grace_start,
                     routing_ips, memory_storage, disk_storage,
                     memory_occupancy, disk_occupancy,
                     key_access_frequency);

  // Stats for the departed node should be cleared.
  EXPECT_EQ(disk_storage.count("127.0.0.2"), 0u);
  EXPECT_EQ(disk_occupancy.count("127.0.0.2"), 0u);
}

// Test that disk-tier depart clears key access frequency entries.
TEST(MembershipHandler, DiskTierDepartClearsKeyAccess) {
  GlobalRingMap global_hash_rings;
  global_hash_rings[Tier::DISK].insert("127.0.0.2", "127.0.0.2", 0, 0);

  unsigned new_memory_count = 0;
  unsigned new_disk_count = 0;
  TimePoint grace_start = std::chrono::system_clock::now();
  vector<Address> routing_ips;
  StorageStats memory_storage;
  StorageStats disk_storage;
  OccupancyStats memory_occupancy;
  OccupancyStats disk_occupancy;
  map<Key, map<Address, unsigned>> key_access_frequency;
  key_access_frequency["key1"]["127.0.0.2:0"] = 10;

  string serialized = "depart:DISK:127.0.0.2:127.0.0.2";

  membership_handler(log_, serialized, global_hash_rings,
                     new_memory_count, new_disk_count, grace_start,
                     routing_ips, memory_storage, disk_storage,
                     memory_occupancy, disk_occupancy,
                     key_access_frequency);

  // Key access frequency for the departed disk node thread should be cleared.
  EXPECT_EQ(key_access_frequency["key1"].count("127.0.0.2:0"), 0u);
}
