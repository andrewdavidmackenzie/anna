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

#include "kvs/kvs_handlers.hpp"

TEST_F(ServerHandlerTest, UserGetLWWTest) {
  Key key = "key";
  string value = "value";
  serializers[kvs::LatticeType::LWW]->put(key, serialize(0, value));
  stored_key_map[key].set_type(kvs::LatticeType::LWW);

  string get_request = get_key_request(key, ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  EXPECT_EQ(local_changeset.size(), 0);

  user_request_handler(access_count, seed, get_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  vector<string> messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 1);

  kvs::KeyResponse response;
  response.ParseFromString(messages[0]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  kvs::KeyTuple rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.payload(), serialize(0, value));
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 0);
  EXPECT_EQ(access_count, 1);
  EXPECT_EQ(key_access_tracker[key].size(), 1);
}

TEST_F(ServerHandlerTest, UserGetSetTest) {
  Key key = "key";
  set<string> s;
  s.emplace("value1");
  s.emplace("value2");
  s.emplace("value3");
  serializers[kvs::LatticeType::SET]->put(key, serialize(SetLattice<string>(s)));
  stored_key_map[key].set_type(kvs::LatticeType::SET);

  string get_request = get_key_request(key, ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  EXPECT_EQ(local_changeset.size(), 0);

  user_request_handler(access_count, seed, get_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  vector<string> messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 1);

  kvs::KeyResponse response;
  response.ParseFromString(messages[0]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  kvs::KeyTuple rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.payload(), serialize(SetLattice<string>(s)));
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 0);
  EXPECT_EQ(access_count, 1);
  EXPECT_EQ(key_access_tracker[key].size(), 1);
}

TEST_F(ServerHandlerTest, UserGetOrderedSetTest) {
  Key key = "key";
  ordered_set<string> s;
  s.emplace("value1");
  s.emplace("value2");
  s.emplace("value3");
  serializers[kvs::LatticeType::ORDERED_SET]->put(
      key, serialize(OrderedSetLattice<string>(s)));
  stored_key_map[key].set_type(kvs::LatticeType::ORDERED_SET);

  string get_request = get_key_request(key, ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  EXPECT_EQ(local_changeset.size(), 0);

  user_request_handler(access_count, seed, get_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  vector<string> messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 1);

  kvs::KeyResponse response;
  response.ParseFromString(messages[0]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  kvs::KeyTuple rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.payload(), serialize(OrderedSetLattice<string>(s)));
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 0);
  EXPECT_EQ(access_count, 1);
  EXPECT_EQ(key_access_tracker[key].size(), 1);
}

TEST_F(ServerHandlerTest, UserGetCausalTest) {
  Key key = "key";
  VectorClockValuePair<SetLattice<string>> p;
  p.vector_clock.insert("1", 1);
  p.vector_clock.insert("2", 1);
  p.value.insert("value1");
  p.value.insert("value2");
  p.value.insert("value3");

  serializers[kvs::LatticeType::SINGLE_CAUSAL]->put(
      key, serialize(SingleKeyCausalLattice<SetLattice<string>>(p)));
  stored_key_map[key].set_type(kvs::LatticeType::SINGLE_CAUSAL);

  string get_request = get_key_request(key, ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  EXPECT_EQ(local_changeset.size(), 0);

  user_request_handler(access_count, seed, get_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  vector<string> messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 1);

  kvs::KeyResponse response;
  response.ParseFromString(messages[0]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  kvs::KeyTuple rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);

  kvs::SingleKeyCausalValue left_value;
  kvs::SingleKeyCausalValue right_value;
  left_value.ParseFromString(rtp.payload());
  right_value.ParseFromString(
      serialize(SingleKeyCausalLattice<SetLattice<string>>(p)));

  set<string> left_set;
  set<string> right_set;

  for (const auto &val : left_value.values()) {
    left_set.insert(val);
  }
  for (const auto &val : right_value.values()) {
    right_set.insert(val);
  }

  EXPECT_THAT(left_set, testing::UnorderedElementsAreArray(right_set));

  map<string, unsigned> left_map;
  map<string, unsigned> right_map;

  for (const auto &pair : left_value.vector_clock()) {
    left_map[pair.first] = pair.second;
  }
  for (const auto &pair : right_value.vector_clock()) {
    right_map[pair.first] = pair.second;
  }

  EXPECT_THAT(left_map, testing::UnorderedElementsAreArray(right_map));

  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 0);
  EXPECT_EQ(access_count, 1);
  EXPECT_EQ(key_access_tracker[key].size(), 1);
}

TEST_F(ServerHandlerTest, UserPutAndGetLWWTest) {
  Key key = "key";
  string value = "value";
  string put_request =
      put_key_request(key, kvs::LatticeType::LWW, serialize(0, value), ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  EXPECT_EQ(local_changeset.size(), 0);

  user_request_handler(access_count, seed, put_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  vector<string> messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 1);

  kvs::KeyResponse response;
  response.ParseFromString(messages[0]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  kvs::KeyTuple rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(access_count, 1);
  EXPECT_EQ(key_access_tracker[key].size(), 1);

  string get_request = get_key_request(key, ip);

  user_request_handler(access_count, seed, get_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 2);

  response.ParseFromString(messages[1]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.payload(), serialize(0, value));
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(access_count, 2);
  EXPECT_EQ(key_access_tracker[key].size(), 2);
}

TEST_F(ServerHandlerTest, UserPutAndGetSetTest) {
  Key key = "key";
  set<string> s;
  s.emplace("value1");
  s.emplace("value2");
  s.emplace("value3");
  string put_request = put_key_request(key, kvs::LatticeType::SET,
                                       serialize(SetLattice<string>(s)), ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  EXPECT_EQ(local_changeset.size(), 0);

  user_request_handler(access_count, seed, put_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  vector<string> messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 1);

  kvs::KeyResponse response;
  response.ParseFromString(messages[0]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  kvs::KeyTuple rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(access_count, 1);
  EXPECT_EQ(key_access_tracker[key].size(), 1);

  string get_request = get_key_request(key, ip);

  user_request_handler(access_count, seed, get_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 2);

  response.ParseFromString(messages[1]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.payload(), serialize(SetLattice<string>(s)));
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(access_count, 2);
  EXPECT_EQ(key_access_tracker[key].size(), 2);
}

TEST_F(ServerHandlerTest, UserPutAndGetOrderedSetTest) {
  Key key = "key";
  ordered_set<string> s;
  s.emplace("value1");
  s.emplace("value2");
  s.emplace("value3");
  string put_request = put_key_request(
      key, kvs::LatticeType::SET, serialize(OrderedSetLattice<string>(s)), ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  EXPECT_EQ(local_changeset.size(), 0);

  user_request_handler(access_count, seed, put_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  vector<string> messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 1);

  kvs::KeyResponse response;
  response.ParseFromString(messages[0]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  kvs::KeyTuple rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(access_count, 1);
  EXPECT_EQ(key_access_tracker[key].size(), 1);

  string get_request = get_key_request(key, ip);

  user_request_handler(access_count, seed, get_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 2);

  response.ParseFromString(messages[1]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.payload(), serialize(OrderedSetLattice<string>(s)));
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(access_count, 2);
  EXPECT_EQ(key_access_tracker[key].size(), 2);
}

TEST_F(ServerHandlerTest, UserPutAndGetCausalTest) {
  Key key = "key";
  VectorClockValuePair<SetLattice<string>> p;
  p.vector_clock.insert("1", 1);
  p.vector_clock.insert("2", 1);
  p.value.insert("value1");
  p.value.insert("value2");
  p.value.insert("value3");
  string put_request = put_key_request(
      key, kvs::LatticeType::SINGLE_CAUSAL,
      serialize(SingleKeyCausalLattice<SetLattice<string>>(p)), ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  EXPECT_EQ(local_changeset.size(), 0);

  user_request_handler(access_count, seed, put_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  vector<string> messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 1);

  kvs::KeyResponse response;
  response.ParseFromString(messages[0]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  kvs::KeyTuple rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);
  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(access_count, 1);
  EXPECT_EQ(key_access_tracker[key].size(), 1);

  string get_request = get_key_request(key, ip);

  user_request_handler(access_count, seed, get_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  messages = get_zmq_messages();
  EXPECT_EQ(messages.size(), 2);

  response.ParseFromString(messages[1]);

  EXPECT_EQ(response.response_id(), kRequestId);
  EXPECT_EQ(response.tuples().size(), 1);

  rtp = response.tuples(0);

  EXPECT_EQ(rtp.key(), key);

  kvs::SingleKeyCausalValue left_value;
  kvs::SingleKeyCausalValue right_value;
  left_value.ParseFromString(rtp.payload());
  right_value.ParseFromString(
      serialize(SingleKeyCausalLattice<SetLattice<string>>(p)));

  set<string> left_set;
  set<string> right_set;

  for (const auto &val : left_value.values()) {
    left_set.insert(val);
  }
  for (const auto &val : right_value.values()) {
    right_set.insert(val);
  }

  EXPECT_THAT(left_set, testing::UnorderedElementsAreArray(right_set));

  map<string, unsigned> left_map;
  map<string, unsigned> right_map;

  for (const auto &pair : left_value.vector_clock()) {
    left_map[pair.first] = pair.second;
  }
  for (const auto &pair : right_value.vector_clock()) {
    right_map[pair.first] = pair.second;
  }

  EXPECT_THAT(left_map, testing::UnorderedElementsAreArray(right_map));

  EXPECT_EQ(rtp.error(), 0);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(access_count, 2);
  EXPECT_EQ(key_access_tracker[key].size(), 2);
}

TEST_F(ServerHandlerTest, UserPutMissingLatticeTypeTest) {
  Key key = "key";
  string put_request =
      put_key_request(key, kvs::LatticeType::NONE, "", ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  user_request_handler(access_count, seed, put_request, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  EXPECT_EQ(local_changeset.size(), 0);
  EXPECT_EQ(stored_key_map.count(key), 0);
}

TEST_F(ServerHandlerTest, UserPutLatticeMismatchTest) {
  Key key = "key";
  string value = "value";

  // First PUT with LWW type succeeds.
  string put_lww =
      put_key_request(key, kvs::LatticeType::LWW, serialize(0, value), ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  user_request_handler(access_count, seed, put_lww, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(stored_key_map[key].type(), kvs::LatticeType::LWW);

  // Second PUT with SET type on the same key — lattice mismatch.
  set<string> s = {"a"};
  string put_set = put_key_request(key, kvs::LatticeType::SET,
                                   serialize(SetLattice<string>(s)), ip);

  local_changeset.clear();
  user_request_handler(access_count, seed, put_set, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  // The mismatched PUT should be silently skipped.
  EXPECT_EQ(local_changeset.size(), 0);
  EXPECT_EQ(stored_key_map[key].type(), kvs::LatticeType::LWW);
}

// Test PUT with TTL sets expiry_epoch_s_ on the stored key.
TEST_F(ServerHandlerTest, UserPutWithTTLSetsExpiry) {
  Key key = "ttl_key";
  string value = "ttl_value";

  // Use an absolute expiry 300 seconds from now (in milliseconds).
  uint64_t expiry_ms = static_cast<uint64_t>(now_epoch_s() + 300) * 1000;
  string put_lww =
      put_key_request(key, kvs::LatticeType::LWW, serialize(0, value), ip,
                      expiry_ms);

  unsigned access_count = 0;
  unsigned seed = 0;

  user_request_handler(access_count, seed, put_lww, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  EXPECT_EQ(stored_key_map[key].type(), kvs::LatticeType::LWW);
  // Expiry should be set (non-zero) and approximately 300 seconds from now.
  EXPECT_GT(stored_key_map[key].expiry_epoch_s_, 0u);
  EXPECT_NEAR(stored_key_map[key].expiry_epoch_s_, now_epoch_s() + 300, 2);
}

// Test GET on an expired key returns KEY_DNE.
TEST_F(ServerHandlerTest, UserGetExpiredKeyReturnsDNE) {
  Key key = "expired_key";
  string value = "expired_value";

  // PUT with an expiry in the past (already expired).
  uint64_t expiry_ms = static_cast<uint64_t>(now_epoch_s() - 10) * 1000;
  string put_lww =
      put_key_request(key, kvs::LatticeType::LWW, serialize(0, value), ip,
                      expiry_ms);

  unsigned access_count = 0;
  unsigned seed = 0;

  user_request_handler(access_count, seed, put_lww, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  // Key should exist but be expired.
  EXPECT_GT(stored_key_map[key].expiry_epoch_s_, 0u);

  // GET should return KEY_DNE.
  string get_req = get_key_request(key, ip);
  size_t msg_count_before = mock_zmq_util.sent_messages.size();
  user_request_handler(access_count, seed, get_req, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  ASSERT_GT(mock_zmq_util.sent_messages.size(), msg_count_before);
  kvs::KeyResponse response;
  response.ParseFromString(mock_zmq_util.sent_messages.back());
  EXPECT_EQ(response.tuples(0).error(), kvs::AnnaError::KEY_DNE);
}

// Test PUT/GET for Counter lattice type.
TEST_F(ServerHandlerTest, UserPutGetCounter) {
  Key key = "counter_key";

  // Build a CounterValue with one increment
  kvs::CounterValue cv;
  (*cv.mutable_increments())["node1"] = 5;
  string payload;
  cv.SerializeToString(&payload);

  string put_counter =
      put_key_request(key, kvs::LatticeType::COUNTER, payload, ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  user_request_handler(access_count, seed, put_counter, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  EXPECT_EQ(local_changeset.size(), 1);
  EXPECT_EQ(stored_key_map[key].type(), kvs::LatticeType::COUNTER);

  // GET should return the counter value
  string get_req = get_key_request(key, ip);
  size_t msg_count_before = mock_zmq_util.sent_messages.size();
  user_request_handler(access_count, seed, get_req, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  ASSERT_GT(mock_zmq_util.sent_messages.size(), msg_count_before);
  kvs::KeyResponse response;
  response.ParseFromString(mock_zmq_util.sent_messages.back());
  EXPECT_EQ(response.tuples(0).error(), kvs::AnnaError::NO_ERROR);
  EXPECT_EQ(response.tuples(0).lattice_type(), kvs::LatticeType::COUNTER);

  // Verify the payload contains our increment
  kvs::CounterValue result;
  result.ParseFromString(response.tuples(0).payload());
  EXPECT_EQ(result.increments().at("node1"), 5);
}

// Test Counter merge: two PUTs from different nodes merge via per-node max.
TEST_F(ServerHandlerTest, UserCounterMerge) {
  Key key = "merge_counter";

  // First PUT: node1 increments 3
  kvs::CounterValue cv1;
  (*cv1.mutable_increments())["node1"] = 3;
  string payload1;
  cv1.SerializeToString(&payload1);

  string put1 =
      put_key_request(key, kvs::LatticeType::COUNTER, payload1, ip);

  unsigned access_count = 0;
  unsigned seed = 0;

  user_request_handler(access_count, seed, put1, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  // Second PUT: node2 increments 7, node1 increments 2 (stale, should not override 3)
  kvs::CounterValue cv2;
  (*cv2.mutable_increments())["node2"] = 7;
  (*cv2.mutable_increments())["node1"] = 2;
  string payload2;
  cv2.SerializeToString(&payload2);

  string put2 =
      put_key_request(key, kvs::LatticeType::COUNTER, payload2, ip);

  local_changeset.clear();
  user_request_handler(access_count, seed, put2, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  // GET and verify merge: node1=max(3,2)=3, node2=7
  string get_req = get_key_request(key, ip);
  size_t msg_count_before = mock_zmq_util.sent_messages.size();
  user_request_handler(access_count, seed, get_req, log_, global_hash_rings,
                       local_hash_rings, pending_requests, key_access_tracker,
                       stored_key_map, key_replication_map, local_changeset, wt,
                       serializers, pushers);

  ASSERT_GT(mock_zmq_util.sent_messages.size(), msg_count_before);
  kvs::KeyResponse response;
  response.ParseFromString(mock_zmq_util.sent_messages.back());

  kvs::CounterValue result;
  result.ParseFromString(response.tuples(0).payload());
  EXPECT_EQ(result.increments().at("node1"), 3);  // max(3, 2) = 3
  EXPECT_EQ(result.increments().at("node2"), 7);
}

// TODO: Test key address cache invalidation
// TODO: Test replication factor request and making the request pending
// TODO: Test metadata operations -- does this matter?
