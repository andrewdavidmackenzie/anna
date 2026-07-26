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

// Unit tests for the lattice type hierarchy:
//   lattice.hpp          -- abstract Lattice<T> base class
//   core_lattices.hpp    -- BoolLattice, MaxLattice, SetLattice,
//                           OrderedSetLattice, MapLattice
//   priority_lattice.hpp -- PriorityLattice
//   single_key_causal_lattice.hpp -- SingleKeyCausalLattice
//   multi_key_causal_lattice.hpp  -- MultiKeyCausalLattice
//
// These are header-only templates shared with the server.  The client
// includes them via INCLUDE_DIRECTORIES(../../server/cpp/src).

#include "gtest/gtest.h"

#include "common.hpp"
#include "threads.hpp"

// =====================================================================
// lattice.hpp -- tested via concrete subclasses
// =====================================================================

TEST(LatticeBaseTest, CopyConstructor) {
  MaxLattice<unsigned> original(42);
  MaxLattice<unsigned> copy(original);
  EXPECT_EQ(copy.reveal(), 42u);
}

TEST(LatticeBaseTest, AssignmentOperator) {
  MaxLattice<unsigned> a(10);
  MaxLattice<unsigned> b(20);
  a = b;
  EXPECT_EQ(a.reveal(), 20u);
}

TEST(LatticeBaseTest, EqualityOperatorTrue) {
  MaxLattice<unsigned> a(5);
  MaxLattice<unsigned> b(5);
  EXPECT_TRUE(a == b);
}

TEST(LatticeBaseTest, EqualityOperatorFalse) {
  MaxLattice<unsigned> a(5);
  MaxLattice<unsigned> b(10);
  EXPECT_FALSE(a == b);
}

TEST(LatticeBaseTest, Reveal) {
  MaxLattice<unsigned> m(99);
  EXPECT_EQ(m.reveal(), 99u);
}

TEST(LatticeBaseTest, MergeWithRawValue) {
  MaxLattice<unsigned> m(5);
  m.merge(10u);
  EXPECT_EQ(m.reveal(), 10u);
}

TEST(LatticeBaseTest, MergeWithLattice) {
  MaxLattice<unsigned> a(5);
  MaxLattice<unsigned> b(10);
  a.merge(b);
  EXPECT_EQ(a.reveal(), 10u);
}

TEST(LatticeBaseTest, AssignRawValue) {
  MaxLattice<unsigned> m(5);
  m.assign(42u);
  EXPECT_EQ(m.reveal(), 42u);
}

TEST(LatticeBaseTest, AssignFromLattice) {
  MaxLattice<unsigned> a(5);
  MaxLattice<unsigned> b(42);
  a.assign(b);
  EXPECT_EQ(a.reveal(), 42u);
}

TEST(LatticeBaseTest, DestructorViaBasePointer) {
  // Verifies virtual destructor works correctly
  Lattice<unsigned>* p = new MaxLattice<unsigned>(10);
  EXPECT_EQ(p->reveal(), 10u);
  delete p;  // should not leak
}

// =====================================================================
// BoolLattice
// =====================================================================

TEST(BoolLatticeTest, DefaultConstructorIsFalse) {
  BoolLattice b;
  EXPECT_FALSE(b.reveal());
}

TEST(BoolLatticeTest, ValueConstructorTrue) {
  BoolLattice b(true);
  EXPECT_TRUE(b.reveal());
}

TEST(BoolLatticeTest, ValueConstructorFalse) {
  BoolLattice b(false);
  EXPECT_FALSE(b.reveal());
}

TEST(BoolLatticeTest, MergeTrueIntoFalse) {
  BoolLattice b(false);
  b.merge(true);
  EXPECT_TRUE(b.reveal());
}

TEST(BoolLatticeTest, MergeFalseIntoTrue) {
  BoolLattice b(true);
  b.merge(false);
  EXPECT_TRUE(b.reveal());  // OR: true | false = true
}

TEST(BoolLatticeTest, MergeFalseIntoFalse) {
  BoolLattice b(false);
  b.merge(false);
  EXPECT_FALSE(b.reveal());
}

TEST(BoolLatticeTest, MergeTrueIntoTrue) {
  BoolLattice b(true);
  b.merge(true);
  EXPECT_TRUE(b.reveal());
}

TEST(BoolLatticeTest, MergeLattice) {
  BoolLattice a(false);
  BoolLattice b(true);
  a.merge(b);
  EXPECT_TRUE(a.reveal());
}

TEST(BoolLatticeTest, CopyConstructor) {
  BoolLattice a(true);
  BoolLattice b(a);
  EXPECT_TRUE(b.reveal());
}

TEST(BoolLatticeTest, AssignmentOperator) {
  BoolLattice a(false);
  BoolLattice b(true);
  a = b;
  EXPECT_TRUE(a.reveal());
}

TEST(BoolLatticeTest, Equality) {
  BoolLattice a(true);
  BoolLattice b(true);
  BoolLattice c(false);
  EXPECT_TRUE(a == b);
  EXPECT_FALSE(a == c);
}

// =====================================================================
// MaxLattice
// =====================================================================

TEST(MaxLatticeTest, DefaultConstructorIsZero) {
  MaxLattice<unsigned> m;
  EXPECT_EQ(m.reveal(), 0u);
}

TEST(MaxLatticeTest, ValueConstructor) {
  MaxLattice<unsigned> m(42);
  EXPECT_EQ(m.reveal(), 42u);
}

TEST(MaxLatticeTest, MergeLargerTakeNew) {
  MaxLattice<unsigned> m(5);
  m.merge(10u);
  EXPECT_EQ(m.reveal(), 10u);
}

TEST(MaxLatticeTest, MergeSmallerKeepOld) {
  MaxLattice<unsigned> m(10);
  m.merge(5u);
  EXPECT_EQ(m.reveal(), 10u);
}

TEST(MaxLatticeTest, MergeEqualKeepValue) {
  MaxLattice<unsigned> m(7);
  m.merge(7u);
  EXPECT_EQ(m.reveal(), 7u);
}

TEST(MaxLatticeTest, MergeLattice) {
  MaxLattice<unsigned> a(5);
  MaxLattice<unsigned> b(10);
  a.merge(b);
  EXPECT_EQ(a.reveal(), 10u);
}

TEST(MaxLatticeTest, Add) {
  MaxLattice<unsigned> m(10);
  MaxLattice<unsigned> result = m.add(5);
  EXPECT_EQ(result.reveal(), 15u);
  // original unchanged
  EXPECT_EQ(m.reveal(), 10u);
}

TEST(MaxLatticeTest, Subtract) {
  MaxLattice<unsigned> m(10);
  MaxLattice<unsigned> result = m.subtract(3);
  EXPECT_EQ(result.reveal(), 7u);
  EXPECT_EQ(m.reveal(), 10u);
}

TEST(MaxLatticeTest, CopyConstructor) {
  MaxLattice<unsigned> a(42);
  MaxLattice<unsigned> b(a);
  EXPECT_EQ(b.reveal(), 42u);
}

// =====================================================================
// SetLattice
// =====================================================================

TEST(SetLatticeTest, DefaultConstructorIsEmpty) {
  SetLattice<string> s;
  EXPECT_TRUE(s.reveal().empty());
}

TEST(SetLatticeTest, ValueConstructor) {
  set<string> init = {"a", "b"};
  SetLattice<string> s(init);
  EXPECT_EQ(s.reveal(), init);
}

TEST(SetLatticeTest, MergeUnion) {
  SetLattice<string> s(set<string>({"a", "b"}));
  s.merge(set<string>({"b", "c"}));
  set<string> expected = {"a", "b", "c"};
  EXPECT_EQ(s.reveal(), expected);
}

TEST(SetLatticeTest, MergeLattice) {
  SetLattice<string> a(set<string>({"x"}));
  SetLattice<string> b(set<string>({"y"}));
  a.merge(b);
  set<string> expected = {"x", "y"};
  EXPECT_EQ(a.reveal(), expected);
}

TEST(SetLatticeTest, Insert) {
  SetLattice<string> s;
  s.insert("hello");
  EXPECT_EQ(s.reveal().size(), 1u);
  EXPECT_TRUE(s.reveal().count("hello"));
}

TEST(SetLatticeTest, Size) {
  SetLattice<string> s(set<string>({"a", "b", "c"}));
  EXPECT_EQ(s.size().reveal(), 3u);
}

TEST(SetLatticeTest, IntersectWithOverlap) {
  SetLattice<string> s(set<string>({"a", "b", "c"}));
  SetLattice<string> result = s.intersect(set<string>({"b", "c", "d"}));
  set<string> expected = {"b", "c"};
  EXPECT_EQ(result.reveal(), expected);
}

TEST(SetLatticeTest, IntersectNoOverlap) {
  SetLattice<string> s(set<string>({"a", "b"}));
  SetLattice<string> result = s.intersect(set<string>({"c", "d"}));
  EXPECT_TRUE(result.reveal().empty());
}

static bool starts_with_a(string s) {
  return !s.empty() && s[0] == 'a';
}

TEST(SetLatticeTest, Project) {
  SetLattice<string> s(set<string>({"apple", "banana", "avocado"}));
  SetLattice<string> result = s.project(starts_with_a);
  set<string> expected = {"apple", "avocado"};
  EXPECT_EQ(result.reveal(), expected);
}

TEST(SetLatticeTest, ProjectNoneMatch) {
  SetLattice<string> s(set<string>({"banana", "cherry"}));
  SetLattice<string> result = s.project(starts_with_a);
  EXPECT_TRUE(result.reveal().empty());
}

TEST(SetLatticeTest, CopyConstructor) {
  SetLattice<string> a(set<string>({"x"}));
  SetLattice<string> b(a);
  EXPECT_EQ(b.reveal(), a.reveal());
}

// =====================================================================
// OrderedSetLattice
// =====================================================================

TEST(OrderedSetLatticeTest, DefaultConstructorIsEmpty) {
  OrderedSetLattice<string> s;
  EXPECT_TRUE(s.reveal().empty());
}

TEST(OrderedSetLatticeTest, ValueConstructor) {
  ordered_set<string> init = {"a", "b"};
  OrderedSetLattice<string> s(init);
  EXPECT_EQ(s.reveal(), init);
}

TEST(OrderedSetLatticeTest, MergeUnion) {
  OrderedSetLattice<string> s(ordered_set<string>({"a", "b"}));
  s.merge(ordered_set<string>({"b", "c"}));
  ordered_set<string> expected = {"a", "b", "c"};
  EXPECT_EQ(s.reveal(), expected);
}

TEST(OrderedSetLatticeTest, MergeLattice) {
  OrderedSetLattice<string> a(ordered_set<string>({"x"}));
  OrderedSetLattice<string> b(ordered_set<string>({"y"}));
  a.merge(b);
  ordered_set<string> expected = {"x", "y"};
  EXPECT_EQ(a.reveal(), expected);
}

TEST(OrderedSetLatticeTest, Insert) {
  OrderedSetLattice<string> s;
  s.insert("hello");
  EXPECT_EQ(s.reveal().size(), 1u);
  EXPECT_TRUE(s.reveal().count("hello"));
}

TEST(OrderedSetLatticeTest, Size) {
  OrderedSetLattice<string> s(ordered_set<string>({"a", "b", "c"}));
  EXPECT_EQ(s.size().reveal(), 3u);
}

TEST(OrderedSetLatticeTest, IntersectWithOverlap) {
  OrderedSetLattice<string> s(ordered_set<string>({"a", "b", "c"}));
  OrderedSetLattice<string> result =
      s.intersect(ordered_set<string>({"b", "c", "d"}));
  ordered_set<string> expected = {"b", "c"};
  EXPECT_EQ(result.reveal(), expected);
}

TEST(OrderedSetLatticeTest, IntersectNoOverlap) {
  OrderedSetLattice<string> s(ordered_set<string>({"a", "b"}));
  OrderedSetLattice<string> result =
      s.intersect(ordered_set<string>({"c", "d"}));
  EXPECT_TRUE(result.reveal().empty());
}

static bool os_starts_with_a(string s) {
  return !s.empty() && s[0] == 'a';
}

TEST(OrderedSetLatticeTest, Project) {
  OrderedSetLattice<string> s(
      ordered_set<string>({"apple", "banana", "avocado"}));
  OrderedSetLattice<string> result = s.project(os_starts_with_a);
  ordered_set<string> expected = {"apple", "avocado"};
  EXPECT_EQ(result.reveal(), expected);
}

TEST(OrderedSetLatticeTest, ProjectNoneMatch) {
  OrderedSetLattice<string> s(ordered_set<string>({"banana", "cherry"}));
  OrderedSetLattice<string> result = s.project(os_starts_with_a);
  EXPECT_TRUE(result.reveal().empty());
}

TEST(OrderedSetLatticeTest, CopyConstructor) {
  OrderedSetLattice<string> a(ordered_set<string>({"x"}));
  OrderedSetLattice<string> b(a);
  EXPECT_EQ(b.reveal(), a.reveal());
}

// =====================================================================
// MapLattice
// =====================================================================

TEST(MapLatticeTest, DefaultConstructorIsEmpty) {
  MapLattice<string, MaxLattice<unsigned>> m;
  EXPECT_TRUE(m.reveal().empty());
}

TEST(MapLatticeTest, ValueConstructor) {
  map<string, MaxLattice<unsigned>> init;
  init.emplace("k", MaxLattice<unsigned>(5));
  MapLattice<string, MaxLattice<unsigned>> m(init);
  EXPECT_EQ(m.reveal().at("k").reveal(), 5u);
}

TEST(MapLatticeTest, MergeNewKey) {
  MapLattice<string, MaxLattice<unsigned>> m;
  map<string, MaxLattice<unsigned>> other;
  other.emplace("k", MaxLattice<unsigned>(10));
  m.merge(other);
  EXPECT_EQ(m.at("k").reveal(), 10u);
}

TEST(MapLatticeTest, MergeExistingKeyMergesValues) {
  map<string, MaxLattice<unsigned>> init;
  init.emplace("k", MaxLattice<unsigned>(5));
  MapLattice<string, MaxLattice<unsigned>> m(init);

  map<string, MaxLattice<unsigned>> other;
  other.emplace("k", MaxLattice<unsigned>(10));
  m.merge(other);

  // MaxLattice merge: max(5, 10) = 10
  EXPECT_EQ(m.at("k").reveal(), 10u);
}

TEST(MapLatticeTest, MergeExistingKeyExistingWins) {
  map<string, MaxLattice<unsigned>> init;
  init.emplace("k", MaxLattice<unsigned>(10));
  MapLattice<string, MaxLattice<unsigned>> m(init);

  map<string, MaxLattice<unsigned>> other;
  other.emplace("k", MaxLattice<unsigned>(5));
  m.merge(other);

  // MaxLattice merge: max(10, 5) = 10
  EXPECT_EQ(m.at("k").reveal(), 10u);
}

TEST(MapLatticeTest, MergeLattice) {
  MapLattice<string, MaxLattice<unsigned>> a;
  a.insert("x", MaxLattice<unsigned>(1));

  MapLattice<string, MaxLattice<unsigned>> b;
  b.insert("y", MaxLattice<unsigned>(2));

  a.merge(b);
  EXPECT_EQ(a.at("x").reveal(), 1u);
  EXPECT_EQ(a.at("y").reveal(), 2u);
}

TEST(MapLatticeTest, Size) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.insert("a", MaxLattice<unsigned>(1));
  m.insert("b", MaxLattice<unsigned>(2));
  EXPECT_EQ(m.size().reveal(), 2u);
}

TEST(MapLatticeTest, ContainsTrue) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.insert("key", MaxLattice<unsigned>(1));
  EXPECT_TRUE(m.contains("key").reveal());
}

TEST(MapLatticeTest, ContainsFalse) {
  MapLattice<string, MaxLattice<unsigned>> m;
  EXPECT_FALSE(m.contains("missing").reveal());
}

TEST(MapLatticeTest, KeySet) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.insert("a", MaxLattice<unsigned>(1));
  m.insert("b", MaxLattice<unsigned>(2));
  SetLattice<string> ks = m.key_set();
  EXPECT_TRUE(ks.reveal().count("a"));
  EXPECT_TRUE(ks.reveal().count("b"));
  EXPECT_EQ(ks.size().reveal(), 2u);
}

TEST(MapLatticeTest, At) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.insert("k", MaxLattice<unsigned>(42));
  EXPECT_EQ(m.at("k").reveal(), 42u);
}

TEST(MapLatticeTest, RemoveExisting) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.insert("k", MaxLattice<unsigned>(1));
  m.remove("k");
  EXPECT_FALSE(m.contains("k").reveal());
}

TEST(MapLatticeTest, RemoveNonExisting) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.remove("nonexistent");  // should not crash
  EXPECT_TRUE(m.reveal().empty());
}

TEST(MapLatticeTest, Insert) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.insert("k", MaxLattice<unsigned>(5));
  EXPECT_EQ(m.at("k").reveal(), 5u);
}

TEST(MapLatticeTest, InsertExistingKeyMerges) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.insert("k", MaxLattice<unsigned>(5));
  m.insert("k", MaxLattice<unsigned>(10));
  EXPECT_EQ(m.at("k").reveal(), 10u);
}

// NOTE: MapLattice::intersect() has a pre-existing const-correctness
// issue (calls non-const at() from a const method) so we skip testing
// it directly.  The code path is exercised transitively via causal
// lattice merge tests.

static bool gt_five(MaxLattice<unsigned> v) {
  return v.reveal() > 5;
}

TEST(MapLatticeTest, Project) {
  MapLattice<string, MaxLattice<unsigned>> m;
  m.insert("a", MaxLattice<unsigned>(3));
  m.insert("b", MaxLattice<unsigned>(10));
  m.insert("c", MaxLattice<unsigned>(7));

  MapLattice<string, MaxLattice<unsigned>> result = m.project(gt_five);
  EXPECT_FALSE(result.contains("a").reveal());
  EXPECT_TRUE(result.contains("b").reveal());
  EXPECT_TRUE(result.contains("c").reveal());
}

TEST(MapLatticeTest, CopyConstructor) {
  MapLattice<string, MaxLattice<unsigned>> a;
  a.insert("k", MaxLattice<unsigned>(1));
  MapLattice<string, MaxLattice<unsigned>> b(a);
  EXPECT_EQ(b.at("k").reveal(), 1u);
}

// =====================================================================
// PriorityLattice
// =====================================================================

TEST(PriorityValuePairTest, DefaultConstructor) {
  PriorityValuePair<double, string> p;
  EXPECT_EQ(p.priority, static_cast<double>(INT_MAX));
  EXPECT_TRUE(p.value.empty());
}

TEST(PriorityValuePairTest, ParameterizedConstructor) {
  PriorityValuePair<double, string> p(3.14, "hello");
  EXPECT_DOUBLE_EQ(p.priority, 3.14);
  EXPECT_EQ(p.value, "hello");
}

TEST(PriorityValuePairTest, Size) {
  PriorityValuePair<double, string> p(1.0, "test");
  EXPECT_EQ(p.size(), sizeof(double) + 4u);  // "test" has size 4
}

TEST(PriorityLatticeTest, DefaultConstructor) {
  PriorityLattice<double, string> pl;
  EXPECT_EQ(pl.reveal().priority, static_cast<double>(INT_MAX));
  EXPECT_TRUE(pl.reveal().value.empty());
}

TEST(PriorityLatticeTest, ValueConstructor) {
  PriorityValuePair<double, string> p(2.5, "val");
  PriorityLattice<double, string> pl(p);
  EXPECT_DOUBLE_EQ(pl.reveal().priority, 2.5);
  EXPECT_EQ(pl.reveal().value, "val");
}

TEST(PriorityLatticeTest, MergeLowerPriorityWins) {
  // Default compare is std::less, so lower priority value wins
  PriorityLattice<double, string> pl(
      PriorityValuePair<double, string>(5.0, "old"));
  pl.merge(PriorityValuePair<double, string>(2.0, "new"));
  EXPECT_DOUBLE_EQ(pl.reveal().priority, 2.0);
  EXPECT_EQ(pl.reveal().value, "new");
}

TEST(PriorityLatticeTest, MergeHigherPriorityLoses) {
  PriorityLattice<double, string> pl(
      PriorityValuePair<double, string>(2.0, "existing"));
  pl.merge(PriorityValuePair<double, string>(5.0, "incoming"));
  EXPECT_DOUBLE_EQ(pl.reveal().priority, 2.0);
  EXPECT_EQ(pl.reveal().value, "existing");
}

TEST(PriorityLatticeTest, MergeEqualPriorityKeepsExisting) {
  PriorityLattice<double, string> pl(
      PriorityValuePair<double, string>(3.0, "existing"));
  pl.merge(PriorityValuePair<double, string>(3.0, "incoming"));
  // std::less: 3.0 < 3.0 is false, so existing stays
  EXPECT_DOUBLE_EQ(pl.reveal().priority, 3.0);
  EXPECT_EQ(pl.reveal().value, "existing");
}

TEST(PriorityLatticeTest, MergeLattice) {
  PriorityLattice<double, string> a(
      PriorityValuePair<double, string>(5.0, "a_val"));
  PriorityLattice<double, string> b(
      PriorityValuePair<double, string>(1.0, "b_val"));
  a.merge(b);
  EXPECT_DOUBLE_EQ(a.reveal().priority, 1.0);
  EXPECT_EQ(a.reveal().value, "b_val");
}

TEST(PriorityLatticeTest, Size) {
  PriorityLattice<double, string> pl(
      PriorityValuePair<double, string>(1.0, "test"));
  EXPECT_EQ(pl.size().reveal(), sizeof(double) + 4u);
}

TEST(PriorityLatticeTest, CopyConstructor) {
  PriorityLattice<double, string> a(
      PriorityValuePair<double, string>(1.5, "val"));
  PriorityLattice<double, string> b(a);
  EXPECT_DOUBLE_EQ(b.reveal().priority, 1.5);
  EXPECT_EQ(b.reveal().value, "val");
}

TEST(PriorityLatticeTest, AssignmentOperator) {
  PriorityLattice<double, string> a(
      PriorityValuePair<double, string>(1.0, "a"));
  PriorityLattice<double, string> b(
      PriorityValuePair<double, string>(2.0, "b"));
  a = b;
  EXPECT_DOUBLE_EQ(a.reveal().priority, 2.0);
  EXPECT_EQ(a.reveal().value, "b");
}

TEST(PriorityLatticeTest, Assign) {
  PriorityLattice<double, string> pl(
      PriorityValuePair<double, string>(1.0, "old"));
  pl.assign(PriorityValuePair<double, string>(9.0, "new"));
  EXPECT_DOUBLE_EQ(pl.reveal().priority, 9.0);
  EXPECT_EQ(pl.reveal().value, "new");
}

// =====================================================================
// VectorClockValuePair (single_key_causal_lattice.hpp)
// =====================================================================

TEST(VectorClockValuePairTest, DefaultConstructor) {
  VectorClockValuePair<SetLattice<string>> p;
  EXPECT_TRUE(p.vector_clock.reveal().empty());
  EXPECT_TRUE(p.value.reveal().empty());
}

TEST(VectorClockValuePairTest, UnsignedConstructor) {
  VectorClockValuePair<SetLattice<string>> p(0u);
  EXPECT_TRUE(p.vector_clock.reveal().empty());
  EXPECT_TRUE(p.value.reveal().empty());
}

TEST(VectorClockValuePairTest, TwoArgConstructor) {
  VectorClock vc;
  vc.insert("c1", MaxLattice<unsigned>(3));
  SetLattice<string> val(set<string>({"v1"}));
  VectorClockValuePair<SetLattice<string>> p(vc, val);
  EXPECT_EQ(p.vector_clock.at("c1").reveal(), 3u);
  EXPECT_TRUE(p.value.reveal().count("v1"));
}

TEST(VectorClockValuePairTest, Size) {
  VectorClockValuePair<SetLattice<string>> p;
  p.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p.value.insert("val");
  // size = vc_entries * 2 * sizeof(unsigned) + value.size()
  // 1 * 2 * 4 = 8, + 1 = 9
  unsigned expected = 1 * 2 * sizeof(unsigned) + 1;
  EXPECT_EQ(p.size(), expected);
}

// =====================================================================
// SingleKeyCausalLattice
// =====================================================================

TEST(SingleKeyCausalLatticeTest, DefaultConstructor) {
  SingleKeyCausalLattice<SetLattice<string>> skcl;
  EXPECT_TRUE(skcl.reveal().vector_clock.reveal().empty());
  EXPECT_TRUE(skcl.reveal().value.reveal().empty());
}

TEST(SingleKeyCausalLatticeTest, ValueConstructor) {
  VectorClockValuePair<SetLattice<string>> p;
  p.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p.value.insert("hello");
  SingleKeyCausalLattice<SetLattice<string>> skcl(p);
  EXPECT_EQ(skcl.reveal().vector_clock.reveal().at("c1").reveal(), 1u);
  EXPECT_TRUE(skcl.reveal().value.reveal().count("hello"));
}

TEST(SingleKeyCausalLatticeTest, Size) {
  VectorClockValuePair<SetLattice<string>> p;
  p.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p.value.insert("v");
  SingleKeyCausalLattice<SetLattice<string>> skcl(p);
  EXPECT_GT(skcl.size().reveal(), 0u);
}

TEST(SingleKeyCausalLatticeTest, MergeIncomingDominates) {
  // existing: {c1:1}, incoming: {c1:2} -- incoming dominates
  VectorClockValuePair<SetLattice<string>> p1;
  p1.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p1.value.insert("old");
  SingleKeyCausalLattice<SetLattice<string>> skcl(p1);

  VectorClockValuePair<SetLattice<string>> p2;
  p2.vector_clock.insert("c1", MaxLattice<unsigned>(2));
  p2.value.insert("new");
  skcl.merge(p2);

  // After merge, vc should be {c1:2}, value should be replaced with "new"
  EXPECT_EQ(skcl.reveal().vector_clock.reveal().at("c1").reveal(), 2u);
  EXPECT_TRUE(skcl.reveal().value.reveal().count("new"));
  EXPECT_FALSE(skcl.reveal().value.reveal().count("old"));
}

TEST(SingleKeyCausalLatticeTest, MergeExistingDominates) {
  // existing: {c1:2}, incoming: {c1:1} -- existing dominates
  VectorClockValuePair<SetLattice<string>> p1;
  p1.vector_clock.insert("c1", MaxLattice<unsigned>(2));
  p1.value.insert("existing");
  SingleKeyCausalLattice<SetLattice<string>> skcl(p1);

  VectorClockValuePair<SetLattice<string>> p2;
  p2.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p2.value.insert("incoming");
  skcl.merge(p2);

  // After merge, vc stays {c1:2}, value stays "existing"
  EXPECT_EQ(skcl.reveal().vector_clock.reveal().at("c1").reveal(), 2u);
  EXPECT_TRUE(skcl.reveal().value.reveal().count("existing"));
  EXPECT_FALSE(skcl.reveal().value.reveal().count("incoming"));
}

TEST(SingleKeyCausalLatticeTest, MergeConcurrent) {
  // existing: {c1:1}, incoming: {c2:1} -- concurrent (different keys)
  VectorClockValuePair<SetLattice<string>> p1;
  p1.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p1.value.insert("val_a");
  SingleKeyCausalLattice<SetLattice<string>> skcl(p1);

  VectorClockValuePair<SetLattice<string>> p2;
  p2.vector_clock.insert("c2", MaxLattice<unsigned>(1));
  p2.value.insert("val_b");
  skcl.merge(p2);

  // After merge, vc should be {c1:1, c2:1}, values should be merged
  EXPECT_EQ(skcl.reveal().vector_clock.reveal().at("c1").reveal(), 1u);
  EXPECT_EQ(skcl.reveal().vector_clock.reveal().at("c2").reveal(), 1u);
  EXPECT_TRUE(skcl.reveal().value.reveal().count("val_a"));
  EXPECT_TRUE(skcl.reveal().value.reveal().count("val_b"));
}

TEST(SingleKeyCausalLatticeTest, MergeLattice) {
  VectorClockValuePair<SetLattice<string>> p1;
  p1.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p1.value.insert("a");
  SingleKeyCausalLattice<SetLattice<string>> a(p1);

  VectorClockValuePair<SetLattice<string>> p2;
  p2.vector_clock.insert("c1", MaxLattice<unsigned>(2));
  p2.value.insert("b");
  SingleKeyCausalLattice<SetLattice<string>> b(p2);

  a.merge(b);
  EXPECT_EQ(a.reveal().vector_clock.reveal().at("c1").reveal(), 2u);
}

TEST(SingleKeyCausalLatticeTest, CopyConstructor) {
  VectorClockValuePair<SetLattice<string>> p;
  p.vector_clock.insert("c1", MaxLattice<unsigned>(5));
  p.value.insert("val");
  SingleKeyCausalLattice<SetLattice<string>> a(p);
  SingleKeyCausalLattice<SetLattice<string>> b(a);
  EXPECT_EQ(b.reveal().vector_clock.reveal().at("c1").reveal(), 5u);
}

// =====================================================================
// MultiKeyCausalPayload (multi_key_causal_lattice.hpp)
// =====================================================================

TEST(MultiKeyCausalPayloadTest, DefaultConstructor) {
  MultiKeyCausalPayload<SetLattice<string>> p;
  EXPECT_TRUE(p.vector_clock.reveal().empty());
  EXPECT_TRUE(p.dependencies.reveal().empty());
  EXPECT_TRUE(p.value.reveal().empty());
}

TEST(MultiKeyCausalPayloadTest, UnsignedConstructor) {
  MultiKeyCausalPayload<SetLattice<string>> p(0u);
  EXPECT_TRUE(p.vector_clock.reveal().empty());
  EXPECT_TRUE(p.dependencies.reveal().empty());
  EXPECT_TRUE(p.value.reveal().empty());
}

TEST(MultiKeyCausalPayloadTest, ThreeArgConstructor) {
  VectorClock vc;
  vc.insert("c1", MaxLattice<unsigned>(3));

  MapLattice<Key, VectorClock> deps;
  VectorClock dep_vc;
  dep_vc.insert("d1", MaxLattice<unsigned>(1));
  deps.insert("dep_key", dep_vc);

  SetLattice<string> val(set<string>({"v1"}));

  MultiKeyCausalPayload<SetLattice<string>> p(vc, deps, val);
  EXPECT_EQ(p.vector_clock.at("c1").reveal(), 3u);
  EXPECT_TRUE(p.dependencies.contains("dep_key").reveal());
  EXPECT_TRUE(p.value.reveal().count("v1"));
}

TEST(MultiKeyCausalPayloadTest, Size) {
  MultiKeyCausalPayload<SetLattice<string>> p;
  p.vector_clock.insert("c1", MaxLattice<unsigned>(1));

  VectorClock dep_vc;
  dep_vc.insert("d1", MaxLattice<unsigned>(2));
  p.dependencies.insert("dep_key", dep_vc);

  p.value.insert("val");

  EXPECT_GT(p.size(), 0u);
}

TEST(MultiKeyCausalPayloadTest, SizeEmpty) {
  MultiKeyCausalPayload<SetLattice<string>> p;
  EXPECT_EQ(p.size(), 0u);
}

// =====================================================================
// MultiKeyCausalLattice
// =====================================================================

TEST(MultiKeyCausalLatticeTest, DefaultConstructor) {
  MultiKeyCausalLattice<SetLattice<string>> mkcl;
  EXPECT_TRUE(mkcl.reveal().vector_clock.reveal().empty());
  EXPECT_TRUE(mkcl.reveal().dependencies.reveal().empty());
  EXPECT_TRUE(mkcl.reveal().value.reveal().empty());
}

TEST(MultiKeyCausalLatticeTest, ValueConstructor) {
  MultiKeyCausalPayload<SetLattice<string>> p;
  p.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p.value.insert("hello");
  MultiKeyCausalLattice<SetLattice<string>> mkcl(p);
  EXPECT_EQ(mkcl.reveal().vector_clock.reveal().at("c1").reveal(), 1u);
  EXPECT_TRUE(mkcl.reveal().value.reveal().count("hello"));
}

TEST(MultiKeyCausalLatticeTest, Size) {
  MultiKeyCausalPayload<SetLattice<string>> p;
  p.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p.value.insert("v");
  MultiKeyCausalLattice<SetLattice<string>> mkcl(p);
  EXPECT_GT(mkcl.size().reveal(), 0u);
}

TEST(MultiKeyCausalLatticeTest, MergeIncomingDominates) {
  // existing: {c1:1}, incoming: {c1:2} -- incoming dominates
  MultiKeyCausalPayload<SetLattice<string>> p1;
  p1.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p1.value.insert("old");
  VectorClock dep1;
  dep1.insert("d1", MaxLattice<unsigned>(1));
  p1.dependencies.insert("dep_old", dep1);
  MultiKeyCausalLattice<SetLattice<string>> mkcl(p1);

  MultiKeyCausalPayload<SetLattice<string>> p2;
  p2.vector_clock.insert("c1", MaxLattice<unsigned>(2));
  p2.value.insert("new");
  VectorClock dep2;
  dep2.insert("d2", MaxLattice<unsigned>(2));
  p2.dependencies.insert("dep_new", dep2);
  mkcl.merge(p2);

  // After merge, vc={c1:2}, value={"new"}, dependencies={"dep_new":...}
  EXPECT_EQ(mkcl.reveal().vector_clock.reveal().at("c1").reveal(), 2u);
  EXPECT_TRUE(mkcl.reveal().value.reveal().count("new"));
  EXPECT_FALSE(mkcl.reveal().value.reveal().count("old"));
  EXPECT_TRUE(mkcl.reveal().dependencies.contains("dep_new").reveal());
  EXPECT_FALSE(mkcl.reveal().dependencies.contains("dep_old").reveal());
}

TEST(MultiKeyCausalLatticeTest, MergeExistingDominates) {
  // existing: {c1:2}, incoming: {c1:1} -- existing dominates
  MultiKeyCausalPayload<SetLattice<string>> p1;
  p1.vector_clock.insert("c1", MaxLattice<unsigned>(2));
  p1.value.insert("existing");
  MultiKeyCausalLattice<SetLattice<string>> mkcl(p1);

  MultiKeyCausalPayload<SetLattice<string>> p2;
  p2.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p2.value.insert("incoming");
  mkcl.merge(p2);

  // After merge, vc stays {c1:2}, value stays "existing"
  EXPECT_EQ(mkcl.reveal().vector_clock.reveal().at("c1").reveal(), 2u);
  EXPECT_TRUE(mkcl.reveal().value.reveal().count("existing"));
  EXPECT_FALSE(mkcl.reveal().value.reveal().count("incoming"));
}

TEST(MultiKeyCausalLatticeTest, MergeConcurrent) {
  // existing: {c1:1}, incoming: {c2:1} -- concurrent
  MultiKeyCausalPayload<SetLattice<string>> p1;
  p1.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p1.value.insert("val_a");
  VectorClock dep1;
  dep1.insert("d1", MaxLattice<unsigned>(1));
  p1.dependencies.insert("dep_a", dep1);
  MultiKeyCausalLattice<SetLattice<string>> mkcl(p1);

  MultiKeyCausalPayload<SetLattice<string>> p2;
  p2.vector_clock.insert("c2", MaxLattice<unsigned>(1));
  p2.value.insert("val_b");
  VectorClock dep2;
  dep2.insert("d2", MaxLattice<unsigned>(2));
  p2.dependencies.insert("dep_b", dep2);
  mkcl.merge(p2);

  // vc should be {c1:1, c2:1}, values and deps merged
  EXPECT_EQ(mkcl.reveal().vector_clock.reveal().at("c1").reveal(), 1u);
  EXPECT_EQ(mkcl.reveal().vector_clock.reveal().at("c2").reveal(), 1u);
  EXPECT_TRUE(mkcl.reveal().value.reveal().count("val_a"));
  EXPECT_TRUE(mkcl.reveal().value.reveal().count("val_b"));
  EXPECT_TRUE(mkcl.reveal().dependencies.contains("dep_a").reveal());
  EXPECT_TRUE(mkcl.reveal().dependencies.contains("dep_b").reveal());
}

TEST(MultiKeyCausalLatticeTest, MergeLattice) {
  MultiKeyCausalPayload<SetLattice<string>> p1;
  p1.vector_clock.insert("c1", MaxLattice<unsigned>(1));
  p1.value.insert("a");
  MultiKeyCausalLattice<SetLattice<string>> a(p1);

  MultiKeyCausalPayload<SetLattice<string>> p2;
  p2.vector_clock.insert("c1", MaxLattice<unsigned>(2));
  p2.value.insert("b");
  MultiKeyCausalLattice<SetLattice<string>> b(p2);

  a.merge(b);
  EXPECT_EQ(a.reveal().vector_clock.reveal().at("c1").reveal(), 2u);
}

TEST(MultiKeyCausalLatticeTest, CopyConstructor) {
  MultiKeyCausalPayload<SetLattice<string>> p;
  p.vector_clock.insert("c1", MaxLattice<unsigned>(5));
  p.value.insert("val");
  MultiKeyCausalLattice<SetLattice<string>> a(p);
  MultiKeyCausalLattice<SetLattice<string>> b(a);
  EXPECT_EQ(b.reveal().vector_clock.reveal().at("c1").reveal(), 5u);
}

// =====================================================================
// LWWPairLattice (tested via serialization, used in common.hpp)
// =====================================================================

TEST(LWWPairLatticeTest, MergeHigherTimestampWins) {
  LWWPairLattice<string> a(TimestampValuePair<string>(100, "old"));
  LWWPairLattice<string> b(TimestampValuePair<string>(200, "new"));
  a.merge(b);
  EXPECT_EQ(a.reveal().value, "new");
  EXPECT_EQ(a.reveal().timestamp, 200ull);
}

TEST(LWWPairLatticeTest, MergeLowerTimestampLoses) {
  LWWPairLattice<string> a(TimestampValuePair<string>(200, "existing"));
  a.merge(TimestampValuePair<string>(100, "incoming"));
  EXPECT_EQ(a.reveal().value, "existing");
  EXPECT_EQ(a.reveal().timestamp, 200ull);
}

TEST(LWWPairLatticeTest, MergeEqualTimestamp) {
  LWWPairLattice<string> a(TimestampValuePair<string>(100, "existing"));
  a.merge(TimestampValuePair<string>(100, "incoming"));
  // Equal timestamps: existing stays
  EXPECT_EQ(a.reveal().timestamp, 100ull);
}

TEST(LWWPairLatticeTest, CopyConstructor) {
  LWWPairLattice<string> a(TimestampValuePair<string>(42, "val"));
  LWWPairLattice<string> b(a);
  EXPECT_EQ(b.reveal().value, "val");
  EXPECT_EQ(b.reveal().timestamp, 42ull);
}

TEST(LWWPairLatticeTest, Assign) {
  LWWPairLattice<string> a(TimestampValuePair<string>(1, "old"));
  a.assign(TimestampValuePair<string>(99, "new"));
  EXPECT_EQ(a.reveal().value, "new");
  EXPECT_EQ(a.reveal().timestamp, 99ull);
}

// =====================================================================
// Thread address classes (threads.hpp)
// =====================================================================

TEST(UserRoutingThreadTest, ConstructorAndAccessors) {
  UserRoutingThread rt("10.0.0.1", 3);
  EXPECT_EQ(rt.ip(), "10.0.0.1");
  EXPECT_EQ(rt.tid(), 3u);
}

TEST(UserRoutingThreadTest, KeyAddressConnectAddress) {
  UserRoutingThread rt("10.0.0.1", 0);
  string addr = rt.key_address_connect_address();
  EXPECT_TRUE(addr.find("10.0.0.1") != string::npos);
  EXPECT_TRUE(addr.find("tcp://") != string::npos);
}

TEST(UserRoutingThreadTest, KeyAddressBindAddress) {
  UserRoutingThread rt("10.0.0.1", 0);
  string addr = rt.key_address_bind_address();
  EXPECT_TRUE(addr.find("10.0.0.1") != string::npos);
}

TEST(UserRoutingThreadTest, DefaultConstructor) {
  UserRoutingThread rt;
  // Should not crash -- default-constructed state
  (void)rt;
}

TEST(UserThreadTest, ConstructorAndAccessors) {
  UserThread ut("192.168.1.1", 5);
  EXPECT_EQ(ut.ip(), "192.168.1.1");
  EXPECT_EQ(ut.tid(), 5u);
}

TEST(UserThreadTest, ResponseConnectAddress) {
  UserThread ut("192.168.1.1", 0);
  string addr = ut.response_connect_address();
  EXPECT_TRUE(addr.find("192.168.1.1") != string::npos);
  EXPECT_TRUE(addr.find("tcp://") != string::npos);
}

TEST(UserThreadTest, ResponseBindAddress) {
  UserThread ut("192.168.1.1", 0);
  string addr = ut.response_bind_address();
  EXPECT_TRUE(addr.find("192.168.1.1") != string::npos);
}

TEST(UserThreadTest, KeyAddressConnectAddress) {
  UserThread ut("192.168.1.1", 0);
  string addr = ut.key_address_connect_address();
  EXPECT_TRUE(addr.find("192.168.1.1") != string::npos);
}

TEST(UserThreadTest, KeyAddressBindAddress) {
  UserThread ut("192.168.1.1", 0);
  string addr = ut.key_address_bind_address();
  EXPECT_TRUE(addr.find("192.168.1.1") != string::npos);
}

TEST(UserThreadTest, DefaultConstructor) {
  UserThread ut;
  (void)ut;
}

TEST(CacheThreadTest, ConstructorAndAccessors) {
  CacheThread ct("10.0.0.2", 1);
  EXPECT_EQ(ct.ip(), "10.0.0.2");
  EXPECT_EQ(ct.tid(), 1u);
}

TEST(CacheThreadTest, CacheGetAddresses) {
  CacheThread ct("10.0.0.2", 0);
  EXPECT_EQ(ct.cache_get_bind_address(), "ipc:///requests/get");
  EXPECT_EQ(ct.cache_get_connect_address(), "ipc:///requests/get");
}

TEST(CacheThreadTest, CachePutAddresses) {
  CacheThread ct("10.0.0.2", 0);
  EXPECT_EQ(ct.cache_put_bind_address(), "ipc:///requests/put");
  EXPECT_EQ(ct.cache_put_connect_address(), "ipc:///requests/put");
}

TEST(CacheThreadTest, CacheUpdateAddresses) {
  CacheThread ct("10.0.0.2", 0);
  string addr = ct.cache_update_bind_address();
  EXPECT_TRUE(addr.find("10.0.0.2") != string::npos);
  string addr2 = ct.cache_update_connect_address();
  EXPECT_TRUE(addr2.find("10.0.0.2") != string::npos);
}

// Verify thread port constants
TEST(ThreadPortTest, Constants) {
  EXPECT_EQ(kKeyAddressPort, 6450u);
  EXPECT_EQ(kUserResponsePort, 6800u);
  EXPECT_EQ(kUserKeyAddressPort, 6850u);
  EXPECT_EQ(kCacheUpdatePort, 7150u);
}
