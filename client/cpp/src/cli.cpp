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

#include <fstream>

#include "kvs_client.hpp"
#include "yaml-cpp/yaml.h"

#include <assert.h>

unsigned kRoutingThreadCount;

ZmqUtil zmq_util;
ZmqUtilInterface *kZmqUtil = &zmq_util;

void print_set(set<string> set) {
  std::cout << "{ ";
  for (const string &val : set) {
    std::cout << val << " ";
  }

  std::cout << "}" << std::endl;
}

string get(KvsClientInterface *client, string key) {
    client->get_async(key);

    vector<kvs::KeyResponse> responses = client->receive_async();
    while (responses.size() == 0) {
      responses = client->receive_async();
    }

    if (responses.size() > 1) {
      std::cout << "Error: received more than one response" << std::endl;
    }

    assert(responses[0].tuples(0).lattice_type() == kvs::LatticeType::LWW);

    LWWPairLattice<string> lww_lattice =
        deserialize_lww(responses[0].tuples(0).payload());

    return lww_lattice.reveal().value;
}

string get_causal(KvsClientInterface *client, string key) {
    client->get_async(key);

    vector<kvs::KeyResponse> responses = client->receive_async();
    while (responses.size() == 0) {
      responses = client->receive_async();
    }

    if (responses.size() > 1) {
      std::cout << "Error: received more than one response" << std::endl;
    }

    assert(responses[0].tuples(0).lattice_type() == kvs::LatticeType::MULTI_CAUSAL);

    MultiKeyCausalLattice<SetLattice<string>> mkcl =
        MultiKeyCausalLattice<SetLattice<string>>(to_multi_key_causal_payload(
            deserialize_multi_key_causal(responses[0].tuples(0).payload())));

    for (const auto &pair : mkcl.reveal().vector_clock.reveal()) {
      std::cout << "{" << pair.first << " : "
                << std::to_string(pair.second.reveal()) << "}" << std::endl;
    }

    for (const auto &dep_key_vc_pair : mkcl.reveal().dependencies.reveal()) {
      std::cout << dep_key_vc_pair.first << " : ";
      for (const auto &vc_pair : dep_key_vc_pair.second.reveal()) {
        std::cout << "{" << vc_pair.first << " : "
                  << std::to_string(vc_pair.second.reveal()) << "}"
                  << std::endl;
      }
    }

    return *(mkcl.reveal().value.reveal().begin());
}

kvs::KeyResponse put(KvsClientInterface *client, string key, string value) {
    LWWPairLattice<string> val(
        TimestampValuePair<string>(generate_timestamp(0), value));

    string rid = client->put_async(key, serialize(val), kvs::LatticeType::LWW);

    vector<kvs::KeyResponse> responses = client->receive_async();
    while (responses.size() == 0) {
      responses = client->receive_async();
    }

    kvs::KeyResponse response = responses[0];

    // TODO encode this error into the response
    if (response.response_id() != rid) {
      std::cout << "Invalid response: ID did not match request ID!"
                << std::endl;
    }

    return response;
}

kvs::KeyResponse put_causal(KvsClientInterface *client, string key, string value) {
    MultiKeyCausalPayload<SetLattice<string>> mkcp;
    // construct a test client id - version pair
    mkcp.vector_clock.insert("test", 1);

    // construct one test dependencies
    mkcp.dependencies.insert(
        "dep1", VectorClock(map<string, MaxLattice<unsigned>>({{"test1", 1}})));

    // populate the value
    mkcp.value.insert(value);

    MultiKeyCausalLattice<SetLattice<string>> mkcl(mkcp);

    string rid = client->put_async(key, serialize(mkcl), kvs::LatticeType::MULTI_CAUSAL);

    vector<kvs::KeyResponse> responses = client->receive_async();
    while (responses.size() == 0) {
      responses = client->receive_async();
    }

    kvs::KeyResponse response = responses[0];

    // TODO encode this error into the response
    if (response.response_id() != rid) {
      std::cout << "Invalid response: ID did not match request ID!"
                << std::endl;
    }

    return response;
}

kvs::KeyResponse put_set(KvsClientInterface *client, string key, set<string> set) {
    string rid = client->put_async(key, serialize(SetLattice<string>(set)),
                                   kvs::LatticeType::SET);

    vector<kvs::KeyResponse> responses = client->receive_async();
    while (responses.size() == 0) {
      responses = client->receive_async();
    }

    kvs::KeyResponse response = responses[0];

    // TODO encode this error into the response
    if (response.response_id() != rid) {
      std::cout << "Invalid response: ID did not match request ID!"
                << std::endl;
    }

    return response;
}

set<string> get_set(KvsClientInterface *client, string key) {
    client->get_async(key);
    string serialized;

    vector<kvs::KeyResponse> responses = client->receive_async();
    while (responses.size() == 0) {
      responses = client->receive_async();
    }

    SetLattice<string> latt = deserialize_set(responses[0].tuples(0).payload());
    set<string> set_value = latt.reveal();

    return set_value;
}

void execute_command(KvsClientInterface *client, string input) {
  vector<string> v;
  split(input, ' ', v);

  if (v.size() == 0) { // EOF?
    std::exit(EXIT_SUCCESS);
  }

  if (v[0] == "GET") {
    std::cout << get(client, v[1]) << std::endl;
  } else if (v[0] == "GET_CAUSAL") {
    std::cout << get_causal(client, v[1]) << std::endl;
  } else if (v[0] == "PUT") {
    kvs::KeyResponse response = put(client, v[1], v[2]);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (v[0] == "PUT_CAUSAL") {
    kvs::KeyResponse response = put_causal(client, v[1], v[2]);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (v[0] == "PUT_SET") {
    set<string> set;
    for (int i = 2; i < v.size(); i++) {
      set.insert(v[i]);
    }
    kvs::KeyResponse response = put_set(client, v[1], set);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (v[0] == "GET_SET") {
    print_set(get_set(client, v[1]));
  } else {
    std::cout << "Unrecognized command " << v[0]
              << ". Valid commands are GET, GET_SET, PUT, PUT_SET, PUT_CAUSAL, "
              << "and GET_CAUSAL." << std::endl;
    ;
  }
}

// Read commands interactively from the terminal
void cli_loop_interactive(KvsClientInterface *client) {
  string input;
  while (true) {
    std::cout << "anna> ";

    getline(std::cin, input);
    execute_command(client, input);
  }
}

// Read commands from `filename` until EOF
void cli_loop_file(KvsClientInterface *client, string filename) {
  string input;
  std::ifstream infile(filename);

  while (getline(infile, input)) {
    execute_command(client, input);
  }
}

int main(int argc, char *argv[]) {
  // There can be two or three options
  // #0 - binary name
  // #1 - "--config" directive
  // #2 - config filename
  // #3 - input file with commands
  if (argc < 3 || argc > 4) {
    std::cerr << "Usage: " << argv[0] << " --config conf-file <input-file>" << std::endl;
    std::cerr
        << "Filename is optional. Omit the filename to run in interactive mode."
        << std::endl;
    return 1;
  }

  // read the YAML conf
  YAML::Node conf = YAML::LoadFile(argv[2]);
  kRoutingThreadCount = conf["threads"]["routing"].as<unsigned>();

  YAML::Node user = conf["user"];
  Address ip = user["ip"].as<Address>();

  vector<Address> routing_ips;
  if (YAML::Node elb = user["routing-elb"]) {
    routing_ips.push_back(elb.as<string>());
  } else {
    YAML::Node routing = user["routing"];
    for (const YAML::Node &node : routing) {
      routing_ips.push_back(node.as<Address>());
    }
  }

  vector<UserRoutingThread> threads;
  for (Address addr : routing_ips) {
    for (unsigned i = 0; i < kRoutingThreadCount; i++) {
      threads.push_back(UserRoutingThread(addr, i));
    }
  }

  KvsClient client(threads, ip, 0, 10000);

  if (argc == 3) {
    cli_loop_interactive(&client);
  } else {
    cli_loop_file(&client, argv[3]);
  }
}
