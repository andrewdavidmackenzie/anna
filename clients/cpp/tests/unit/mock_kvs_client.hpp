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

  // Returns one queued response per call (FIFO), mirroring how a real
  // KvsClient returns one response per receive poll.
  vector<kvs::KeyResponse> receive_async() {
    if (responses_.empty()) {
      return {};
    }
    vector<kvs::KeyResponse> result = {responses_.front()};
    responses_.erase(responses_.begin());
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

// A mock that returns all queued responses in a single receive_async() call,
// used to test the "received more than one response" warning path in get().
class BatchMockKvsClient : public MockKvsClient {
 public:
  vector<kvs::KeyResponse> receive_async() {
    vector<kvs::KeyResponse> result = responses_;
    responses_.clear();
    return result;
  }
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

// A mock that auto-generates success responses for every put/get,
// suitable for benchmarking where we need an unlimited response stream.
class AutoRespondMockKvsClient : public KvsClientInterface {
 public:
  AutoRespondMockKvsClient() : rid_(0), put_count_(0) {}
  ~AutoRespondMockKvsClient() {}

  string put_async(const Key& key, const string& payload,
                   kvs::LatticeType lattice_type) {
    put_count_++;
    string rid = get_request_id();
    // Queue a success response for this request.
    kvs::KeyResponse resp;
    resp.set_response_id(rid);
    auto* tp = resp.add_tuples();
    tp->set_key(key);
    tp->set_error(kvs::AnnaError::NO_ERROR);
    pending_.push_back(resp);
    return rid;
  }

  void get_async(const Key& key) {
    string rid = get_request_id();
    // Queue a success response with an LWW value.
    kvs::KeyResponse resp;
    resp.set_response_id(rid);
    auto* tp = resp.add_tuples();
    tp->set_key(key);
    tp->set_error(kvs::AnnaError::NO_ERROR);
    tp->set_lattice_type(kvs::LatticeType::LWW);
    kvs::LWWValue lww;
    lww.set_timestamp(1);
    lww.set_value("bench_value");
    string payload;
    lww.SerializeToString(&payload);
    tp->set_payload(payload);
    pending_.push_back(resp);
  }

  vector<kvs::KeyResponse> receive_async() {
    if (pending_.empty()) return {};
    vector<kvs::KeyResponse> result = {pending_.front()};
    pending_.erase(pending_.begin());
    return result;
  }

  zmq::context_t* get_context() { return nullptr; }

  unsigned put_count_;

 private:
  string get_request_id() {
    return std::to_string(++rid_);
  }
  unsigned rid_;
  vector<kvs::KeyResponse> pending_;
};

#endif  // TESTS_UNIT_MOCK_KVS_CLIENT_HPP_
