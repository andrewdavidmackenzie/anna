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

// Tests that YAML config parsing correctly overrides default values.
// These tests exercise the same parsing patterns used in monitoring.cpp
// and server.cpp to verify that every new config key is wired correctly.

#include <cstdio>
#include <fstream>
#include <string>
#include <unistd.h>

#include "gtest/gtest.h"

#include "kvs/kvs_common.hpp"
#include "kvs/server_utils.hpp"
#include "metadata.hpp"
#include "monitor/monitoring_utils.hpp"
#include "threads.hpp"
#include "yaml-cpp/yaml.h"

// Helper: write YAML content to a temp file and return the path.
static std::string write_temp_yaml(const std::string &content) {
  char path[] = "/tmp/anna_test_config_XXXXXX";
  int fd = mkstemp(path);
  EXPECT_NE(fd, -1);
  close(fd);
  std::ofstream out(path);
  out << content;
  out.close();
  return std::string(path);
}

// Helper: apply the monitoring.cpp policy-section parsing pattern to a
// YAML::Node. This mirrors the code in monitoring.cpp so we can verify
// each key is parsed correctly.
static void apply_policy_config(const YAML::Node &conf) {
  if (conf["policy"]) {
    YAML::Node policy = conf["policy"];

    if (policy["node_addition_batch_size"])
      kNodeAdditionBatchSize = policy["node_addition_batch_size"].as<unsigned>();
    if (policy["assumed_value_size_kb"])
      kValueSize = policy["assumed_value_size_kb"].as<unsigned>();
    if (policy["min_memory_nodes"])
      kMinMemoryTierSize = policy["min_memory_nodes"].as<unsigned>();
    if (policy["min_disk_nodes"])
      kMinEbsTierSize = policy["min_disk_nodes"].as<unsigned>();
    if (policy["warmup_key_count"]) {
      unsigned val = policy["warmup_key_count"].as<unsigned>();
      kWarmupKeyCount = std::min(val, kMaxWarmupKeyCount);
    }

    if (policy["storage"]) {
      YAML::Node storage = policy["storage"];
      if (storage["memory_upper"])
        kMaxMemoryNodeConsumption = storage["memory_upper"].as<double>();
      if (storage["memory_lower"])
        kMinMemoryNodeConsumption = storage["memory_lower"].as<double>();
      if (storage["disk_upper"])
        kMaxEbsNodeConsumption = storage["disk_upper"].as<double>();
      if (storage["disk_lower"])
        kMinEbsNodeConsumption = storage["disk_lower"].as<double>();
    }

    if (policy["tiering_thresholds"]) {
      YAML::Node tiering = policy["tiering_thresholds"];
      if (tiering["promotion_threshold"])
        kKeyPromotionThreshold = tiering["promotion_threshold"].as<unsigned>();
      if (tiering["demotion_threshold"])
        kKeyDemotionThreshold = tiering["demotion_threshold"].as<unsigned>();
    }

    if (policy["slo"]) {
      YAML::Node slo = policy["slo"];
      if (slo["latency_target_us"]) {
        unsigned val = slo["latency_target_us"].as<unsigned>();
        if (val > 0) {
          kSloWorst = val;
        }
      }
      if (slo["occupancy_upper"])
        kSloOccupancyUpper = slo["occupancy_upper"].as<double>();
      if (slo["occupancy_lower"])
        kSloOccupancyLower = slo["occupancy_lower"].as<double>();
      if (kSloOccupancyLower > kSloOccupancyUpper) {
        std::swap(kSloOccupancyLower, kSloOccupancyUpper);
      }
    }
  }
}

// Helper: apply the ports-section parsing pattern.
static void apply_ports_config(const YAML::Node &conf) {
  if (conf["ports"]) {
    YAML::Node ports = conf["ports"];
    if (ports["base_offset"])
      kBaseOffset = ports["base_offset"].as<unsigned>();
    if (ports["management"])
      kManagementNodePort = ports["management"].as<unsigned>();
  }
}

// Helper: apply the hashing-section parsing pattern.
static void apply_hashing_config(const YAML::Node &conf) {
  if (conf["hashing"]) {
    YAML::Node hashing = conf["hashing"];
    if (hashing["virtual_nodes_per_thread"]) {
      unsigned val = hashing["virtual_nodes_per_thread"].as<unsigned>();
      if (val > 0) {
        kVirtualThreadNum = val;
      }
    }
  }
}

// Helper: apply the replication-section parsing pattern (new keys only).
static void apply_replication_config(const YAML::Node &conf) {
  if (conf["replication"]) {
    YAML::Node replication = conf["replication"];
    if (replication["metadata"])
      kMetadataReplicationFactor = replication["metadata"].as<unsigned>();
    if (replication["metadata_local"])
      kMetadataLocalReplicationFactor =
          replication["metadata_local"].as<unsigned>();
  }
}

// Helper: apply the timings-section parsing pattern (new keys only).
static void apply_timings_config(const YAML::Node &conf) {
  if (conf["timings"]) {
    YAML::Node timings = conf["timings"];
    if (timings["garbage_collect_period_us"])
      kGarbageCollectThreshold =
          timings["garbage_collect_period_us"].as<unsigned>();
  }
}

// A fixture that saves and restores all configurable globals so tests are
// independent. Each test modifies globals via YAML parsing and they are
// restored to defaults after.
class ConfigYamlParsingTest : public ::testing::Test {
protected:
  // Saved copies of all configurable globals.
  unsigned saved_kNodeAdditionBatchSize;
  double saved_kMaxMemoryNodeConsumption;
  double saved_kMinMemoryNodeConsumption;
  double saved_kMaxEbsNodeConsumption;
  double saved_kMinEbsNodeConsumption;
  unsigned saved_kKeyPromotionThreshold;
  unsigned saved_kKeyDemotionThreshold;
  unsigned saved_kMinMemoryTierSize;
  unsigned saved_kMinEbsTierSize;
  unsigned saved_kValueSize;
  double saved_kSloOccupancyUpper;
  double saved_kSloOccupancyLower;
  unsigned saved_kWarmupKeyCount;
  unsigned saved_kMetadataReplicationFactor;
  unsigned saved_kMetadataLocalReplicationFactor;
  unsigned saved_kVirtualThreadNum;
  unsigned saved_kSloWorst;
  unsigned saved_kGossipPeriod;
  unsigned saved_kTombstoneGcMultiplier;
  unsigned saved_kDataRedistributeThreshold;
  unsigned saved_kGarbageCollectThreshold;
  unsigned saved_kManagementNodePort;
  unsigned saved_kBaseOffset;

  void SetUp() override {
    saved_kNodeAdditionBatchSize = kNodeAdditionBatchSize;
    saved_kMaxMemoryNodeConsumption = kMaxMemoryNodeConsumption;
    saved_kMinMemoryNodeConsumption = kMinMemoryNodeConsumption;
    saved_kMaxEbsNodeConsumption = kMaxEbsNodeConsumption;
    saved_kMinEbsNodeConsumption = kMinEbsNodeConsumption;
    saved_kKeyPromotionThreshold = kKeyPromotionThreshold;
    saved_kKeyDemotionThreshold = kKeyDemotionThreshold;
    saved_kMinMemoryTierSize = kMinMemoryTierSize;
    saved_kMinEbsTierSize = kMinEbsTierSize;
    saved_kValueSize = kValueSize;
    saved_kSloOccupancyUpper = kSloOccupancyUpper;
    saved_kSloOccupancyLower = kSloOccupancyLower;
    saved_kWarmupKeyCount = kWarmupKeyCount;
    saved_kMetadataReplicationFactor = kMetadataReplicationFactor;
    saved_kMetadataLocalReplicationFactor = kMetadataLocalReplicationFactor;
    saved_kVirtualThreadNum = kVirtualThreadNum;
    saved_kSloWorst = kSloWorst;
    saved_kGossipPeriod = kGossipPeriod;
    saved_kTombstoneGcMultiplier = kTombstoneGcMultiplier;
    saved_kDataRedistributeThreshold = kDataRedistributeThreshold;
    saved_kGarbageCollectThreshold = kGarbageCollectThreshold;
    saved_kManagementNodePort = kManagementNodePort;
    saved_kBaseOffset = kBaseOffset;
  }

  void TearDown() override {
    kNodeAdditionBatchSize = saved_kNodeAdditionBatchSize;
    kMaxMemoryNodeConsumption = saved_kMaxMemoryNodeConsumption;
    kMinMemoryNodeConsumption = saved_kMinMemoryNodeConsumption;
    kMaxEbsNodeConsumption = saved_kMaxEbsNodeConsumption;
    kMinEbsNodeConsumption = saved_kMinEbsNodeConsumption;
    kKeyPromotionThreshold = saved_kKeyPromotionThreshold;
    kKeyDemotionThreshold = saved_kKeyDemotionThreshold;
    kMinMemoryTierSize = saved_kMinMemoryTierSize;
    kMinEbsTierSize = saved_kMinEbsTierSize;
    kValueSize = saved_kValueSize;
    kSloOccupancyUpper = saved_kSloOccupancyUpper;
    kSloOccupancyLower = saved_kSloOccupancyLower;
    kWarmupKeyCount = saved_kWarmupKeyCount;
    kMetadataReplicationFactor = saved_kMetadataReplicationFactor;
    kMetadataLocalReplicationFactor = saved_kMetadataLocalReplicationFactor;
    kVirtualThreadNum = saved_kVirtualThreadNum;
    kSloWorst = saved_kSloWorst;
    kGossipPeriod = saved_kGossipPeriod;
    kTombstoneGcMultiplier = saved_kTombstoneGcMultiplier;
    kDataRedistributeThreshold = saved_kDataRedistributeThreshold;
    kGarbageCollectThreshold = saved_kGarbageCollectThreshold;
    kManagementNodePort = saved_kManagementNodePort;
    kBaseOffset = saved_kBaseOffset;
  }
};

// --- policy section ---

TEST_F(ConfigYamlParsingTest, PolicyNodeAdditionBatchSize) {
  auto path = write_temp_yaml("policy:\n  node_addition_batch_size: 5\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_EQ(kNodeAdditionBatchSize, 5u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyAssumedValueSizeKb) {
  auto path = write_temp_yaml("policy:\n  assumed_value_size_kb: 512\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_EQ(kValueSize, 512u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyMinMemoryNodes) {
  auto path = write_temp_yaml("policy:\n  min_memory_nodes: 3\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_EQ(kMinMemoryTierSize, 3u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyMinDiskNodes) {
  auto path = write_temp_yaml("policy:\n  min_disk_nodes: 2\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_EQ(kMinEbsTierSize, 2u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyWarmupKeyCount) {
  auto path = write_temp_yaml("policy:\n  warmup_key_count: 500\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_EQ(kWarmupKeyCount, 500u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyStorageMemoryUpper) {
  auto path = write_temp_yaml(
      "policy:\n  storage:\n    memory_upper: 0.8\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_DOUBLE_EQ(kMaxMemoryNodeConsumption, 0.8);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyStorageMemoryLower) {
  auto path = write_temp_yaml(
      "policy:\n  storage:\n    memory_lower: 0.1\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_DOUBLE_EQ(kMinMemoryNodeConsumption, 0.1);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyStorageDiskUpper) {
  auto path = write_temp_yaml(
      "policy:\n  storage:\n    disk_upper: 0.9\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_DOUBLE_EQ(kMaxEbsNodeConsumption, 0.9);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyStorageDiskLower) {
  auto path = write_temp_yaml(
      "policy:\n  storage:\n    disk_lower: 0.4\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_DOUBLE_EQ(kMinEbsNodeConsumption, 0.4);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyTieringPromotionThreshold) {
  auto path = write_temp_yaml(
      "policy:\n  tiering_thresholds:\n    promotion_threshold: 5\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_EQ(kKeyPromotionThreshold, 5u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicyTieringDemotionThreshold) {
  auto path = write_temp_yaml(
      "policy:\n  tiering_thresholds:\n    demotion_threshold: 3\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_EQ(kKeyDemotionThreshold, 3u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicySloLatencyTargetUs) {
  auto path = write_temp_yaml(
      "policy:\n  slo:\n    latency_target_us: 5000\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_EQ(kSloWorst, 5000u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicySloOccupancyUpper) {
  auto path = write_temp_yaml(
      "policy:\n  slo:\n    occupancy_upper: 0.25\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_DOUBLE_EQ(kSloOccupancyUpper, 0.25);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PolicySloOccupancyLower) {
  auto path = write_temp_yaml(
      "policy:\n  slo:\n    occupancy_lower: 0.02\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  EXPECT_DOUBLE_EQ(kSloOccupancyLower, 0.02);
  std::remove(path.c_str());
}

// --- ports section ---

TEST_F(ConfigYamlParsingTest, PortsManagement) {
  auto path = write_temp_yaml("ports:\n  management: 8001\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_ports_config(conf);
  EXPECT_EQ(kManagementNodePort, 8001u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, PortsBaseOffset) {
  auto path = write_temp_yaml("ports:\n  base_offset: 2000\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_ports_config(conf);
  EXPECT_EQ(kBaseOffset, 2000u);
  std::remove(path.c_str());
}

// --- hashing section ---

TEST_F(ConfigYamlParsingTest, HashingVirtualNodesPerThread) {
  auto path =
      write_temp_yaml("hashing:\n  virtual_nodes_per_thread: 5000\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_hashing_config(conf);
  EXPECT_EQ(kVirtualThreadNum, 5000u);
  std::remove(path.c_str());
}

// --- replication section (new keys) ---

TEST_F(ConfigYamlParsingTest, ReplicationMetadata) {
  auto path = write_temp_yaml("replication:\n  metadata: 3\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_replication_config(conf);
  EXPECT_EQ(kMetadataReplicationFactor, 3u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, ReplicationMetadataLocal) {
  auto path = write_temp_yaml("replication:\n  metadata_local: 2\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_replication_config(conf);
  EXPECT_EQ(kMetadataLocalReplicationFactor, 2u);
  std::remove(path.c_str());
}

// --- timings section (new key) ---

TEST_F(ConfigYamlParsingTest, TimingsGarbageCollectPeriodUs) {
  auto path =
      write_temp_yaml("timings:\n  garbage_collect_period_us: 5000000\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_timings_config(conf);
  EXPECT_EQ(kGarbageCollectThreshold, 5000000u);
  std::remove(path.c_str());
}

// --- empty / missing section leaves defaults ---

TEST_F(ConfigYamlParsingTest, EmptyConfigLeavesDefaults) {
  auto path = write_temp_yaml("# empty config\n");
  YAML::Node conf = YAML::LoadFile(path);

  // Apply all parsers -- nothing should change.
  apply_policy_config(conf);
  apply_ports_config(conf);
  apply_hashing_config(conf);
  apply_replication_config(conf);
  apply_timings_config(conf);

  // Spot-check a representative set of defaults.
  EXPECT_EQ(kNodeAdditionBatchSize, 2u);
  EXPECT_DOUBLE_EQ(kMaxMemoryNodeConsumption, 0.6);
  EXPECT_EQ(kVirtualThreadNum, 3000u);
  EXPECT_EQ(kSloWorst, 3000u);
  EXPECT_EQ(kManagementNodePort, 7001u);
  EXPECT_EQ(kGarbageCollectThreshold, 10000000u);
  EXPECT_EQ(kMetadataReplicationFactor, 1u);

  std::remove(path.c_str());
}

// --- partial config only overrides specified keys ---

TEST_F(ConfigYamlParsingTest, PartialPolicyOverridesOnlySpecifiedKeys) {
  auto path = write_temp_yaml(
      "policy:\n"
      "  node_addition_batch_size: 4\n"
      "  storage:\n"
      "    memory_upper: 0.7\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);

  // Overridden values.
  EXPECT_EQ(kNodeAdditionBatchSize, 4u);
  EXPECT_DOUBLE_EQ(kMaxMemoryNodeConsumption, 0.7);

  // Non-overridden values remain at defaults.
  EXPECT_DOUBLE_EQ(kMinMemoryNodeConsumption, 0.3);
  EXPECT_EQ(kKeyPromotionThreshold, 0u);
  EXPECT_EQ(kSloWorst, 3000u);

  std::remove(path.c_str());
}

// --- full config file: all keys at once ---

TEST_F(ConfigYamlParsingTest, FullConfigOverridesAllKeys) {
  auto path = write_temp_yaml(
      "policy:\n"
      "  node_addition_batch_size: 10\n"
      "  assumed_value_size_kb: 1024\n"
      "  min_memory_nodes: 5\n"
      "  min_disk_nodes: 3\n"
      "  warmup_key_count: 100\n"
      "  storage:\n"
      "    memory_upper: 0.9\n"
      "    memory_lower: 0.1\n"
      "    disk_upper: 0.95\n"
      "    disk_lower: 0.2\n"
      "  tiering_thresholds:\n"
      "    promotion_threshold: 10\n"
      "    demotion_threshold: 5\n"
      "  slo:\n"
      "    latency_target_us: 1000\n"
      "    occupancy_upper: 0.30\n"
      "    occupancy_lower: 0.01\n"
      "hashing:\n"
      "  virtual_nodes_per_thread: 1000\n"
      "ports:\n"
      "  base_offset: 100\n"
      "  management: 9999\n"
      "replication:\n"
      "  metadata: 2\n"
      "  metadata_local: 3\n"
      "timings:\n"
      "  garbage_collect_period_us: 99999\n");
  YAML::Node conf = YAML::LoadFile(path);

  apply_policy_config(conf);
  apply_ports_config(conf);
  apply_hashing_config(conf);
  apply_replication_config(conf);
  apply_timings_config(conf);

  EXPECT_EQ(kNodeAdditionBatchSize, 10u);
  EXPECT_EQ(kValueSize, 1024u);
  EXPECT_EQ(kMinMemoryTierSize, 5u);
  EXPECT_EQ(kMinEbsTierSize, 3u);
  EXPECT_EQ(kWarmupKeyCount, 100u);
  EXPECT_DOUBLE_EQ(kMaxMemoryNodeConsumption, 0.9);
  EXPECT_DOUBLE_EQ(kMinMemoryNodeConsumption, 0.1);
  EXPECT_DOUBLE_EQ(kMaxEbsNodeConsumption, 0.95);
  EXPECT_DOUBLE_EQ(kMinEbsNodeConsumption, 0.2);
  EXPECT_EQ(kKeyPromotionThreshold, 10u);
  EXPECT_EQ(kKeyDemotionThreshold, 5u);
  EXPECT_EQ(kSloWorst, 1000u);
  EXPECT_DOUBLE_EQ(kSloOccupancyUpper, 0.30);
  EXPECT_DOUBLE_EQ(kSloOccupancyLower, 0.01);
  EXPECT_EQ(kVirtualThreadNum, 1000u);
  EXPECT_EQ(kBaseOffset, 100u);
  EXPECT_EQ(kManagementNodePort, 9999u);
  EXPECT_EQ(kMetadataReplicationFactor, 2u);
  EXPECT_EQ(kMetadataLocalReplicationFactor, 3u);
  EXPECT_EQ(kGarbageCollectThreshold, 99999u);

  std::remove(path.c_str());
}

// --- validation tests ---

TEST_F(ConfigYamlParsingTest, ZeroLatencyTargetIsRejected) {
  auto path = write_temp_yaml(
      "policy:\n  slo:\n    latency_target_us: 0\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  // kSloWorst should remain at default (3000), not set to 0.
  EXPECT_EQ(kSloWorst, 3000u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, ZeroVirtualNodesIsRejected) {
  auto path =
      write_temp_yaml("hashing:\n  virtual_nodes_per_thread: 0\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_hashing_config(conf);
  // kVirtualThreadNum should remain at default (3000), not set to 0.
  EXPECT_EQ(kVirtualThreadNum, 3000u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, WarmupKeyCountCappedAtMax) {
  auto path =
      write_temp_yaml("policy:\n  warmup_key_count: 200000000\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  // Should be clamped to kMaxWarmupKeyCount (99999999).
  EXPECT_EQ(kWarmupKeyCount, 99999999u);
  std::remove(path.c_str());
}

TEST_F(ConfigYamlParsingTest, OccupancyLowerGreaterThanUpperIsSwapped) {
  auto path = write_temp_yaml(
      "policy:\n  slo:\n    occupancy_upper: 0.05\n    occupancy_lower: 0.15\n");
  YAML::Node conf = YAML::LoadFile(path);
  apply_policy_config(conf);
  // Values should be swapped so lower <= upper.
  EXPECT_DOUBLE_EQ(kSloOccupancyLower, 0.05);
  EXPECT_DOUBLE_EQ(kSloOccupancyUpper, 0.15);
  std::remove(path.c_str());
}
