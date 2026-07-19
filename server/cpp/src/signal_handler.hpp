#ifndef SIGNAL_HANDLER_HPP
#define SIGNAL_HANDLER_HPP

#include <csignal>
#include <atomic>

inline std::atomic<bool> shutdown_requested{false};
inline std::atomic<bool> self_depart_requested{false};

inline void install_shutdown_handler() {
  struct sigaction sa;
  sa.sa_handler = [](int) { shutdown_requested.store(true); };
  sigemptyset(&sa.sa_mask);
  sa.sa_flags = 0;
  sigaction(SIGTERM, &sa, nullptr);
  sigaction(SIGINT, &sa, nullptr);

  struct sigaction depart_sa;
  depart_sa.sa_handler = [](int) { self_depart_requested.store(true); };
  sigemptyset(&depart_sa.sa_mask);
  depart_sa.sa_flags = 0;
  sigaction(SIGUSR1, &depart_sa, nullptr);
}

#endif // SIGNAL_HANDLER_HPP
