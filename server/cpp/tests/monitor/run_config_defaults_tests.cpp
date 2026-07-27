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

#include "kvs.pb.h"
#include "metadata.pb.h"
#include "kvs/kvs_common.hpp"
#include "kvs/server_utils.hpp"
#include "monitor/monitoring_utils.hpp"
#include "metadata.hpp"
#include "threads.hpp"

#include "test_config_defaults.hpp"
#include "test_config_yaml_parsing.hpp"

// Global variables required by the linker (normally defined in main() files).
unsigned kDefaultLocalReplication = 1;
unsigned kDefaultGlobalMemoryReplication = 1;
unsigned kDefaultGlobalDiskReplication = 0;
unsigned kMinimumReplicaNumber = 1;

unsigned kMemoryNodeCapacity = 0;
unsigned kDiskNodeCapacity = 0;

unsigned kMemoryThreadCount = 1;
unsigned kDiskThreadCount = 1;
unsigned kRoutingThreadCount = 1;
unsigned kThreadNum = 1;

Tier kSelfTier = Tier::MEMORY;
vector<Tier> kSelfTierIdVector = {Tier::MEMORY};
hmap<Tier, TierMetadata, TierEnumHash> kTierMetadata = {};

bool kEnableTiering = false;
bool kEnableElasticity = false;
bool kEnableSelectiveRep = false;

int main(int argc, char *argv[]) {
  testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
