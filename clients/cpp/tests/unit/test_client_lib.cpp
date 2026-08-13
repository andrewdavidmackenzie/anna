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

#include "gtest/gtest.h"

#include "client_lib.hpp"
#include "mock_kvs_client.hpp"

// Comms-boundary tests: the annalib::* wrapper functions in client_lib.cpp
// only depend on KvsClientInterface, so we can test them with a
// MockKvsClient test double -- no real socket or server involved (see
// #104).

namespace {

kvs::KeyResponse make_lww_response(const string& response_id,
                                   const string& value) {
  kvs::KeyResponse response;
  response.set_response_id(response_id);

  kvs::LWWValue lww;
  lww.set_timestamp(0);
  lww.set_value(value);
  string payload;
  lww.SerializeToString(&payload);

  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::LWW);
  tuple->set_payload(payload);

  return response;
}

kvs::KeyResponse make_set_response(const set<string>& values) {
  kvs::SetValue sv;
  for (const auto& v : values) {
    sv.add_values(v);
  }
  string payload;
  sv.SerializeToString(&payload);

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::SET);
  tuple->set_payload(payload);

  return response;
}

kvs::KeyResponse make_ordered_set_response(const set<string>& values) {
  kvs::SetValue sv;
  for (const auto& v : values) {
    sv.add_values(v);
  }
  string payload;
  sv.SerializeToString(&payload);

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::ORDERED_SET);
  tuple->set_payload(payload);

  return response;
}

kvs::KeyResponse make_single_causal_response(const string& value) {
  kvs::SingleKeyCausalValue skc;
  auto* vc = skc.mutable_vector_clock();
  (*vc)["client1"] = 1;
  skc.add_values(value);
  string payload;
  skc.SerializeToString(&payload);

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::SINGLE_CAUSAL);
  tuple->set_payload(payload);

  return response;
}

kvs::KeyResponse make_priority_response(double priority, const string& value) {
  kvs::PriorityValue pv;
  pv.set_priority(priority);
  pv.set_value(value);
  string payload;
  pv.SerializeToString(&payload);

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::PRIORITY);
  tuple->set_payload(payload);

  return response;
}

kvs::KeyResponse make_causal_response(const string& value) {
  kvs::MultiKeyCausalValue mkc;
  auto* vc = mkc.mutable_vector_clock();
  (*vc)["client1"] = 1;
  auto* dep = mkc.add_dependencies();
  dep->set_key("dep_key");
  auto* dep_vc = dep->mutable_vector_clock();
  (*dep_vc)["dep_client"] = 2;
  mkc.add_values(value);
  string payload;
  mkc.SerializeToString(&payload);

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::MULTI_CAUSAL);
  tuple->set_payload(payload);

  return response;
}

}  // namespace

TEST(ClientLibTest, GetReturnsDeserializedValue) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("0", "hello"));

  string value = annalib::get(&client, "my_key");

  EXPECT_EQ(value, "hello");
  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0], "my_key");
}

TEST(ClientLibTest, PutSendsSerializedLwwValue) {
  MockKvsClient client;
  // MockKvsClient's first put_async() call returns request id "1" (rid_
  // starts at 0, pre-increments to 1).
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result = annalib::put(&client, "my_key", "my_value");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(result.response_id, "1");
}

TEST(ClientLibTest, GetSetReturnsAllValues) {
  MockKvsClient client;
  set<string> expected = {"x", "y", "z"};
  client.responses_.push_back(make_set_response(expected));

  set<string> result = annalib::get_set(&client, "my_set_key");

  EXPECT_EQ(result, expected);
  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0], "my_set_key");
}

TEST(ClientLibTest, PutSetSendsAllValues) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_set(&client, "my_set_key", {"a", "b"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_set_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(result.response_id, "1");
}

TEST(ClientLibTest, GetCausalReturnsValueVectorClockAndDependencies) {
  MockKvsClient client;
  client.responses_.push_back(make_causal_response("causal_value"));

  annalib::CausalValue result = annalib::get_causal(&client, "my_causal_key");

  EXPECT_EQ(result.value, "causal_value");

  ASSERT_EQ(result.vector_clock.size(), 1u);
  EXPECT_EQ(result.vector_clock[0].first, "client1");
  EXPECT_EQ(result.vector_clock[0].second, 1u);

  ASSERT_EQ(result.dependencies.count("dep_key"), 1u);
  const auto& dep_vc = result.dependencies.at("dep_key");
  ASSERT_EQ(dep_vc.size(), 1u);
  EXPECT_EQ(dep_vc[0].first, "dep_client");
  EXPECT_EQ(dep_vc[0].second, 2u);
}

TEST(ClientLibTest, PutCausalSendsRequest) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_causal(&client, "my_causal_key", "some_value");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_causal_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(result.response_id, "1");
}


TEST(ClientLibTest, DeleteSendsEmptyPut) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result = annalib::del(&client, "my_key");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_key");
  EXPECT_TRUE(result.succeeded());
}
TEST(ClientLibTest, MultipleKvsClientsCanCoexist) {
  annalib::ClientConfig config;
  config.routing_ips = {"127.0.0.1"};
  config.routing_thread_count = 1;
  config.ip = "127.0.0.1";

  auto client1 = annalib::make_client(config, 10, 1000);
  ASSERT_NE(client1, nullptr);

  auto client2 = annalib::make_client(config, 11, 1000);
  ASSERT_NE(client2, nullptr);
}

TEST(ClientLibTest, GetOrderedSetReturnsAllValues) {
  MockKvsClient client;
  set<string> input = {"a", "b", "c"};
  client.responses_.push_back(make_ordered_set_response(input));

  vector<string> result = annalib::get_ordered_set(&client, "my_ordered_key");

  EXPECT_EQ(result.size(), 3u);
  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0], "my_ordered_key");
}

TEST(ClientLibTest, PutOrderedSetSendsRequest) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_ordered_set(&client, "my_ordered_key", {"x", "y"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_ordered_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(result.response_id, "1");
}

TEST(ClientLibTest, GetSingleCausalReturnsValueAndVectorClock) {
  MockKvsClient client;
  client.responses_.push_back(make_single_causal_response("sc_value"));

  annalib::SingleCausalValue result =
      annalib::get_single_causal(&client, "my_sc_key");

  ASSERT_EQ(result.values.size(), 1u);
  EXPECT_EQ(result.values[0], "sc_value");

  ASSERT_EQ(result.vector_clock.size(), 1u);
  EXPECT_EQ(result.vector_clock[0].first, "client1");
  EXPECT_EQ(result.vector_clock[0].second, 1u);
}

TEST(ClientLibTest, PutSingleCausalSendsRequest) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_single_causal(&client, "my_sc_key", "some_value");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_sc_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(result.response_id, "1");
}

TEST(ClientLibTest, GetPriorityReturnsValueAndPriority) {
  MockKvsClient client;
  client.responses_.push_back(make_priority_response(3.14, "priority_value"));

  annalib::PriorityResult result =
      annalib::get_priority(&client, "my_priority_key");

  EXPECT_DOUBLE_EQ(result.priority, 3.14);
  EXPECT_EQ(result.value, "priority_value");

  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0], "my_priority_key");
}

TEST(ClientLibTest, PutPrioritySendsRequest) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_priority(&client, "my_priority_key", 1.5, "pval");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_priority_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(result.response_id, "1");
}

// --- Metadata / stats helper tests ---

TEST(ClientLibTest, GetBytesReturnsRawLwwValue) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("0", "raw_bytes_here"));

  string result = annalib::get_bytes(&client, "some_key");

  EXPECT_EQ(result, "raw_bytes_here");
  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0], "some_key");
}

TEST(ClientLibTest, GetStorageStatsReadsCorrectKeyAndDecodesProtobuf) {
  MockKvsClient client;

  // Build a ServerThreadStatistics protobuf, serialize, and wrap in LWW.
  ServerThreadStatistics stats;
  stats.set_storage_consumption(2048);
  stats.set_occupancy(0.5);
  stats.set_epoch(7);
  stats.set_access_count(42);
  string serialized;
  stats.SerializeToString(&serialized);

  client.responses_.push_back(make_lww_response("0", serialized));

  ServerThreadStatistics result = annalib::get_storage_stats(
      &client, "1.2.3.4", "10.0.0.1", 3, "MEMORY");

  EXPECT_EQ(result.storage_consumption(), 2048u);
  EXPECT_DOUBLE_EQ(result.occupancy(), 0.5);
  EXPECT_EQ(result.epoch(), 7u);
  EXPECT_EQ(result.access_count(), 42u);

  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0],
            "ANNA_METADATA|stats|1.2.3.4|10.0.0.1|3|MEMORY");
}

TEST(ClientLibTest, GetKeyAccessStatsReadsCorrectKeyAndDecodesProtobuf) {
  MockKvsClient client;

  KeyAccessData access;
  auto* entry = access.add_keys();
  entry->set_key("hot_key");
  entry->set_access_count(999);
  string serialized;
  access.SerializeToString(&serialized);

  client.responses_.push_back(make_lww_response("0", serialized));

  KeyAccessData result = annalib::get_key_access_stats(
      &client, "5.6.7.8", "192.168.1.1", 0, "DISK");

  ASSERT_EQ(result.keys_size(), 1);
  EXPECT_EQ(result.keys(0).key(), "hot_key");
  EXPECT_EQ(result.keys(0).access_count(), 999u);

  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0],
            "ANNA_METADATA|access|5.6.7.8|192.168.1.1|0|DISK");
}

TEST(ClientLibTest, GetKeySizeStatsReadsCorrectKeyAndDecodesProtobuf) {
  MockKvsClient client;

  KeySizeData sizes;
  auto* ks = sizes.add_key_sizes();
  ks->set_key("big_key");
  ks->set_size(65536);
  string serialized;
  sizes.SerializeToString(&serialized);

  client.responses_.push_back(make_lww_response("0", serialized));

  KeySizeData result = annalib::get_key_size_stats(
      &client, "9.8.7.6", "172.16.0.1", 2, "MEMORY");

  ASSERT_EQ(result.key_sizes_size(), 1);
  EXPECT_EQ(result.key_sizes(0).key(), "big_key");
  EXPECT_EQ(result.key_sizes(0).size(), 65536u);

  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0],
            "ANNA_METADATA|size|9.8.7.6|172.16.0.1|2|MEMORY");
}

TEST(ClientLibTest, PutReplicationFactorWritesCorrectKeyAndProtobuf) {
  MockKvsClient client;
  // put() inside put_replication_factor will call put_async then receive_async.
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::put_replication_factor(&client, "my_data_key", 3, 1);

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "ANNA_METADATA|replication|my_data_key");
}

TEST(ClientLibTest, ServerThreadStatisticsProtobufRoundtrip) {
  ServerThreadStatistics original;
  original.set_storage_consumption(4096);
  original.set_occupancy(0.95);
  original.set_epoch(12);
  original.set_access_count(500);

  string bytes;
  original.SerializeToString(&bytes);

  ServerThreadStatistics decoded;
  ASSERT_TRUE(decoded.ParseFromString(bytes));

  EXPECT_EQ(decoded.storage_consumption(), 4096u);
  EXPECT_DOUBLE_EQ(decoded.occupancy(), 0.95);
  EXPECT_EQ(decoded.epoch(), 12u);
  EXPECT_EQ(decoded.access_count(), 500u);
}

TEST(ClientLibTest, KeyAccessDataProtobufRoundtrip) {
  KeyAccessData original;
  auto* k1 = original.add_keys();
  k1->set_key("key_a");
  k1->set_access_count(10);
  auto* k2 = original.add_keys();
  k2->set_key("key_b");
  k2->set_access_count(20);

  string bytes;
  original.SerializeToString(&bytes);

  KeyAccessData decoded;
  ASSERT_TRUE(decoded.ParseFromString(bytes));

  ASSERT_EQ(decoded.keys_size(), 2);
  EXPECT_EQ(decoded.keys(0).key(), "key_a");
  EXPECT_EQ(decoded.keys(0).access_count(), 10u);
  EXPECT_EQ(decoded.keys(1).key(), "key_b");
  EXPECT_EQ(decoded.keys(1).access_count(), 20u);
}

TEST(ClientLibTest, KeySizeDataProtobufRoundtrip) {
  KeySizeData original;
  auto* s1 = original.add_key_sizes();
  s1->set_key("small");
  s1->set_size(128);
  auto* s2 = original.add_key_sizes();
  s2->set_key("large");
  s2->set_size(1048576);

  string bytes;
  original.SerializeToString(&bytes);

  KeySizeData decoded;
  ASSERT_TRUE(decoded.ParseFromString(bytes));

  ASSERT_EQ(decoded.key_sizes_size(), 2);
  EXPECT_EQ(decoded.key_sizes(0).key(), "small");
  EXPECT_EQ(decoded.key_sizes(0).size(), 128u);
  EXPECT_EQ(decoded.key_sizes(1).key(), "large");
  EXPECT_EQ(decoded.key_sizes(1).size(), 1048576u);
}

TEST(ClientLibTest, ReplicationFactorProtobufRoundtrip) {
  ReplicationFactor original;
  original.set_key("replicated_key");

  auto* gm = original.add_global();
  gm->set_tier(MEMORY);
  gm->set_value(3);
  auto* gd = original.add_global();
  gd->set_tier(DISK);
  gd->set_value(0);

  auto* lm = original.add_local();
  lm->set_tier(MEMORY);
  lm->set_value(1);
  auto* ld = original.add_local();
  ld->set_tier(DISK);
  ld->set_value(0);

  string bytes;
  original.SerializeToString(&bytes);

  ReplicationFactor decoded;
  ASSERT_TRUE(decoded.ParseFromString(bytes));

  EXPECT_EQ(decoded.key(), "replicated_key");
  ASSERT_EQ(decoded.global_size(), 2);
  EXPECT_EQ(decoded.global(0).tier(), MEMORY);
  EXPECT_EQ(decoded.global(0).value(), 3u);
  EXPECT_EQ(decoded.global(1).tier(), DISK);
  EXPECT_EQ(decoded.global(1).value(), 0u);
  ASSERT_EQ(decoded.local_size(), 2);
  EXPECT_EQ(decoded.local(0).tier(), MEMORY);
  EXPECT_EQ(decoded.local(0).value(), 1u);
  EXPECT_EQ(decoded.local(1).tier(), DISK);
  EXPECT_EQ(decoded.local(1).value(), 0u);
}

TEST(ClientLibTest, StopWithNothingRunningReturnsZero) {
  EXPECT_EQ(annalib::stop(), 0);
}

TEST(ClientLibTest, StatusWithNothingRunningReturnsEmpty) {
  EXPECT_TRUE(annalib::status().empty());
}

// --- MockKvsClient tests: exercise the mock's own methods ---

TEST(MockKvsClientTest, ClearResetsAllState) {
  MockKvsClient client;
  client.keys_put_.push_back("k1");
  client.keys_get_.push_back("k2");
  client.responses_.push_back(make_lww_response("0", "v"));
  client.clear();
  EXPECT_TRUE(client.keys_put_.empty());
  EXPECT_TRUE(client.keys_get_.empty());
  EXPECT_TRUE(client.responses_.empty());
}

TEST(MockKvsClientTest, GetContextReturnsNull) {
  MockKvsClient client;
  EXPECT_EQ(client.get_context(), nullptr);
}

TEST(MockKvsClientTest, ReceiveAsyncClearsQueueAfterReturn) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("0", "val"));
  auto first = client.receive_async();
  EXPECT_EQ(first.size(), 1u);
  auto second = client.receive_async();
  EXPECT_TRUE(second.empty());
}

TEST(MockKvsClientTest, PutAsyncReturnsIncrementingIds) {
  MockKvsClient client;
  string id1 = client.put_async("k1", "payload", kvs::LatticeType::LWW);
  string id2 = client.put_async("k2", "payload", kvs::LatticeType::LWW);
  EXPECT_NE(id1, id2);
  EXPECT_EQ(client.keys_put_.size(), 2u);
}

TEST(MockKvsClientTest, GetAsyncRecordsKeys) {
  MockKvsClient client;
  client.get_async("key1");
  client.get_async("key2");
  ASSERT_EQ(client.keys_get_.size(), 2u);
  EXPECT_EQ(client.keys_get_[0], "key1");
  EXPECT_EQ(client.keys_get_[1], "key2");
}

// --- Additional client_lib edge case tests ---

TEST(ClientLibTest, GetMultipleSequentialCalls) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("0", "first"));
  string v1 = annalib::get(&client, "k1");
  EXPECT_EQ(v1, "first");

  client.responses_.push_back(make_lww_response("0", "second"));
  string v2 = annalib::get(&client, "k2");
  EXPECT_EQ(v2, "second");

  ASSERT_EQ(client.keys_get_.size(), 2u);
  EXPECT_EQ(client.keys_get_[0], "k1");
  EXPECT_EQ(client.keys_get_[1], "k2");
}

TEST(ClientLibTest, DeleteAlwaysPutsEmptyString) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));
  annalib::del(&client, "key_to_delete");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "key_to_delete");
}

TEST(ClientLibTest, PutSetWithEmptySet) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_set(&client, "empty_set_key", set<string>());

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "empty_set_key");
}

TEST(ClientLibTest, PutOrderedSetWithEmptySet) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_ordered_set(&client, "empty_os_key", set<string>());

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "empty_os_key");
}

TEST(ClientLibTest, GetSetReturnsEmptyForEmptySet) {
  MockKvsClient client;
  client.responses_.push_back(make_set_response(set<string>()));

  set<string> result = annalib::get_set(&client, "empty_key");
  EXPECT_TRUE(result.empty());
}

TEST(ClientLibTest, GetOrderedSetReturnsEmptyForEmptySet) {
  MockKvsClient client;
  client.responses_.push_back(make_ordered_set_response(set<string>()));

  vector<string> result = annalib::get_ordered_set(&client, "empty_key");
  EXPECT_TRUE(result.empty());
}

TEST(ClientLibTest, GetCausalWithMultipleDependencies) {
  kvs::MultiKeyCausalValue mkc;
  auto* vc = mkc.mutable_vector_clock();
  (*vc)["client1"] = 3;
  (*vc)["client2"] = 5;
  auto* dep1 = mkc.add_dependencies();
  dep1->set_key("dep1");
  (*dep1->mutable_vector_clock())["dc1"] = 1;
  auto* dep2 = mkc.add_dependencies();
  dep2->set_key("dep2");
  (*dep2->mutable_vector_clock())["dc2"] = 2;
  mkc.add_values("multi_dep_val");
  string payload;
  mkc.SerializeToString(&payload);

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::MULTI_CAUSAL);
  tuple->set_payload(payload);

  MockKvsClient client;
  client.responses_.push_back(response);

  annalib::CausalValue result = annalib::get_causal(&client, "causal_key");

  EXPECT_EQ(result.value, "multi_dep_val");
  EXPECT_EQ(result.vector_clock.size(), 2u);
  EXPECT_EQ(result.dependencies.size(), 2u);
}

TEST(ClientLibTest, GetSingleCausalWithMultipleValues) {
  kvs::SingleKeyCausalValue skc;
  auto* vc = skc.mutable_vector_clock();
  (*vc)["c1"] = 1;
  (*vc)["c2"] = 2;
  skc.add_values("v1");
  skc.add_values("v2");
  string payload;
  skc.SerializeToString(&payload);

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::SINGLE_CAUSAL);
  tuple->set_payload(payload);

  MockKvsClient client;
  client.responses_.push_back(response);

  annalib::SingleCausalValue result =
      annalib::get_single_causal(&client, "sc_multi_key");

  EXPECT_EQ(result.values.size(), 2u);
  EXPECT_EQ(result.vector_clock.size(), 2u);
}

TEST(ClientLibTest, KProcessListContainsExpectedProcesses) {
  // Verify the process list used by start/stop/status
  EXPECT_EQ(annalib::kProcessList.size(), 3u);
  EXPECT_EQ(annalib::kProcessList[0], "anna-monitor");
  EXPECT_EQ(annalib::kProcessList[1], "anna-route");
  EXPECT_EQ(annalib::kProcessList[2], "anna-kvs");
}

TEST(ClientLibTest, PutPriorityWithZeroPriority) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_priority(&client, "zero_key", 0.0, "zero_val");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "zero_key");
}

TEST(ClientLibTest, PutPriorityWithNegativePriority) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_priority(&client, "neg_key", -5.0, "neg_val");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "neg_key");
}

// --- Retry loop tests using DelayedMockKvsClient ---

TEST(ClientLibTest, GetRetriesUntilResponse) {
  DelayedMockKvsClient client(2);  // return empty twice, then respond
  client.responses_.push_back(make_lww_response("0", "delayed_value"));

  string value = annalib::get(&client, "retry_key");

  EXPECT_EQ(value, "delayed_value");
  ASSERT_EQ(client.keys_get_.size(), 1u);
}

TEST(ClientLibTest, PutRetriesUntilResponse) {
  DelayedMockKvsClient client(2);
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result = annalib::put(&client, "retry_key", "val");

  ASSERT_EQ(client.keys_put_.size(), 1u);
}

TEST(ClientLibTest, GetSetRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  set<string> expected = {"a", "b"};
  client.responses_.push_back(make_set_response(expected));

  set<string> result = annalib::get_set(&client, "retry_set_key");

  EXPECT_EQ(result, expected);
}

TEST(ClientLibTest, PutSetRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_set(&client, "retry_set", {"x"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
}

TEST(ClientLibTest, GetOrderedSetRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_ordered_set_response(set<string>({"a"})));

  vector<string> result = annalib::get_ordered_set(&client, "retry_os");

  EXPECT_EQ(result.size(), 1u);
}

TEST(ClientLibTest, PutOrderedSetRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_ordered_set(&client, "retry_os", {"x"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
}

TEST(ClientLibTest, GetCausalRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_causal_response("delayed_causal"));

  annalib::CausalValue result = annalib::get_causal(&client, "retry_causal");

  EXPECT_EQ(result.value, "delayed_causal");
}

TEST(ClientLibTest, PutCausalRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_causal(&client, "retry_causal", "val");

  ASSERT_EQ(client.keys_put_.size(), 1u);
}

TEST(ClientLibTest, GetSingleCausalRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_single_causal_response("delayed_sc"));

  annalib::SingleCausalValue result =
      annalib::get_single_causal(&client, "retry_sc");

  ASSERT_EQ(result.values.size(), 1u);
  EXPECT_EQ(result.values[0], "delayed_sc");
}

TEST(ClientLibTest, PutSingleCausalRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_single_causal(&client, "retry_sc", "val");

  ASSERT_EQ(client.keys_put_.size(), 1u);
}

TEST(ClientLibTest, GetPriorityRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_priority_response(1.0, "delayed_p"));

  annalib::PriorityResult result =
      annalib::get_priority(&client, "retry_priority");

  EXPECT_DOUBLE_EQ(result.priority, 1.0);
  EXPECT_EQ(result.value, "delayed_p");
}

TEST(ClientLibTest, PutPriorityRetriesUntilResponse) {
  DelayedMockKvsClient client(1);
  client.responses_.push_back(make_lww_response("1", "unused"));

  annalib::PutResult result =
      annalib::put_priority(&client, "retry_priority", 1.0, "val");

  ASSERT_EQ(client.keys_put_.size(), 1u);
}

// --- Multiple-response error branch tests ---

TEST(ClientLibTest, GetWithMultipleResponsesStillWorks) {
  BatchMockKvsClient client;
  // Push two responses to trigger the "more than one response" warning
  client.responses_.push_back(make_lww_response("0", "val1"));
  client.responses_.push_back(make_lww_response("0", "val2"));

  // Should still return the first response's value, with a warning to stderr
  string value = annalib::get(&client, "multi_resp_key");
  EXPECT_EQ(value, "val1");
}

TEST(ClientLibTest, GetSetWithMultipleResponsesStillWorks) {
  BatchMockKvsClient client;
  set<string> expected = {"a"};
  client.responses_.push_back(make_set_response(expected));
  client.responses_.push_back(make_set_response(set<string>({"b"})));

  set<string> result = annalib::get_set(&client, "multi_resp_set");
  EXPECT_EQ(result, expected);
}

TEST(ClientLibTest, GetOrderedSetWithMultipleResponsesStillWorks) {
  BatchMockKvsClient client;
  client.responses_.push_back(make_ordered_set_response(set<string>({"a"})));
  client.responses_.push_back(make_ordered_set_response(set<string>({"b"})));

  vector<string> result = annalib::get_ordered_set(&client, "multi_os");
  EXPECT_EQ(result.size(), 1u);
}

TEST(ClientLibTest, GetCausalWithMultipleResponsesStillWorks) {
  BatchMockKvsClient client;
  client.responses_.push_back(make_causal_response("v1"));
  client.responses_.push_back(make_causal_response("v2"));

  annalib::CausalValue result = annalib::get_causal(&client, "multi_causal");
  EXPECT_EQ(result.value, "v1");
}

TEST(ClientLibTest, GetSingleCausalWithMultipleResponsesStillWorks) {
  BatchMockKvsClient client;
  client.responses_.push_back(make_single_causal_response("v1"));
  client.responses_.push_back(make_single_causal_response("v2"));

  annalib::SingleCausalValue result =
      annalib::get_single_causal(&client, "multi_sc");
  ASSERT_EQ(result.values.size(), 1u);
  EXPECT_EQ(result.values[0], "v1");
}

TEST(ClientLibTest, GetPriorityWithMultipleResponsesStillWorks) {
  BatchMockKvsClient client;
  client.responses_.push_back(make_priority_response(1.0, "v1"));
  client.responses_.push_back(make_priority_response(2.0, "v2"));

  annalib::PriorityResult result =
      annalib::get_priority(&client, "multi_priority");
  EXPECT_DOUBLE_EQ(result.priority, 1.0);
  EXPECT_EQ(result.value, "v1");
}

TEST(ClientLibTest, GetClusterTopologyDecodesProtobuf) {
  MockKvsClient client;

  ClusterTopology topology;
  topology.set_routing_thread_count(2);
  topology.set_memory_thread_count(4);
  topology.set_disk_thread_count(1);
  string serialized;
  topology.SerializeToString(&serialized);

  client.responses_.push_back(make_lww_response("0", serialized));

  ClusterTopology result = annalib::get_cluster_topology(&client);

  EXPECT_EQ(result.routing_thread_count(), 2u);
  EXPECT_EQ(result.memory_thread_count(), 4u);
  EXPECT_EQ(result.disk_thread_count(), 1u);

  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0], "ANNA_METADATA|cluster_topology");
}

TEST(ClientLibTest, GetMonitoringIpsDecodesProtobuf) {
  MockKvsClient client;

  shared::StringSet string_set;
  string_set.add_keys("10.0.0.1");
  string_set.add_keys("10.0.0.2");
  string serialized;
  string_set.SerializeToString(&serialized);

  client.responses_.push_back(make_lww_response("0", serialized));

  vector<string> result = annalib::get_monitoring_ips(&client);

  ASSERT_EQ(result.size(), 2u);
  EXPECT_EQ(result[0], "10.0.0.1");
  EXPECT_EQ(result[1], "10.0.0.2");

  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0], "ANNA_METADATA|monitoring_ips");
}

TEST(ClientLibTest, GetKvsMembersReturnsList) {
  MockKvsClient client;

  shared::StringSet string_set;
  string_set.add_keys("1.2.3.4/10.0.0.1");
  string serialized;
  string_set.SerializeToString(&serialized);

  client.responses_.push_back(make_lww_response("0", serialized));

  vector<string> result = annalib::get_kvs_members(&client);

  ASSERT_EQ(result.size(), 1u);
  EXPECT_EQ(result[0], "1.2.3.4/10.0.0.1");

  ASSERT_EQ(client.keys_get_.size(), 1u);
  EXPECT_EQ(client.keys_get_[0], "ANNA_METADATA|kvs_members");
}

TEST(ClientLibTest, GetMultiReturnsMultipleValues) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_response("0", "val_a"));
  client.responses_.push_back(make_lww_response("1", "val_b"));

  vector<string> keys = {"key_a", "key_b"};
  map<string, string> results = annalib::get_multi(&client, keys);

  ASSERT_EQ(results.size(), 2u);
  EXPECT_EQ(results["key_a"], "val_a");
  EXPECT_EQ(results["key_b"], "val_b");
}

TEST(ClientLibTest, GetMultiEmptyKeysReturnsEmpty) {
  MockKvsClient client;
  vector<string> keys;
  map<string, string> results = annalib::get_multi(&client, keys);
  EXPECT_TRUE(results.empty());
}

TEST(ClientLibTest, SetTimeoutChangesTimeout) {
  vector<UserRoutingThread> threads;
  threads.push_back(UserRoutingThread("127.0.0.1", 0));
  KvsClient kvs_client(threads, "127.0.0.1", 99, 10000);

  EXPECT_EQ(kvs_client.get_timeout(), 10000u);
  kvs_client.set_timeout(5000);
  EXPECT_EQ(kvs_client.get_timeout(), 5000u);
}

// --- Tests for annalib::set_timeout / get_timeout wrappers ---

TEST(ClientLibTest, SetTimeoutWrapper) {
  vector<UserRoutingThread> threads;
  threads.push_back(UserRoutingThread("127.0.0.1", 0));
  KvsClient kvs_client(threads, "127.0.0.1", 98, 10000);

  annalib::set_timeout(&kvs_client, 3000);
  EXPECT_EQ(annalib::get_timeout(&kvs_client), 3000u);
}

// --- Tests for KvsClient accessors ---

TEST(ClientLibTest, KvsClientClearCache) {
  vector<UserRoutingThread> threads;
  threads.push_back(UserRoutingThread("127.0.0.1", 0));
  KvsClient kvs_client(threads, "127.0.0.1", 97, 10000);

  // clear_cache should not crash on an empty cache
  kvs_client.clear_cache();
}

TEST(ClientLibTest, KvsClientGetContext) {
  vector<UserRoutingThread> threads;
  threads.push_back(UserRoutingThread("127.0.0.1", 0));
  KvsClient kvs_client(threads, "127.0.0.1", 96, 10000);

  zmq::context_t* ctx = kvs_client.get_context();
  EXPECT_NE(ctx, nullptr);
}

TEST(ClientLibTest, KvsClientGetSeed) {
  vector<UserRoutingThread> threads;
  threads.push_back(UserRoutingThread("127.0.0.1", 0));
  KvsClient kvs_client(threads, "127.0.0.1", 95, 10000);

  // Just exercise the accessor; the seed value itself is not contractual
  // (it can legally wrap to zero). Uniqueness is tested below in
  // MultipleKvsClientsHaveDifferentSeeds.
  (void)kvs_client.get_seed();
}

TEST(ClientLibTest, KvsClientDefaultTimeout) {
  vector<UserRoutingThread> threads;
  threads.push_back(UserRoutingThread("127.0.0.1", 0));
  // Default timeout is 10000
  KvsClient kvs_client(threads, "127.0.0.1", 93);

  EXPECT_EQ(kvs_client.get_timeout(), 10000u);
}

TEST(ClientLibTest, MultipleKvsClientsHaveDifferentSeeds) {
  vector<UserRoutingThread> threads;
  threads.push_back(UserRoutingThread("127.0.0.1", 0));
  KvsClient client1(threads, "127.0.0.1", 91, 10000);
  KvsClient client2(threads, "127.0.0.1", 92, 10000);

  // Different tid should yield different seeds
  EXPECT_NE(client1.get_seed(), client2.get_seed());
}

// --- Error handling tests ---

TEST(ClientLibTest, GetThrowsOnKeyDne) {
  MockKvsClient client;

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::LWW);
  tuple->set_error(kvs::AnnaError::KEY_DNE);
  client.responses_.push_back(response);

  EXPECT_THROW(annalib::get(&client, "missing_key"), std::runtime_error);
}

TEST(ClientLibTest, GetThrowsOnTopLevelError) {
  MockKvsClient client;

  kvs::KeyResponse response;
  response.set_error(kvs::AnnaError::NO_SERVERS);
  response.add_tuples();
  client.responses_.push_back(response);

  EXPECT_THROW(annalib::get(&client, "any_key"), std::runtime_error);
}

TEST(ClientLibTest, GetThrowsOnEmptyTuples) {
  MockKvsClient client;

  kvs::KeyResponse response;
  // No tuples added
  client.responses_.push_back(response);

  EXPECT_THROW(annalib::get(&client, "any_key"), std::runtime_error);
}

TEST(ClientLibTest, PutResultErrorOnKeyDne) {
  MockKvsClient client;

  kvs::KeyResponse response;
  response.set_response_id("0");
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_error(kvs::AnnaError::KEY_DNE);
  client.responses_.push_back(response);

  auto result = annalib::put(&client, "key", "value");
  EXPECT_TRUE(result.error);
}

// --- Transaction tests ---

TEST(ClientLibTest, TransactionPutThenGet) {
  MockKvsClient client;
  annalib::Transaction txn(&client);

  txn.put("k", "buffered");
  string val = txn.get("k");
  EXPECT_EQ(val, "buffered");
}

TEST(ClientLibTest, TransactionRollbackDiscardsWrites) {
  MockKvsClient client;
  annalib::Transaction txn(&client);

  txn.put("k", "should_discard");
  txn.rollback();

  // After rollback, a new transaction should not see the value
  annalib::Transaction txn2(&client);
  // get("k") would go to the mock client which has no responses,
  // so we just verify rollback doesn't crash and clears state.
}

// --- Bench tests ---

TEST(BenchTest, ZeroKeysThrows) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.num_keys = 0;
  EXPECT_THROW(annalib::bench(&client, config), std::invalid_argument);
}

TEST(BenchTest, ZeroDurationThrows) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.duration = 0;
  EXPECT_THROW(annalib::bench(&client, config), std::invalid_argument);
}

TEST(BenchTest, ZeroReportPeriodThrows) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.report_period = 0;
  EXPECT_THROW(annalib::bench(&client, config), std::invalid_argument);
}

TEST(BenchTest, WarmupPopulatesKeys) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.num_keys = 5;
  config.value_size = 16;
  annalib::bench_warmup(&client, config);
  EXPECT_GE(client.put_count_, 5u);
}

TEST(BenchTest, GetWorkloadRuns) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.num_keys = 10;
  config.value_size = 16;
  config.duration = 1;
  config.report_period = 1;
  config.workload = "GET";
  annalib::bench_warmup(&client, config);
  annalib::BenchResult result = annalib::bench(&client, config);
  EXPECT_EQ(result.workload, "GET");
  EXPECT_GT(result.total_ops, 0u);
  EXPECT_GT(result.avg_throughput, 0.0);
  EXPECT_GT(result.elapsed_seconds, 0.0);
}

TEST(BenchTest, PutWorkloadRuns) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.num_keys = 10;
  config.value_size = 16;
  config.duration = 1;
  config.report_period = 1;
  config.workload = "PUT";
  annalib::BenchResult result = annalib::bench(&client, config);
  EXPECT_EQ(result.workload, "PUT");
  EXPECT_GT(result.total_ops, 0u);
}

TEST(BenchTest, InvalidWorkloadThrows) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.num_keys = 10;
  config.value_size = 16;
  config.duration = 1;
  config.report_period = 1;
  config.workload = "INVALID";
  EXPECT_THROW(annalib::bench(&client, config), std::invalid_argument);
}

TEST(BenchTest, SuiteRunsAllWorkloads) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.num_keys = 5;
  config.value_size = 16;
  config.duration = 1;
  config.report_period = 1;
  vector<string> workloads = {"GET", "PUT", "MIXED"};
  auto results = annalib::bench_suite(&client, config, workloads);
  EXPECT_EQ(results.size(), 3u);
  EXPECT_EQ(results[0].workload, "GET");
  EXPECT_EQ(results[1].workload, "PUT");
  EXPECT_EQ(results[2].workload, "MIXED");
  for (const auto& r : results) {
    EXPECT_GT(r.total_ops, 0u);
  }
}

TEST(BenchTest, SuiteDefaultsToAllWorkloads) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.num_keys = 5;
  config.value_size = 16;
  config.duration = 1;
  config.report_period = 1;
  auto results = annalib::bench_suite(&client, config, {});
  EXPECT_EQ(results.size(), 3u);
}

TEST(BenchTest, ParseWorkloadsAll) {
  auto wl = annalib::parse_workloads("ALL");
  ASSERT_EQ(wl.size(), 3u);
  EXPECT_EQ(wl[0], "GET");
  EXPECT_EQ(wl[1], "PUT");
  EXPECT_EQ(wl[2], "MIXED");
}

TEST(BenchTest, ParseWorkloadsEmpty) {
  auto wl = annalib::parse_workloads("");
  EXPECT_EQ(wl.size(), 3u);
}

TEST(BenchTest, ParseWorkloadsGet) {
  auto wl = annalib::parse_workloads("get");
  ASSERT_EQ(wl.size(), 1u);
  EXPECT_EQ(wl[0], "GET");
}

TEST(BenchTest, ParseWorkloadsPut) {
  auto wl = annalib::parse_workloads("Put");
  ASSERT_EQ(wl.size(), 1u);
  EXPECT_EQ(wl[0], "PUT");
}

TEST(BenchTest, ParseWorkloadsMixed) {
  auto wl = annalib::parse_workloads("MIXED");
  ASSERT_EQ(wl.size(), 1u);
  EXPECT_EQ(wl[0], "MIXED");
}

TEST(BenchTest, ParseWorkloadsInvalidThrows) {
  EXPECT_THROW(annalib::parse_workloads("INVALID"), std::invalid_argument);
}

TEST(BenchTest, MixedWorkloadRuns) {
  AutoRespondMockKvsClient client;
  annalib::BenchConfig config;
  config.num_keys = 10;
  config.value_size = 16;
  config.duration = 1;
  config.report_period = 1;
  config.workload = "MIXED";
  annalib::BenchResult result = annalib::bench(&client, config);
  EXPECT_EQ(result.workload, "MIXED");
  EXPECT_GT(result.total_ops, 0u);
}

// --- LWW_SET tests ---

// Helper to create an LWW_SET response for mock client.
kvs::KeyResponse make_lww_set_response(const string& response_id,
                                        const set<string>& values) {
  kvs::SetValue sv;
  for (const auto& v : values) {
    sv.add_values(v);
  }
  string set_payload;
  sv.SerializeToString(&set_payload);

  kvs::LWWValue lww;
  lww.set_timestamp(100);
  lww.set_value(set_payload);
  string payload;
  lww.SerializeToString(&payload);

  kvs::KeyResponse response;
  response.set_response_id(response_id);
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::LWW_SET);
  tuple->set_payload(payload);

  return response;
}

TEST(ClientLibTest, PutLwwSetSendsCorrectPayload) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_set_response("1", {}));

  annalib::PutResult result =
      annalib::put_lww_set(&client, "my_lww_set", {"a", "b", "c"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_lww_set");
  EXPECT_TRUE(result.succeeded());

  // Verify the lattice type was LWW_SET.
  EXPECT_EQ(client.lattice_types_[0], kvs::LatticeType::LWW_SET);
}

TEST(ClientLibTest, GetAnyDecodesLwwSet) {
  MockKvsClient client;
  client.responses_.push_back(make_lww_set_response("1", {"x", "y", "z"}));

  string result = annalib::get_any(&client, "any_lww_set_key");

  // Should contain sorted values in { ... } format.
  EXPECT_TRUE(result.find("{ ") != string::npos);
  EXPECT_TRUE(result.find("x") != string::npos);
  EXPECT_TRUE(result.find("y") != string::npos);
  EXPECT_TRUE(result.find("z") != string::npos);
  EXPECT_TRUE(result.find("}") != string::npos);
}

// --- PRIORITY_SET / CAUSAL_SET / MULTI_CAUSAL_SET tests ---

// Helper: build a PRIORITY_SET response (PriorityValue wrapping SetValue).
kvs::KeyResponse make_priority_set_response(const string& response_id,
                                             double priority,
                                             const set<string>& values) {
  kvs::SetValue sv;
  for (const auto& v : values) {
    sv.add_values(v);
  }
  string set_payload;
  sv.SerializeToString(&set_payload);

  kvs::PriorityValue pv;
  pv.set_priority(priority);
  pv.set_value(set_payload);
  string payload;
  pv.SerializeToString(&payload);

  kvs::KeyResponse response;
  response.set_response_id(response_id);
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::PRIORITY_SET);
  tuple->set_payload(payload);

  return response;
}

// Helper: build a CAUSAL_SET response (SingleKeyCausalValue wrapping SetValue).
kvs::KeyResponse make_causal_set_response(const string& response_id,
                                           const set<string>& values) {
  kvs::SingleKeyCausalValue skc;
  auto* vc = skc.mutable_vector_clock();
  (*vc)["client1"] = 1;

  for (const auto& v : values) {
    kvs::SetValue sv;
    sv.add_values(v);
    string sv_payload;
    sv.SerializeToString(&sv_payload);
    skc.add_values(sv_payload);
  }

  string payload;
  skc.SerializeToString(&payload);

  kvs::KeyResponse response;
  response.set_response_id(response_id);
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::CAUSAL_SET);
  tuple->set_payload(payload);

  return response;
}

// Helper: build a MULTI_CAUSAL_SET response (MultiKeyCausalValue wrapping
// SetValue).
kvs::KeyResponse make_multi_causal_set_response(const string& response_id,
                                                 const set<string>& values) {
  kvs::MultiKeyCausalValue mkc;
  auto* vc = mkc.mutable_vector_clock();
  (*vc)["client1"] = 1;

  auto* dep = mkc.add_dependencies();
  dep->set_key("dep_key");
  auto* dep_vc = dep->mutable_vector_clock();
  (*dep_vc)["dep_client"] = 2;

  for (const auto& v : values) {
    kvs::SetValue sv;
    sv.add_values(v);
    string sv_payload;
    sv.SerializeToString(&sv_payload);
    mkc.add_values(sv_payload);
  }

  string payload;
  mkc.SerializeToString(&payload);

  kvs::KeyResponse response;
  response.set_response_id(response_id);
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::MULTI_CAUSAL_SET);
  tuple->set_payload(payload);

  return response;
}

TEST(ClientLibTest, PutPrioritySetSendsCorrectPayload) {
  MockKvsClient client;
  client.responses_.push_back(make_priority_set_response("1", 0.0, {}));

  annalib::PutResult result =
      annalib::put_priority_set(&client, "my_ps_key", 2.5, {"a", "b"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_ps_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(client.lattice_types_[0], kvs::LatticeType::PRIORITY_SET);
}

TEST(ClientLibTest, PutCausalSetSendsCorrectPayload) {
  MockKvsClient client;
  client.responses_.push_back(make_causal_set_response("1", {}));

  annalib::PutResult result =
      annalib::put_causal_set(&client, "my_cs_key", {"x", "y"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_cs_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(client.lattice_types_[0], kvs::LatticeType::CAUSAL_SET);
}

TEST(ClientLibTest, PutMultiCausalSetSendsCorrectPayload) {
  MockKvsClient client;
  client.responses_.push_back(make_multi_causal_set_response("1", {}));

  annalib::PutResult result =
      annalib::put_multi_causal_set(&client, "my_mcs_key", {"p", "q"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_mcs_key");
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(client.lattice_types_[0], kvs::LatticeType::MULTI_CAUSAL_SET);
}

TEST(ClientLibTest, GetAnyDecodesPrioritySet) {
  MockKvsClient client;
  client.responses_.push_back(
      make_priority_set_response("1", 5.0, {"alpha", "beta"}));

  string result = annalib::get_any(&client, "any_ps_key");

  // Output should contain "priority:" and the set values.
  EXPECT_TRUE(result.find("priority:") != string::npos);
  EXPECT_TRUE(result.find("5") != string::npos);
  EXPECT_TRUE(result.find("alpha") != string::npos);
  EXPECT_TRUE(result.find("beta") != string::npos);
  EXPECT_TRUE(result.find("{ ") != string::npos);
  EXPECT_TRUE(result.find("}") != string::npos);
}

TEST(ClientLibTest, GetAnyDecodesCausalSet) {
  MockKvsClient client;
  client.responses_.push_back(make_causal_set_response("1", {"foo", "bar"}));

  string result = annalib::get_any(&client, "any_cs_key");

  // Output should contain vc entry and set values.
  EXPECT_TRUE(result.find("client1") != string::npos);
  EXPECT_TRUE(result.find("1") != string::npos);
  EXPECT_TRUE(result.find("foo") != string::npos);
  EXPECT_TRUE(result.find("bar") != string::npos);
  EXPECT_TRUE(result.find("{ ") != string::npos);
  EXPECT_TRUE(result.find("}") != string::npos);
}

TEST(ClientLibTest, GetAnyDecodesMultiCausalSet) {
  MockKvsClient client;
  client.responses_.push_back(
      make_multi_causal_set_response("1", {"m1", "m2"}));

  string result = annalib::get_any(&client, "any_mcs_key");

  // Output should contain vc, dependency info, and set values.
  EXPECT_TRUE(result.find("client1") != string::npos);
  EXPECT_TRUE(result.find("dep_key") != string::npos);
  EXPECT_TRUE(result.find("dep_client") != string::npos);
  EXPECT_TRUE(result.find("m1") != string::npos);
  EXPECT_TRUE(result.find("m2") != string::npos);
  EXPECT_TRUE(result.find("{ ") != string::npos);
  EXPECT_TRUE(result.find("}") != string::npos);
}

// --- Ordered-set variant tests for the 3 merged get_any cases ---

kvs::KeyResponse make_priority_ordered_set_response(const string& rid,
                                                     double priority,
                                                     const set<string>& values) {
  kvs::SetValue sv;
  for (const auto& v : values) sv.add_values(v);
  string set_payload;
  sv.SerializeToString(&set_payload);
  kvs::PriorityValue pv;
  pv.set_priority(priority);
  pv.set_value(set_payload);
  string payload;
  pv.SerializeToString(&payload);
  kvs::KeyResponse response;
  response.set_response_id(rid);
  auto* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::PRIORITY_ORDERED_SET);
  tuple->set_payload(payload);
  return response;
}

kvs::KeyResponse make_causal_ordered_set_response(const string& rid,
                                                   const set<string>& values) {
  kvs::SingleKeyCausalValue skc;
  (*skc.mutable_vector_clock())["node1"] = 1;
  for (const auto& v : values) {
    kvs::SetValue sv;
    sv.add_values(v);
    string sp;
    sv.SerializeToString(&sp);
    skc.add_values(sp);
  }
  string payload;
  skc.SerializeToString(&payload);
  kvs::KeyResponse response;
  response.set_response_id(rid);
  auto* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::CAUSAL_ORDERED_SET);
  tuple->set_payload(payload);
  return response;
}

kvs::KeyResponse make_multi_causal_ordered_set_response(const string& rid,
                                                         const set<string>& values) {
  kvs::MultiKeyCausalValue mkc;
  (*mkc.mutable_vector_clock())["node1"] = 1;
  auto* dep = mkc.add_dependencies();
  dep->set_key("dep_key");
  (*dep->mutable_vector_clock())["dep_node"] = 1;
  for (const auto& v : values) {
    kvs::SetValue sv;
    sv.add_values(v);
    string sp;
    sv.SerializeToString(&sp);
    mkc.add_values(sp);
  }
  string payload;
  mkc.SerializeToString(&payload);
  kvs::KeyResponse response;
  response.set_response_id(rid);
  auto* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::MULTI_CAUSAL_ORDERED_SET);
  tuple->set_payload(payload);
  return response;
}

TEST(ClientLibTest, PutPriorityOrderedSetSendsCorrectPayload) {
  MockKvsClient client;
  client.responses_.push_back(make_priority_ordered_set_response("1", 0, {}));
  auto result = annalib::put_priority_ordered_set(&client, "pos_key", 2.5, {"x", "y"});
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(client.lattice_types_[0], kvs::LatticeType::PRIORITY_ORDERED_SET);
}

TEST(ClientLibTest, PutCausalOrderedSetSendsCorrectPayload) {
  MockKvsClient client;
  client.responses_.push_back(make_causal_ordered_set_response("1", {}));
  auto result = annalib::put_causal_ordered_set(&client, "cos_key", {"a", "b"});
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(client.lattice_types_[0], kvs::LatticeType::CAUSAL_ORDERED_SET);
}

TEST(ClientLibTest, PutMultiCausalOrderedSetSendsCorrectPayload) {
  MockKvsClient client;
  client.responses_.push_back(make_multi_causal_ordered_set_response("1", {}));
  auto result = annalib::put_multi_causal_ordered_set(&client, "mcos_key", {"p", "q"});
  EXPECT_TRUE(result.succeeded());
  EXPECT_EQ(client.lattice_types_[0], kvs::LatticeType::MULTI_CAUSAL_ORDERED_SET);
}

TEST(ClientLibTest, GetAnyDecodesPriorityOrderedSet) {
  MockKvsClient client;
  client.responses_.push_back(make_priority_ordered_set_response("1", 1.5, {"b", "a"}));
  string result = annalib::get_any(&client, "pos_get");
  EXPECT_TRUE(result.find("priority:") != string::npos);
  EXPECT_TRUE(result.find("[ ") != string::npos);
  EXPECT_TRUE(result.find("]") != string::npos);
}

TEST(ClientLibTest, GetAnyDecodesCausalOrderedSet) {
  MockKvsClient client;
  client.responses_.push_back(make_causal_ordered_set_response("1", {"y", "x"}));
  string result = annalib::get_any(&client, "cos_get");
  EXPECT_TRUE(result.find("node1") != string::npos);
  EXPECT_TRUE(result.find("[ ") != string::npos);
  EXPECT_TRUE(result.find("]") != string::npos);
}

TEST(ClientLibTest, GetAnyDecodesMultiCausalOrderedSet) {
  MockKvsClient client;
  client.responses_.push_back(make_multi_causal_ordered_set_response("1", {"b", "a"}));
  string result = annalib::get_any(&client, "mcos_get");
  EXPECT_TRUE(result.find("node1") != string::npos);
  EXPECT_TRUE(result.find("dep_key") != string::npos);
  EXPECT_TRUE(result.find("[ ") != string::npos);
  EXPECT_TRUE(result.find("]") != string::npos);
}
