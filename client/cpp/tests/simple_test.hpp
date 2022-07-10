#include "kvs/kvs_handlers.hpp"

TEST_F(ClientBaseTest, SimpleTest) {
  Key key = "key";
  string value = "value1";
  serializer->put(key, value, (unsigned)0);

  value = "value2";

  string put_request = put_key_request(key, value, ip);

  unsigned access_count = 0;
  unsigned seed = 0;
  unsigned error;
  auto now = std::chrono::system_clock::now();

  EXPECT_EQ(local_changeset.size(), 0);

  gossip_handler(seed, put_request, global_hash_rings, local_hash_rings,
                 key_size_map, pending_gossip, metadata_map, wt, serializer,
                 pushers);

  EXPECT_EQ(pending_gossip.size(), 0);
  EXPECT_EQ(serializer->get(key, error).reveal().value, value);
}
