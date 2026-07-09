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

#include "common.hpp"

// Pure-function tests: serialize()/deserialize_*() and generate_timestamp()
// are plain protobuf/data-structure transforms with no socket or server
// dependency, so these need no mocking at all (see #104).

TEST(SerializationTest, LwwRoundTrip) {
  LWWPairLattice<string> original(
      TimestampValuePair<string>(generate_timestamp(0), "hello"));

  string serialized = serialize(original);
  LWWPairLattice<string> result = deserialize_lww(serialized);

  EXPECT_EQ(result.reveal().value, "hello");
  EXPECT_EQ(result.reveal().timestamp, original.reveal().timestamp);
}

TEST(SerializationTest, SetRoundTrip) {
  set<string> original = {"a", "b", "c"};

  string serialized = serialize(original);
  SetLattice<string> result = deserialize_set(serialized);

  EXPECT_EQ(result.reveal(), original);
}

TEST(SerializationTest, EmptySetRoundTrip) {
  set<string> original;

  string serialized = serialize(original);
  SetLattice<string> result = deserialize_set(serialized);

  EXPECT_TRUE(result.reveal().empty());
}

TEST(SerializationTest, MultiKeyCausalRoundTrip) {
  MultiKeyCausalPayload<SetLattice<string>> payload;
  payload.vector_clock.insert("client1", 3);
  payload.dependencies.insert(
      "dep_key",
      VectorClock(map<string, MaxLattice<unsigned>>({{"dep_client", 7}})));
  payload.value.insert("causal_value");

  MultiKeyCausalLattice<SetLattice<string>> original(payload);
  string serialized = serialize(original);

  kvs::MultiKeyCausalValue parsed = deserialize_multi_key_causal(serialized);
  MultiKeyCausalLattice<SetLattice<string>> result(
      to_multi_key_causal_payload(parsed));

  EXPECT_EQ(*(result.reveal().value.reveal().begin()), "causal_value");

  auto vc = result.reveal().vector_clock.reveal();
  ASSERT_EQ(vc.count("client1"), 1u);
  EXPECT_EQ(vc.at("client1").reveal(), 3u);

  auto deps = result.reveal().dependencies.reveal();
  ASSERT_EQ(deps.count("dep_key"), 1u);
  auto dep_vc = deps.at("dep_key").reveal();
  ASSERT_EQ(dep_vc.count("dep_client"), 1u);
  EXPECT_EQ(dep_vc.at("dep_client").reveal(), 7u);
}

TEST(SerializationTest, GenerateTimestampEncodesId) {
  // generate_timestamp appends `id` as the low-order digits of the
  // current time, so decoding it with the same id should recover a
  // multiple of the current time exactly (mod the digit count of id).
  unsigned long long ts0 = generate_timestamp(0);
  unsigned long long ts5 = generate_timestamp(5);

  EXPECT_EQ(ts0 % 10, 0u);
  EXPECT_EQ(ts5 % 10, 5u);
}

TEST(SplitTest, SplitsOnDelimiter) {
  vector<string> parts;
  split("GET foo bar", ' ', parts);

  ASSERT_EQ(parts.size(), 3u);
  EXPECT_EQ(parts[0], "GET");
  EXPECT_EQ(parts[1], "foo");
  EXPECT_EQ(parts[2], "bar");
}

TEST(SplitTest, EmptyStringProducesNoElements) {
  vector<string> parts;
  split("", ' ', parts);

  EXPECT_TRUE(parts.empty());
}
