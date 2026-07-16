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

#ifndef INCLUDE_CLIENT_LIB_HPP_
#define INCLUDE_CLIENT_LIB_HPP_

#include <memory>

#include "kvs_client.hpp"

// This header (together with client_lib.cpp) is the "library" half of the
// C++ client: it wraps KvsClientInterface with the KVS operations (get, put,
// ...) and process management (start, stop, status), with no dependency on
// stdin/stdout or argv. `cli.cpp` is a thin example CLI built on top of this
// library -- see issue #75.
namespace annalib {

// The set of configuration needed to construct a KvsClient: this client's
// own IP address, and the set of routing threads it should talk to.
struct ClientConfig {
  vector<UserRoutingThread> routing_threads;
  Address ip;
};

// Parse a YAML anna config file (the same format used by the server) into a
// ClientConfig. Throws a YAML::Exception if the file is missing or
// malformed.
ClientConfig load_config(const string& config_file_path);

// Construct a KvsClient connected to the routing tier described by `config`.
std::unique_ptr<KvsClient> make_client(const ClientConfig& config,
                                        unsigned tid = 0,
                                        unsigned timeout = 10000);

// Issue a blocking GET for `key` under the default (LWW) lattice type and
// return its value.
string get(KvsClientInterface* client, const string& key);

// The result of a causal GET: the value plus the vector clock and
// dependencies attached to it, for the caller to display/inspect as needed.
struct CausalValue {
  string value;
  vector<pair<string, unsigned>> vector_clock;
  map<string, vector<pair<string, unsigned>>> dependencies;
};

// The result of a single-key-causal GET: the value plus the vector clock
// (no dependencies, unlike multi-key-causal).
struct SingleCausalValue {
  string value;
  vector<pair<string, unsigned>> vector_clock;
};

// The result of a priority GET: the value plus its priority.
struct PriorityResult {
  double priority;
  string value;
};

// Issue a blocking GET for `key` under the multi-key-causal lattice type.
CausalValue get_causal(KvsClientInterface* client, const string& key);

// Delete a key by writing an empty LWW value with a dominating timestamp.
kvs::KeyResponse del(KvsClientInterface* client, const string& key);

// Issue a blocking PUT of `value` for `key` under the default (LWW) lattice
// type.
kvs::KeyResponse put(KvsClientInterface* client, const string& key,
                     const string& value);

// Delete a key by writing an empty LWW value with a dominating timestamp.
kvs::KeyResponse del(KvsClientInterface* client, const string& key);

// Issue a blocking PUT of `value` for `key` under the multi-key-causal
// lattice type.
kvs::KeyResponse put_causal(KvsClientInterface* client, const string& key,
                            const string& value);

// Issue a blocking PUT of `values` for `key` under the set lattice type.
kvs::KeyResponse put_set(KvsClientInterface* client, const string& key,
                         const set<string>& values);

// Issue a blocking GET for `key` under the set lattice type.
set<string> get_set(KvsClientInterface* client, const string& key);

// Issue a blocking PUT of `values` for `key` under the ordered-set lattice
// type. Same serialization as SET, but the server preserves insertion order.
kvs::KeyResponse put_ordered_set(KvsClientInterface* client, const string& key,
                                 const set<string>& values);

// Issue a blocking GET for `key` under the ordered-set lattice type.
set<string> get_ordered_set(KvsClientInterface* client, const string& key);

// Delete a key by writing an empty LWW value with a dominating timestamp.
kvs::KeyResponse del(KvsClientInterface* client, const string& key);

// Issue a blocking PUT of `value` for `key` under the single-key-causal
// lattice type.
kvs::KeyResponse put_single_causal(KvsClientInterface* client,
                                   const string& key, const string& value);

// Issue a blocking GET for `key` under the single-key-causal lattice type.
SingleCausalValue get_single_causal(KvsClientInterface* client,
                                    const string& key);

// Delete a key by writing an empty LWW value with a dominating timestamp.
kvs::KeyResponse del(KvsClientInterface* client, const string& key);

// Issue a blocking PUT of `value` with `priority` for `key` under the
// priority lattice type (lower priority value wins).
kvs::KeyResponse put_priority(KvsClientInterface* client, const string& key,
                              double priority, const string& value);

// Issue a blocking GET for `key` under the priority lattice type.
PriorityResult get_priority(KvsClientInterface* client, const string& key);

// The anna server processes managed by start()/stop()/status().
extern const vector<string> kProcessList;

// Start the anna server processes described by the config file at
// `config_file_path`. Skips processes that are already running.
// Returns the number of processes started.
int start(const string& config_file_path);

// Return the names of currently running anna server processes.
vector<string> status();

// Stop all running anna server processes via SIGTERM.
// Returns the number of processes killed.
int stop();

}  // namespace annalib

#endif  // INCLUDE_CLIENT_LIB_HPP_
