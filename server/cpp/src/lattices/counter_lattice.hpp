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

#ifndef INCLUDE_LATTICES_COUNTER_LATTICE_HPP_
#define INCLUDE_LATTICES_COUNTER_LATTICE_HPP_

#include <string>
#include "core_lattices.hpp"

// PN-Counter state: per-node cumulative increments and decrements.
struct PNCounterState {
  std::map<std::string, unsigned long long> increments;
  std::map<std::string, unsigned long long> decrements;

  // Compute the counter value: sum(increments) - sum(decrements).
  long long value() const {
    unsigned long long pos = 0, neg = 0;
    for (const auto &p : increments) pos += p.second;
    for (const auto &p : decrements) neg += p.second;
    return static_cast<long long>(pos) - static_cast<long long>(neg);
  }
};

// PN-Counter lattice. The merge takes the per-node max for both
// increments and decrements. This is commutative, associative,
// and idempotent -- safe for gossip-based replication.
class CounterLattice : public Lattice<PNCounterState> {
 protected:
  void do_merge(const PNCounterState &other) override {
    for (const auto &p : other.increments) {
      auto &local = this->element.increments[p.first];
      if (p.second > local) local = p.second;
    }
    for (const auto &p : other.decrements) {
      auto &local = this->element.decrements[p.first];
      if (p.second > local) local = p.second;
    }
  }

 public:
  CounterLattice() : Lattice<PNCounterState>(PNCounterState()) {}
  CounterLattice(const PNCounterState &s) : Lattice<PNCounterState>(s) {}

  MaxLattice<unsigned> size() const {
    // Return non-zero if the counter has any state.
    return MaxLattice<unsigned>(
        element.increments.size() + element.decrements.size() > 0 ? 1 : 0);
  }
};

#endif  // INCLUDE_LATTICES_COUNTER_LATTICE_HPP_
