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

#include <thread>

#include "monitor/monitoring_handlers.hpp"
#include "monitor/monitoring_utils.hpp"
#include "monitor/policies.hpp"
#include "signal_handler.hpp"
#include "yaml-cpp/yaml.h"

unsigned kMemoryThreadCount;
unsigned kDiskThreadCount;

unsigned kMemoryNodeCapacity;
unsigned kDiskNodeCapacity;

unsigned kDefaultGlobalMemoryReplication;
unsigned kDefaultGlobalDiskReplication;
unsigned kDefaultLocalReplication;
unsigned kMinimumReplicaNumber;

bool kEnableElasticity;
bool kEnableTiering;
bool kEnableSelectiveRep;

// read-only per-tier metadata
hmap<Tier, TierMetadata, TierEnumHash> kTierMetadata;

ZmqUtil zmq_util;
ZmqUtilInterface *kZmqUtil = &zmq_util;

HashRingUtil hash_ring_util;
HashRingUtilInterface *kHashRingUtil = &hash_ring_util;

int main(int argc, char *argv[]) {
  auto log = spdlog::stdout_color_mt("monitoring_log");
  log->flush_on(spdlog::level::info);

  install_shutdown_handler();

  if (argc != 3) {
    std::cerr << "Usage: " << argv[0] << "--config <config file path>" << std::endl;
    return 1;
  }

  // read the YAML conf
  YAML::Node conf = YAML::LoadFile(argv[2]);

  if (conf["ports"]) {
    YAML::Node ports = conf["ports"];
    if (ports["base_offset"])
      kBaseOffset = ports["base_offset"].as<unsigned>();
    if (ports["scaling_alert"])
      kScalingAlertPort = ports["scaling_alert"].as<unsigned>();
  }

  unsigned monitoringResponseTimeout = 10000;
  if (conf["timings"]) {
    YAML::Node timings = conf["timings"];
    if (timings["monitoring_timeout"])
      kMonitoringThreshold = timings["monitoring_timeout"].as<unsigned>();
    if (timings["grace_period"])
      kGracePeriod = timings["grace_period"].as<unsigned>();
    if (timings["monitoring_response_timeout_ms"])
      monitoringResponseTimeout = timings["monitoring_response_timeout_ms"].as<unsigned>();
  }

  YAML::Node monitoring = conf["monitoring"];
  Address ip = monitoring["ip"].as<Address>();
  Address scaling_alert_ip = monitoring["scaling_alert_ip"].as<Address>();

  YAML::Node policy = conf["policy"];
  kEnableElasticity = policy["elasticity"].as<bool>();
  kEnableSelectiveRep = policy["selective-rep"].as<bool>();
  kEnableTiering = policy["tiering"].as<bool>();

  if (policy["node_addition_batch_size"])
    kNodeAdditionBatchSize = policy["node_addition_batch_size"].as<unsigned>();
  if (policy["assumed_value_size_kb"])
    kValueSize = policy["assumed_value_size_kb"].as<unsigned>();
  if (policy["min_memory_nodes"])
    kMinMemoryTierSize = policy["min_memory_nodes"].as<unsigned>();
  if (policy["min_disk_nodes"])
    kMinDiskTierSize = policy["min_disk_nodes"].as<unsigned>();
  if (policy["warmup_key_count"]) {
    unsigned val = policy["warmup_key_count"].as<unsigned>();
    if (val > kMaxWarmupKeyCount) {
      log->warn("warmup_key_count {} exceeds max safe value {}; clamping.",
                val, kMaxWarmupKeyCount);
      val = kMaxWarmupKeyCount;
    }
    kWarmupKeyCount = val;
  }

  if (policy["storage"]) {
    YAML::Node storage = policy["storage"];
    if (storage["memory_upper"])
      kMaxMemoryNodeConsumption = storage["memory_upper"].as<double>();
    if (storage["memory_lower"])
      kMinMemoryNodeConsumption = storage["memory_lower"].as<double>();
    if (storage["disk_upper"])
      kMaxDiskNodeConsumption = storage["disk_upper"].as<double>();
    if (storage["disk_lower"])
      kMinDiskNodeConsumption = storage["disk_lower"].as<double>();
  }

  if (policy["tiering_thresholds"]) {
    YAML::Node tiering_thresholds = policy["tiering_thresholds"];
    if (tiering_thresholds["promotion_threshold"])
      kKeyPromotionThreshold = tiering_thresholds["promotion_threshold"].as<unsigned>();
    if (tiering_thresholds["demotion_threshold"])
      kKeyDemotionThreshold = tiering_thresholds["demotion_threshold"].as<unsigned>();
  }

  if (policy["slo"]) {
    YAML::Node slo = policy["slo"];
    if (slo["latency_target_us"]) {
      unsigned val = slo["latency_target_us"].as<unsigned>();
      if (val == 0) {
        log->error("latency_target_us must be > 0; keeping default {}.",
                   kSloWorst);
      } else {
        kSloWorst = val;
      }
    }
    if (slo["occupancy_upper"])
      kSloOccupancyUpper = slo["occupancy_upper"].as<double>();
    if (slo["occupancy_lower"])
      kSloOccupancyLower = slo["occupancy_lower"].as<double>();
    if (kSloOccupancyLower > kSloOccupancyUpper) {
      log->error("slo.occupancy_lower ({}) > occupancy_upper ({}); "
                 "swapping values.",
                 kSloOccupancyLower, kSloOccupancyUpper);
      std::swap(kSloOccupancyLower, kSloOccupancyUpper);
    }
  }

  log->info("Elasticity policy enabled: {}", kEnableElasticity);
  log->info("Tiering policy enabled: {}", kEnableTiering);
  log->info("Selective replication policy enabled: {}", kEnableSelectiveRep);

  YAML::Node threads = conf["threads"];
  kMemoryThreadCount = threads["memory"].as<unsigned>();
  kDiskThreadCount = threads["disk"].as<unsigned>();

  // A thread count of 0 means "auto-detect from available cores".
  unsigned hw_threads = std::thread::hardware_concurrency();
  if (hw_threads == 0) hw_threads = 1;  // fallback if detection fails

  if (kMemoryThreadCount == 0) kMemoryThreadCount = hw_threads;
  if (kDiskThreadCount == 0) kDiskThreadCount = hw_threads;

  YAML::Node capacities = conf["capacities"];
  if (capacities["memory-cap-kb"]) {
    kMemoryNodeCapacity = capacities["memory-cap-kb"].as<unsigned>();
  } else {
    kMemoryNodeCapacity = capacities["memory-cap"].as<unsigned>() * 1000000;
  }
  if (capacities["disk-cap-kb"]) {
    kDiskNodeCapacity = capacities["disk-cap-kb"].as<unsigned>();
  } else {
    kDiskNodeCapacity = capacities["disk-cap"].as<unsigned>() * 1000000;
  }

  if (conf["hashing"]) {
    YAML::Node hashing = conf["hashing"];
    if (hashing["virtual_nodes_per_thread"]) {
      unsigned val = hashing["virtual_nodes_per_thread"].as<unsigned>();
      if (val == 0) {
        log->error("virtual_nodes_per_thread must be > 0; keeping default {}.",
                   kVirtualThreadNum);
      } else {
        kVirtualThreadNum = val;
      }
    }
  }

  YAML::Node replication = conf["replication"];
  kDefaultGlobalMemoryReplication = replication["memory"].as<unsigned>();
  kDefaultGlobalDiskReplication = replication["disk"].as<unsigned>();
  kDefaultLocalReplication = replication["local"].as<unsigned>();
  kMinimumReplicaNumber = replication["minimum"].as<unsigned>();
  if (replication["metadata"])
    kMetadataReplicationFactor = replication["metadata"].as<unsigned>();
  if (replication["metadata_local"])
    kMetadataLocalReplicationFactor = replication["metadata_local"].as<unsigned>();

  kTierMetadata[Tier::MEMORY] =
      TierMetadata(Tier::MEMORY, kMemoryThreadCount,
                   kDefaultGlobalMemoryReplication, kMemoryNodeCapacity);
  kTierMetadata[Tier::DISK] =
      TierMetadata(Tier::DISK, kDiskThreadCount, kDefaultGlobalDiskReplication,
                   kDiskNodeCapacity);

  GlobalRingMap global_hash_rings;
  LocalRingMap local_hash_rings;

  // form local hash rings
  for (const auto &pair : kTierMetadata) {
    TierMetadata tier = pair.second;
    for (unsigned tid = 0; tid < tier.thread_number_; tid++) {
      local_hash_rings[tier.id_].insert(ip, ip, 0, tid);
    }
  }

  // keep track of the keys' replication info
  map<Key, KeyReplication> key_replication_map;

  unsigned memory_node_count;
  unsigned disk_node_count;

  map<Key, map<Address, unsigned>> key_access_frequency;

  map<Key, unsigned> key_access_summary;

  map<Key, unsigned> key_size;

  StorageStats memory_storage;

  StorageStats disk_storage;

  OccupancyStats memory_occupancy;

  OccupancyStats disk_occupancy;

  AccessStats memory_accesses;

  AccessStats disk_accesses;

  SummaryStats ss;

  map<string, double> user_latency;

  map<string, double> user_throughput;

  map<Key, std::pair<double, unsigned>> latency_miss_ratio_map;

  vector<Address> routing_ips;

  MonitoringThread mt = MonitoringThread(ip);

  zmq::context_t context(1);
  SocketCache pushers(&context, ZMQ_PUSH);

  // responsible for listening to the response of the replication factor change request
  zmq::socket_t response_puller(context, ZMQ_PULL);

  response_puller.set(zmq::sockopt::rcvtimeo, static_cast<int>(monitoringResponseTimeout));
  response_puller.bind(mt.response_bind_address());

  // keep track of departing node status
  map<Address, unsigned> departing_node_map;

  // responsible for both node join and departure
  zmq::socket_t notify_puller(context, ZMQ_PULL);
  notify_puller.bind(mt.notify_bind_address());

  // responsible for receiving depart done notice
  zmq::socket_t depart_done_puller(context, ZMQ_PULL);
  depart_done_puller.bind(mt.depart_done_bind_address());

  // responsible for receiving feedback from users
  zmq::socket_t feedback_puller(context, ZMQ_PULL);
  feedback_puller.bind(mt.feedback_report_bind_address());

  vector<zmq::pollitem_t> pollitems = {
      {static_cast<void *>(notify_puller), 0, ZMQ_POLLIN, 0},
      {static_cast<void *>(depart_done_puller), 0, ZMQ_POLLIN, 0},
      {static_cast<void *>(feedback_puller), 0, ZMQ_POLLIN, 0}};

  auto report_start = std::chrono::system_clock::now();
  auto report_end = std::chrono::system_clock::now();

  auto grace_start = std::chrono::system_clock::now();

  unsigned new_memory_count = 0;
  unsigned new_disk_count = 0;
  bool removing_memory_node = false;
  bool removing_disk_node = false;

  unsigned server_monitoring_epoch = 0;

  // Track last observed epoch per node for crash detection
  map<Address, unsigned> last_observed_epoch;
  map<Address, std::chrono::time_point<std::chrono::system_clock>>
      last_epoch_change;

  unsigned rid = 0;

  while (!shutdown_requested.load()) {
   try {
    kZmqUtil->poll(&pollitems, std::chrono::milliseconds{0});

    if (pollitems[0].revents & ZMQ_POLLIN) {
      string serialized = kZmqUtil->recv_string(&notify_puller);

      // Track join time for crash detection
      vector<string> parts;
      split(serialized, ':', parts);
      if (parts.size() >= 4 && parts[0] == "join") {
        Address node_id = parts[2] + "/" + parts[3];
        last_epoch_change[node_id] = std::chrono::system_clock::now();
      } else if (parts.size() >= 4 && parts[0] == "depart") {
        Address node_id = parts[2] + "/" + parts[3];
        last_epoch_change.erase(node_id);
      }

      membership_handler(log, serialized, global_hash_rings, new_memory_count,
                         new_disk_count, grace_start, routing_ips,
                         memory_storage, disk_storage, memory_occupancy,
                         disk_occupancy, key_access_frequency);
    }

    if (pollitems[1].revents & ZMQ_POLLIN) {
      string serialized = kZmqUtil->recv_string(&depart_done_puller);
      depart_done_handler(log, serialized, departing_node_map, scaling_alert_ip,
                          removing_memory_node, removing_disk_node, pushers,
                          grace_start);
    }

    if (pollitems[2].revents & ZMQ_POLLIN) {
      string serialized = kZmqUtil->recv_string(&feedback_puller);
      feedback_handler(serialized, user_latency, user_throughput,
                       latency_miss_ratio_map);
    }

    report_end = std::chrono::system_clock::now();

    if (std::chrono::duration_cast<std::chrono::seconds>(report_end -
                                                         report_start)
            .count() >= kMonitoringThreshold) {
      server_monitoring_epoch += 1;

      memory_node_count =
          global_hash_rings[Tier::MEMORY].size() / kVirtualThreadNum;
      disk_node_count = global_hash_rings[Tier::DISK].size() / kVirtualThreadNum;

      key_access_frequency.clear();
      key_access_summary.clear();

      memory_storage.clear();
      disk_storage.clear();

      memory_occupancy.clear();
      disk_occupancy.clear();

      ss.clear();

      collect_internal_stats(
          global_hash_rings, local_hash_rings, pushers, mt, response_puller,
          log, rid, key_access_frequency, key_size, memory_storage, disk_storage,
          memory_occupancy, disk_occupancy, memory_accesses, disk_accesses);

      // Crash detection: nodes that reported stats before but stopped
      auto now = std::chrono::system_clock::now();

      // Mark nodes that successfully reported stats this cycle
      set<Address> reporting_nodes;
      auto mark_reporting = [&](const OccupancyStats &occupancy) {
        for (const auto &node_pair : occupancy) {
          reporting_nodes.insert(node_pair.first);
          last_epoch_change[node_pair.first] = now;
        }
      };
      mark_reporting(memory_occupancy);
      mark_reporting(disk_occupancy);

      // Only check nodes that were previously seen but stopped reporting
      vector<Address> dead_nodes;
      for (const auto &epoch_pair : last_epoch_change) {
        if (reporting_nodes.find(epoch_pair.first) == reporting_nodes.end()) {
          auto stale_duration =
              std::chrono::duration_cast<std::chrono::seconds>(
                  now - epoch_pair.second)
                  .count();
          if (stale_duration > kMonitoringThreshold) {
            dead_nodes.push_back(epoch_pair.first);
          }
        }
      }

      for (const Address &node_id : dead_nodes) {
        // node_id is "public_ip/private_ip"
        vector<string> ips;
        split(node_id, '/', ips);
        if (ips.size() == 2) {
          Address pub_ip = ips[0];
          Address priv_ip = ips[1];

          for (const Tier &tier : kAllTiers) {
            if (global_hash_rings[tier].size() > 0) {
              ServerThread st(pub_ip, priv_ip, 0);
              auto servers = global_hash_rings[tier].get_unique_servers();
              if (servers.find(st) != servers.end()) {
                log->info("Detected dead node {}/{} (tier {}), sending depart.",
                          pub_ip, priv_ip, Tier_Name(tier));

                global_hash_rings[tier].remove(pub_ip, priv_ip, 0);

                string msg =
                    "depart:" + Tier_Name(tier) + ":" + pub_ip + ":" + priv_ip;
                for (const string &routing_ip : routing_ips) {
                  kZmqUtil->send_string(
                      msg,
                      &pushers[RoutingThread(routing_ip, 0)
                                   .notify_connect_address()]);
                }
              }
            }
          }
        }
        last_observed_epoch.erase(node_id);
        last_epoch_change.erase(node_id);
      }

      compute_summary_stats(key_access_frequency, memory_storage, disk_storage,
                            memory_occupancy, disk_occupancy, memory_accesses,
                            disk_accesses, key_access_summary, ss, log,
                            server_monitoring_epoch);

      collect_external_stats(user_latency, user_throughput, ss, log);

      // initialize replication factor for new keys
      for (const auto &key_access_pair : key_access_summary) {
        Key key = key_access_pair.first;
        if (!is_metadata(key) &&
            key_replication_map.find(key) == key_replication_map.end()) {
          init_replication(key_replication_map, key);
        }
      }

      storage_policy(log, global_hash_rings, grace_start, ss, memory_node_count,
                     disk_node_count, new_memory_count, new_disk_count,
                     removing_disk_node, scaling_alert_ip, mt,
                     departing_node_map, pushers);

      movement_policy(log, global_hash_rings, local_hash_rings, grace_start, ss,
                      memory_node_count, disk_node_count, new_memory_count,
                      new_disk_count, scaling_alert_ip, key_replication_map,
                      key_access_summary, key_size, mt, pushers,
                      response_puller, routing_ips, rid);

      slo_policy(log, global_hash_rings, local_hash_rings, grace_start, ss,
                 memory_node_count, new_memory_count, removing_memory_node,
                 scaling_alert_ip, key_replication_map, key_access_summary, mt,
                 departing_node_map, pushers, response_puller, routing_ips, rid,
                 latency_miss_ratio_map);

      // Clear feedback maps after all policies have consumed them
      user_latency.clear();
      user_throughput.clear();
      latency_miss_ratio_map.clear();

      report_start = std::chrono::system_clock::now();
    }
   } catch (const zmq::error_t &e) { // LCOV_EXCL_START
     if (e.num() == EINTR && shutdown_requested.load()) {
       break;
     }
     throw;
   } // LCOV_EXCL_STOP
  }
}
