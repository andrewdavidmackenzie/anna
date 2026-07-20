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
#include "value_change_subscriber.hpp"

TEST(ValueChangeSubscriberTest, PortConstants) {
  EXPECT_EQ(kClientCacheRegistrationPort, 7200u);
  EXPECT_EQ(kClientCacheUpdatePort, 7150u);
}

TEST(ValueChangeSubscriberTest, GetCachedMissing) {
  ValueChangeSubscriber client("127.0.0.1", "127.0.0.1", 1, 50000);
  std::string value;
  EXPECT_FALSE(client.get_cached("nonexistent", value));
}

TEST(ValueChangeSubscriberTest, WatchedKeysInitiallyEmpty) {
  ValueChangeSubscriber client("127.0.0.1", "127.0.0.1", 1, 50100);
  EXPECT_TRUE(client.watched_keys().empty());
}
