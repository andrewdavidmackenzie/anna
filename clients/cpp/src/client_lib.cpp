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

#include "yaml-cpp/yaml.h"

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

ClientConfig load_config(const string& config_file_path) {
  YAML::Node conf = YAML::LoadFile(config_file_path);
  unsigned routing_thread_count = conf["threads"]["routing"].as<unsigned>();

  YAML::Node user = conf["user"];
  Address ip = user["ip"].as<Address>();

  vector<Address> routing_ips;
  if (YAML::Node elb = user["routing-elb"]) {
    routing_ips.push_back(elb.as<string>());
  } else {
    YAML::Node routing = user["routing"];
    for (const YAML::Node& node : routing) {
      routing_ips.push_back(node.as<Address>());
    }
  }

  ClientConfig config;
  config.ip = ip;
  for (const Address& addr : routing_ips) {
    for (unsigned i = 0; i < routing_thread_count; i++) {
      config.routing_threads.push_back(UserRoutingThread(addr, i));
    }
  }

  return config;
}

std::unique_ptr<KvsClient> make_client(const ClientConfig& config,
                                        unsigned tid, unsigned timeout) {
  return std::make_unique<KvsClient>(config.routing_threads, config.ip, tid,
                                      timeout);
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

kvs::KeyResponse put(KvsClientInterface* client, const string& key,
                     const string& value) {
  LWWPairLattice<string> val(
      TimestampValuePair<string>(generate_timestamp(0), value));

  string rid =
      client->put_async(key, serialize(val), kvs::LatticeType::LWW);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  kvs::KeyResponse response = responses[0];

  // TODO encode this error into the response
  if (response.response_id() != rid) {
    std::cerr << "Invalid response: ID did not match request ID!"
              << std::endl;
  }

  return response;
}

kvs::KeyResponse put_causal(KvsClientInterface* client, const string& key,
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

  kvs::KeyResponse response = responses[0];

  // TODO encode this error into the response
  if (response.response_id() != rid) {
    std::cerr << "Invalid response: ID did not match request ID!"
              << std::endl;
  }

  return response;
}

kvs::KeyResponse put_set(KvsClientInterface* client, const string& key,
                         const set<string>& values) {
  string rid = client->put_async(key, serialize(SetLattice<string>(values)),
                                 kvs::LatticeType::SET);

  vector<kvs::KeyResponse> responses = client->receive_async();
  while (responses.size() == 0) {
    responses = client->receive_async();
  }

  kvs::KeyResponse response = responses[0];

  // TODO encode this error into the response
  if (response.response_id() != rid) {
    std::cerr << "Invalid response: ID did not match request ID!"
              << std::endl;
  }

  return response;
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
