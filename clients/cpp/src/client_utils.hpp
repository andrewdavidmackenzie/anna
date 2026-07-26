// Client-internal utility functions. Not part of the public API.
// These duplicate functions from the server's common.hpp to avoid
// depending on server headers.

#ifndef CLIENT_UTILS_HPP_
#define CLIENT_UTILS_HPP_

#include <chrono>
#include <string>
#include <vector>

// Skip if server's common.hpp is already included in this translation unit.
// If we provide these definitions, set the guard so common.hpp won't
// redefine them. Note: common.hpp has additional content (lattice types,
// serialize/deserialize) that won't be available when this guard is set.
// Test files that need lattice types must include common.hpp BEFORE
// client headers.
#ifndef INCLUDE_COMMON_HPP_
#define INCLUDE_COMMON_HPP_

inline void split(const std::string &s, char delim,
                  std::vector<std::string> &tokens) {
  size_t start = 0;
  size_t end;
  while ((end = s.find(delim, start)) != std::string::npos) {
    tokens.push_back(s.substr(start, end - start));
    start = end + 1;
  }
  tokens.push_back(s.substr(start));
}

inline unsigned long long generate_timestamp(unsigned tid) {
  auto now = std::chrono::system_clock::now();
  unsigned long long t =
      std::chrono::duration_cast<std::chrono::milliseconds>(
          now.time_since_epoch())
          .count();
  t <<= 8;
  t |= (tid & 0xFF);
  return t;
}

const std::string kMetadataIdentifier = "ANNA_METADATA";
const std::string kMetadataDelimiter = "|";

#endif  // INCLUDE_COMMON_HPP_

#endif  // CLIENT_UTILS_HPP_
