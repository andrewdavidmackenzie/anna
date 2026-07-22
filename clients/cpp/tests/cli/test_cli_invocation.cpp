#include <gtest/gtest.h>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <array>
#include <string>

namespace {

struct ProcessResult {
  std::string stdout_str;
  std::string stderr_str;
  int exit_code;
};

ProcessResult run_command(const std::string& cmd) {
  ProcessResult result;

  std::string stdout_cmd = cmd + " 2>/dev/null";
  std::string stderr_cmd = cmd + " 2>&1 1>/dev/null";

  std::array<char, 4096> buffer;

  FILE* stdout_pipe = popen(stdout_cmd.c_str(), "r");
  if (stdout_pipe) {
    while (fgets(buffer.data(), buffer.size(), stdout_pipe)) {
      result.stdout_str += buffer.data();
    }
    result.exit_code = pclose(stdout_pipe);
    result.exit_code = WEXITSTATUS(result.exit_code);
  }

  FILE* stderr_pipe = popen(stderr_cmd.c_str(), "r");
  if (stderr_pipe) {
    while (fgets(buffer.data(), buffer.size(), stderr_pipe)) {
      result.stderr_str += buffer.data();
    }
    pclose(stderr_pipe);
  }

  return result;
}

std::string cli_binary;
std::string test_config;

void write_test_config() {
  std::ofstream f(test_config);
  f << "monitoring:\n"
    << "  mgmt_ip: 127.0.0.1\n"
    << "  ip: 127.0.0.1\n"
    << "routing:\n"
    << "  monitoring:\n"
    << "      - 127.0.0.1\n"
    << "  ip: 127.0.0.1\n"
    << "user:\n"
    << "  monitoring:\n"
    << "      - 127.0.0.1\n"
    << "  routing:\n"
    << "      - 127.0.0.1\n"
    << "  ip: 127.0.0.1\n"
    << "server:\n"
    << "  monitoring:\n"
    << "      - 127.0.0.1\n"
    << "  routing:\n"
    << "      - 127.0.0.1\n"
    << "  seed_ip: 127.0.0.1\n"
    << "  public_ip: 127.0.0.1\n"
    << "  private_ip: 127.0.0.1\n"
    << "  mgmt_ip: \"NULL\"\n"
    << "ebs: ./\n"
    << "capacities:\n"
    << "  memory-cap: 1\n"
    << "  ebs-cap: 0\n"
    << "threads:\n"
    << "  memory: 1\n"
    << "  ebs: 1\n"
    << "  routing: 1\n"
    << "  benchmark: 1\n"
    << "replication:\n"
    << "  memory: 1\n"
    << "  ebs: 0\n"
    << "  minimum: 1\n"
    << "  local: 1\n"
    << "policy:\n"
    << "  elasticity: false\n"
    << "  selective-rep: false\n"
    << "  tiering: false\n";
}

}  // namespace

class CliInvocationTest : public ::testing::Test {
protected:
  static void SetUpTestSuite() {
    const char* bin = std::getenv("ANNA_CLI_PATH");
    if (bin) {
      cli_binary = bin;
    } else {
#ifdef ANNA_CLI_PATH
      cli_binary = ANNA_CLI_PATH;
#else
      cli_binary = "./cli/anna-cli";
#endif
    }
    test_config = "cli_test_config.yml";
    write_test_config();
  }

  static void TearDownTestSuite() {
    std::remove(test_config.c_str());
    std::remove("client_log.txt");
  }
};

TEST_F(CliInvocationTest, UnrecognizedCommandFails) {
  auto r = run_command(cli_binary + " foobar");
  EXPECT_EQ(r.exit_code, 1);
}

TEST_F(CliInvocationTest, NoArgsPrintsUsage) {
  auto r = run_command(cli_binary);
  EXPECT_EQ(r.exit_code, 1);
}

TEST_F(CliInvocationTest, HelpPrintsCommands) {
  auto r = run_command(cli_binary + " help");
  EXPECT_EQ(r.exit_code, 0);
  EXPECT_TRUE(r.stdout_str.find("routing") != std::string::npos ||
              r.stdout_str.find("cli") != std::string::npos);
}

TEST_F(CliInvocationTest, StopPrintsCount) {
  auto r = run_command(cli_binary + " stop");
  EXPECT_EQ(r.exit_code, 0);
  EXPECT_TRUE(r.stdout_str.find("anna processes were stopped") != std::string::npos);
}

TEST_F(CliInvocationTest, StatusReturnsSuccess) {
  auto r = run_command(cli_binary + " status");
  EXPECT_EQ(r.exit_code, 0);
}

TEST_F(CliInvocationTest, StartWithoutConfigFails) {
  auto r = run_command(cli_binary + " start");
  EXPECT_EQ(r.exit_code, 1);
  EXPECT_TRUE(r.stderr_str.find("config") != std::string::npos);
}

TEST_F(CliInvocationTest, StartWithConfigReturnsSuccess) {
  auto r = run_command(cli_binary + " --config " + test_config + " start");
  EXPECT_EQ(r.exit_code, 0);
  EXPECT_TRUE(r.stdout_str.find("anna processes were started") != std::string::npos);
  // Clean up any started processes
  run_command(cli_binary + " stop");
}

TEST_F(CliInvocationTest, CliWithoutRoutingFails) {
  // CLI command requires --routing and --client-ip
  auto r = run_command(cli_binary + " cli");
  EXPECT_EQ(r.exit_code, 1);
  EXPECT_TRUE(r.stderr_str.find("routing") != std::string::npos);
}

TEST_F(CliInvocationTest, CliWithRoutingNoClientIpFails) {
  auto r = run_command(cli_binary + " --routing 127.0.0.1 cli");
  EXPECT_EQ(r.exit_code, 1);
  EXPECT_TRUE(r.stderr_str.find("client-ip") != std::string::npos);
}

TEST_F(CliInvocationTest, CliFileWithHelpAndStatus) {
  // Write a command file with non-KVS commands
  std::string cmd_file = "cli_test_commands.txt";
  {
    std::ofstream f(cmd_file);
    f << "HELP\n";
    f << "STATUS\n";
    f << "STOP\n";
  }
  auto r = run_command(cli_binary + " --routing 127.0.0.1 --client-ip 127.0.0.1 cli " + cmd_file);
  // These commands should complete (HELP prints usage, STATUS checks processes,
  // STOP tries to stop processes)
  EXPECT_EQ(r.exit_code, 0);
  EXPECT_TRUE(r.stdout_str.find("GET") != std::string::npos ||
              r.stdout_str.find("PUT") != std::string::npos);
  std::remove(cmd_file.c_str());
}

TEST_F(CliInvocationTest, CliFileWithUnrecognizedCommand) {
  std::string cmd_file = "cli_test_unrecognized.txt";
  {
    std::ofstream f(cmd_file);
    f << "INVALID_COMMAND\n";
  }
  auto r = run_command(cli_binary + " --routing 127.0.0.1 --client-ip 127.0.0.1 cli " + cmd_file);
  EXPECT_EQ(r.exit_code, 0);
  EXPECT_TRUE(r.stdout_str.find("Unrecognized command") != std::string::npos);
  std::remove(cmd_file.c_str());
}

TEST_F(CliInvocationTest, CliFileWithStartCommand) {
  std::string cmd_file = "cli_test_start.txt";
  {
    std::ofstream f(cmd_file);
    f << "START\n";
    f << "STOP\n";  // Clean up
  }
  auto r = run_command(cli_binary + " --routing 127.0.0.1 --client-ip 127.0.0.1 --config " + test_config + " cli " + cmd_file);
  EXPECT_EQ(r.exit_code, 0);
  EXPECT_TRUE(r.stdout_str.find("anna processes") != std::string::npos);
  std::remove(cmd_file.c_str());
}

TEST_F(CliInvocationTest, CliFileWithDeleteCommand) {
  // DELETE with a mock client would hang since no server is running.
  // But we can test that the command file processing works with
  // non-blocking commands.
  std::string cmd_file = "cli_test_delete.txt";
  {
    std::ofstream f(cmd_file);
    f << "HELP\n";
    f << "help\n";  // lowercase should also work (case conversion)
  }
  auto r = run_command(cli_binary + " --routing 127.0.0.1 --client-ip 127.0.0.1 cli " + cmd_file);
  EXPECT_EQ(r.exit_code, 0);
  std::remove(cmd_file.c_str());
}

TEST_F(CliInvocationTest, CliWithThreadsArg) {
  // Test --threads flag parsing
  auto r = run_command(cli_binary + " --routing 127.0.0.1 --client-ip 127.0.0.1 --threads 2 help");
  EXPECT_EQ(r.exit_code, 0);
}

int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
