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

TEST_F(SystemTest, OrderedSetPutGet) {
  std::string config_path = "test_config.yml";
  annalib::ClientConfig config = annalib::load_config(config_path);
  config.ip = "127.0.0.1";

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_oset";
  set<string> values = {"alpha", "beta", "gamma"};

  // 1. Put ordered set
  kvs::KeyResponse resp = annalib::put_ordered_set(client.get(), key, values);
  ASSERT_EQ(resp.error(), kvs::AnnaError::NO_ERROR);

  // 2. Get ordered set
  vector<string> result = annalib::get_ordered_set(client.get(), key);
  EXPECT_EQ(result.size(), 3);
}

TEST_F(SystemTest, SingleCausalPutGet) {
  std::string config_path = "test_config.yml";
  annalib::ClientConfig config = annalib::load_config(config_path);
  config.ip = "127.0.0.1";

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_sc";
  string val = "sc_hello";

  // 1. Put single causal
  kvs::KeyResponse resp =
      annalib::put_single_causal(client.get(), key, val);
  ASSERT_EQ(resp.error(), kvs::AnnaError::NO_ERROR);

  // 2. Get single causal
  annalib::SingleCausalValue result =
      annalib::get_single_causal(client.get(), key);
  EXPECT_FALSE(result.values.empty());
  bool found = false;
  for (const auto& v : result.values) {
    if (v == val) {
      found = true;
      break;
    }
  }
  EXPECT_TRUE(found) << "Expected values to contain 'sc_hello'";
  EXPECT_FALSE(result.vector_clock.empty());
}

TEST_F(SystemTest, MultiCausalPutGet) {
  std::string config_path = "test_config.yml";
  annalib::ClientConfig config = annalib::load_config(config_path);
  config.ip = "127.0.0.1";

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_mc";
  string val = "mc_hello";

  // 1. Put causal
  kvs::KeyResponse resp = annalib::put_causal(client.get(), key, val);
  ASSERT_EQ(resp.error(), kvs::AnnaError::NO_ERROR);

  // 2. Get causal
  annalib::CausalValue result = annalib::get_causal(client.get(), key);
  EXPECT_EQ(result.value, val);
}

TEST_F(SystemTest, PriorityPutGet) {
  std::string config_path = "test_config.yml";
  annalib::ClientConfig config = annalib::load_config(config_path);
  config.ip = "127.0.0.1";

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_pri";
  double priority = 1.5;
  string val = "important";

  // 1. Put priority
  kvs::KeyResponse resp =
      annalib::put_priority(client.get(), key, priority, val);
  ASSERT_EQ(resp.error(), kvs::AnnaError::NO_ERROR);

  // 2. Get priority
  annalib::PriorityResult result = annalib::get_priority(client.get(), key);
  EXPECT_NEAR(result.priority, 1.5, 1e-9);
  EXPECT_EQ(result.value, val);
}

TEST_F(SystemTest, DeleteKey) {
  std::string config_path = "test_config.yml";
  annalib::ClientConfig config = annalib::load_config(config_path);
  config.ip = "127.0.0.1";

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_del";
  string val = "to_delete";

  // 1. Put
  kvs::KeyResponse resp = annalib::put(client.get(), key, val);
  ASSERT_EQ(resp.error(), kvs::AnnaError::NO_ERROR);

  // 2. Get - verify value exists
  string result = annalib::get(client.get(), key);
  EXPECT_EQ(result, val);

  // 3. Delete
  kvs::KeyResponse del_resp = annalib::del(client.get(), key);
  ASSERT_EQ(del_resp.error(), kvs::AnnaError::NO_ERROR);
}

int main(int argc, char **argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
