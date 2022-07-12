#include "spdlog/sinks/basic_file_sink.h"
#include "spdlog/spdlog.h"

logger log_ = spdlog::basic_logger_mt("client_log", "client_log.txt", true);

class ClientBaseTest : public ::testing::Test {
protected:

  ClientBaseTest() {
  }

  virtual ~ClientBaseTest() {
  }

public:
  void SetUp() {
    // reset all global variables
  }

  void TearDown() {
  }
};
