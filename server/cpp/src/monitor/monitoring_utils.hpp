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

#ifndef KVS_INCLUDE_MONITOR_MONITORING_UTILS_HPP_
#define KVS_INCLUDE_MONITOR_MONITORING_UTILS_HPP_

#include "hash_ring/hash_ring.hpp"
#include "metadata.pb.h"
#include "requests.hpp"

inline unsigned kMonitoringThreshold = 30;
inline unsigned kGracePeriod = 120;

// the default number of nodes to add concurrently for storage
inline unsigned kNodeAdditionBatchSize = 2;

// define capacity for both tiers
inline double kMaxMemoryNodeConsumption = 0.6;
inline double kMinMemoryNodeConsumption = 0.3;
inline double kMaxDiskNodeConsumption = 0.75;
inline double kMinDiskNodeConsumption = 0.5;

// define threshold for promotion/demotion
inline unsigned kKeyPromotionThreshold = 0;
inline unsigned kKeyDemotionThreshold = 1;

// define minimum number of nodes for each tier
inline unsigned kMinMemoryTierSize = 1;
inline unsigned kMinDiskTierSize = 0;

// value size in KB
inline unsigned kValueSize = 256;

// SLO occupancy thresholds: min occupancy to trigger node addition (upper)
// and max occupancy to trigger node removal (lower)
inline double kSloOccupancyUpper = 0.15;
inline double kSloOccupancyLower = 0.05;

struct SummaryStats {
  void clear() {
    key_access_mean = 0;
    key_access_std = 0;
    total_memory_access = 0;
    total_disk_access = 0;
    total_memory_consumption = 0;
    total_disk_consumption = 0;
    max_memory_consumption_percentage = 0;
    max_disk_consumption_percentage = 0;
    avg_memory_consumption_percentage = 0;
    avg_disk_consumption_percentage = 0;
    required_memory_node = 0;
    required_disk_node = 0;
    max_memory_occupancy = 0;
    min_memory_occupancy = 1;
    avg_memory_occupancy = 0;
    max_disk_occupancy = 0;
    min_disk_occupancy = 1;
    avg_disk_occupancy = 0;
    min_occupancy_memory_public_ip = Address();
    min_occupancy_memory_private_ip = Address();
    avg_latency = 0;
    total_throughput = 0;
  }

  SummaryStats() { clear(); }
  double key_access_mean;
  double key_access_std;
  unsigned total_memory_access;
  unsigned total_disk_access;
  unsigned long long total_memory_consumption;
  unsigned long long total_disk_consumption;
  double max_memory_consumption_percentage;
  double max_disk_consumption_percentage;
  double avg_memory_consumption_percentage;
  double avg_disk_consumption_percentage;
  unsigned required_memory_node;
  unsigned required_disk_node;
  double max_memory_occupancy;
  double min_memory_occupancy;
  double avg_memory_occupancy;
  double max_disk_occupancy;
  double min_disk_occupancy;
  double avg_disk_occupancy;
  Address min_occupancy_memory_public_ip;
  Address min_occupancy_memory_private_ip;
  double avg_latency;
  double total_throughput;
};

void collect_internal_stats(
    GlobalRingMap &global_hash_rings, LocalRingMap &local_hash_rings,
    SocketCache &pushers, MonitoringThread &mt, zmq::socket_t &response_puller,
    logger log, unsigned &rid,
    map<Key, map<Address, unsigned>> &key_access_frequency,
    map<Key, unsigned> &key_size, StorageStats &memory_storage,
    StorageStats &disk_storage, OccupancyStats &memory_occupancy,
    OccupancyStats &disk_occupancy, AccessStats &memory_access,
    AccessStats &disk_access);

void compute_summary_stats(
    map<Key, map<Address, unsigned>> &key_access_frequency,
    StorageStats &memory_storage, StorageStats &disk_storage,
    OccupancyStats &memory_occupancy, OccupancyStats &disk_occupancy,
    AccessStats &memory_access, AccessStats &disk_access,
    map<Key, unsigned> &key_access_summary, SummaryStats &ss, logger log,
    unsigned &server_monitoring_epoch);

void collect_external_stats(map<string, double> &user_latency,
                            map<string, double> &user_throughput,
                            SummaryStats &ss, logger log);

KeyReplication create_new_replication_vector(unsigned gm, unsigned ge,
                                             unsigned lm, unsigned le);

void prepare_replication_factor_update(
    const Key &key,
    map<Address, ReplicationFactorUpdate> &replication_factor_map,
    Address server_address, map<Key, KeyReplication> &key_replication_map);

void change_replication_factor(map<Key, KeyReplication> &requests,
                               GlobalRingMap &global_hash_rings,
                               LocalRingMap &local_hash_rings,
                               vector<Address> &routing_ips,
                               map<Key, KeyReplication> &key_replication_map,
                               SocketCache &pushers, MonitoringThread &mt,
                               zmq::socket_t &response_puller, logger log,
                               unsigned &rid);

void add_node(logger log, string tier, unsigned number, unsigned &adding,
              SocketCache &pushers, const Address &management_ip);

void remove_node(logger log, ServerThread &node, string tier,
                 bool &removing_flag, SocketCache &pushers,
                 map<Address, unsigned> &departing_node_map,
                 MonitoringThread &mt);

#endif // KVS_INCLUDE_MONITOR_MONITORING_UTILS_HPP_
