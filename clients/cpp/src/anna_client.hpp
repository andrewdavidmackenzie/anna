// Client-local types, address helpers, and request utilities.
// This replaces the server includes (types.hpp, threads.hpp,
// requests.hpp) with client-owned definitions, matching the approach
// used by the Rust, Python, and Go clients.
//
// Utility functions (split, generate_timestamp, metadata key constants)
// that overlap with common.hpp are defined in client_lib.cpp to avoid
// redefinition errors when test files include both this header and
// server headers.

#ifndef ANNA_CLIENT_HPP_
#define ANNA_CLIENT_HPP_

#include <chrono>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include "kvs.pb.h"
#include "zmq/socket_cache.hpp"
#include "zmq/zmq_util.hpp"

// --- Type aliases (replaces types.hpp) ---

#ifndef INCLUDE_TYPES_HPP_

using string = std::string;

template <class K, class V>
using map = std::unordered_map<K, V>;

template <class V>
using set = std::unordered_set<V>;

template <class T>
using vector = std::vector<T>;

template <class A, class B>
using pair = std::pair<A, B>;

using Address = string;
using Key = string;

#endif  // INCLUDE_TYPES_HPP_

// --- Port constants and address helpers (replaces threads.hpp) ---

#ifndef INCLUDE_THREADS_HPP_
#define INCLUDE_THREADS_HPP_

const unsigned kKeyAddressPort = 6450;
const unsigned kUserResponsePort = 6800;
const unsigned kUserKeyAddressPort = 6850;

inline unsigned kBaseOffset = 0;

class UserRoutingThread {
  Address ip_;
  Address ip_base_;
  unsigned tid_;

 public:
  UserRoutingThread() {}

  UserRoutingThread(Address ip, unsigned tid)
      : ip_(ip), tid_(tid), ip_base_("tcp://" + ip_ + ":") {}

  Address ip() const { return ip_; }
  unsigned tid() const { return tid_; }

  Address key_address_connect_address() const {
    return ip_base_ + std::to_string(tid_ + kKeyAddressPort + kBaseOffset);
  }

  Address key_address_bind_address() const {
    return ip_base_ + std::to_string(tid_ + kKeyAddressPort + kBaseOffset);
  }
};

class UserThread {
  Address ip_;
  Address ip_base_;
  unsigned tid_;

 public:
  UserThread() {}
  UserThread(Address ip, unsigned tid)
      : ip_(ip), tid_(tid), ip_base_("tcp://" + ip_ + ":") {}

  Address ip() const { return ip_; }
  unsigned tid() const { return tid_; }

  Address response_connect_address() const {
    return ip_base_ + std::to_string(tid_ + kUserResponsePort + kBaseOffset);
  }

  Address response_bind_address() const {
    return ip_base_ + std::to_string(tid_ + kUserResponsePort + kBaseOffset);
  }

  Address key_address_connect_address() const {
    return ip_base_ + std::to_string(tid_ + kUserKeyAddressPort + kBaseOffset);
  }

  Address key_address_bind_address() const {
    return ip_base_ + std::to_string(tid_ + kUserKeyAddressPort + kBaseOffset);
  }
};

#endif  // INCLUDE_THREADS_HPP_

// --- Request helper (replaces requests.hpp send_request) ---

#ifndef INCLUDE_REQUESTS_HPP_

template <typename REQ>
void send_request(const REQ &request, zmq::socket_t &send_socket) {
  string serialized_req;
  request.SerializeToString(&serialized_req);
  kZmqUtil->send_string(serialized_req, &send_socket);
}

#endif  // INCLUDE_REQUESTS_HPP_

#endif  // ANNA_CLIENT_HPP_
