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

#ifndef KVS_INCLUDE_HASHERS_HPP_
#define KVS_INCLUDE_HASHERS_HPP_

#include "anna_hashring.h"
#include "kvs/kvs_threads.hpp"
#include <vector>

// Hashers delegate to the Rust anna-hashring library for cross-language
// consistency. All Anna components (Rust server, C++ server, all clients)
// use the same hash function.

struct GlobalHasher {
  uint64_t operator()(const ServerThread &th) {
    // anna_hash_global adds "GLOBAL" prefix internally.
    return anna_hash_global(th.virtual_id().c_str());
  }

  uint64_t operator()(const Key &key) {
    // anna_hash_global adds "GLOBAL" prefix internally.
    return anna_hash_global(key.c_str());
  }

  typedef uint64_t ResultType;
};

struct LocalHasher {
  typedef uint64_t ResultType;

  ResultType operator()(const ServerThread &th) {
    string input = std::to_string(th.tid()) + "_" +
                   std::to_string(th.virtual_num());
    return anna_hash_local(input.c_str());
  }

  ResultType operator()(const Key &key) {
    return anna_hash_local(key.c_str());
  }
};

#endif // KVS_INCLUDE_HASHERS_HPP_
