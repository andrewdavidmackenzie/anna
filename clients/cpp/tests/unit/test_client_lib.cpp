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

  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::LWW);
  tuple->set_payload(serialize(
      LWWPairLattice<string>(TimestampValuePair<string>(0, value))));

  return response;
}

kvs::KeyResponse make_set_response(const set<string>& values) {
  kvs::KeyResponse response;

  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::SET);
  tuple->set_payload(serialize(values));

  return response;
}

kvs::KeyResponse make_causal_response(const string& value) {
  MultiKeyCausalPayload<SetLattice<string>> payload;
  payload.vector_clock.insert("client1", 1);
  payload.dependencies.insert(
      "dep_key",
      VectorClock(map<string, MaxLattice<unsigned>>({{"dep_client", 2}})));
  payload.value.insert(value);

  MultiKeyCausalLattice<SetLattice<string>> lattice(payload);

  kvs::KeyResponse response;
  kvs::KeyTuple* tuple = response.add_tuples();
  tuple->set_lattice_type(kvs::LatticeType::MULTI_CAUSAL);
  tuple->set_payload(serialize(lattice));

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

  kvs::KeyResponse response = annalib::put(&client, "my_key", "my_value");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_key");
  EXPECT_EQ(response.response_id(), "1");
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

  kvs::KeyResponse response =
      annalib::put_set(&client, "my_set_key", {"a", "b"});

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_set_key");
  EXPECT_EQ(response.response_id(), "1");
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

  kvs::KeyResponse response =
      annalib::put_causal(&client, "my_causal_key", "some_value");

  ASSERT_EQ(client.keys_put_.size(), 1u);
  EXPECT_EQ(client.keys_put_[0], "my_causal_key");
  EXPECT_EQ(response.response_id(), "1");
}

TEST(ClientLibTest, MultipleKvsClientsShareLogger) {
  spdlog::drop("client_log");

  vector<UserRoutingThread> routing_threads;
  routing_threads.push_back(UserRoutingThread("127.0.0.1", 0));

  {
    auto client1 = annalib::make_client(
        annalib::ClientConfig{routing_threads, "127.0.0.1"}, 10, 1000);
    ASSERT_NE(client1, nullptr);

    auto client2 = annalib::make_client(
        annalib::ClientConfig{routing_threads, "127.0.0.1"}, 11, 1000);
    ASSERT_NE(client2, nullptr);
  }

  spdlog::drop("client_log");
}

// start()/stop()/status() are unimplemented stubs pending #103; these tests
// just pin down the current (documented) stub behavior so a future change
// is a deliberate decision, not an accidental regression.
TEST(ClientLibTest, StubProcessManagementFunctions) {
  EXPECT_EQ(annalib::start("unused-config-path"), 3);
  EXPECT_EQ(annalib::stop(), 3);
  EXPECT_TRUE(annalib::status().empty());
}
