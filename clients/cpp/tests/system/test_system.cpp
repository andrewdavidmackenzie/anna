#include <gtest/gtest.h>
#include "client_lib.hpp"
#include <iostream>
#include <filesystem>
#include <cstdlib>

namespace fs = std::filesystem;

class SystemTest : public ::testing::Test {
protected:
  // Build a ClientConfig from environment variables or defaults.
  // The system test runner sets ANNA_ROUTING_IP and ANNA_CLIENT_IP.
  annalib::ClientConfig make_test_config() {
    annalib::ClientConfig config;

    const char* routing_ip = std::getenv("ANNA_ROUTING_IP");
    config.ip = "127.0.0.1";
    config.routing_ips.push_back(routing_ip ? routing_ip : "127.0.0.1");
    config.routing_thread_count = 1;

    return config;
  }
};

TEST_F(SystemTest, BasicPutGet) {
  annalib::ClientConfig config = make_test_config();

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "test_key";
  string val = "test_value";

  // 1. Put
  ASSERT_TRUE(annalib::put(client.get(), key, val).succeeded());

  // 2. Get
  string result = annalib::get(client.get(), key);
  EXPECT_EQ(result, val);
}

TEST_F(SystemTest, PutSetGetSet) {
  annalib::ClientConfig config = make_test_config();

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "test_set_key";
  set<string> values = {"a", "b", "c"};

  // 1. Put set
  ASSERT_TRUE(annalib::put_set(client.get(), key, values).succeeded());

  // 2. Get set
  set<string> result = annalib::get_set(client.get(), key);
  EXPECT_EQ(result, values);
}

TEST_F(SystemTest, OrderedSetPutGet) {
  annalib::ClientConfig config = make_test_config();

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_oset";
  set<string> values = {"alpha", "beta", "gamma"};

  // 1. Put ordered set
  ASSERT_TRUE(annalib::put_ordered_set(client.get(), key, values).succeeded());

  // 2. Get ordered set
  vector<string> result = annalib::get_ordered_set(client.get(), key);
  EXPECT_EQ(result.size(), 3);
}

TEST_F(SystemTest, SingleCausalPutGet) {
  annalib::ClientConfig config = make_test_config();

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_sc";
  string val = "sc_hello";

  // 1. Put single causal
  ASSERT_TRUE(annalib::put_single_causal(client.get(), key, val).succeeded());

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
  annalib::ClientConfig config = make_test_config();

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_mc";
  string val = "mc_hello";

  // 1. Put causal
  ASSERT_TRUE(annalib::put_causal(client.get(), key, val).succeeded());

  // 2. Get causal
  annalib::CausalValue result = annalib::get_causal(client.get(), key);
  EXPECT_EQ(result.value, val);
}

TEST_F(SystemTest, PriorityPutGet) {
  annalib::ClientConfig config = make_test_config();

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_pri";
  double priority = 1.5;
  string val = "important";

  // 1. Put priority
  ASSERT_TRUE(annalib::put_priority(client.get(), key, priority, val).succeeded());

  // 2. Get priority
  annalib::PriorityResult result = annalib::get_priority(client.get(), key);
  EXPECT_NEAR(result.priority, 1.5, 1e-9);
  EXPECT_EQ(result.value, val);
}

TEST_F(SystemTest, DeleteKey) {
  annalib::ClientConfig config = make_test_config();

  auto client = annalib::make_client(config, 0, 5000);
  ASSERT_NE(client, nullptr);

  string key = "sys_del";
  string val = "to_delete";

  // 1. Put
  ASSERT_TRUE(annalib::put(client.get(), key, val).succeeded());

  // 2. Get - verify value exists
  string result = annalib::get(client.get(), key);
  EXPECT_EQ(result, val);

  // 3. Delete
  ASSERT_TRUE(annalib::del(client.get(), key).succeeded());
}

int main(int argc, char **argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
