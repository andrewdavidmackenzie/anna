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
