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

#include "kvs/kvs_handlers.hpp"

void send_gossip(AddressKeysetMap &addr_keyset_map, SocketCache &pushers,
                 SerializerMap &serializers,
                 map<Key, KeyProperty> &stored_key_map) {
  map<Address, kvs::KeyRequest> gossip_map;

  for (const auto &key_pair : addr_keyset_map) {
    string address = key_pair.first;
    kvs::RequestType type;
    RequestType_Parse("PUT", &type);
    gossip_map[address].set_type(type);

    for (const auto &key : key_pair.second) {
      kvs::LatticeType type;
      if (stored_key_map.find(key) == stored_key_map.end()) {
        // we don't have this key stored, so skip
        continue;
      } else {
        type = stored_key_map[key].type();
      }

      auto res = process_get(key, serializers[type]);

      if (res.second == 0) {
        uint64_t expiry_ms = stored_key_map[key].expiry_epoch_s_ > 0
            ? static_cast<uint64_t>(stored_key_map[key].expiry_epoch_s_) * 1000
            : 0;
        prepare_put_tuple(gossip_map[address], key, type, res.first, expiry_ms);
      }
    }
  }

  // send gossip
  for (const auto &gossip_pair : gossip_map) {
    string serialized;
    gossip_pair.second.SerializeToString(&serialized);
    kZmqUtil->send_string(serialized, &pushers[gossip_pair.first]);
  }
}

std::pair<string, kvs::AnnaError> process_get(const Key &key,
                                         Serializer *serializer) {
  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  auto res = serializer->get(key, error);
  return std::pair<string, kvs::AnnaError>(std::move(res), error);
}



bool process_put(const Key &key, kvs::LatticeType lattice_type,
                 const string &payload, Serializer *serializer,
                 map<Key, KeyProperty> &stored_key_map,
                 uint64_t expiry_epoch_ms) {
  int result = serializer->put(key, payload);
  if (result < 0) {
    spdlog::error("Failed to put key {}", key);
    return false;
  }
  stored_key_map[key].set_size(static_cast<unsigned>(result));
  stored_key_map[key].set_type(lattice_type);

  // Set expiry based on client TTL or tombstone.
  if (expiry_epoch_ms > 0) {
    // Client-specified absolute expiry (milliseconds -> seconds).
    stored_key_map[key].expiry_epoch_s_ =
        static_cast<uint32_t>(expiry_epoch_ms / 1000);
  } else if (result == 0 && kTombstoneGcMultiplier > 0) {
    // Tombstone (delete = PUT of empty value): expire after gc_threshold.
    // Always set (overrides any previous TTL expiry), but don't reset if
    // already a tombstone (re-gossip of same delete).
    unsigned gc_threshold_s = (kGossipPeriod / 1000000) * kTombstoneGcMultiplier;
    uint32_t tombstone_expiry = now_epoch_s() + gc_threshold_s;
    if (stored_key_map[key].expiry_epoch_s_ == 0 ||
        stored_key_map[key].size() > 0) {
      // First tombstone or transition from non-empty: set expiry.
      stored_key_map[key].expiry_epoch_s_ = tombstone_expiry;
    }
  } else if (result > 0 && expiry_epoch_ms == 0) {
    // Non-empty value with no expiry: clear any previous expiry.
    stored_key_map[key].expiry_epoch_s_ = 0;
  }

  return true;
}

bool is_primary_replica(const Key &key,
                        map<Key, KeyReplication> &key_replication_map,
                        GlobalRingMap &global_hash_rings,
                        LocalRingMap &local_hash_rings, ServerThread &st) {
  if (key_replication_map[key].global_replication_[kSelfTier] == 0) {
    return false;
  } else {
    if (kSelfTier > Tier::MEMORY) {
      bool has_upper_tier_replica = false;
      for (const Tier &tier : kAllTiers) {
        if (tier < kSelfTier &&
            key_replication_map[key].global_replication_[tier] > 0) {
          has_upper_tier_replica = true;
        }
      }
      if (has_upper_tier_replica) {
        return false;
      }
    }

    auto global_pos = global_hash_rings[kSelfTier].find(key);
    if (global_pos != global_hash_rings[kSelfTier].end() &&
        st.private_ip().compare(global_pos->second.private_ip()) == 0) {
      auto local_pos = local_hash_rings[kSelfTier].find(key);

      if (local_pos != local_hash_rings[kSelfTier].end() &&
          st.tid() == local_pos->second.tid()) {
        return true;
      }
    }

    return false;
  }
}
