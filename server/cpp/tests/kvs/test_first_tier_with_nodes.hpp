#include "hash_ring/hash_ring.hpp"

TEST(HashRingHelperTest, FirstTierWithNodesReturnsSomeTierWhenBothExist) {
  GlobalRingMap rings;
  rings[Tier::MEMORY].insert("10.0.0.1", "10.0.0.1", 0, 0);
  rings[Tier::DISK].insert("10.0.0.2", "10.0.0.2", 0, 0);
  Tier result = first_tier_with_nodes(rings);
  EXPECT_TRUE(result == Tier::MEMORY || result == Tier::DISK);
}

TEST(HashRingHelperTest, FirstTierWithNodesReturnsDiskWhenMemoryEmpty) {
  GlobalRingMap rings;
  rings[Tier::DISK].insert("10.0.0.2", "10.0.0.2", 0, 0);
  EXPECT_EQ(first_tier_with_nodes(rings), Tier::DISK);
}

TEST(HashRingHelperTest, FirstTierWithNodesReturnsMemoryWhenDiskEmpty) {
  GlobalRingMap rings;
  rings[Tier::MEMORY].insert("10.0.0.1", "10.0.0.1", 0, 0);
  EXPECT_EQ(first_tier_with_nodes(rings), Tier::MEMORY);
}

TEST(HashRingHelperTest, FirstTierWithNodesFallbackWhenAllEmpty) {
  GlobalRingMap rings;
  rings[Tier::MEMORY]; // empty ring
  rings[Tier::DISK];   // empty ring
  // Should return whichever tier exists in the map
  Tier result = first_tier_with_nodes(rings);
  EXPECT_TRUE(result == Tier::MEMORY || result == Tier::DISK);
}

TEST(HashRingHelperTest, FirstTierWithNodesEmptyMap) {
  GlobalRingMap rings;
  // Empty map — falls back to MEMORY
  EXPECT_EQ(first_tier_with_nodes(rings), Tier::MEMORY);
}
