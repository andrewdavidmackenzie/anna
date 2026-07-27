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

#include "monitor/monitoring_utils.hpp"
#include "monitor/policies.hpp"

void storage_policy(logger log, GlobalRingMap &global_hash_rings,
                    TimePoint &grace_start, SummaryStats &ss,
                    unsigned &memory_node_count, unsigned &disk_node_count,
                    unsigned &new_memory_count, unsigned &new_disk_count,
                    bool &removing_disk_node, Address management_ip,
                    MonitoringThread &mt,
                    map<Address, unsigned> &departing_node_map,
                    SocketCache &pushers) {
  // check storage consumption and trigger elasticity if necessary
  if (kEnableElasticity) {
    if (new_memory_count == 0 && ss.required_memory_node > memory_node_count) {
      auto time_elapsed = std::chrono::duration_cast<std::chrono::seconds>(
                              std::chrono::system_clock::now() - grace_start)
                              .count();
      if (time_elapsed > kGracePeriod) {
        add_node(log, "memory", kNodeAdditionBatchSize, new_memory_count,
                 pushers, management_ip);
      }
    }

    if (kEnableTiering && new_disk_count == 0 &&
        ss.required_disk_node > disk_node_count) {
      auto time_elapsed = std::chrono::duration_cast<std::chrono::seconds>(
                              std::chrono::system_clock::now() - grace_start)
                              .count();
      if (time_elapsed > kGracePeriod) {
        add_node(log, "disk", kNodeAdditionBatchSize, new_disk_count, pushers,
                 management_ip);
      }
    }

    if (kEnableTiering &&
        ss.avg_disk_consumption_percentage < kMinDiskNodeConsumption &&
        !removing_disk_node &&
        disk_node_count >
            std::max(ss.required_disk_node, (unsigned)kMinDiskTierSize)) {
      auto time_elapsed = std::chrono::duration_cast<std::chrono::seconds>(
                              std::chrono::system_clock::now() - grace_start)
                              .count();

      if (time_elapsed > kGracePeriod) {
        // pick a random disk node and send remove node command
        auto node = next(global_hash_rings[Tier::DISK].begin(),
                         rand() % global_hash_rings[Tier::DISK].size())
                        ->second;
        remove_node(log, node, "disk", removing_disk_node, pushers,
                    departing_node_map, mt);
      }
    }
  }
}
