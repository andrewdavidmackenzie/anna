// Basic example of using the anna C++ client library.
//
// This example starts the anna server processes (monitor, route, kvs),
// connects a client, performs basic key-value operations (put, get, delete),
// and then shuts the server down.
//
// Prerequisites:
//   The anna server binaries (anna-monitor, anna-route, anna-kvs) must
//   be in your PATH. Build them first with `make server-cpp` or `make server-rust`.
//
// Building and running:
//   This example is built automatically as part of `make client-cpp`.
//   Run it from the repository root:
//     ./clients/cpp/build/examples/basic-example

#include <arpa/inet.h>
#include <sys/socket.h>
#include <unistd.h>

#include <chrono>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <sstream>
#include <thread>

#include "client_lib.hpp"

static const char* kConfigTemplate = R"(monitoring:
  scaling_alert_ip: 127.0.0.1
  ip: 127.0.0.1
routing:
  monitoring:
    - 127.0.0.1
  ip: 127.0.0.1
user:
  monitoring:
    - 127.0.0.1
  routing:
    - 127.0.0.1
  ip: 127.0.0.1
server:
  monitoring:
    - 127.0.0.1
  routing:
    - 127.0.0.1
  seed_ip: 127.0.0.1
  public_ip: 127.0.0.1
  private_ip: 127.0.0.1
  scaling_alert_ip: "NULL"
policy:
  elasticity: false
  selective-rep: false
  tiering: false
disk: /tmp/anna_example_cpp/disk
capacities:
  memory-cap: 1
  disk-cap: 0
threads:
  memory: 1
  disk: 1
  routing: 1
  benchmark: 1
replication:
  memory: 1
  disk: 0
  minimum: 1
  local: 1
ports:
  base_offset: 0
)";

// Write a config file and return the path.
static std::string generate_config() {
  std::string dir = "/tmp/anna_example_cpp";
  system(("mkdir -p " + dir + "/disk").c_str());
  std::string path = dir + "/config.yml";
  std::ofstream f(path);
  f << kConfigTemplate;
  f.close();
  return path;
}

// Wait for the routing tier to accept TCP connections.
static bool wait_for_routing(int timeout_secs = 30) {
  auto deadline =
      std::chrono::steady_clock::now() + std::chrono::seconds(timeout_secs);
  while (std::chrono::steady_clock::now() < deadline) {
    int sock = socket(AF_INET, SOCK_STREAM, 0);
    if (sock < 0) {
      std::this_thread::sleep_for(std::chrono::milliseconds(500));
      continue;
    }
    struct sockaddr_in addr;
    addr.sin_family = AF_INET;
    addr.sin_port = htons(6450);
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    bool ok = (connect(sock, (struct sockaddr*)&addr, sizeof(addr)) == 0);
    close(sock);
    if (ok) {
      std::this_thread::sleep_for(std::chrono::seconds(1));
      return true;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(500));
  }
  return false;
}

int main() {
  std::string config_path = generate_config();

  // Start the anna server
  std::cout << "Starting anna server..." << std::endl;
  int count = annalib::start(config_path);
  std::cout << "  Started " << count << " processes" << std::endl;

  if (!wait_for_routing()) {
    std::cerr << "Routing tier did not start" << std::endl;
    annalib::stop();
    return 1;
  }

  // Connect a client
  annalib::ClientConfig config;
  config.routing_ips = {"127.0.0.1"};
  config.routing_thread_count = 1;
  config.ip = "127.0.0.1";
  auto client = annalib::make_client(config, 50);

  // PUT a value
  std::cout << "\nPUT greeting = hello" << std::endl;
  auto result = annalib::put(client.get(), "greeting", "hello");
  if (!result.succeeded()) {
    std::cerr << "PUT failed" << std::endl;
    annalib::stop();
    return 1;
  }

  // GET it back
  std::string val = annalib::get(client.get(), "greeting");
  std::cout << "GET greeting = " << val << std::endl;

  // Overwrite the value
  std::cout << "\nPUT greeting = hello world" << std::endl;
  annalib::put(client.get(), "greeting", "hello world");

  val = annalib::get(client.get(), "greeting");
  std::cout << "GET greeting = " << val << std::endl;

  // PUT a second key
  std::cout << "\nPUT count = 42" << std::endl;
  annalib::put(client.get(), "count", "42");

  // DELETE the first key
  std::cout << "\nDELETE greeting" << std::endl;
  annalib::del(client.get(), "greeting");

  // Verify deletion
  val = annalib::get(client.get(), "greeting");
  if (val.empty()) {
    std::cout << "GET greeting = (deleted)" << std::endl;
  } else {
    std::cout << "GET greeting = " << val << " (unexpected)" << std::endl;
  }

  // GET the remaining key
  val = annalib::get(client.get(), "count");
  std::cout << "GET count = " << val << std::endl;

  // Stop the server
  std::cout << "\nStopping anna server..." << std::endl;
  int stopped = annalib::stop();
  std::cout << "  Stopped " << stopped << " processes" << std::endl;

  std::cout << "\nDone!" << std::endl;
  return 0;
}
