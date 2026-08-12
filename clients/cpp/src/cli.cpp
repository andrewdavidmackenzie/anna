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
#include <iomanip>
#include <string>
#include "client_utils.hpp"

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
  return "Valid commands are:\n"
         "  GET {key}                       - get the value of any key (auto-detects type)\n"
         "  MGET {key1} {key2} ...          - get multiple keys at once\n"
         "  PUT {key} {value}               - store a value (LWW, default)\n"
         "  PUT set {key} {vals...}         - store a set (union merge)\n"
         "  PUT ordered_set {key} {vals...} - store an ordered set\n"
         "  PUT lww_set {key} {vals...}     - store a set (LWW, replaces on write)\n"
         "  PUT lww_ordered_set {key} {vals...} - store an ordered set (LWW)\n"
         "  PUT union {key} {value}         - append a value (accumulates via union)\n"
         "  PUT priority {key} {pri} {val}  - store with priority (lowest wins)\n"
         "  PUT priority_set {key} {pri} {vals...} - store a set with priority\n"
         "  PUT priority_ordered_set {key} {pri} {vals...} - store an ordered set with priority\n"
         "  PUT causal {key} {value}        - store with multi-key causal consistency\n"
         "  PUT single_causal {key} {value} - store with single-key causal consistency\n"
         "  PUT causal_set {key} {vals...}  - store a set with single-key causal consistency\n"
         "  PUT causal_ordered_set {key} {vals...} - store an ordered set with single-key causal\n"
         "  PUT multi_causal_set {key} {vals...} - store a set with multi-key causal consistency\n"
         "  PUT multi_causal_ordered_set {key} {vals...} - store an ordered set with multi-key causal\n"
         "  DEL {key}                       - delete a key (alias: DELETE)\n"
         "  SADD {key} {member} [member...] - add members to an OR-Set (not yet implemented)\n"
         "  SREM {key} {member} [member...] - remove members from an OR-Set (not yet implemented)\n"
         "  SMEMBERS {key}                  - list members of an OR-Set (not yet implemented)\n"
         "  SUBSCRIBE {key1} [key2...]      - subscribe to value changes on keys (not available in C++ CLI, use Rust CLI or library API)\n"
         "  BENCH [keys] [value_size] [duration] [workload] - run a benchmark\n"
         "  START, STOP, STATUS, HELP, EXIT";
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

  // Helper: check if a string is a known lattice type name.
  auto is_type_name = [](const string& s) -> bool {
    string lower = s;
    std::transform(lower.begin(), lower.end(), lower.begin(), ::tolower);
    return lower == "lww" || lower == "set" || lower == "ordered_set" ||
           lower == "lww_set" || lower == "lww_ordered_set" || lower == "union" ||
           lower == "priority" || lower == "priority_set" ||
           lower == "priority_ordered_set" ||
           lower == "causal" || lower == "single_causal" ||
           lower == "causal_set" || lower == "causal_ordered_set" ||
           lower == "multi_causal_set" || lower == "multi_causal_ordered_set";
  };

  try {
  // Unified GET: auto-detect lattice type from server response.
  if (command == "GET" || command == "GET_SET" || command == "GET_ORDERED_SET" ||
      command == "GET_CAUSAL" || command == "GET_SINGLE_CAUSAL" ||
      command == "GET_PRIORITY") {
    if (v.size() < 2) {
      std::cerr << "Usage: GET <key>" << std::endl;
    } else {
      try {
        std::cout << annalib::get_any(client, v[1]) << std::endl;
      } catch (const std::runtime_error &e) {
        if (string(e.what()).find("KEY_DNE") != string::npos) {
          std::cout << "(nil)" << std::endl;
        } else {
          throw;
        }
      }
    }
  } else if (command == "DEL" || command == "DELETE") {
    if (v.size() < 2) {
      std::cerr << "Usage: DEL <key>" << std::endl;
    } else if (!annalib::del(client, v[1]).succeeded()) {
      std::cerr << "Error: DEL failed" << std::endl;
    }
  } else if (command == "MGET") {
    if (v.size() < 2) {
      std::cerr << "Usage: MGET <key1> [key2 ...]" << std::endl;
    } else {
      vector<string> keys(v.begin() + 1, v.end());
      auto results = annalib::get_multi(client, keys);
      for (const auto& key : keys) {
        auto it = results.find(key);
        if (it != results.end()) {
          std::cout << key << ": " << it->second << std::endl;
        } else {
          std::cout << key << ": (not found)" << std::endl;
        }
      }
    }
  } else if (command == "SADD") {
    // TODO: requires or_set_add in C++ client library
    if (v.size() < 3) {
      std::cerr << "Usage: SADD <key> <member> [member ...]" << std::endl;
    } else {
      std::cerr << "Error: SADD is not yet implemented (requires OR-Set add support in C++ client library). Use the Rust CLI (anna) for this command." << std::endl;
    }
  } else if (command == "SREM") {
    // TODO: requires or_set_remove in C++ client library
    if (v.size() < 3) {
      std::cerr << "Usage: SREM <key> <member> [member ...]" << std::endl;
    } else {
      std::cerr << "Error: SREM is not yet implemented (requires OR-Set remove support in C++ client library). Use the Rust CLI (anna) for this command." << std::endl;
    }
  } else if (command == "SMEMBERS") {
    // TODO: requires or_set_get / get_set in C++ client library
    if (v.size() < 2) {
      std::cerr << "Usage: SMEMBERS <key>" << std::endl;
    } else {
      std::cerr << "Error: SMEMBERS is not yet implemented (requires OR-Set get support in C++ client library). Use the Rust CLI (anna) for this command." << std::endl;
    }
  } else if (command == "SUBSCRIBE") {
    if (v.size() < 2) {
      std::cerr << "Usage: SUBSCRIBE <key1> [key2 ...]" << std::endl;
    } else {
      std::cerr << "Error: SUBSCRIBE requires server IP and cache IP configuration "
                << "(use ValueChangeSubscriber directly from the C++ client library)" << std::endl;
    }
  } else if (command == "PUT" || command == "PUT_SET" ||
             command == "PUT_ORDERED_SET" || command == "PUT_CAUSAL" ||
             command == "PUT_SINGLE_CAUSAL" || command == "PUT_PRIORITY") {
    // Determine the type prefix and adjust argument positions.
    string type_name;
    size_t key_idx, val_start;

    // 4+ tokens with a type name: typed PUT.
    // 3 tokens or unrecognized first arg: LWW (preserves keys named "set" etc.)
    if (command == "PUT" && v.size() >= 4 && is_type_name(v[1])) {
      type_name = v[1];
      std::transform(type_name.begin(), type_name.end(), type_name.begin(), ::tolower);
      key_idx = 2;
      val_start = 3;
    } else if (command == "PUT") {
      if (v.size() < 3) {
        std::cerr << "Usage: PUT [type] <key> <value(s)>" << std::endl;
        return;
      }
      type_name = "lww";
      key_idx = 1;
      val_start = 2;
    } else if (command == "PUT_SET") {
      if (v.size() < 3) { std::cerr << "Usage: PUT_SET <key> <values...>" << std::endl; return; }
      type_name = "set"; key_idx = 1; val_start = 2;
    } else if (command == "PUT_ORDERED_SET") {
      if (v.size() < 3) { std::cerr << "Usage: PUT_ORDERED_SET <key> <values...>" << std::endl; return; }
      type_name = "ordered_set"; key_idx = 1; val_start = 2;
    } else if (command == "PUT_CAUSAL") {
      if (v.size() < 3) { std::cerr << "Usage: PUT_CAUSAL <key> <value>" << std::endl; return; }
      type_name = "causal"; key_idx = 1; val_start = 2;
    } else if (command == "PUT_SINGLE_CAUSAL") {
      if (v.size() < 3) { std::cerr << "Usage: PUT_SINGLE_CAUSAL <key> <value>" << std::endl; return; }
      type_name = "single_causal"; key_idx = 1; val_start = 2;
    } else { // PUT_PRIORITY
      if (v.size() < 4) { std::cerr << "Usage: PUT_PRIORITY <key> <priority> <value>" << std::endl; return; }
      type_name = "priority"; key_idx = 1; val_start = 2;
    }

    if (key_idx >= v.size() || val_start > v.size()) {
      std::cerr << "Usage: PUT [type] <key> <value(s)>" << std::endl;
    } else {
      string key = v[key_idx];
      bool ok = true;

      if (type_name == "lww") {
        if (val_start >= v.size()) { std::cerr << "Usage: PUT <key> <value>" << std::endl; return; }
        ok = annalib::put(client, key, v[val_start]).succeeded();
      } else if (type_name == "set") {
        set<string> values;
        for (size_t i = val_start; i < v.size(); i++) values.insert(v[i]);
        ok = annalib::put_set(client, key, values).succeeded();
      } else if (type_name == "ordered_set") {
        // Use vector to preserve insertion order (std::set would sort).
        vector<string> values;
        for (size_t i = val_start; i < v.size(); i++) values.push_back(v[i]);
        // put_ordered_set takes set<string> — this is a known limitation
        // of the C++ client library API (see #494).
        set<string> value_set(values.begin(), values.end());
        ok = annalib::put_ordered_set(client, key, value_set).succeeded();
      } else if (type_name == "lww_set") {
        set<string> values;
        for (size_t i = val_start; i < v.size(); i++) values.insert(v[i]);
        ok = annalib::put_lww_set(client, key, values).succeeded();
      } else if (type_name == "lww_ordered_set") {
        vector<string> values;
        for (size_t i = val_start; i < v.size(); i++) values.push_back(v[i]);
        ok = annalib::put_lww_ordered_set(client, key, values).succeeded();
      } else if (type_name == "union") {
        ok = annalib::put_union_scalar(client, key, v[val_start]).succeeded();
      } else if (type_name == "priority") {
        if (val_start + 1 >= v.size()) { std::cerr << "Usage: PUT priority <key> <priority> <value>" << std::endl; return; }
        double priority = std::stod(v[val_start]);
        ok = annalib::put_priority(client, key, priority, v[val_start + 1]).succeeded();
      } else if (type_name == "causal") {
        ok = annalib::put_causal(client, key, v[val_start]).succeeded();
      } else if (type_name == "single_causal") {
        ok = annalib::put_single_causal(client, key, v[val_start]).succeeded();
      } else if (type_name == "priority_set") {
        if (val_start + 1 >= v.size()) { std::cerr << "Usage: PUT priority_set <key> <priority> <values...>" << std::endl; return; }
        double priority = std::stod(v[val_start]);
        set<string> values;
        for (size_t i = val_start + 1; i < v.size(); i++) values.insert(v[i]);
        ok = annalib::put_priority_set(client, key, priority, values).succeeded();
      } else if (type_name == "priority_ordered_set") {
        if (val_start + 1 >= v.size()) { std::cerr << "Usage: PUT priority_ordered_set <key> <priority> <values...>" << std::endl; return; }
        double priority = std::stod(v[val_start]);
        set<string> values;
        for (size_t i = val_start + 1; i < v.size(); i++) values.insert(v[i]);
        ok = annalib::put_priority_ordered_set(client, key, priority, values).succeeded();
      } else if (type_name == "causal_set") {
        set<string> values;
        for (size_t i = val_start; i < v.size(); i++) values.insert(v[i]);
        ok = annalib::put_causal_set(client, key, values).succeeded();
      } else if (type_name == "causal_ordered_set") {
        set<string> values;
        for (size_t i = val_start; i < v.size(); i++) values.insert(v[i]);
        ok = annalib::put_causal_ordered_set(client, key, values).succeeded();
      } else if (type_name == "multi_causal_set") {
        set<string> values;
        for (size_t i = val_start; i < v.size(); i++) values.insert(v[i]);
        ok = annalib::put_multi_causal_set(client, key, values).succeeded();
      } else if (type_name == "multi_causal_ordered_set") {
        set<string> values;
        for (size_t i = val_start; i < v.size(); i++) values.insert(v[i]);
        ok = annalib::put_multi_causal_ordered_set(client, key, values).succeeded();
      }

      if (!ok) {
        std::cerr << "Error: PUT " << type_name << " failed" << std::endl;
      }
    }
  } else if (command == "BENCH") {
    annalib::BenchConfig bc;
    if (v.size() > 1) bc.num_keys = std::stoul(v[1]);
    if (v.size() > 2) bc.value_size = std::stoul(v[2]);
    if (v.size() > 3) bc.duration = std::stoul(v[3]);
    string wl_arg = (v.size() > 4) ? v[4] : "ALL";
    annalib::bench_suite(client, bc, annalib::parse_workloads(wl_arg));
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
  } else if (command == "MEMBERS") {
    auto members = annalib::get_kvs_members(client);
    if (members.empty()) {
      std::cout << "(no members found)" << std::endl;
    } else {
      for (const auto& m : members) {
        std::cout << m << std::endl;
      }
    }
  } else if (command == "TOPOLOGY") {
    auto members = annalib::get_kvs_members(client);
    auto topo = annalib::get_cluster_topology(client);
    if (members.empty()) {
      std::cout << "(no members found)" << std::endl;
    } else {
      std::cout << "Nodes: " << members.size() << std::endl;
      std::cout << "Memory threads/node: " << topo.memory_thread_count() << std::endl;
      std::cout << "Disk threads/node: " << topo.disk_thread_count() << std::endl;
      std::cout << "Routing threads/node: " << topo.routing_thread_count() << std::endl;
      std::cout << "---" << std::endl;
      for (const auto& m : members) {
        std::cout << "  " << m << std::endl;
      }
    }
  } else if (command == "HELP") {
    std::cout << cli_usage() << std::endl;
  } else if (command == "EXIT") {
    std::exit(EXIT_SUCCESS);
  } else {
    std::cout << "Unrecognized command: " << command << std::endl
              << cli_usage() << std::endl;
  }
  } catch (const std::exception& e) {
    std::cerr << "Error: " << e.what() << std::endl;
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
         "Valid commands are help, start, stop, status, cli, bench\n"
         "\nbench options:\n"
         "  --keys <n>       key space size (default: 1000)\n"
         "  --value-size <n> value size in bytes (default: 256)\n"
         "  --duration <n>   benchmark duration in seconds (default: 10)\n"
         "  --report <n>     seconds between reports (default: 2)\n"
         "  --workload <w>   GET, PUT, or MIXED (default: all three)\n";
}

}  // namespace

int main(int argc, char* argv[]) {
  // Parse named arguments:
  //   --routing ip1,ip2,...   routing tier IP addresses (required)
  //   --client-ip ip         this client's IP address (required)
  //   --threads n            routing threads per IP (default: 1)
  //   --config path          config file for server start/stop (optional)
  //   <command>              CLI / START / STOP / STATUS / HELP / BENCH
  //   [file]                 command file when command is CLI
  //   --keys n               bench: key space size
  //   --value-size n         bench: value size in bytes
  //   --duration n           bench: total duration in seconds
  //   --report n             bench: report period in seconds
  //   --workload w           bench: GET, PUT, MIXED, or ALL
  string routing_arg;
  string client_ip;
  string config_filename;
  unsigned thread_count = 1;
  string command;
  string cli_file;

  // Bench-specific args
  annalib::BenchConfig bench_config;
  string bench_workload;

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
    } else if (arg == "--keys" && i + 1 < argc) {
      try { bench_config.num_keys = std::stoul(argv[++i]); }
      catch (...) { std::cerr << "Error: invalid --keys value" << std::endl; return 1; }
    } else if (arg == "--value-size" && i + 1 < argc) {
      try { bench_config.value_size = std::stoul(argv[++i]); }
      catch (...) { std::cerr << "Error: invalid --value-size value" << std::endl; return 1; }
    } else if (arg == "--duration" && i + 1 < argc) {
      try { bench_config.duration = std::stoul(argv[++i]); }
      catch (...) { std::cerr << "Error: invalid --duration value" << std::endl; return 1; }
    } else if (arg == "--report" && i + 1 < argc) {
      try { bench_config.report_period = std::stoul(argv[++i]); }
      catch (...) { std::cerr << "Error: invalid --report value" << std::endl; return 1; }
    } else if (arg == "--workload" && i + 1 < argc) {
      bench_workload = argv[++i];
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
  if (command == "CLI" || command == "GET" || command == "PUT" ||
      command == "BENCH") {
    if (routing_arg.empty() || client_ip.empty()) {
      std::cerr << "Error: --routing and --client-ip are required for "
                << command << std::endl;
      return 1;
    }
  }

  // Build ClientConfig from command-line arguments
  annalib::ClientConfig config;
  config.ip = client_ip;
  config.routing_thread_count = thread_count;

  if (!routing_arg.empty()) {
    // Split comma-separated routing IPs
    split(routing_arg, ',', config.routing_ips);
  }

  if (command == "BENCH") {
    try {
      std::unique_ptr<KvsClient> client = annalib::make_client(config);
      annalib::bench_suite(client.get(), bench_config,
                           annalib::parse_workloads(bench_workload));
    } catch (const std::exception& e) {
      std::cerr << "Error: " << e.what() << std::endl;
      return 1;
    }
  } else if (command == "CLI") {
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
