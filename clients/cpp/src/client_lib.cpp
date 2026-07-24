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

#include <cassert>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <fcntl.h>
#include <unistd.h>
#include <sys/wait.h>

// kZmqUtil is declared `extern` (in the global namespace) by
// zmq/zmq_util.hpp and used by KvsClient/requests.hpp; this is its one
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
  vector<UserRoutingThread> routing_threads;
  for (const auto& ip : config.routing_ips) {
    for (unsigned t = 0; t < config.routing_thread_count; t++) {
      routing_threads.push_back(UserRoutingThread(ip, t));
    }
  }
  return std::make_unique<KvsClient>(routing_threads, config.ip, tid, timeout);
}

namespace {

// Convert a kvs::KeyResponse into a PutResult for the public API.
PutResult to_put_result(const kvs::KeyResponse& response,
                        const string& expected_rid) {
  PutResult result;
  result.error = (response.error() != kvs::AnnaError::NO_ERROR);
  result.response_id = response.response_id();

  if (result.response_id != expected_rid) {
    std::cerr << "Invalid response: ID did not match request ID!" << std::endl;
    result.error = true;
  }

  return result;
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

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  if (responses.size() > 1) {
    std::cerr << "Error: received more than one response" << std::endl;
  }

  assert(responses[0].tuples(0).lattice_type() == kvs::LatticeType::LWW);

  LWWPairLattice<string> lww_lattice =
      deserialize_lww(responses[0].tuples(0).payload());

  return lww_lattice.reveal().value;
}

CausalValue get_causal(KvsClientInterface* client, const string& key) {
  client->get_async(key);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  if (responses.size() > 1) {
    std::cerr << "Error: received more than one response" << std::endl;
  }

  assert(responses[0].tuples(0).lattice_type() ==
         kvs::LatticeType::MULTI_CAUSAL);

  MultiKeyCausalLattice<SetLattice<string>> mkcl =
      MultiKeyCausalLattice<SetLattice<string>>(to_multi_key_causal_payload(
          deserialize_multi_key_causal(responses[0].tuples(0).payload())));

  CausalValue result;
  result.value = *(mkcl.reveal().value.reveal().begin());

  for (const auto& pair : mkcl.reveal().vector_clock.reveal()) {
    result.vector_clock.push_back({pair.first, pair.second.reveal()});
  }

  for (const auto& dep_key_vc_pair : mkcl.reveal().dependencies.reveal()) {
    vector<pair<string, unsigned>> vc;
    for (const auto& vc_pair : dep_key_vc_pair.second.reveal()) {
      vc.push_back({vc_pair.first, vc_pair.second.reveal()});
    }
    result.dependencies[dep_key_vc_pair.first] = vc;
  }

  return result;
}

PutResult put(KvsClientInterface* client, const string& key,
              const string& value) {
  LWWPairLattice<string> val(
      TimestampValuePair<string>(generate_timestamp(0), value));

  string rid =
      client->put_async(key, serialize(val), kvs::LatticeType::LWW);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  return to_put_result(responses[0], rid);
}

PutResult put_causal(KvsClientInterface* client, const string& key,
                     const string& value) {
  MultiKeyCausalPayload<SetLattice<string>> mkcp;
  // construct a test client id - version pair
  mkcp.vector_clock.insert("test", 1);

  // construct one test dependency
  mkcp.dependencies.insert(
      "dep1", VectorClock(map<string, MaxLattice<unsigned>>({{"test1", 1}})));

  // populate the value
  mkcp.value.insert(value);

  MultiKeyCausalLattice<SetLattice<string>> mkcl(mkcp);

  string rid = client->put_async(key, serialize(mkcl),
                                 kvs::LatticeType::MULTI_CAUSAL);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  return to_put_result(responses[0], rid);
}

PutResult put_set(KvsClientInterface* client, const string& key,
                  const set<string>& values) {
  string rid = client->put_async(key, serialize(SetLattice<string>(values)),
                                 kvs::LatticeType::SET);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  return to_put_result(responses[0], rid);
}

set<string> get_set(KvsClientInterface* client, const string& key) {
  client->get_async(key);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  if (responses.size() > 1) {
    std::cerr << "Error: received more than one response" << std::endl;
  }

  assert(responses[0].tuples(0).lattice_type() == kvs::LatticeType::SET);

  SetLattice<string> latt = deserialize_set(responses[0].tuples(0).payload());

  return latt.reveal();
}

PutResult put_ordered_set(KvsClientInterface* client, const string& key,
                          const set<string>& values) {
  // Same serialization as SET, but use ORDERED_SET lattice type so the
  // server stores as OrderedSetLattice.
  string rid = client->put_async(key, serialize(SetLattice<string>(values)),
                                 kvs::LatticeType::ORDERED_SET);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  return to_put_result(responses[0], rid);
}

vector<string> get_ordered_set(KvsClientInterface* client, const string& key) {
  client->get_async(key);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  if (responses.size() > 1) {
    std::cerr << "Error: received more than one response" << std::endl;
  }

  assert(responses[0].tuples(0).lattice_type() ==
         kvs::LatticeType::ORDERED_SET);

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
  VectorClockValuePair<SetLattice<string>> p;
  // construct a test client id - version pair
  p.vector_clock.insert("test", 1);
  // populate the value
  p.value.insert(value);

  SingleKeyCausalLattice<SetLattice<string>> skcl(p);

  string rid = client->put_async(key, serialize(skcl),
                                 kvs::LatticeType::SINGLE_CAUSAL);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  return to_put_result(responses[0], rid);
}

SingleCausalValue get_single_causal(KvsClientInterface* client,
                                    const string& key) {
  client->get_async(key);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  if (responses.size() > 1) {
    std::cerr << "Error: received more than one response" << std::endl;
  }

  assert(responses[0].tuples(0).lattice_type() ==
         kvs::LatticeType::SINGLE_CAUSAL);

  kvs::SingleKeyCausalValue cv =
      deserialize_causal(responses[0].tuples(0).payload());
  VectorClockValuePair<SetLattice<string>> p = to_vector_clock_value_pair(cv);

  SingleCausalValue result;
  for (const auto& v : p.value.reveal()) {
    result.values.push_back(v);
  }

  for (const auto& pair : p.vector_clock.reveal()) {
    result.vector_clock.push_back({pair.first, pair.second.reveal()});
  }

  return result;
}

PutResult put_priority(KvsClientInterface* client, const string& key,
                       double priority, const string& value) {
  PriorityLattice<double, string> pl(
      PriorityValuePair<double, string>(priority, value));

  string rid = client->put_async(key, serialize(pl),
                                 kvs::LatticeType::PRIORITY);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  return to_put_result(responses[0], rid);
}

PriorityResult get_priority(KvsClientInterface* client, const string& key) {
  client->get_async(key);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  if (responses.size() > 1) {
    std::cerr << "Error: received more than one response" << std::endl;
  }

  assert(responses[0].tuples(0).lattice_type() ==
         kvs::LatticeType::PRIORITY);

  PriorityLattice<double, string> pl =
      deserialize_priority(responses[0].tuples(0).payload());

  PriorityResult result;
  result.priority = pl.reveal().priority;
  result.value = pl.reveal().value;

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
