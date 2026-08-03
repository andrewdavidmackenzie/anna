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

#ifndef INCLUDE_LATTICES_OR_SET_LATTICE_HPP_
#define INCLUDE_LATTICES_OR_SET_LATTICE_HPP_

#include <string>
#include <set>
#include "core_lattices.hpp"

// OR-Set state: tagged elements and tombstones.
struct OrSetState {
  // tag -> element value
  std::map<std::string, std::string> elements;
  // tombstoned tags
  std::set<std::string> tombstones;

  // Compute the live set: elements whose tags are not tombstoned.
  std::set<std::string> value() const {
    std::set<std::string> result;
    for (const auto &p : elements) {
      if (tombstones.find(p.first) == tombstones.end()) {
        result.insert(p.second);
      }
    }
    return result;
  }
};

// OR-Set (Observed-Remove Set) lattice. The merge takes the union of
// both elements and tombstones. This is commutative, associative,
// and idempotent -- safe for gossip-based replication.
// Add wins over concurrent remove (new tags are not in the tombstone set).
class OrSetLattice : public Lattice<OrSetState> {
 protected:
  void do_merge(const OrSetState &other) override {
    // Union of elements.
    for (const auto &p : other.elements) {
      this->element.elements.insert(p);
    }
    // Union of tombstones.
    for (const auto &t : other.tombstones) {
      this->element.tombstones.insert(t);
    }
  }

 public:
  OrSetLattice() : Lattice<OrSetState>(OrSetState()) {}
  OrSetLattice(const OrSetState &s) : Lattice<OrSetState>(s) {}

  MaxLattice<unsigned> size() const {
    auto live = element.value();
    return MaxLattice<unsigned>(live.empty() ? 0 : 1);
  }
};

#endif  // INCLUDE_LATTICES_OR_SET_LATTICE_HPP_
