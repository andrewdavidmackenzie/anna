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

#include "client_lib.hpp"
#include "client_utils.hpp"

#include <algorithm>
#include <chrono>
#include <iomanip>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <stdexcept>
#include <thread>
#include <fcntl.h>
#include <unistd.h>
#include <sys/wait.h>

// kZmqUtil is declared `extern` (in the global namespace) by
// zmq/zmq_util.hpp and used by KvsClient; this is its one
// definition for any binary that links against this library.
namespace {
ZmqUtil zmq_util;
}  // namespace
ZmqUtilInterface* kZmqUtil = &zmq_util;

namespace annalib {

const vector<string> kProcessList = {"anna-monitor", "anna-route",
                                      "anna-kvs"};

std::unique_ptr<KvsClient> make_client(const ClientConfig& config,
                                        unsigned tid, unsigned timeout) {
  if (config.routing_ips.empty() || config.routing_thread_count == 0) {
    throw std::invalid_argument(
        "ClientConfig requires at least one routing IP and a non-zero "
        "routing_thread_count");
  }
  vector<UserRoutingThread> routing_threads;
  for (const auto& ip : config.routing_ips) {
    for (unsigned t = 0; t < config.routing_thread_count; t++) {
      routing_threads.push_back(UserRoutingThread(ip, t));
    }
  }
  return std::make_unique<KvsClient>(routing_threads, config.ip, tid, timeout);
}

namespace {

// Receive a response from the client, with a 10-second deadline.
// Throws std::runtime_error if no response is received in time.
vector<kvs::KeyResponse> receive_with_deadline(KvsClientInterface* client) {
  auto deadline = std::chrono::system_clock::now() + std::chrono::seconds(10);
  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.empty()) {
    if (std::chrono::system_clock::now() > deadline) {
      throw std::runtime_error("Request timed out: no response within 10s");
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
    responses = client->receive_async();
  }
  return responses;
}

// Check a response for server-side errors (top-level and per-tuple).
// Throws std::runtime_error with the error name if present.
void check_response_error(const kvs::KeyResponse& response) {
  if (response.error() != kvs::AnnaError::NO_ERROR) {
    throw std::runtime_error(
        kvs::AnnaError_Name(response.error()));
  }
  if (response.tuples_size() == 0) {
    throw std::runtime_error("Empty response: no tuples");
  }
  if (response.tuples(0).error() != 0) {
    throw std::runtime_error(
        kvs::AnnaError_Name(response.tuples(0).error()));
  }
}

// Convert a kvs::KeyResponse into a PutResult for the public API.
PutResult to_put_result(const kvs::KeyResponse& response,
                        const string& expected_rid) {
  PutResult result;
  result.error = (response.error() != kvs::AnnaError::NO_ERROR);
  result.response_id = response.response_id();

  if (result.response_id != expected_rid) {
    result.error = true;
  }

  if (!result.error && response.tuples_size() > 0 &&
      response.tuples(0).error() != 0) {
    result.error = true;
  }

  return result;
}

// Build an LWW protobuf payload from a value string.
string make_lww_payload(const string& value) {
  kvs::LWWValue lww;
  lww.set_timestamp(generate_timestamp(0));
  lww.set_value(value);
  string payload;
  lww.SerializeToString(&payload);
  return payload;
}

// Monotonic read cache: per-key high-water mark of LWW timestamps and
// the corresponding value. When a GET returns a stale timestamp, the
// cached value is returned instead, guaranteeing monotonic reads.
// TODO: This should be per-client-instance, not namespace-global.
// Currently safe for single-client-per-process but would need to move
// into KvsClient for multi-client scenarios.
map<string, pair<uint64_t, string>> lww_read_cache;

// High-water mark of timestamps seen by this client (reads and writes).
// Ensures each PUT uses a timestamp strictly greater than any previously
// seen timestamp, providing the Writes Follow Reads guarantee.
uint64_t last_seen_ts = 0;

// Decode an LWW protobuf payload and return the value string,
// enforcing monotonic reads via the lww_read_cache.
string decode_lww_value(const string& key, const string& payload) {
  kvs::LWWValue lww;
  lww.ParseFromString(payload);

  auto it = lww_read_cache.find(key);
  if (it != lww_read_cache.end() && lww.timestamp() < it->second.first) {
    return it->second.second;
  }

  if (lww.timestamp() > last_seen_ts) {
    last_seen_ts = lww.timestamp();
  }
  lww_read_cache[key] = {lww.timestamp(), lww.value()};
  return lww.value();
}

// Decode without monotonic read enforcement (for internal/metadata use).
string decode_lww_value_raw(const string& payload) {
  kvs::LWWValue lww;
  lww.ParseFromString(payload);
  return lww.value();
}

// Build a SetValue protobuf payload from a set of strings.
string make_set_payload(const set<string>& values) {
  kvs::SetValue sv;
  for (const auto& v : values) {
    sv.add_values(v);
  }
  string payload;
  sv.SerializeToString(&payload);
  return payload;
}

}  // namespace

PutResult del(KvsClientInterface* client, const string& key) {
  return put(client, key, "");
}

map<string, string> get_multi(KvsClientInterface* client,
                              const vector<string>& keys) {
  map<string, string> results;
  for (const auto& key : keys) {
    string val = get(client, key);
    if (!val.empty()) {
      results[key] = val;
    }
  }
  return results;
}

string get(KvsClientInterface* client, const string& key) {
  client->get_async(key);
  auto responses = receive_with_deadline(client);
  check_response_error(responses[0]);
  return decode_lww_value(key, responses[0].tuples(0).payload());
}

CausalValue get_causal(KvsClientInterface* client, const string& key) {
  client->get_async(key);
  auto responses = receive_with_deadline(client);
  check_response_error(responses[0]);

  kvs::MultiKeyCausalValue mkc;
  mkc.ParseFromString(responses[0].tuples(0).payload());

  CausalValue result;
  if (mkc.values_size() > 0) {
    result.value = mkc.values(0);
  }

  for (const auto& pair : mkc.vector_clock()) {
    result.vector_clock.push_back({pair.first, pair.second});
  }

  for (const auto& dep : mkc.dependencies()) {
    vector<pair<string, unsigned>> vc;
    for (const auto& vc_pair : dep.vector_clock()) {
      vc.push_back({vc_pair.first, vc_pair.second});
    }
    result.dependencies[dep.key()] = vc;
  }

  return result;
}

PutResult put(KvsClientInterface* client, const string& key,
              const string& value) {
  uint64_t ts = generate_timestamp(0);
  if (ts <= last_seen_ts) {
    ts = last_seen_ts + 1;
  }
  last_seen_ts = ts;
  kvs::LWWValue lww;
  lww.set_timestamp(ts);
  lww.set_value(value);
  string payload;
  lww.SerializeToString(&payload);

  string rid = client->put_async(key, payload, kvs::LatticeType::LWW);

  auto responses = receive_with_deadline(client);

  auto result = to_put_result(responses[0], rid);

  // Cache the written value for read-your-writes consistency.
  if (!result.error) {
    lww_read_cache[key] = {ts, value};
  }

  return result;
}

PutResult put_causal(KvsClientInterface* client, const string& key,
                     const string& value) {
  kvs::MultiKeyCausalValue mkc;

  // Vector clock: test client id with version 1
  auto* vc = mkc.mutable_vector_clock();
  (*vc)["test"] = 1;

  // One test dependency
  auto* dep = mkc.add_dependencies();
  dep->set_key("dep1");
  auto* dep_vc = dep->mutable_vector_clock();
  (*dep_vc)["test1"] = 1;

  // Value
  mkc.add_values(value);

  string payload;
  mkc.SerializeToString(&payload);

  string rid = client->put_async(key, payload,
                                 kvs::LatticeType::MULTI_CAUSAL);

  auto responses = receive_with_deadline(client);

  return to_put_result(responses[0], rid);
}

PutResult put_set(KvsClientInterface* client, const string& key,
                  const set<string>& values) {
  string rid = client->put_async(key, make_set_payload(values),
                                 kvs::LatticeType::SET);

  auto responses = receive_with_deadline(client);

  return to_put_result(responses[0], rid);
}

set<string> get_set(KvsClientInterface* client, const string& key) {
  client->get_async(key);
  auto responses = receive_with_deadline(client);
  check_response_error(responses[0]);

  kvs::SetValue sv;
  sv.ParseFromString(responses[0].tuples(0).payload());

  set<string> result;
  for (const auto& v : sv.values()) {
    result.insert(v);
  }
  return result;
}

PutResult put_ordered_set(KvsClientInterface* client, const string& key,
                          const set<string>& values) {
  // Same serialization as SET, but use ORDERED_SET lattice type so the
  // server stores as OrderedSetLattice.
  string rid = client->put_async(key, make_set_payload(values),
                                 kvs::LatticeType::ORDERED_SET);

  auto responses = receive_with_deadline(client);

  return to_put_result(responses[0], rid);
}

vector<string> get_ordered_set(KvsClientInterface* client, const string& key) {
  client->get_async(key);
  auto responses = receive_with_deadline(client);
  check_response_error(responses[0]);

  kvs::SetValue set_val;
  set_val.ParseFromString(responses[0].tuples(0).payload());
  vector<string> result;
  for (const auto& v : set_val.values()) {
    result.push_back(v);
  }
  return result;
}

PutResult put_single_causal(KvsClientInterface* client,
                            const string& key, const string& value) {
  kvs::SingleKeyCausalValue skc;

  // Vector clock: test client id with version 1
  auto* vc = skc.mutable_vector_clock();
  (*vc)["test"] = 1;

  // Value
  skc.add_values(value);

  string payload;
  skc.SerializeToString(&payload);

  string rid = client->put_async(key, payload,
                                 kvs::LatticeType::SINGLE_CAUSAL);

  auto responses = receive_with_deadline(client);

  return to_put_result(responses[0], rid);
}

SingleCausalValue get_single_causal(KvsClientInterface* client,
                                    const string& key) {
  client->get_async(key);
  auto responses = receive_with_deadline(client);
  check_response_error(responses[0]);

  kvs::SingleKeyCausalValue skc;
  skc.ParseFromString(responses[0].tuples(0).payload());

  SingleCausalValue result;
  for (const auto& v : skc.values()) {
    result.values.push_back(v);
  }
  for (const auto& pair : skc.vector_clock()) {
    result.vector_clock.push_back({pair.first, pair.second});
  }

  return result;
}

PutResult put_priority(KvsClientInterface* client, const string& key,
                       double priority, const string& value) {
  kvs::PriorityValue pv;
  pv.set_priority(priority);
  pv.set_value(value);

  string payload;
  pv.SerializeToString(&payload);

  string rid = client->put_async(key, payload,
                                 kvs::LatticeType::PRIORITY);

  auto responses = receive_with_deadline(client);

  return to_put_result(responses[0], rid);
}

PriorityResult get_priority(KvsClientInterface* client, const string& key) {
  client->get_async(key);
  auto responses = receive_with_deadline(client);
  check_response_error(responses[0]);

  kvs::PriorityValue pv;
  pv.ParseFromString(responses[0].tuples(0).payload());

  PriorityResult result;
  result.priority = pv.priority();
  result.value = pv.value();

  return result;
}

string get_bytes(KvsClientInterface* client, const string& key) {
  return get(client, key);
}

namespace {

// Build a metadata key for per-thread stats/access/size data:
//   ANNA_METADATA|<type>|<public_ip>|<private_ip>|<tid>|<tier>
string make_stats_metadata_key(const string& type, const string& public_ip,
                               const string& private_ip, unsigned tid,
                               const string& tier) {
  return kMetadataIdentifier + kMetadataDelimiter + type +
         kMetadataDelimiter + public_ip + kMetadataDelimiter + private_ip +
         kMetadataDelimiter + std::to_string(tid) + kMetadataDelimiter + tier;
}

}  // namespace

ServerThreadStatistics get_storage_stats(KvsClientInterface* client,
                                         const string& public_ip,
                                         const string& private_ip,
                                         unsigned tid,
                                         const string& tier) {
  string key = make_stats_metadata_key("stats", public_ip, private_ip, tid, tier);
  string bytes = get_bytes(client, key);

  ServerThreadStatistics stats;
  stats.ParseFromString(bytes);
  return stats;
}

KeyAccessData get_key_access_stats(KvsClientInterface* client,
                                   const string& public_ip,
                                   const string& private_ip,
                                   unsigned tid,
                                   const string& tier) {
  string key = make_stats_metadata_key("access", public_ip, private_ip, tid, tier);
  string bytes = get_bytes(client, key);

  KeyAccessData data;
  data.ParseFromString(bytes);
  return data;
}

KeySizeData get_key_size_stats(KvsClientInterface* client,
                               const string& public_ip,
                               const string& private_ip,
                               unsigned tid,
                               const string& tier) {
  string key = make_stats_metadata_key("size", public_ip, private_ip, tid, tier);
  string bytes = get_bytes(client, key);

  KeySizeData data;
  data.ParseFromString(bytes);
  return data;
}

void put_replication_factor(KvsClientInterface* client,
                            const string& key,
                            unsigned memory_rep,
                            unsigned local_rep) {
  ReplicationFactor rep;
  rep.set_key(key);

  // Global replication: MEMORY tier gets the requested factor, DISK gets 0.
  auto* gm = rep.add_global();
  gm->set_tier(MEMORY);
  gm->set_value(memory_rep);
  auto* gd = rep.add_global();
  gd->set_tier(DISK);
  gd->set_value(0);

  // Local replication: MEMORY tier gets the requested factor, DISK gets 0.
  auto* lm = rep.add_local();
  lm->set_tier(MEMORY);
  lm->set_value(local_rep);
  auto* ld = rep.add_local();
  ld->set_tier(DISK);
  ld->set_value(0);

  string payload;
  rep.SerializeToString(&payload);

  string meta_key = kMetadataIdentifier + kMetadataDelimiter +
                    string("replication") + kMetadataDelimiter + key;
  put(client, meta_key, payload);
}

void set_timeout(KvsClient* client, unsigned timeout_ms) {
  client->set_timeout(timeout_ms);
}

unsigned get_timeout(KvsClient* client) {
  return client->get_timeout();
}

ClusterTopology get_cluster_topology(KvsClientInterface* client) {
  string key = kMetadataIdentifier + kMetadataDelimiter +
               string("cluster_topology");
  string bytes = get_bytes(client, key);

  ClusterTopology topology;
  topology.ParseFromString(bytes);
  return topology;
}

vector<string> get_monitoring_ips(KvsClientInterface* client) {
  string key = kMetadataIdentifier + kMetadataDelimiter +
               string("monitoring_ips");
  string bytes = get_bytes(client, key);

  shared::StringSet string_set;
  if (!string_set.ParseFromString(bytes)) {
    return {};
  }
  return {string_set.keys().begin(), string_set.keys().end()};
}

namespace {

vector<int> pids_from_name(const string& name) {
  vector<int> pids;
  string uid = std::to_string(getuid());
  string cmd = "pgrep -x -u " + uid + " " + name;
  FILE* fp = popen(cmd.c_str(), "r");
  if (!fp) return pids;

  char buf[64];
  while (fgets(buf, sizeof(buf), fp)) {
    int pid = atoi(buf);
    if (pid > 0) pids.push_back(pid);
  }
  pclose(fp);
  return pids;
}

string find_binary(const string& name) {
  const char* server_path = getenv("ANNA_SERVER_PATH");
  if (server_path) {
    string full = string(server_path) + "/" + name;
    if (access(full.c_str(), X_OK) == 0) return full;
  }
  return name;
}

}  // namespace

int start(const string& config_file_path) {
  int started = 0;

  for (const string& process_name : kProcessList) {
    vector<int> existing = pids_from_name(process_name);
    if (!existing.empty()) continue;

    string bin = find_binary(process_name);

    pid_t pid = fork();
    if (pid < 0) continue;

    if (pid == 0) {
      setsid();
      int devnull_r = open("/dev/null", O_RDONLY);
      int devnull_w = open("/dev/null", O_WRONLY);
      if (devnull_r >= 0) { dup2(devnull_r, STDIN_FILENO); close(devnull_r); }
      if (devnull_w >= 0) {
        dup2(devnull_w, STDOUT_FILENO);
        dup2(devnull_w, STDERR_FILENO);
        close(devnull_w);
      }
      const char* args[] = {bin.c_str(), "--config",
                            config_file_path.c_str(), nullptr};
      execvp(args[0], const_cast<char* const*>(args));
      _exit(127);
    }

    // Check if exec succeeded by waiting briefly for the child.
    // If it exits immediately (127 = exec failed), don't count it.
    usleep(50000);  // 50ms
    int wstatus;
    pid_t result = waitpid(pid, &wstatus, WNOHANG);
    if (result == 0) {
      started++;  // child still running — exec succeeded
    }
    // If result > 0, child already exited (exec failed) — don't count
  }

  return started;
}

vector<string> status() {
  vector<string> result;

  for (const string& process_name : kProcessList) {
    vector<int> pids = pids_from_name(process_name);
    if (!pids.empty()) {
      result.push_back(process_name);
    }
  }

  return result;
}

int stop() {
  int killed = 0;
  vector<int> signaled_pids;

  for (const string& process_name : kProcessList) {
    vector<int> pids = pids_from_name(process_name);
    for (int pid : pids) {
      if (kill(pid, SIGTERM) == 0) {
        signaled_pids.push_back(pid);
        killed++;
      }
    }
  }

  for (int pid : signaled_pids) {
    waitpid(pid, nullptr, 0);
  }

  return killed;
}

void Transaction::put(const string& key, const string& value) {
  write_buffer_[key] = value;
  read_cache_[key] = value;
}

string Transaction::get(const string& key) {
  auto it = read_cache_.find(key);
  if (it != read_cache_.end()) {
    return it->second;
  }
  string value = annalib::get(client_, key);
  read_cache_[key] = value;
  return value;
}

PutResult Transaction::commit() {
  PutResult last_result;
  last_result.error = false;
  for (const auto& kv : write_buffer_) {
    last_result = annalib::put(client_, kv.first, kv.second);
    if (last_result.error) break;
  }
  write_buffer_.clear();
  read_cache_.clear();
  return last_result;
}

void Transaction::rollback() {
  write_buffer_.clear();
  read_cache_.clear();
}

// --- Benchmark ---

static string generate_bench_key(unsigned n) {
  string s = std::to_string(n);
  return string(8 - s.length(), '0') + s;
}

BenchResult bench(KvsClientInterface* client, const BenchConfig& config) {
  string value(config.value_size, 'a');

  // Warm up: populate keys.
  std::cout << "Warming up " << config.num_keys << " keys ("
            << config.value_size << " bytes each)..." << std::endl;
  auto warmup_start = std::chrono::steady_clock::now();
  for (unsigned i = 1; i <= config.num_keys; i++) {
    put(client, generate_bench_key(i), value);
  }
  auto warmup_elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
                             std::chrono::steady_clock::now() - warmup_start)
                             .count();
  std::cout << "Warmup complete in " << warmup_elapsed << " ms" << std::endl;

  // Determine workload type.
  string wl = config.workload;
  std::transform(wl.begin(), wl.end(), wl.begin(), ::toupper);

  std::cout << "Running " << wl << " benchmark for " << config.duration
            << "s (" << config.num_keys << " keys, "
            << config.value_size << " B values)..." << std::endl;

  unsigned seed = static_cast<unsigned>(
      std::chrono::steady_clock::now().time_since_epoch().count());
  size_t total_ops = 0;
  size_t epoch_ops = 0;
  double throughput_sum = 0;
  unsigned epochs = 0;

  auto bench_start = std::chrono::steady_clock::now();
  auto epoch_start = bench_start;

  while (true) {
    unsigned k = rand_r(&seed) % config.num_keys + 1;
    string key = generate_bench_key(k);

    if (wl == "GET") {
      get(client, key);
      total_ops += 1;
      epoch_ops += 1;
    } else if (wl == "PUT") {
      put(client, key, value);
      total_ops += 1;
      epoch_ops += 1;
    } else {
      // MIXED: PUT then GET
      put(client, key, value);
      get(client, key);
      total_ops += 2;
      epoch_ops += 2;
    }

    auto now = std::chrono::steady_clock::now();
    auto epoch_elapsed = std::chrono::duration_cast<std::chrono::seconds>(
                              now - epoch_start)
                              .count();

    if (epoch_elapsed >= config.report_period) {
      epochs += 1;
      double secs = std::chrono::duration<double>(now - epoch_start).count();
      double throughput = static_cast<double>(epoch_ops) / secs;
      throughput_sum += throughput;
      std::cout << "[Epoch " << epochs << "] Throughput: "
                << static_cast<unsigned>(throughput) << " ops/sec"
                << std::endl;
      epoch_ops = 0;
      epoch_start = now;
    }

    auto total_elapsed = std::chrono::duration_cast<std::chrono::seconds>(
                              now - bench_start)
                              .count();
    if (total_elapsed >= config.duration) {
      break;
    }
  }

  double elapsed = std::chrono::duration<double>(
                        std::chrono::steady_clock::now() - bench_start)
                        .count();
  double avg_throughput = (epochs > 0) ? throughput_sum / epochs
                                       : static_cast<double>(total_ops) / elapsed;
  double avg_latency_us = (avg_throughput > 0) ? 1000000.0 / avg_throughput : 0;

  std::cout << "\n=== " << wl << " Results ===" << std::endl;
  std::cout << "Total ops:      " << total_ops << std::endl;
  std::cout << "Elapsed:        " << std::fixed << std::setprecision(2)
            << elapsed << " s" << std::endl;
  std::cout << "Avg throughput: " << static_cast<unsigned>(avg_throughput)
            << " ops/sec" << std::endl;
  std::cout << "Avg latency:    " << std::fixed << std::setprecision(1)
            << avg_latency_us << " us/op" << std::endl;

  BenchResult result;
  result.workload = wl;
  result.num_keys = config.num_keys;
  result.value_size = config.value_size;
  result.avg_throughput = avg_throughput;
  result.avg_latency_us = avg_latency_us;
  result.total_ops = static_cast<unsigned>(total_ops);
  result.elapsed_seconds = elapsed;
  return result;
}

}  // namespace annalib
