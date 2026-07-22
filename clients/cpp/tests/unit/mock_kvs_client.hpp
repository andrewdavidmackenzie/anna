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

#ifndef TESTS_UNIT_MOCK_KVS_CLIENT_HPP_
#define TESTS_UNIT_MOCK_KVS_CLIENT_HPP_

#include "kvs_client.hpp"

// A test double for KvsClientInterface: records the keys passed to
// get_async()/put_async() and returns whatever KeyResponses have been queued
// via `responses_`, with no socket/server involved at all. Used to unit test
// the comms-boundary wrapper functions in client_lib.cpp (see #104).
class MockKvsClient : public KvsClientInterface {
 public:
  MockKvsClient() : rid_(0) {}

  ~MockKvsClient() {}

  string put_async(const Key& key, const string& payload,
                   kvs::LatticeType lattice_type) {
    keys_put_.push_back(key);
    return get_request_id();
  }

  void get_async(const Key& key) { keys_get_.push_back(key); }

  // Returns whatever has been queued in `responses_`, then clears the queue
  // (so a test can enqueue a response, call the function under test once,
  // and get an empty result on any subsequent poll -- mirroring how a real
  // KvsClient only returns a response once).
  vector<kvs::KeyResponse> receive_async() {
    vector<kvs::KeyResponse> result = responses_;
    responses_.clear();
    return result;
  }

  zmq::context_t* get_context() { return nullptr; }

  void clear() {
    keys_put_.clear();
    keys_get_.clear();
    responses_.clear();
  }

  // keys passed to put_async(), in call order
  vector<Key> keys_put_;
  // keys passed to get_async(), in call order
  vector<Key> keys_get_;
  // responses to return from the next call to receive_async()
  vector<kvs::KeyResponse> responses_;

 private:
  string get_request_id() {
    if (++rid_ % 10000 == 0) rid_ = 0;
    return std::to_string(rid_++);
  }

  unsigned rid_;
};

// A mock that delays responses for N calls to receive_async() before
// returning them, exercising the retry loops in client_lib.cpp.
class DelayedMockKvsClient : public KvsClientInterface {
 public:
  DelayedMockKvsClient(unsigned delay) : delay_(delay), calls_(0), rid_(0) {}

  ~DelayedMockKvsClient() {}

  string put_async(const Key& key, const string& payload,
                   kvs::LatticeType lattice_type) {
    keys_put_.push_back(key);
    return get_request_id();
  }

  void get_async(const Key& key) { keys_get_.push_back(key); }

  vector<kvs::KeyResponse> receive_async() {
    if (calls_++ < delay_) {
      return {};  // return empty to trigger retry
    }
    vector<kvs::KeyResponse> result = responses_;
    responses_.clear();
    return result;
  }

  zmq::context_t* get_context() { return nullptr; }

  vector<Key> keys_put_;
  vector<Key> keys_get_;
  vector<kvs::KeyResponse> responses_;

 private:
  string get_request_id() {
    if (++rid_ % 10000 == 0) rid_ = 0;
    return std::to_string(rid_++);
  }

  unsigned delay_;
  unsigned calls_;
  unsigned rid_;
};

#endif  // TESTS_UNIT_MOCK_KVS_CLIENT_HPP_
