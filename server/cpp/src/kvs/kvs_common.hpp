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

#ifndef KVS_INCLUDE_KVS_COMMON_HPP_
#define KVS_INCLUDE_KVS_COMMON_HPP_

#include "kvs/kvs_types.hpp"
#include "metadata.pb.h"

inline unsigned kMetadataReplicationFactor = 1;
inline unsigned kMetadataLocalReplicationFactor = 1;

inline unsigned kVirtualThreadNum = 3000;

const vector<Tier> kAllTiers = {
    Tier::MEMORY,
    Tier::DISK}; // TODO(vikram): Is there a better way to make this vector?

inline unsigned kSloWorst = 3000;

// run-time constants
extern Tier kSelfTier;
extern vector<Tier> kSelfTierIdVector;

extern unsigned kMemoryNodeCapacity;
extern unsigned kDiskNodeCapacity;

// the number of threads running in this executable
extern unsigned kThreadNum;
extern unsigned kMemoryThreadCount;
extern unsigned kDiskThreadCount;
extern unsigned kRoutingThreadCount;

extern unsigned kDefaultGlobalMemoryReplication;
extern unsigned kDefaultGlobalDiskReplication;
extern unsigned kDefaultLocalReplication;
extern unsigned kMinimumReplicaNumber;

inline void prepare_get_tuple(kvs::KeyRequest &req, Key key,
                              kvs::LatticeType lattice_type) {
  kvs::KeyTuple *tp = req.add_tuples();
  tp->set_key(std::move(key));
  tp->set_lattice_type(std::move(lattice_type));
}

inline void prepare_put_tuple(kvs::KeyRequest &req, Key key,
                              kvs::LatticeType lattice_type, string payload,
                              unsigned ttl_seconds = 0) {
  kvs::KeyTuple *tp = req.add_tuples();
  tp->set_key(std::move(key));
  tp->set_lattice_type(std::move(lattice_type));
  tp->set_payload(std::move(payload));
  if (ttl_seconds > 0) {
    tp->set_ttl_seconds(ttl_seconds);
  }
}

#endif // KVS_INCLUDE_KVS_COMMON_HPP_
