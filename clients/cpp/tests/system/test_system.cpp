#include <gtest/gtest.h>
#include "client_lib.hpp"
#include <iostream>
#include <filesystem>

namespace fs = std::filesystem;

class SystemTest : public ::testing::Test {
protected:
  // We rely on the environment variable to know where the server binary is.
  // But for the test itself, we just use the library.
};

TEST_F(SystemTest, BasicPutGet) {
  // Load config from the file created by the runner
  std::string config_path = "test_config.yml";
  
  // The configuration should define the server address.
  // For testing, we can use the routing threads from the config.
  annalib::ClientConfig config = annalib::load_config(config_path);
  
  // We need to provide our own IP for the client to be useful in a local test.
  // Let's just use a dummy IP since we only care about the routing threads.
  config.ip = "127.0.0.1";

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "test_key";
  string val = "test_value";

  // 1. Put
  kvs::KeyResponse resp = annalib::put(*client, key, val);
  ASSERT_EQ(resp.status(), kvs::Status::OK);

  // 2. Get
  string result = annalib::get(*client, key);
  EXPECT_EQ(result, val);
}

TEST_F(SystemTest, PutSetGetSet) {
  std::string config_path = "test_config.yml";
  annalib::ClientConfig config = annalib::load_config(config_path);
  config.ip = "127.0.0.1";
  
  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "test_set_key";
  set<string> values = {"a", "b", "c"};

  // 1. Put set
  kvs::KeyResponse resp = annalib::put_set(*client, key, values);
  ASSERT_EQ(resp.status(), kvs::Status::OK);

  // 2. Get set
  set<string> result = annalib::get_set(*client, key);
  EXPECT_EQ(result, values);
}
