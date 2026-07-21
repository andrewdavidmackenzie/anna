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
#include <algorithm>
#include <string>

#include "client_lib.hpp"

// This is an example CLI built on top of the annalib client library
// (client_lib.hpp / kvs_client.hpp) -- it only handles argv/stdin parsing
// and formatting output, all KVS/process-management logic lives in the
// library. See issue #75.

namespace {

void print_set(const set<string>& values) {
  std::cout << "{ ";
  for (const string& val : values) {
    std::cout << val << " ";
  }

  std::cout << "}" << std::endl;
}

void print_causal_value(const annalib::CausalValue& causal) {
  for (const auto& pair : causal.vector_clock) {
    std::cout << "{" << pair.first << " : " << std::to_string(pair.second)
              << "}" << std::endl;
  }

  for (const auto& dep_key_vc_pair : causal.dependencies) {
    std::cout << dep_key_vc_pair.first << " : ";
    for (const auto& vc_pair : dep_key_vc_pair.second) {
      std::cout << "{" << vc_pair.first << " : "
                << std::to_string(vc_pair.second) << "}" << std::endl;
    }
  }

  std::cout << causal.value << std::endl;
}

void print_single_causal_value(const annalib::SingleCausalValue& causal) {
  for (const auto& pair : causal.vector_clock) {
    std::cout << "{" << pair.first << " : " << std::to_string(pair.second)
              << "}" << std::endl;
  }

  for (const auto& v : causal.values) {
    std::cout << v << std::endl;
  }
}

void print_priority_value(const annalib::PriorityResult& result) {
  std::cout << "priority: " << result.priority << std::endl;
  std::cout << result.value << std::endl;
}

string cli_usage() {
  return "Valid commands are GET, GET_SET, GET_ORDERED_SET, GET_CAUSAL, "
         "GET_SINGLE_CAUSAL, GET_PRIORITY, PUT, PUT_SET, PUT_ORDERED_SET, "
         "PUT_CAUSAL, PUT_SINGLE_CAUSAL, PUT_PRIORITY, START, STOP, STATUS, "
         "HELP and EXIT";
}

void execute_cli_command(KvsClientInterface* client, const string& config_file,
                         const string& input) {
  vector<string> v;
  split(input, ' ', v);

  if (v.size() == 0) {  // EOF?
    std::exit(EXIT_SUCCESS);
  }

  string command = v[0];
  std::transform(command.begin(), command.end(), command.begin(), ::toupper);

  if (command == "GET") {
    std::cout << annalib::get(client, v[1]) << std::endl;
  } else if (command == "GET_CAUSAL") {
    print_causal_value(annalib::get_causal(client, v[1]));
  } else if (command == "DELETE") {
    kvs::KeyResponse response = annalib::del(client, v[1]);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (command == "PUT") {
    kvs::KeyResponse response = annalib::put(client, v[1], v[2]);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (command == "PUT_CAUSAL") {
    kvs::KeyResponse response = annalib::put_causal(client, v[1], v[2]);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (command == "PUT_SET") {
    set<string> values;
    for (size_t i = 2; i < v.size(); i++) {
      values.insert(v[i]);
    }
    kvs::KeyResponse response = annalib::put_set(client, v[1], values);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (command == "GET_SET") {
    print_set(annalib::get_set(client, v[1]));
  } else if (command == "PUT_ORDERED_SET") {
    set<string> values;
    for (size_t i = 2; i < v.size(); i++) {
      values.insert(v[i]);
    }
    kvs::KeyResponse response =
        annalib::put_ordered_set(client, v[1], values);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (command == "GET_ORDERED_SET") {
    vector<string> values = annalib::get_ordered_set(client, v[1]);
    std::cout << "[ ";
    for (const auto& val : values) {
      std::cout << val << " ";
    }
    std::cout << "]" << std::endl;
  } else if (command == "PUT_SINGLE_CAUSAL") {
    kvs::KeyResponse response =
        annalib::put_single_causal(client, v[1], v[2]);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (command == "GET_SINGLE_CAUSAL") {
    print_single_causal_value(annalib::get_single_causal(client, v[1]));
  } else if (command == "PUT_PRIORITY") {
    double priority = std::stod(v[2]);
    kvs::KeyResponse response =
        annalib::put_priority(client, v[1], priority, v[3]);
    if (response.error() != kvs::AnnaError::NO_ERROR) {
      std::cout << "Failure!" << std::endl;
    }
  } else if (command == "GET_PRIORITY") {
    print_priority_value(annalib::get_priority(client, v[1]));
  } else if (command == "STATUS") {
    for (const string& name : annalib::status()) {
      std::cout << name << " process is running" << std::endl;
    }
  } else if (command == "START") {
    std::cout << annalib::start(config_file) << " anna processes were started"
              << std::endl;
  } else if (command == "STOP") {
    std::cout << annalib::stop() << " anna processes were stopped"
              << std::endl;
  } else if (command == "HELP") {
    std::cout << cli_usage() << std::endl;
  } else if (command == "EXIT") {
    std::exit(EXIT_SUCCESS);
  } else {
    std::cout << "Unrecognized command: " << command << std::endl
              << cli_usage() << std::endl;
  }
}

// Read commands interactively from the terminal
void cli_loop_interactive(KvsClientInterface* client, const string& config_file) {
  string input;
  while (true) {
    std::cout << "anna> ";

    getline(std::cin, input);
    execute_cli_command(client, config_file, input);
  }
}

// Read commands from `filename` until EOF
void cli_loop_file(KvsClientInterface* client, const string& config_file,
                   const string& filename) {
  string input;
  std::ifstream infile(filename);

  while (getline(infile, input)) {
    execute_cli_command(client, config_file, input);
  }
}

string usage(const string& name) {
  return name +
         " --routing <ip>[,<ip>...] --client-ip <ip> [--threads <n>] "
         "<command> [CLI command file]\n"
         "Valid commands are help, start, stop, status, cli\n";
}

}  // namespace

int main(int argc, char* argv[]) {
  // Parse named arguments:
  //   --routing ip1,ip2,...   routing tier IP addresses (required)
  //   --client-ip ip         this client's IP address (required)
  //   --threads n            routing threads per IP (default: 1)
  //   --config path          config file for server start/stop (optional)
  //   <command>              CLI / START / STOP / STATUS / HELP
  //   [file]                 command file when command is CLI
  string routing_arg;
  string client_ip;
  string config_filename;
  unsigned thread_count = 1;
  string command;
  string cli_file;

  for (int i = 1; i < argc; i++) {
    string arg = argv[i];
    if (arg == "--routing" && i + 1 < argc) {
      routing_arg = argv[++i];
    } else if (arg == "--client-ip" && i + 1 < argc) {
      client_ip = argv[++i];
    } else if (arg == "--threads" && i + 1 < argc) {
      thread_count = std::stoul(argv[++i]);
    } else if (arg == "--config" && i + 1 < argc) {
      config_filename = argv[++i];
    } else if (command.empty()) {
      command = arg;
    } else if (cli_file.empty()) {
      cli_file = arg;
    }
  }

  std::transform(command.begin(), command.end(), command.begin(), ::toupper);

  if (command.empty()) {
    std::cerr << "Usage: " << usage(argv[0]) << std::endl;
    return 1;
  }

  // Commands that need a connected client require --routing and --client-ip
  if (command == "CLI" || command == "GET" || command == "PUT") {
    if (routing_arg.empty() || client_ip.empty()) {
      std::cerr << "Error: --routing and --client-ip are required for "
                << command << std::endl;
      return 1;
    }
  }

  // Build ClientConfig from command-line arguments
  annalib::ClientConfig config;
  config.ip = client_ip;

  if (!routing_arg.empty()) {
    // Split comma-separated routing IPs
    vector<string> routing_ips;
    split(routing_arg, ',', routing_ips);

    for (const string& ip : routing_ips) {
      for (unsigned t = 0; t < thread_count; t++) {
        config.routing_threads.push_back(UserRoutingThread(ip, t));
      }
    }
  }

  if (command == "CLI") {
    std::unique_ptr<KvsClient> client = annalib::make_client(config);

    if (cli_file.empty()) {
      cli_loop_interactive(client.get(), config_filename);
    } else {
      cli_loop_file(client.get(), config_filename, cli_file);
    }
  } else if (command == "START") {
    if (config_filename.empty()) {
      std::cerr << "Error: --config is required for START" << std::endl;
      return 1;
    }
    std::cout << annalib::start(config_filename)
              << " anna processes were started" << std::endl;
  } else if (command == "STOP") {
    std::cout << annalib::stop() << " anna processes were stopped"
              << std::endl;
  } else if (command == "STATUS") {
    for (const string& name : annalib::status()) {
      std::cout << name << " process is running" << std::endl;
    }
  } else if (command == "HELP") {
    std::cout << usage(argv[0]) << std::endl;
  } else {
    std::cerr << "Unrecognized command: " << command << std::endl;
    std::cerr << usage(argv[0]) << std::endl;
    return 1;
  }
}
