//  Wrapper around the Rust anna-hashring C library.
//
//  Provides the same GlobalHashRing / LocalHashRing / GlobalRingMap /
//  LocalRingMap types used throughout the server, but delegates all
//  hash computation to the shared Rust library for cross-language
//  consistency.

#ifndef INCLUDE_HASH_RING_FFI_HPP_
#define INCLUDE_HASH_RING_FFI_HPP_

#include <map>
#include <set>
#include <string>
#include <vector>

#include "anna_hashring.h"
#include "metadata.hpp"
#include "kvs/kvs_threads.hpp"
#include "types.hpp"

// Number of virtual nodes per thread on the hash ring.
// Must match the value used by the Rust server.
inline unsigned kVirtualThreadNum = 3000;

/// Set of unique server threads (used by get_unique_servers).
typedef std::set<ServerThread, ThreadHash> ServerThreadSet;

/// List of server threads.
typedef std::vector<ServerThread> ServerThreadList;

/// Wrapper around the Rust anna-hashring C library.
///
/// Drop-in replacement for the old HashRing<GlobalHasher> and
/// HashRing<LocalHasher>. Exposes the same methods but delegates
/// hashing to the shared Rust library.
class HashRingWrapper {
  AnnaHashRing *ring_;
  bool global_;

public:
  explicit HashRingWrapper(bool global)
      : ring_(anna_hashring_new(global, kBaseOffset)), global_(global) {}

  ~HashRingWrapper() {
    if (ring_) anna_hashring_free(ring_);
  }

  // Non-copyable, moveable.
  HashRingWrapper(const HashRingWrapper &) = delete;
  HashRingWrapper &operator=(const HashRingWrapper &) = delete;
  HashRingWrapper(HashRingWrapper &&other) noexcept
      : ring_(other.ring_), global_(other.global_) {
    other.ring_ = nullptr;
  }
  HashRingWrapper &operator=(HashRingWrapper &&other) noexcept {
    if (ring_) anna_hashring_free(ring_);
    ring_ = other.ring_;
    global_ = other.global_;
    other.ring_ = nullptr;
    return *this;
  }

  /// Insert a server with kVirtualThreadNum virtual nodes.
  /// Returns true if the server was inserted, false on error (e.g., tid >= 50).
  bool insert(Address public_ip, Address private_ip, int join_count,
              unsigned tid) {
    // TODO: track join_count for rejoin detection (not in C API yet)
    (void)join_count;
    return anna_hashring_insert(ring_, public_ip.c_str(), private_ip.c_str(),
                                tid, kVirtualThreadNum) == 0;
  }

  /// Remove all virtual nodes for a server.
  void remove(Address public_ip, Address private_ip, unsigned tid) {
    anna_hashring_remove(ring_, public_ip.c_str(), private_ip.c_str(), tid);
  }

  /// Number of entries (including virtual nodes).
  unsigned size() const { return anna_hashring_size(ring_); }

  /// Whether the ring is empty.
  bool empty() const { return size() == 0; }

  /// Get all unique (non-virtual) server threads.
  ServerThreadSet get_unique_servers() const {
    ServerThreadSet result;
    const unsigned max = 1024;
    ServerInfo servers[max];
    unsigned count = anna_hashring_get_unique_servers(ring_, servers, max);
    for (unsigned i = 0; i < count; i++) {
      result.insert(ServerThread(servers[i].public_ip, servers[i].private_ip,
                                 servers[i].tid));
      anna_string_free(servers[i].public_ip);
      anna_string_free(servers[i].private_ip);
    }
    return result;
  }

  /// Internal: get the raw C handle (for responsible_global/local).
  const AnnaHashRing *raw() const { return ring_; }
};

/// Global hash ring — default-constructs with global=true.
class GlobalHashRing : public HashRingWrapper {
public:
  GlobalHashRing() : HashRingWrapper(true) {}
  explicit GlobalHashRing(bool global) : HashRingWrapper(global) {}
};

/// Local hash ring — default-constructs with global=false.
class LocalHashRing : public HashRingWrapper {
public:
  LocalHashRing() : HashRingWrapper(false) {}
  explicit LocalHashRing(bool global) : HashRingWrapper(global) {}
};

typedef map<Tier, GlobalHashRing> GlobalRingMap;
typedef map<Tier, LocalHashRing> LocalRingMap;

/// Find `rep` unique responsible servers for a key on the global ring.
inline ServerThreadList responsible_global(const Key &key, unsigned global_rep,
                                           GlobalHashRing &global_hash_ring) {
  ServerThreadList threads;
  const unsigned max = 64;
  ServerInfo servers[max];
  unsigned count = anna_responsible_servers(global_hash_ring.raw(), key.c_str(),
                                            global_rep, servers, max);
  for (unsigned i = 0; i < count; i++) {
    threads.push_back(
        ServerThread(servers[i].public_ip, servers[i].private_ip,
                     servers[i].tid));
    anna_string_free(servers[i].public_ip);
    anna_string_free(servers[i].private_ip);
  }
  return threads;
}

/// Find `rep` unique responsible thread IDs for a key on the local ring.
inline set<unsigned> responsible_local(const Key &key, unsigned local_rep,
                                        LocalHashRing &local_hash_ring) {
  set<unsigned> tids;
  const unsigned max = 64;
  uint32_t out_tids[max];
  unsigned count = anna_responsible_local(local_hash_ring.raw(), key.c_str(),
                                           local_rep, out_tids, max);
  for (unsigned i = 0; i < count; i++) {
    tids.insert(out_tids[i]);
  }
  return tids;
}

/// Return the first tier that has any nodes in the global hash rings.
inline Tier first_tier_with_nodes(GlobalRingMap &global_hash_rings) {
  for (const auto &pair : global_hash_rings) {
    if (!pair.second.empty()) {
      return pair.first;
    }
  }
  return Tier::MEMORY;
}

#endif // INCLUDE_HASH_RING_FFI_HPP_
