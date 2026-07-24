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

#include "benchmark.pb.h"
#include "latency_reporter.hpp"

// A mock ZmqUtil that records sent messages and their destination sockets.
// Each unique address in the SocketCache gets a unique zmq::socket_t*, so
// we can verify that messages are routed to distinct destinations.
struct SentMessage {
  string payload;
  zmq::socket_t* socket;
};

class MockZmqUtil : public ZmqUtilInterface {
public:
  void send_string(const string& s, zmq::socket_t* socket) override {
    sent.push_back({s, socket});
  }
  string recv_string(zmq::socket_t* socket) override { return ""; }
  int poll(vector<zmq::pollitem_t>* items, std::chrono::milliseconds timeout) override {
    return 0;
  }
  vector<SentMessage> sent;
};

// Test that the UserFeedback protobuf roundtrips correctly. This validates
// the protobuf generation for benchmark.proto in the main kvs-proto library.

TEST(LatencyReporterTest, UserFeedbackProtobufRoundtrip) {
  UserFeedback feedback;
  feedback.set_uid("cpp_client:0");
  feedback.set_latency(42.5);
  feedback.set_throughput(1000.0);
  feedback.set_warmup(true);
  feedback.set_finish(false);

  auto* kl = feedback.add_key_latency();
  kl->set_key("test_key");
  kl->set_latency(10.0);

  std::string serialized;
  ASSERT_TRUE(feedback.SerializeToString(&serialized));

  UserFeedback deserialized;
  ASSERT_TRUE(deserialized.ParseFromString(serialized));

  EXPECT_EQ(deserialized.uid(), "cpp_client:0");
  EXPECT_DOUBLE_EQ(deserialized.latency(), 42.5);
  EXPECT_DOUBLE_EQ(deserialized.throughput(), 1000.0);
  EXPECT_TRUE(deserialized.warmup());
  EXPECT_FALSE(deserialized.finish());

  ASSERT_EQ(deserialized.key_latency_size(), 1);
  EXPECT_EQ(deserialized.key_latency(0).key(), "test_key");
  EXPECT_DOUBLE_EQ(deserialized.key_latency(0).latency(), 10.0);
}

TEST(LatencyReporterTest, UserFeedbackFinishMessage) {
  UserFeedback feedback;
  feedback.set_uid("cpp_client:1");
  feedback.set_finish(true);

  std::string serialized;
  ASSERT_TRUE(feedback.SerializeToString(&serialized));

  UserFeedback deserialized;
  ASSERT_TRUE(deserialized.ParseFromString(serialized));

  EXPECT_EQ(deserialized.uid(), "cpp_client:1");
  EXPECT_TRUE(deserialized.finish());
  // Default values for unset fields
  EXPECT_DOUBLE_EQ(deserialized.latency(), 0.0);
  EXPECT_DOUBLE_EQ(deserialized.throughput(), 0.0);
  EXPECT_FALSE(deserialized.warmup());
  EXPECT_EQ(deserialized.key_latency_size(), 0);
}

TEST(LatencyReporterTest, UserFeedbackMultipleKeyLatencies) {
  UserFeedback feedback;
  feedback.set_uid("cpp_client:2");
  feedback.set_latency(100.0);

  auto* kl1 = feedback.add_key_latency();
  kl1->set_key("key_a");
  kl1->set_latency(50.0);

  auto* kl2 = feedback.add_key_latency();
  kl2->set_key("key_b");
  kl2->set_latency(150.0);

  std::string serialized;
  ASSERT_TRUE(feedback.SerializeToString(&serialized));

  UserFeedback deserialized;
  ASSERT_TRUE(deserialized.ParseFromString(serialized));

  ASSERT_EQ(deserialized.key_latency_size(), 2);
  EXPECT_EQ(deserialized.key_latency(0).key(), "key_a");
  EXPECT_DOUBLE_EQ(deserialized.key_latency(0).latency(), 50.0);
  EXPECT_EQ(deserialized.key_latency(1).key(), "key_b");
  EXPECT_DOUBLE_EQ(deserialized.key_latency(1).latency(), 150.0);
}

// --- Tests for the LatencyReporter class itself ---

// RAII helper that swaps kZmqUtil with a mock and restores it on destruction.
class ZmqUtilGuard {
public:
  ZmqUtilGuard(ZmqUtilInterface* mock) : original_(kZmqUtil) {
    kZmqUtil = mock;
  }
  ~ZmqUtilGuard() { kZmqUtil = original_; }
private:
  ZmqUtilInterface* original_;
};

TEST(LatencyReporterTest, ConstructorSetsUid) {
  // Just verify the reporter can be constructed without crashing.
  // The constructor creates a ZMQ context and socket cache internally.
  vector<Address> ips = {"10.0.0.1"};
  LatencyReporter reporter(ips, 0, 5);
  // If we get here without throwing, construction succeeded.
}

TEST(LatencyReporterTest, SetWarmupFlag) {
  vector<Address> ips;
  LatencyReporter reporter(ips, 0);
  // set_warmup should not throw
  reporter.set_warmup(true);
  reporter.set_warmup(false);
}

TEST(LatencyReporterTest, ReportSendsToAllMonitoringIps) {
  MockZmqUtil mock;
  ZmqUtilGuard guard(&mock);

  vector<Address> ips = {"10.0.0.1", "10.0.0.2"};
  LatencyReporter reporter(ips, 0, 3);

  vector<std::pair<string, double>> key_latencies = {
      {"key_a", 5.0}, {"key_b", 15.0}};
  reporter.report(42.5, 1000.0, key_latencies);

  // Should have sent one message per monitoring IP
  ASSERT_EQ(mock.sent.size(), 2u);

  // Deserialize the first message and verify contents
  UserFeedback feedback;
  ASSERT_TRUE(feedback.ParseFromString(mock.sent[0].payload));
  EXPECT_EQ(feedback.uid(), "cpp_client:3");
  EXPECT_DOUBLE_EQ(feedback.latency(), 42.5);
  EXPECT_DOUBLE_EQ(feedback.throughput(), 1000.0);
  EXPECT_FALSE(feedback.warmup());
  ASSERT_EQ(feedback.key_latency_size(), 2);
  EXPECT_EQ(feedback.key_latency(0).key(), "key_a");
  EXPECT_DOUBLE_EQ(feedback.key_latency(0).latency(), 5.0);
  EXPECT_EQ(feedback.key_latency(1).key(), "key_b");
  EXPECT_DOUBLE_EQ(feedback.key_latency(1).latency(), 15.0);

  // Both messages have the same payload (sent to different addresses)
  EXPECT_EQ(mock.sent[0].payload, mock.sent[1].payload);

  // Messages were sent to different sockets (one per monitoring IP)
  EXPECT_NE(mock.sent[0].socket, mock.sent[1].socket);
}

TEST(LatencyReporterTest, ReportRespectsWarmupFlag) {
  MockZmqUtil mock;
  ZmqUtilGuard guard(&mock);

  vector<Address> ips = {"10.0.0.1"};
  LatencyReporter reporter(ips, 0);
  reporter.set_warmup(true);

  reporter.report(10.0, 500.0, {});

  ASSERT_EQ(mock.sent.size(), 1u);
  UserFeedback feedback;
  ASSERT_TRUE(feedback.ParseFromString(mock.sent[0].payload));
  EXPECT_TRUE(feedback.warmup());
}

TEST(LatencyReporterTest, ReportWithNoMonitoringIpsSendsNothing) {
  MockZmqUtil mock;
  ZmqUtilGuard guard(&mock);

  vector<Address> ips;  // empty
  LatencyReporter reporter(ips, 0);

  reporter.report(10.0, 500.0, {});
  EXPECT_TRUE(mock.sent.empty());
}

TEST(LatencyReporterTest, FinishSendsFinishMessage) {
  MockZmqUtil mock;
  ZmqUtilGuard guard(&mock);

  vector<Address> ips = {"10.0.0.1"};
  LatencyReporter reporter(ips, 0, 7);

  reporter.finish();

  ASSERT_EQ(mock.sent.size(), 1u);
  UserFeedback feedback;
  ASSERT_TRUE(feedback.ParseFromString(mock.sent[0].payload));
  EXPECT_EQ(feedback.uid(), "cpp_client:7");
  EXPECT_TRUE(feedback.finish());
  // Other fields should be default
  EXPECT_DOUBLE_EQ(feedback.latency(), 0.0);
  EXPECT_DOUBLE_EQ(feedback.throughput(), 0.0);
}

TEST(LatencyReporterTest, FinishSendsToAllMonitoringIps) {
  MockZmqUtil mock;
  ZmqUtilGuard guard(&mock);

  vector<Address> ips = {"10.0.0.1", "10.0.0.2", "10.0.0.3"};
  LatencyReporter reporter(ips, 0);

  reporter.finish();

  ASSERT_EQ(mock.sent.size(), 3u);

  // Each message should go to a distinct socket (one per IP)
  set<zmq::socket_t*> sockets;
  for (const auto& m : mock.sent) {
    sockets.insert(m.socket);
  }
  EXPECT_EQ(sockets.size(), 3u);
}

TEST(LatencyReporterTest, BaseOffsetAffectsPort) {
  // Two reporters with the same IP but different base_offsets should use
  // different socket-cache entries (since the address string contains the
  // port = kFeedbackReportPort + base_offset). We verify this by checking
  // that the socket pointers differ.
  MockZmqUtil mock;
  ZmqUtilGuard guard(&mock);

  vector<Address> ips = {"10.0.0.1"};
  LatencyReporter reporter_a(ips, 0, 0);
  LatencyReporter reporter_b(ips, 100, 0);

  reporter_a.report(1.0, 1.0, {});
  ASSERT_EQ(mock.sent.size(), 1u);
  zmq::socket_t* socket_a = mock.sent[0].socket;

  reporter_b.report(1.0, 1.0, {});
  ASSERT_EQ(mock.sent.size(), 2u);
  zmq::socket_t* socket_b = mock.sent[1].socket;

  // Different base_offset -> different address -> different socket
  EXPECT_NE(socket_a, socket_b);
}
