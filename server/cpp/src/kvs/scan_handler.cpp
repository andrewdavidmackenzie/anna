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

static const unsigned kDefaultScanCount = 100;
static const unsigned kMaxScanCount = 10000;

void scan_handler(string &serialized, logger log,
                  map<Key, KeyProperty> &stored_key_map,
                  SocketCache &pushers) {
  kvs::KeyRequest request;
  request.ParseFromString(serialized);

  kvs::KeyResponse response;
  response.set_response_id(request.request_id());
  response.set_type(kvs::RequestType::SCAN);

  string prefix = request.scan_prefix();
  uint64_t cursor = request.scan_cursor();
  unsigned count = request.scan_count();
  if (count == 0) count = kDefaultScanCount;
  if (count > kMaxScanCount) count = kMaxScanCount;

  response.set_scan_total_keys(stored_key_map.size());

  // Iterate from begin(), skip `cursor` entries, collect up to `count`
  // keys matching the prefix.
  uint64_t index = 0;
  unsigned collected = 0;
  auto it = stored_key_map.begin();

  // Skip to cursor position.
  while (it != stored_key_map.end() && index < cursor) {
    ++it;
    ++index;
  }

  // Collect matching keys.
  while (it != stored_key_map.end() && collected < count) {
    const Key &key = it->first;
    const KeyProperty &prop = it->second;

    // Skip metadata keys — they are internal and should not be exposed
    // to client scans.
    if (!is_metadata(key)) {
      if (prefix.empty() || key.substr(0, prefix.size()) == prefix) {
        auto *entry = response.add_scan_keys();
        entry->set_key(key);
        entry->set_lattice_type(prop.type());
        entry->set_size(prop.size());
        entry->set_expiry_epoch_s(prop.expiry_epoch_s_);
        collected++;
      }
    }

    ++it;
    ++index;
  }

  // Set next cursor: 0 means done, otherwise the index to resume from.
  if (it == stored_key_map.end()) {
    response.set_scan_next_cursor(0);
  } else {
    response.set_scan_next_cursor(index);
  }

  string serialized_response;
  response.SerializeToString(&serialized_response);
  kZmqUtil->send_string(serialized_response,
                        &pushers[request.response_address()]);
}
