//  Validate the anna-hashring Rust C library works from C++.

#include "gtest/gtest.h"
#include "anna_hashring.h"

TEST(HashRingFFI, LifecycleAndBasicOps) {
  AnnaHashRing *ring = anna_hashring_new(true, 0);
  ASSERT_NE(ring, nullptr);
  EXPECT_EQ(anna_hashring_size(ring), 0);

  // Insert a server with 100 virtual nodes.
  int rc = anna_hashring_insert(ring, "1.2.3.4", "10.0.0.1", 0, 100);
  EXPECT_EQ(rc, 0);
  EXPECT_EQ(anna_hashring_size(ring), 100);

  // Invalid tid should fail.
  rc = anna_hashring_insert(ring, "1.2.3.4", "10.0.0.1", 50, 100);
  EXPECT_EQ(rc, -1);

  // Remove.
  anna_hashring_remove(ring, "1.2.3.4", "10.0.0.1", 0);
  EXPECT_EQ(anna_hashring_size(ring), 0);

  anna_hashring_free(ring);
}

TEST(HashRingFFI, ResponsibleServers) {
  AnnaHashRing *ring = anna_hashring_new(true, 0);
  anna_hashring_insert(ring, "1.2.3.4", "10.0.0.1", 0, 3000);
  anna_hashring_insert(ring, "5.6.7.8", "10.0.0.2", 0, 3000);

  ServerInfo servers[4];
  uint32_t count = anna_responsible_servers(ring, "test_key", 2, servers, 4);
  EXPECT_EQ(count, 2);

  // Free strings.
  for (uint32_t i = 0; i < count; i++) {
    EXPECT_NE(servers[i].public_ip, nullptr);
    anna_string_free(servers[i].public_ip);
    anna_string_free(servers[i].private_ip);
  }

  anna_hashring_free(ring);
}

TEST(HashRingFFI, UniqueServers) {
  AnnaHashRing *ring = anna_hashring_new(true, 0);
  anna_hashring_insert(ring, "1.2.3.4", "10.0.0.1", 0, 100);
  anna_hashring_insert(ring, "5.6.7.8", "10.0.0.2", 0, 100);

  ServerInfo servers[4];
  uint32_t count = anna_hashring_get_unique_servers(ring, servers, 4);
  EXPECT_EQ(count, 2);

  for (uint32_t i = 0; i < count; i++) {
    anna_string_free(servers[i].public_ip);
    anna_string_free(servers[i].private_ip);
  }

  anna_hashring_free(ring);
}

TEST(HashRingFFI, LocalRingThreadDistribution) {
  AnnaHashRing *ring = anna_hashring_new(false, 0);
  anna_hashring_insert(ring, "1.2.3.4", "10.0.0.1", 0, 3000);
  anna_hashring_insert(ring, "1.2.3.4", "10.0.0.1", 1, 3000);

  uint32_t tids[4];
  uint32_t count = anna_responsible_local(ring, "test_key", 1, tids, 4);
  EXPECT_EQ(count, 1);
  EXPECT_TRUE(tids[0] == 0 || tids[0] == 1);

  anna_hashring_free(ring);
}

TEST(HashRingFFI, LocalLookupOnGlobalRingReturnsZero) {
  AnnaHashRing *ring = anna_hashring_new(true, 0);
  anna_hashring_insert(ring, "1.2.3.4", "10.0.0.1", 0, 100);

  uint32_t tids[4];
  uint32_t count = anna_responsible_local(ring, "test_key", 1, tids, 4);
  EXPECT_EQ(count, 0);  // Wrong ring type.

  anna_hashring_free(ring);
}
