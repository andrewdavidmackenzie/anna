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

void cache_registration_handler(string &serialized,
                                set<Address> &extant_caches,
                                map<Address, set<Key>> &cache_ip_to_keys,
                                map<Key, set<Address>> &key_to_cache_ips,
                                logger log) {
  shared::StringSet msg;
  msg.ParseFromString(serialized);

  if (msg.keys_size() < 1) {
    log->error("Cache registration message with no cache IP.");
    return;
  }

  Address cache_ip = msg.keys(0);
  extant_caches.insert(cache_ip);

  for (int i = 1; i < msg.keys_size(); i++) {
    Key key = msg.keys(i);
    cache_ip_to_keys[cache_ip].insert(key);
    key_to_cache_ips[key].insert(cache_ip);
  }

  log->info("Registered cache {} watching {} keys.", cache_ip,
            std::to_string(msg.keys_size() - 1));
}
