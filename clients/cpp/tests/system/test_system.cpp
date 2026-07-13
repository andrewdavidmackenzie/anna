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
  std::string config_path = "test_config.yml";
  annalib::ClientConfig config = annalib::load_config(config_path);
  config.ip = "127.0.0.1";

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "test_key";
  string val = "test_value";

  // 1. Put
  kvs::KeyResponse resp = annalib::put(client.get(), key, val);
  ASSERT_EQ(resp.error(), kvs::AnnaError::NO_ERROR);

  // 2. Get
  string result = annalib::get(client.get(), key);
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
  kvs::KeyResponse resp = annalib::put_set(client.get(), key, values);
  ASSERT_EQ(resp.error(), kvs::AnnaError::NO_ERROR);

  // 2. Get set
  set<string> result = annalib::get_set(client.get(), key);
  EXPECT_EQ(result, values);
}

int main(int argc, char **argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
