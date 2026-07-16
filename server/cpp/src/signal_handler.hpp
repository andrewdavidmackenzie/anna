#ifndef SIGNAL_HANDLER_HPP
#define SIGNAL_HANDLER_HPP

#include <csignal>
#include <atomic>

inline std::atomic<bool> shutdown_requested{false};

inline void install_shutdown_handler() {
  struct sigaction sa;
  sa.sa_handler = [](int) { shutdown_requested.store(true); };
  sigemptyset(&sa.sa_mask);
  sa.sa_flags = 0;
  sigaction(SIGTERM, &sa, nullptr);
  sigaction(SIGINT, &sa, nullptr);
}

#endif // SIGNAL_HANDLER_HPP
