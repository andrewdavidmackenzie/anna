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
#include <string>
#include <vector>

#include "kvs_client.hpp"
#include "metadata.pb.h"
#include "shared.pb.h"

// This header (together with client_lib.cpp) is the "library" half of the
// C++ client: it wraps KvsClientInterface with the KVS operations (get, put,
// ...) and process management (start, stop, status), with no dependency on
// stdin/stdout or argv. `cli.cpp` is a thin example CLI built on top of this
// library -- see issue #75.
namespace annalib {

// The set of configuration needed to construct a KvsClient: this client's
// own IP address, the routing tier IP addresses, and the number of routing
// threads per IP.
struct ClientConfig {
  std::vector<std::string> routing_ips;
  unsigned routing_thread_count = 1;
  std::string ip;
};

// The result of a PUT or DELETE operation. Callers should check succeeded()
// rather than inspecting protobuf types directly.
struct PutResult {
  bool succeeded() const { return !error; }
  bool error = false;
  std::string response_id;
};

// Construct a KvsClient connected to the routing tier described by `config`.
std::unique_ptr<KvsClient> make_client(const ClientConfig& config,
                                        unsigned tid = 0,
                                        unsigned timeout = 10000);

// Issue a blocking GET for `key` under the default (LWW) lattice type and
// return its value.
string get(KvsClientInterface* client, const string& key);

// Retrieve multiple keys in a single call. Returns a map of key to value
// for all keys successfully retrieved (keys with errors are omitted).
map<string, string> get_multi(KvsClientInterface* client,
                              const vector<string>& keys);

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
  vector<string> values;
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
PutResult del(KvsClientInterface* client, const string& key);

// Issue a blocking PUT of `value` for `key` under the default (LWW) lattice
// type.
PutResult put(KvsClientInterface* client, const string& key,
              const string& value);

// Issue a blocking PUT of `value` for `key` under the multi-key-causal
// lattice type.
PutResult put_causal(KvsClientInterface* client, const string& key,
                     const string& value);

// Issue a blocking PUT of `values` for `key` under the set lattice type.
PutResult put_set(KvsClientInterface* client, const string& key,
                  const set<string>& values);

// Issue a blocking GET for `key` under the set lattice type.
set<string> get_set(KvsClientInterface* client, const string& key);

// Issue a blocking PUT of `values` for `key` under the ordered-set lattice
// type. Same serialization as SET, but the server preserves insertion order.
PutResult put_ordered_set(KvsClientInterface* client, const string& key,
                          const set<string>& values);

// Issue a blocking GET for `key` under the ordered-set lattice type.
vector<string> get_ordered_set(KvsClientInterface* client, const string& key);

// Issue a blocking PUT of `value` for `key` under the single-key-causal
// lattice type.
PutResult put_single_causal(KvsClientInterface* client,
                            const string& key, const string& value);

// Issue a blocking GET for `key` under the single-key-causal lattice type.
SingleCausalValue get_single_causal(KvsClientInterface* client,
                                    const string& key);

// Issue a blocking PUT of `value` with `priority` for `key` under the
// priority lattice type (lower priority value wins).
PutResult put_priority(KvsClientInterface* client, const string& key,
                       double priority, const string& value);

// Issue a blocking GET for `key` under the priority lattice type.
PriorityResult get_priority(KvsClientInterface* client, const string& key);

// Issue a blocking GET for `key` under the default (LWW) lattice type and
// return its raw value bytes.  Functionally identical to get() in C++ (since
// std::string is byte-transparent), but provided for API symmetry with the
// Rust client where get() performs a UTF-8 conversion.
string get_bytes(KvsClientInterface* client, const string& key);

// Retrieve server thread statistics for a specific node and thread.
// Reads the metadata key
//   ANNA_METADATA|stats|<public_ip>|<private_ip>|<tid>|<tier>
// and decodes the ServerThreadStatistics protobuf.
ServerThreadStatistics get_storage_stats(KvsClientInterface* client,
                                         const string& public_ip,
                                         const string& private_ip,
                                         unsigned tid,
                                         const string& tier);

// Retrieve per-key access frequency data for a specific node and thread.
// Reads the metadata key
//   ANNA_METADATA|access|<public_ip>|<private_ip>|<tid>|<tier>
// and decodes the KeyAccessData protobuf.
KeyAccessData get_key_access_stats(KvsClientInterface* client,
                                   const string& public_ip,
                                   const string& private_ip,
                                   unsigned tid,
                                   const string& tier);

// Retrieve per-key size data for a specific node and thread.
// Reads the metadata key
//   ANNA_METADATA|size|<public_ip>|<private_ip>|<tid>|<tier>
// and decodes the KeySizeData protobuf.
KeySizeData get_key_size_stats(KvsClientInterface* client,
                               const string& public_ip,
                               const string& private_ip,
                               unsigned tid,
                               const string& tier);

// Set the per-key replication factor by writing to the metadata key
//   ANNA_METADATA|replication|<key>
// with a ReplicationFactor protobuf wrapped in an LWW value.
void put_replication_factor(KvsClientInterface* client,
                            const string& key,
                            unsigned memory_rep,
                            unsigned local_rep);

// Set the request timeout in milliseconds for a client created with make_client.
void set_timeout(KvsClient* client, unsigned timeout_ms);

// Get the current request timeout in milliseconds.
unsigned get_timeout(KvsClient* client);

// Retrieve cluster topology (thread counts) from the metadata key
//   ANNA_METADATA|cluster_topology
// and decode the ClusterTopology protobuf.
// Returns a default-constructed ClusterTopology if the key does not exist.
ClusterTopology get_cluster_topology(KvsClientInterface* client);

// Retrieve monitoring node IP addresses from the metadata key
//   ANNA_METADATA|monitoring_ips
// and decode the StringSet protobuf.
// Returns an empty vector if the key does not exist.
vector<string> get_monitoring_ips(KvsClientInterface* client);

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

// Client-side transaction providing Read Committed and Item Cut Isolation.
// Writes are buffered locally until commit(). Reads within a transaction
// are cached for repeatable reads (Item Cut Isolation). The local write
// buffer is checked first so uncommitted writes are visible within the
// transaction (read-your-writes).
class Transaction {
 public:
  Transaction(KvsClientInterface* client) : client_(client) {}

  // Buffer a PUT. Not sent until commit().
  void put(const string& key, const string& value);

  // Read a key. Returns buffered write if present, otherwise reads from
  // server and caches the result for repeatable reads.
  string get(const string& key);

  // Flush all buffered writes to the server.
  PutResult commit();

  // Discard all buffered writes.
  void rollback();

 private:
  KvsClientInterface* client_;
  std::unordered_map<string, string> write_buffer_;
  std::unordered_map<string, string> read_cache_;
};

// Configuration for the bench command.
struct BenchConfig {
  unsigned num_keys = 1000;       // key space size
  unsigned value_size = 256;      // value size in bytes
  unsigned duration = 10;         // benchmark duration in seconds
  unsigned report_period = 2;     // seconds between throughput reports
  std::string workload = "GET";   // GET, PUT, or MIXED
};

// Results from a single benchmark run.
struct BenchResult {
  std::string workload;
  unsigned num_keys;
  unsigned value_size;
  double avg_throughput;          // ops/sec averaged over all epochs
  double avg_latency_us;          // microseconds per operation
  unsigned total_ops;
  double elapsed_seconds;
};

// Populate the KVS with `config.num_keys` keys of `config.value_size`
// bytes each. Call once before running workloads.
void bench_warmup(KvsClientInterface* client, const BenchConfig& config);

// Run a single benchmark workload for `config.duration` seconds,
// printing periodic throughput reports to stdout.
BenchResult bench(KvsClientInterface* client, const BenchConfig& config);

}  // namespace annalib

#endif  // INCLUDE_CLIENT_LIB_HPP_
