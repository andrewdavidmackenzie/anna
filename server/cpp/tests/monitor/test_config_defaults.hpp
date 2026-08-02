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

// Tests that all configurable constants have the expected default values.
// This ensures that behavior is unchanged when no config overrides are
// provided.

#include "gtest/gtest.h"

#include "kvs/kvs_common.hpp"
#include "kvs/server_utils.hpp"
#include "metadata.hpp"
#include "monitor/monitoring_utils.hpp"
#include "threads.hpp"

// --- monitoring_utils.hpp defaults ---

TEST(ConfigDefaults, MonitoringThresholdDefault) {
  EXPECT_EQ(kMonitoringThreshold, 30u);
}

TEST(ConfigDefaults, GracePeriodDefault) {
  EXPECT_EQ(kGracePeriod, 120u);
}

TEST(ConfigDefaults, NodeAdditionBatchSizeDefault) {
  EXPECT_EQ(kNodeAdditionBatchSize, 2u);
}

TEST(ConfigDefaults, MaxMemoryNodeConsumptionDefault) {
  EXPECT_DOUBLE_EQ(kMaxMemoryNodeConsumption, 0.6);
}

TEST(ConfigDefaults, MinMemoryNodeConsumptionDefault) {
  EXPECT_DOUBLE_EQ(kMinMemoryNodeConsumption, 0.3);
}

TEST(ConfigDefaults, MaxDiskNodeConsumptionDefault) {
  EXPECT_DOUBLE_EQ(kMaxDiskNodeConsumption, 0.75);
}

TEST(ConfigDefaults, MinDiskNodeConsumptionDefault) {
  EXPECT_DOUBLE_EQ(kMinDiskNodeConsumption, 0.5);
}

TEST(ConfigDefaults, KeyPromotionThresholdDefault) {
  EXPECT_EQ(kKeyPromotionThreshold, 0u);
}

TEST(ConfigDefaults, KeyDemotionThresholdDefault) {
  EXPECT_EQ(kKeyDemotionThreshold, 1u);
}

TEST(ConfigDefaults, MinMemoryTierSizeDefault) {
  EXPECT_EQ(kMinMemoryTierSize, 1u);
}

TEST(ConfigDefaults, MinDiskTierSizeDefault) {
  EXPECT_EQ(kMinDiskTierSize, 0u);
}

TEST(ConfigDefaults, ValueSizeDefault) {
  EXPECT_EQ(kValueSize, 256u);
}

TEST(ConfigDefaults, SloOccupancyUpperDefault) {
  EXPECT_DOUBLE_EQ(kSloOccupancyUpper, 0.15);
}

TEST(ConfigDefaults, SloOccupancyLowerDefault) {
  EXPECT_DOUBLE_EQ(kSloOccupancyLower, 0.05);
}

// --- kvs_common.hpp defaults ---

TEST(ConfigDefaults, MetadataReplicationFactorDefault) {
  EXPECT_EQ(kMetadataReplicationFactor, 1u);
}

TEST(ConfigDefaults, MetadataLocalReplicationFactorDefault) {
  EXPECT_EQ(kMetadataLocalReplicationFactor, 1u);
}

TEST(ConfigDefaults, VirtualThreadNumDefault) {
  EXPECT_EQ(kVirtualThreadNum, 3000u);
}

TEST(ConfigDefaults, SloWorstDefault) {
  EXPECT_EQ(kSloWorst, 3000u);
}

// --- server_utils.hpp defaults ---

TEST(ConfigDefaults, GossipPeriodDefault) {
  EXPECT_EQ(kGossipPeriod, 10000000u);
}

TEST(ConfigDefaults, TombstoneGcMultiplierDefault) {
  EXPECT_EQ(kTombstoneGcMultiplier, 30u);
}

TEST(ConfigDefaults, DataRedistributeThresholdDefault) {
  EXPECT_EQ(kDataRedistributeThreshold, 50u);
}

TEST(ConfigDefaults, GarbageCollectThresholdDefault) {
  EXPECT_EQ(kGarbageCollectThreshold, 10000000u);
}

// --- threads.hpp defaults ---

TEST(ConfigDefaults, ScalingAlertPortDefault) {
  EXPECT_EQ(kScalingAlertPort, 6955u);
}

TEST(ConfigDefaults, BaseOffsetDefault) {
  EXPECT_EQ(kBaseOffset, 0u);
}

// --- metadata.hpp defaults ---

TEST(ConfigDefaults, WarmupKeyCountDefault) {
  EXPECT_EQ(kWarmupKeyCount, 1000000u);
}

TEST(ConfigDefaults, MaxWarmupKeyCountSafeForKeyFormat) {
  // The warmup loop generates 8-char zero-padded keys, so max must fit
  // in 8 digits (i.e., <= 99,999,999).
  EXPECT_EQ(kMaxWarmupKeyCount, 99999999u);
  EXPECT_LE(kWarmupKeyCount, kMaxWarmupKeyCount);
}
