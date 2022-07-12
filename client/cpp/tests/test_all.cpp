#include <stdlib.h>
#include <vector>

#include "gtest/gtest.h"

#include "kvs.pb.h"
#include "types.hpp"
#include "client_base.hpp"

int main(int argc, char *argv[]) {
//  log->set_level(spdlog::level::info);
  testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
