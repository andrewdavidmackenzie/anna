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

// start()/stop()/status() are not yet implemented -- see #103. These stubs
// preserve the exact (non-)behavior that previously lived in cli.cpp.
int start(const string& config_file_path) {
  int process_count = 3;  // TODO until implemented
  for (const string& process_name : kProcessList) {
    (void)process_name;
  }

  return process_count;
}

vector<string> status() {
  vector<string> result = {};

  for (const string& process_name : kProcessList) {
    (void)process_name;
  }

  return result;
}

int stop() {
  int kill_count = 3;  // TODO until we implement
  for (const string& process_name : kProcessList) {
    (void)process_name;
  }

  return kill_count;
}

}  // namespace annalib
