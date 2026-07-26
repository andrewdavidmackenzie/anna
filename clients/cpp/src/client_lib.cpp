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

#include <chrono>
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

// Decode an LWW protobuf payload and return the value string.
string decode_lww_value(const string& payload) {
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
  return decode_lww_value(responses[0].tuples(0).payload());
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
  string rid = client->put_async(key, make_lww_payload(value),
                                 kvs::LatticeType::LWW);

  auto responses = receive_with_deadline(client);

  return to_put_result(responses[0], rid);
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

}  // namespace annalib
