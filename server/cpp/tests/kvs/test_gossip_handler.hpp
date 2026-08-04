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

TEST_F(ServerHandlerTest, SimpleGossipReceive) {
  Key key = "key";
  string value = "value";

  // Build a gossip request (PUT with LWW payload).
  string gossip_request =
      put_key_request(key, kvs::LatticeType::LWW, serialize(1, value), ip);

  unsigned seed = 0;

  EXPECT_EQ(pending_gossip.size(), 0);

  gossip_handler(seed, gossip_request, global_hash_rings, local_hash_rings,
                 pending_gossip, stored_key_map, key_replication_map, wt,
                 serializers, pushers, log_);

  // Key should be stored locally (thread IS responsible via mock).
  EXPECT_EQ(pending_gossip.size(), 0);
  EXPECT_EQ(stored_key_map.count(key), 1);
}

TEST_F(ServerHandlerTest, GossipUpdate) {
  Key key = "key";
  // Pre-store a value.
  serializers[kvs::LatticeType::LWW]->put(key, serialize(1, string("old")));
  stored_key_map[key].set_type(kvs::LatticeType::LWW);
  stored_key_map[key].set_size(3);

  // Gossip a newer value (higher timestamp wins in LWW).
  string gossip_request = put_key_request(
      key, kvs::LatticeType::LWW, serialize(2, string("new")), ip);

  unsigned seed = 0;

  gossip_handler(seed, gossip_request, global_hash_rings, local_hash_rings,
                 pending_gossip, stored_key_map, key_replication_map, wt,
                 serializers, pushers, log_);

  EXPECT_EQ(pending_gossip.size(), 0);
  // Value should be updated to the newer one.
  auto result = process_get(key, serializers[kvs::LatticeType::LWW]);
  EXPECT_NE(result.first.find("new"), string::npos);
}

TEST_F(ServerHandlerTest, GossipTypeMismatch) {
  Key key = "key";
  // Pre-store as LWW.
  serializers[kvs::LatticeType::LWW]->put(key, serialize(1, string("lww")));
  stored_key_map[key].set_type(kvs::LatticeType::LWW);
  stored_key_map[key].set_size(3);

  // Gossip the same key as SET type — should be handled as type mismatch.
  kvs::SetValue sv;
  sv.add_values("elem");
  string payload;
  sv.SerializeToString(&payload);
  string gossip_request =
      put_key_request(key, kvs::LatticeType::SET, payload, ip);

  unsigned seed = 0;

  gossip_handler(seed, gossip_request, global_hash_rings, local_hash_rings,
                 pending_gossip, stored_key_map, key_replication_map, wt,
                 serializers, pushers, log_);

  // Type mismatch: gossip is rejected, original value preserved.
  EXPECT_EQ(pending_gossip.size(), 0);
  EXPECT_EQ(stored_key_map[key].type(), kvs::LatticeType::LWW);
  auto result = process_get(key, serializers[kvs::LatticeType::LWW]);
  EXPECT_NE(result.first.find("lww"), string::npos);
}

TEST_F(ServerHandlerTest, GossipForwardMetadata) {
  // Configure mock: metadata key goes to a DIFFERENT thread.
  Key meta_key = "ANNA_METADATA|replication|somekey";
  ServerThread other_thread("10.0.0.2", "10.0.0.2", 0);
  mock_hash_ring_util.thread_overrides[meta_key] = {other_thread};

  string gossip_request = put_key_request(
      meta_key, kvs::LatticeType::LWW, serialize(1, string("rep_data")), ip);

  unsigned seed = 0;
  size_t msg_count_before = mock_zmq_util.sent_messages.size();

  gossip_handler(seed, gossip_request, global_hash_rings, local_hash_rings,
                 pending_gossip, stored_key_map, key_replication_map, wt,
                 serializers, pushers, log_);

  // Key should NOT be stored locally — it was forwarded.
  EXPECT_EQ(stored_key_map.count(meta_key), 0);
  EXPECT_EQ(pending_gossip.size(), 0);

  // A forwarding message should have been sent.
  EXPECT_GT(mock_zmq_util.sent_messages.size(), msg_count_before);
}

TEST_F(ServerHandlerTest, GossipPendingOnLookupFailure) {
  // Configure mock: key lookup fails (succeed = false).
  Key key = "failing_key";
  mock_hash_ring_util.thread_overrides[key] = {};  // empty thread list
  mock_hash_ring_util.succeed_overrides[key] = false;

  string gossip_request = put_key_request(
      key, kvs::LatticeType::LWW, serialize(1, string("val")), ip);

  unsigned seed = 0;

  gossip_handler(seed, gossip_request, global_hash_rings, local_hash_rings,
                 pending_gossip, stored_key_map, key_replication_map, wt,
                 serializers, pushers, log_);

  // Key should NOT be stored locally.
  EXPECT_EQ(stored_key_map.count(key), 0);

  // Should be in pending gossip.
  EXPECT_EQ(pending_gossip.size(), 1);
  EXPECT_EQ(pending_gossip.count(key), 1);
  EXPECT_EQ(pending_gossip[key].size(), 1);
}
