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

#ifndef INCLUDE_CACHE_CLIENT_HPP_
#define INCLUDE_CACHE_CLIENT_HPP_

#include "kvs.pb.h"
#include "shared.pb.h"
#include "types.hpp"

#include <map>
#include <set>
#include <string>
#include <vector>
#include <zmq.hpp>

using std::map;
using std::set;
using std::string;
using std::vector;

const unsigned kClientCacheRegistrationPort = 7200;
const unsigned kClientCacheUpdatePort = 7150;

/// A cache client that receives key updates pushed from the KVS during gossip.
///
/// Registers with KVS server threads to watch specific keys. When those keys
/// are updated, the KVS pushes new values during its gossip epoch.
class CacheClient {
 public:
  CacheClient(const string &server_ip, const string &cache_ip,
              unsigned memory_threads = 1, unsigned offset = 0,
              unsigned tid = 0)
      : server_ip_(server_ip),
        cache_ip_(cache_ip),
        memory_threads_(memory_threads),
        offset_(offset),
        tid_(tid),
        context_(1),
        update_puller_(context_, ZMQ_PULL) {
    string bind_addr = "tcp://" + cache_ip_ + ":" +
                       std::to_string(tid_ + kClientCacheUpdatePort + offset_);
    update_puller_.bind(bind_addr);
  }

  /// Register interest in keys with all KVS server threads.
  void watch(const vector<string> &keys) {
    watched_keys_.insert(watched_keys_.end(), keys.begin(), keys.end());

    shared::StringSet msg;
    msg.add_keys(cache_ip_);
    for (const auto &key : keys) {
      msg.add_keys(key);
    }

    string payload;
    msg.SerializeToString(&payload);

    for (unsigned tid = 0; tid < memory_threads_; tid++) {
      string addr = "tcp://" + server_ip_ + ":" +
                    std::to_string(tid + kClientCacheRegistrationPort + offset_);
      if (push_sockets_.find(addr) == push_sockets_.end()) {
        push_sockets_[addr] =
            std::make_unique<zmq::socket_t>(context_, ZMQ_PUSH);
        push_sockets_[addr]->connect(addr);
      }

      zmq::message_t zmq_msg(payload.size());
      memcpy(zmq_msg.data(), payload.data(), payload.size());
      push_sockets_[addr]->send(zmq_msg, zmq::send_flags::none);
    }
  }

  /// Receive the next update pushed from the KVS.
  /// Returns true if an update was received, false on timeout.
  bool recv_update(string &key_out, string &payload_out,
                   long timeout_ms = 15000) {
    zmq::pollitem_t item = {static_cast<void *>(update_puller_), 0,
                            ZMQ_POLLIN, 0};
    zmq::poll(&item, 1, timeout_ms);

    if (item.revents & ZMQ_POLLIN) {
      zmq::message_t msg;
      auto result = update_puller_.recv(msg);
      if (result) {
        kvs::KeyResponse response;
        response.ParseFromString(
            string(static_cast<char *>(msg.data()), msg.size()));

        for (const auto &tuple : response.tuples()) {
          if (!tuple.payload().empty()) {
            key_out = tuple.key();
            payload_out = tuple.payload();
            local_cache_[key_out] = payload_out;
            return true;
          }
        }
      }
    }
    return false;
  }

  /// Read a value from the local cache.
  bool get_cached(const string &key, string &value_out) const {
    auto it = local_cache_.find(key);
    if (it != local_cache_.end()) {
      value_out = it->second;
      return true;
    }
    return false;
  }

  /// Return the list of watched keys.
  const vector<string> &watched_keys() const { return watched_keys_; }

 private:
  string server_ip_;
  string cache_ip_;
  unsigned memory_threads_;
  unsigned offset_;
  unsigned tid_;
  zmq::context_t context_;
  zmq::socket_t update_puller_;
  map<string, std::unique_ptr<zmq::socket_t>> push_sockets_;
  map<string, string> local_cache_;
  vector<string> watched_keys_;
};

#endif  // INCLUDE_CACHE_CLIENT_HPP_
