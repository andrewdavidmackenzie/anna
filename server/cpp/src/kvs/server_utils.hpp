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

#ifndef INCLUDE_KVS_SERVER_UTILS_HPP_
#define INCLUDE_KVS_SERVER_UTILS_HPP_

#include <fcntl.h>
#include <fstream>
#include <string>
#include <unistd.h> // for ftruncate(2)

#include "base_kv_store.hpp"
#include "common.hpp"
#include "kvs/kvs_common.hpp"
#include "lattices/lww_pair_lattice.hpp"
#include "yaml-cpp/yaml.h"

// Gossip period in microseconds (default 10 seconds)
inline unsigned kGossipPeriod = 10000000;

// Tombstone GC: multiplier of the gossip period. Tombstoned keys (size_ == 0)
// are reaped after gossip_epoch * tombstone_gc_multiplier seconds. This must
// be long enough for all replicas to receive the tombstone via gossip.
// Default 30 means 30 * 10s = 5 minutes with the default gossip epoch.
// Set to 0 to disable tombstone GC.
inline unsigned kTombstoneGcMultiplier = 30;

// Max keys redistributed per gossip batch
inline unsigned kDataRedistributeThreshold = 50;

// Memory garbage collection trigger threshold
inline unsigned kGarbageCollectThreshold = 10000000;

typedef KVStore<Key, LWWPairLattice<string>> MemoryLWWKVS;
typedef KVStore<Key, SetLattice<string>> MemorySetKVS;
typedef KVStore<Key, OrderedSetLattice<string>> MemoryOrderedSetKVS;
typedef KVStore<Key, SingleKeyCausalLattice<SetLattice<string>>>
    MemorySingleKeyCausalKVS;
typedef KVStore<Key, MultiKeyCausalLattice<SetLattice<string>>>
    MemoryMultiKeyCausalKVS;
typedef KVStore<Key, PriorityLattice<double, string>> MemoryPriorityKVS;

// a map that represents which keys should be sent to which IP-port combinations
typedef map<Address, set<Key>> AddressKeysetMap;

class Serializer {
public:
  virtual string get(const Key &key, kvs::AnnaError &error) = 0;
  virtual int put(const Key &key, const string &serialized) = 0;
  virtual bool remove(const Key &key) = 0;
  virtual ~Serializer(){};
};

class MemoryLWWSerializer : public Serializer {
  MemoryLWWKVS *kvs_;

public:
  MemoryLWWSerializer(MemoryLWWKVS *kvs) : kvs_(kvs) {}

  string get(const Key &key, kvs::AnnaError &error) {
    auto val = kvs_->get(key, error);

    if (val.reveal().value == "") {
      error = kvs::AnnaError::KEY_DNE;
    }

    return serialize(val);
  }

  int put(const Key &key, const string &serialized) {
    LWWPairLattice<string> val = deserialize_lww(serialized);
    kvs_->put(key, val);
    return static_cast<int>(kvs_->size(key));
  }

  bool remove(const Key &key) {
    kvs_->remove(key);
    return true;
  }
};

class MemorySetSerializer : public Serializer {
  MemorySetKVS *kvs_;

public:
  MemorySetSerializer(MemorySetKVS *kvs) : kvs_(kvs) {}

  string get(const Key &key, kvs::AnnaError &error) {
    auto val = kvs_->get(key, error);
    if (val.size().reveal() == 0) {
      error = kvs::AnnaError::KEY_DNE;
    }
    return serialize(val);
  }

  int put(const Key &key, const string &serialized) {
    SetLattice<string> sl = deserialize_set(serialized);
    kvs_->put(key, sl);
    return static_cast<int>(kvs_->size(key));
  }

  bool remove(const Key &key) {
    kvs_->remove(key);
    return true;
  }
};

class MemoryOrderedSetSerializer : public Serializer {
  MemoryOrderedSetKVS *kvs_;

public:
  MemoryOrderedSetSerializer(MemoryOrderedSetKVS *kvs) : kvs_(kvs) {}

  string get(const Key &key, kvs::AnnaError &error) {
    auto val = kvs_->get(key, error);
    return serialize(val);
  }

  int put(const Key &key, const string &serialized) {
    OrderedSetLattice<string> sl = deserialize_ordered_set(serialized);
    kvs_->put(key, sl);
    return static_cast<int>(kvs_->size(key));
  }

  bool remove(const Key &key) {
    kvs_->remove(key);
    return true;
  }
};

class MemorySingleKeyCausalSerializer : public Serializer {
  MemorySingleKeyCausalKVS *kvs_;

public:
  MemorySingleKeyCausalSerializer(MemorySingleKeyCausalKVS *kvs) : kvs_(kvs) {}

  string get(const Key &key, kvs::AnnaError &error) {
    auto val = kvs_->get(key, error);
    if (val.reveal().value.size().reveal() == 0) {
      error = kvs::AnnaError::KEY_DNE;
    }
    return serialize(val);
  }

  int put(const Key &key, const string &serialized) {
    kvs::SingleKeyCausalValue causal_value = deserialize_causal(serialized);
    VectorClockValuePair<SetLattice<string>> p =
        to_vector_clock_value_pair(causal_value);
    kvs_->put(key, SingleKeyCausalLattice<SetLattice<string>>(p));
    return static_cast<int>(kvs_->size(key));
  }

  bool remove(const Key &key) {
    kvs_->remove(key);
    return true;
  }
};

class MemoryMultiKeyCausalSerializer : public Serializer {
  MemoryMultiKeyCausalKVS *kvs_;

public:
  MemoryMultiKeyCausalSerializer(MemoryMultiKeyCausalKVS *kvs) : kvs_(kvs) {}

  string get(const Key &key, kvs::AnnaError &error) {
    auto val = kvs_->get(key, error);
    if (val.reveal().value.size().reveal() == 0) {
      error = kvs::AnnaError::KEY_DNE;
    }
    return serialize(val);
  }

  int put(const Key &key, const string &serialized) {
    kvs::MultiKeyCausalValue multi_key_causal_value =
        deserialize_multi_key_causal(serialized);
    MultiKeyCausalPayload<SetLattice<string>> p =
        to_multi_key_causal_payload(multi_key_causal_value);
    kvs_->put(key, MultiKeyCausalLattice<SetLattice<string>>(p));
    return static_cast<int>(kvs_->size(key));
  }

  bool remove(const Key &key) {
    kvs_->remove(key);
    return true;
  }
};

class MemoryPrioritySerializer : public Serializer {
  MemoryPriorityKVS *kvs_;

public:
  MemoryPrioritySerializer(MemoryPriorityKVS *kvs) : kvs_(kvs) {}

  string get(const Key &key, kvs::AnnaError &error) {
    auto val = kvs_->get(key, error);
    if (val.reveal().value == "") {
      error = kvs::AnnaError::KEY_DNE;
    }
    return serialize(val);
  }

  int put(const Key &key, const string &serialized) {
    PriorityLattice<double, string> val = deserialize_priority(serialized);
    kvs_->put(key, val);
    return static_cast<int>(kvs_->size(key));
  }

  bool remove(const Key &key) {
    kvs_->remove(key);
    return true;
  }
};

// Common disk I/O helpers shared by all disk serializers.
// Each serializer stores protobuf values as files at:
//   <ebs_root>/ebs_<tid>/<key>

inline string disk_fname(const string &ebs_root, unsigned tid, const Key &key) {
  return ebs_root + "ebs_" + std::to_string(tid) + "/" + key;
}

template <typename T>
inline bool disk_read(const string &path, T &value, kvs::AnnaError &error) {
  std::fstream input(path, std::ios::in | std::ios::binary);
  if (!input) {
    error = kvs::AnnaError::KEY_DNE;
    return false;
  }
  if (!value.ParseFromIstream(&input)) {
    spdlog::error("Failed to parse payload from {}", path);
    error = kvs::AnnaError::KEY_DNE;
    return false;
  }
  return true;
}

template <typename T>
inline int disk_write(const string &path, const T &value) {
  std::fstream output(path, std::ios::out | std::ios::trunc | std::ios::binary);
  if (!output) {
    spdlog::error("Failed to open file for writing: {}", path);
    return -1;
  }
  if (!value.SerializeToOstream(&output)) {
    spdlog::error("Failed to serialize payload to {}", path);
    return -1;
  }
  output.flush();
  if (output.fail()) {
    spdlog::error("Failed to flush payload to {}", path);
    return -1;
  }
  auto pos = output.tellp();
  if (pos == std::streampos(-1)) {
    spdlog::error("Failed to determine write position for {}", path);
    return -1;
  }
  return static_cast<int>(pos);
}

inline bool disk_remove(const string &path) {
  if (std::remove(path.c_str()) != 0) {
    spdlog::error("Error deleting file: {}", path);
    return false;
  }
  return true;
}

class DiskLWWSerializer : public Serializer {
  unsigned tid_;
  string ebs_root_;

public:
  DiskLWWSerializer(unsigned &tid, string ebs_root) : tid_(tid) {
    ebs_root_ = ebs_root;
    if (ebs_root_.back() != '/') {
      ebs_root_ += "/";
    }
  }

  string get(const Key &key, kvs::AnnaError &error) {
    string res;
    kvs::LWWValue value;
    if (disk_read(disk_fname(ebs_root_, tid_, key), value, error)) {
      if (value.value() == "") {
        error = kvs::AnnaError::KEY_DNE;
      } else {
        value.SerializeToString(&res);
      }
    }
    return res;
  }

  int put(const Key &key, const string &serialized) {
    kvs::LWWValue input_value;
    input_value.ParseFromString(serialized);

    string path = disk_fname(ebs_root_, tid_, key);
    kvs::LWWValue original_value;
    kvs::AnnaError error = kvs::AnnaError::NO_ERROR;

    if (!disk_read(path, original_value, error)) {
      return disk_write(path, input_value);
    } else if (input_value.timestamp() >= original_value.timestamp()) {
      return disk_write(path, input_value);
    } else {
      std::fstream input(path, std::ios::in | std::ios::binary);
      input.seekg(0, std::ios::end);
      return static_cast<int>(input.tellg());
    }
  }

  bool remove(const Key &key) {
    return disk_remove(disk_fname(ebs_root_, tid_, key));
  }
};

class DiskSetSerializer : public Serializer {
  unsigned tid_;
  string ebs_root_;

public:
  DiskSetSerializer(unsigned &tid, string ebs_root) : tid_(tid) {
    ebs_root_ = ebs_root;
    if (ebs_root_.back() != '/') ebs_root_ += "/";
  }

  string get(const Key &key, kvs::AnnaError &error) {
    string res;
    kvs::SetValue value;
    if (disk_read(disk_fname(ebs_root_, tid_, key), value, error)) {
      if (value.values_size() == 0) {
        error = kvs::AnnaError::KEY_DNE;
      } else {
        value.SerializeToString(&res);
      }
    }
    return res;
  }

  int put(const Key &key, const string &serialized) {
    kvs::SetValue input_value;
    input_value.ParseFromString(serialized);

    string path = disk_fname(ebs_root_, tid_, key);
    kvs::SetValue original_value;
    kvs::AnnaError error = kvs::AnnaError::NO_ERROR;

    if (!disk_read(path, original_value, error)) {
      return disk_write(path, input_value);
    } else {
      set<string> set_union;
      for (auto &val : original_value.values()) {
        set_union.emplace(std::move(val));
      }
      for (auto &val : input_value.values()) {
        set_union.emplace(std::move(val));
      }
      kvs::SetValue new_value;
      for (auto &val : set_union) {
        new_value.add_values(std::move(val));
      }
      return disk_write(path, new_value);
    }
  }

  bool remove(const Key &key) {
    return disk_remove(disk_fname(ebs_root_, tid_, key));
  }
};

class DiskOrderedSetSerializer : public Serializer {
  unsigned tid_;
  string ebs_root_;

public:
  DiskOrderedSetSerializer(unsigned &tid, string ebs_root) : tid_(tid) {
    ebs_root_ = ebs_root;
    if (ebs_root_.back() != '/') ebs_root_ += "/";
  }

  string get(const Key &key, kvs::AnnaError &error) {
    string res;
    kvs::SetValue value;
    if (disk_read(disk_fname(ebs_root_, tid_, key), value, error)) {
      value.SerializeToString(&res);
    }
    return res;
  }

  int put(const Key &key, const string &serialized) {
    kvs::SetValue input_value;
    input_value.ParseFromString(serialized);

    string path = disk_fname(ebs_root_, tid_, key);
    kvs::SetValue original_value;
    kvs::AnnaError error = kvs::AnnaError::NO_ERROR;

    if (!disk_read(path, original_value, error)) {
      return disk_write(path, input_value);
    } else {
      ordered_set<string> set_union;
      for (auto &val : original_value.values()) {
        set_union.emplace(std::move(val));
      }
      for (auto &val : input_value.values()) {
        set_union.emplace(std::move(val));
      }
      kvs::SetValue new_value;
      for (auto &val : set_union) {
        new_value.add_values(std::move(val));
      }
      return disk_write(path, new_value);
    }
  }

  bool remove(const Key &key) {
    return disk_remove(disk_fname(ebs_root_, tid_, key));
  }
};

class DiskSingleKeyCausalSerializer : public Serializer {
  unsigned tid_;
  string ebs_root_;

public:
  DiskSingleKeyCausalSerializer(unsigned &tid, string ebs_root) : tid_(tid) {
    ebs_root_ = ebs_root;
    if (ebs_root_.back() != '/') ebs_root_ += "/";
  }

  string get(const Key &key, kvs::AnnaError &error) {
    string res;
    kvs::SingleKeyCausalValue value;
    if (disk_read(disk_fname(ebs_root_, tid_, key), value, error)) {
      if (value.values_size() == 0) {
        error = kvs::AnnaError::KEY_DNE;
      } else {
        value.SerializeToString(&res);
      }
    }
    return res;
  }

  int put(const Key &key, const string &serialized) {
    kvs::SingleKeyCausalValue input_value;
    input_value.ParseFromString(serialized);

    string path = disk_fname(ebs_root_, tid_, key);
    kvs::SingleKeyCausalValue original_value;
    kvs::AnnaError error = kvs::AnnaError::NO_ERROR;

    if (!disk_read(path, original_value, error)) {
      return disk_write(path, input_value);
    } else {
      // get the existing value that we have and merge
      VectorClockValuePair<SetLattice<string>> orig_pair;
      for (const auto &pair : original_value.vector_clock()) {
        orig_pair.vector_clock.insert(pair.first, pair.second);
      }
      for (auto &val : original_value.values()) {
        orig_pair.value.insert(std::move(val));
      }
      SingleKeyCausalLattice<SetLattice<string>> orig(orig_pair);

      VectorClockValuePair<SetLattice<string>> input_pair;
      for (const auto &pair : input_value.vector_clock()) {
        input_pair.vector_clock.insert(pair.first, pair.second);
      }
      for (auto &val : input_value.values()) {
        input_pair.value.insert(std::move(val));
      }
      SingleKeyCausalLattice<SetLattice<string>> input(input_pair);

      orig.merge(input);

      kvs::SingleKeyCausalValue new_value;
      auto ptr = new_value.mutable_vector_clock();
      // serialize vector clock
      for (const auto &pair : orig.reveal().vector_clock.reveal()) {
        (*ptr)[pair.first] = pair.second.reveal();
      }

      // serialize values
      // note that this creates unnecessary copy of val, but
      // we have to since the reveal() method is marked as "const"
      for (const string &val : orig.reveal().value.reveal()) {
        new_value.add_values(val);
      }

      return disk_write(path, new_value);
    }
  }

  bool remove(const Key &key) {
    return disk_remove(disk_fname(ebs_root_, tid_, key));
  }
};

class DiskMultiKeyCausalSerializer : public Serializer {
  unsigned tid_;
  string ebs_root_;

public:
  DiskMultiKeyCausalSerializer(unsigned &tid, string ebs_root) : tid_(tid) {
    ebs_root_ = ebs_root;
    if (ebs_root_.back() != '/') ebs_root_ += "/";
  }

  string get(const Key &key, kvs::AnnaError &error) {
    string res;
    kvs::MultiKeyCausalValue value;
    if (disk_read(disk_fname(ebs_root_, tid_, key), value, error)) {
      if (value.values_size() == 0) {
        error = kvs::AnnaError::KEY_DNE;
      } else {
        value.SerializeToString(&res);
      }
    }
    return res;
  }

  int put(const Key &key, const string &serialized) {
    kvs::MultiKeyCausalValue input_value;
    input_value.ParseFromString(serialized);

    string path = disk_fname(ebs_root_, tid_, key);
    kvs::MultiKeyCausalValue original_value;
    kvs::AnnaError error = kvs::AnnaError::NO_ERROR;

    if (!disk_read(path, original_value, error)) {
      return disk_write(path, input_value);
    } else {
      // get the existing value that we have and merge
      MultiKeyCausalPayload<SetLattice<string>> orig_payload;
      for (const auto &pair : original_value.vector_clock()) {
        orig_payload.vector_clock.insert(pair.first, pair.second);
      }

      for (const auto &dep : original_value.dependencies()) {
        VectorClock vc;
        for (const auto &pair : dep.vector_clock()) {
          vc.insert(pair.first, pair.second);
        }

        orig_payload.dependencies.insert(dep.key(), vc);
      }

      for (auto &val : original_value.values()) {
        orig_payload.value.insert(std::move(val));
      }

      MultiKeyCausalLattice<SetLattice<string>> orig(orig_payload);
      MultiKeyCausalPayload<SetLattice<string>> input_payload;

      for (const auto &pair : input_value.vector_clock()) {
        input_payload.vector_clock.insert(pair.first, pair.second);
      }

      for (const auto &dep : input_value.dependencies()) {
        VectorClock vc;
        for (const auto &pair : dep.vector_clock()) {
          vc.insert(pair.first, pair.second);
        }
        input_payload.dependencies.insert(dep.key(), vc);
      }

      for (auto &val : input_value.values()) {
        input_payload.value.insert(std::move(val));
      }

      MultiKeyCausalLattice<SetLattice<string>> input(input_payload);
      orig.merge(input);

      kvs::MultiKeyCausalValue new_value;
      auto ptr = new_value.mutable_vector_clock();

      // serialize vector clock
      for (const auto &pair : orig.reveal().vector_clock.reveal()) {
        (*ptr)[pair.first] = pair.second.reveal();
      }

      // serialize dependencies
      for (const auto &pair : orig.reveal().dependencies.reveal()) {
        auto dep = new_value.add_dependencies();
        dep->set_key(pair.first);
        auto vc_ptr = dep->mutable_vector_clock();

        for (const auto &vc_pair : pair.second.reveal()) {
          (*vc_ptr)[vc_pair.first] = vc_pair.second.reveal();
        }
      }

      // note that this creates unnecessary copy of val, but
      // we have to since the reveal() method is marked as "const"
      for (const string &val : orig.reveal().value.reveal()) {
        new_value.add_values(val);
      }

      return disk_write(path, new_value);
    }
  }

  bool remove(const Key &key) {
    return disk_remove(disk_fname(ebs_root_, tid_, key));
  }
};

class DiskPrioritySerializer : public Serializer {
  unsigned tid_;
  string ebs_root_;

  //! Compute the name of the file that stores a value for a given key
  string fname(const Key &key) const {
    return ebs_root_ + "ebs_" + std::to_string(tid_) + "/" + key;
  }

public:
  DiskPrioritySerializer(unsigned tid, string ebs_root) : tid_(tid) {
    ebs_root_ = ebs_root;
    if (ebs_root_.back() != '/') {
      ebs_root_ += "/";
    }
  }

  string get(const Key &key, kvs::AnnaError &error) override {
    string res;
    kvs::PriorityValue value;
    if (disk_read(fname(key), value, error)) {
      if (value.value() == "") {
        error = kvs::AnnaError::KEY_DNE;
      } else {
        value.SerializeToString(&res);
      }
    }
    return res;
  }

  int put(const Key &key, const string &serialized) override {
    kvs::PriorityValue input_value;
    input_value.ParseFromString(serialized);

    kvs::PriorityValue original_value;
    kvs::AnnaError error = kvs::AnnaError::NO_ERROR;

    if (!disk_read(fname(key), original_value, error)) {
      return disk_write(fname(key), input_value);
    } else if (input_value.priority() < original_value.priority()) {
      return disk_write(fname(key), input_value);
    } else {
      std::fstream input(fname(key), std::ios::in | std::ios::binary);
      input.seekg(0, std::ios::end);
      return static_cast<int>(input.tellg());
    }
  }

  bool remove(const Key &key) override {
    return disk_remove(fname(key));
  }
};

using SerializerMap =
    std::unordered_map<kvs::LatticeType, Serializer *, lattice_type_hash>;

struct PendingRequest {
  PendingRequest() {}
  PendingRequest(kvs::RequestType type, kvs::LatticeType lattice_type, string payload,
                 Address addr, string response_id)
      : type_(type), lattice_type_(std::move(lattice_type)),
        payload_(std::move(payload)), addr_(addr), response_id_(response_id) {}

  kvs::RequestType type_;
  kvs::LatticeType lattice_type_;
  string payload_;
  Address addr_;
  string response_id_;
};

struct PendingGossip {
  PendingGossip() {}
  PendingGossip(kvs::LatticeType lattice_type, string payload)
      : lattice_type_(std::move(lattice_type)), payload_(std::move(payload)) {}
  kvs::LatticeType lattice_type_;
  string payload_;
};

#endif // INCLUDE_KVS_SERVER_UTILS_HPP_
