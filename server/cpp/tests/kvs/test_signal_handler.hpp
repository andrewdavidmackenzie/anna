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

#include "signal_handler.hpp"

TEST(SignalHandlerTest, InitialState) {
  EXPECT_FALSE(shutdown_requested.load());
  EXPECT_FALSE(self_depart_requested.load());
}

TEST(SignalHandlerTest, Sigusr1TriggersSelfDepart) {
  install_shutdown_handler();
  self_depart_requested.store(false);

  raise(SIGUSR1);

  EXPECT_TRUE(self_depart_requested.load());
  self_depart_requested.store(false);
}

TEST(SignalHandlerTest, SigtermTriggersShutdown) {
  install_shutdown_handler();
  shutdown_requested.store(false);

  raise(SIGTERM);

  EXPECT_TRUE(shutdown_requested.load());
  shutdown_requested.store(false);
}
