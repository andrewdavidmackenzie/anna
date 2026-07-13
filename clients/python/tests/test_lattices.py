from anna.lattices import (
    LWWPairLattice, SetLattice, ListBasedOrderedSet, OrderedSetLattice,
    MaxIntLattice, MapLattice, VectorClock, SingleKeyCausalLattice,
    MultiKeyCausalLattice, PriorityLattice, Lattice
)


class TestLWWPairLattice:
    def test_constructor(self):
        l = LWWPairLattice(1, b"hello")
        assert l.ts == 1
        assert l.val == b"hello"

    def test_constructor_invalid_type(self):
        try:
            LWWPairLattice("not_int", b"val")
            assert False
        except ValueError:
            pass

    def test_reveal(self):
        l = LWWPairLattice(1, b"hello")
        assert l.reveal() == b"hello"

    def test_assign_tuple(self):
        l = LWWPairLattice(1, b"hello")
        l.assign((2, b"world"))
        assert l.ts == 2
        assert l.val == b"world"

    def test_assign_str_value(self):
        l = LWWPairLattice(1, b"hello")
        l.assign((3, b"test"))
        assert l.ts == 3
        assert l.val == b"test"

    def test_assign_invalid_type(self):
        l = LWWPairLattice(1, b"hello")
        try:
            l.assign("invalid")
            assert False
        except ValueError:
            pass

    def test_merge_newer_wins(self):
        older = LWWPairLattice(1, b"old")
        newer = LWWPairLattice(2, b"new")
        result = older.merge(newer)
        assert result.reveal() == b"new"

    def test_merge_older_keeps(self):
        newer = LWWPairLattice(2, b"new")
        older = LWWPairLattice(1, b"old")
        result = newer.merge(older)
        assert result.reveal() == b"new"

    def test_serialize(self):
        l = LWWPairLattice(5, b"data")
        pb, typ = l.serialize()
        assert pb.timestamp == 5
        assert pb.value == b"data"

    def test_str(self):
        l = LWWPairLattice(1, b"hello")
        assert str(l) == str(b"hello")


class TestSetLattice:
    def test_constructor_empty(self):
        s = SetLattice()
        assert s.reveal() == set()

    def test_constructor_with_values(self):
        s = SetLattice({b"a", b"b"})
        assert s.reveal() == {b"a", b"b"}

    def test_constructor_invalid_type(self):
        try:
            SetLattice("not a set")
            assert False
        except ValueError:
            pass

    def test_assign(self):
        s = SetLattice({b"a"})
        s.assign({b"b", b"c"})
        assert s.reveal() == {b"b", b"c"}

    def test_assign_invalid_type(self):
        s = SetLattice()
        try:
            s.assign("invalid")
            assert False
        except ValueError:
            pass

    def test_merge_same_type(self):
        s1 = SetLattice({b"a", b"b"})
        s2 = SetLattice({b"b", b"c"})
        merged = s1.merge(s2)
        assert merged.reveal() == {b"a", b"b", b"c"}

    def test_merge_invalid_type(self):
        s = SetLattice()
        try:
            s.merge("invalid")
            assert False
        except ValueError:
            pass

    def test_serialize(self):
        s = SetLattice({b"hello"})
        pb, typ = s.serialize()
        assert b"hello" in pb.values

    def test_serialize_invalid_type(self):
        s = SetLattice({"string_not_bytes"})
        try:
            s.serialize()
            assert False
        except ValueError:
            pass

    def test_eq(self):
        s1 = SetLattice({b"a", b"b"})
        s2 = SetLattice({b"b", b"a"})
        assert s1 == s2

    def test_eq_none(self):
        s = SetLattice()
        assert (s == None) == False

    def test_eq_different_type(self):
        s = SetLattice()
        assert (s == "other") == False


class TestListBasedOrderedSet:
    def test_constructor_empty(self):
        s = ListBasedOrderedSet()
        assert s.lst == []

    def test_constructor_with_values(self):
        s = ListBasedOrderedSet([b"a", b"b", b"c"])
        assert s.lst == [b"a", b"b", b"c"]

    def test_constructor_unsorted(self):
        s = ListBasedOrderedSet([b"c", b"a", b"b"])
        assert s.lst == [b"a", b"b", b"c"]

    def test_constructor_with_duplicates(self):
        s = ListBasedOrderedSet([b"a", b"b", b"a"])
        assert s.lst == [b"a", b"b"]

    def test_insert_empty(self):
        s = ListBasedOrderedSet()
        s.insert(b"a")
        assert s.lst == [b"a"]

    def test_insert_end(self):
        s = ListBasedOrderedSet([b"a"])
        s.insert(b"b")
        assert s.lst == [b"a", b"b"]

    def test_insert_beginning(self):
        s = ListBasedOrderedSet([b"b", b"c"])
        s.insert(b"a")
        assert s.lst == [b"a", b"b", b"c"]

    def test_insert_middle(self):
        s = ListBasedOrderedSet([b"a", b"c"])
        s.insert(b"b")
        assert s.lst == [b"a", b"b", b"c"]

    def test_insert_duplicate(self):
        s = ListBasedOrderedSet([b"a", b"c"])
        s.insert(b"a")
        assert s.lst == [b"a", b"c"]

    def test_index_of_found(self):
        s = ListBasedOrderedSet([b"a", b"b", b"c"])
        idx, found = s._index_of(b"b")
        assert idx == 1
        assert found == True

    def test_index_of_not_found(self):
        s = ListBasedOrderedSet([b"a", b"c"])
        idx, found = s._index_of(b"b")
        assert idx == 1
        assert found == False

    def test_index_of_empty(self):
        s = ListBasedOrderedSet()
        idx, found = s._index_of(b"a")
        assert idx == 0
        assert found == False


class TestOrderedSetLattice:
    def test_constructor_empty(self):
        o = OrderedSetLattice()
        assert o.reveal() == []

    def test_constructor_with_values(self):
        o = OrderedSetLattice(ListBasedOrderedSet([b"a", b"b"]))
        assert o.reveal() == [b"a", b"b"]

    def test_constructor_invalid_type(self):
        try:
            OrderedSetLattice("invalid")
            assert False
        except ValueError:
            pass

    def test_reveal(self):
        o = OrderedSetLattice(ListBasedOrderedSet([b"a"]))
        assert o.reveal() == [b"a"]

    def test_assign(self):
        o = OrderedSetLattice()
        o.assign(ListBasedOrderedSet([b"x"]))
        assert o.reveal() == [b"x"]

    def test_merge_disjoint(self):
        o1 = OrderedSetLattice(ListBasedOrderedSet([b"a", b"c"]))
        o2 = OrderedSetLattice(ListBasedOrderedSet([b"b", b"d"]))
        merged = o1.merge(o2)
        assert merged.reveal() == [b"a", b"b", b"c", b"d"]

    def test_merge_overlapping(self):
        o1 = OrderedSetLattice(ListBasedOrderedSet([b"a", b"b", b"d"]))
        o2 = OrderedSetLattice(ListBasedOrderedSet([b"b", b"c", b"e"]))
        merged = o1.merge(o2)
        assert merged.reveal() == [b"a", b"b", b"c", b"d", b"e"]

    def test_merge_empty(self):
        o1 = OrderedSetLattice(ListBasedOrderedSet([b"a"]))
        o2 = OrderedSetLattice()
        merged = o1.merge(o2)
        assert merged.reveal() == [b"a"]

    def test_merge_subset(self):
        o1 = OrderedSetLattice(ListBasedOrderedSet([b"a", b"b", b"c"]))
        o2 = OrderedSetLattice(ListBasedOrderedSet([b"b"]))
        merged = o1.merge(o2)
        assert merged.reveal() == [b"a", b"b", b"c"]

    def test_merge_invalid_type(self):
        o = OrderedSetLattice()
        try:
            o.merge("invalid")
            assert False
        except ValueError:
            pass

    def test_serialize(self):
        o = OrderedSetLattice(ListBasedOrderedSet([b"a"]))
        pb, typ = o.serialize()
        assert b"a" in pb.values

    def test_str(self):
        o = OrderedSetLattice(ListBasedOrderedSet([b"a"]))
        assert str(o) == "[b'a']"


class TestMaxIntLattice:
    def test_constructor(self):
        m = MaxIntLattice(5)
        assert m.value == 5

    def test_constructor_invalid_type(self):
        try:
            MaxIntLattice("not_int")
            assert False
        except ValueError:
            pass

    def test_reveal(self):
        m = MaxIntLattice(42)
        assert m.reveal() == 42

    def test_assign(self):
        m = MaxIntLattice(1)
        m.assign(5)
        assert m.value == 5

    def test_assign_invalid_type(self):
        m = MaxIntLattice(1)
        try:
            m.assign("invalid")
            assert False
        except ValueError:
            pass

    def test_merge_higher_wins(self):
        m = MaxIntLattice(1)
        m.merge(MaxIntLattice(5))
        assert m.value == 5

    def test_merge_lower_no_change(self):
        m = MaxIntLattice(5)
        m.merge(MaxIntLattice(1))
        assert m.value == 5

    def test_merge_equal(self):
        m = MaxIntLattice(3)
        m.merge(MaxIntLattice(3))
        assert m.value == 3

    def test_merge_invalid_type(self):
        m = MaxIntLattice(1)
        try:
            m.merge("invalid")
            assert False
        except ValueError:
            pass


class TestMapLattice:
    def test_constructor(self):
        m = MapLattice({})
        assert m.reveal() == {}

    def test_constructor_invalid_type(self):
        try:
            MapLattice("not dict")
            assert False
        except ValueError:
            pass

    def test_reveal(self):
        m = MapLattice({"a": MaxIntLattice(1)})
        assert m.reveal() == {"a": MaxIntLattice(1)}

    def test_assign(self):
        m = MapLattice({})
        m.assign({"x": MaxIntLattice(2)})
        assert m.reveal() == {"x": MaxIntLattice(2)}

    def test_assign_invalid_type(self):
        m = MapLattice({})
        try:
            m.assign("invalid")
            assert False
        except ValueError:
            pass

    def test_merge_new_key(self):
        m1 = MapLattice({"a": MaxIntLattice(1)})
        m2 = MapLattice({"b": MaxIntLattice(2)})
        m1.merge(m2)
        assert m1.reveal()["b"].reveal() == 2

    def test_merge_existing_key(self):
        m1 = MapLattice({"a": MaxIntLattice(1)})
        m2 = MapLattice({"a": MaxIntLattice(5)})
        m1.merge(m2)
        assert m1.reveal()["a"].reveal() == 5

    def test_copy(self):
        m = MapLattice({"a": MaxIntLattice(1)})
        c = m.copy()
        assert c.reveal() == {"a": MaxIntLattice(1)}

    def test_eq(self):
        m1 = MapLattice({"a": MaxIntLattice(1)})
        m2 = MapLattice({"a": MaxIntLattice(1)})
        assert m1 == m2


class TestVectorClock:
    def test_constructor(self):
        vc = VectorClock({"a": MaxIntLattice(1)})
        assert vc.reveal()["a"].reveal() == 1

    def test_constructor_invalid_type(self):
        try:
            VectorClock("not dict")
            assert False
        except ValueError:
            pass

    def test_deserialize(self):
        mp = {"a": 1, "b": 2}
        vc = VectorClock(mp, deserialize=True)
        assert vc.reveal()["a"].reveal() == 1
        assert vc.reveal()["b"].reveal() == 2

    def test_deserialize_non_int(self):
        try:
            VectorClock({"a": "not_int"}, deserialize=True)
            assert False
        except ValueError:
            pass

    def test_update(self):
        vc = VectorClock({"a": MaxIntLattice(1)})
        vc.update("a", 5)
        assert vc.reveal()["a"].reveal() == 5

    def test_update_new_key_does_nothing(self):
        vc = VectorClock({"a": MaxIntLattice(1)})
        vc.update("b", 5)
        assert "b" not in vc.reveal()

    def test_serialize(self):
        vc = VectorClock({"a": MaxIntLattice(3)})
        pobj = {}
        vc.serialize(pobj)
        assert pobj["a"] == 3


class TestSingleKeyCausalLattice:
    def test_constructor(self):
        vc = VectorClock({"a": MaxIntLattice(1)})
        val = SetLattice({b"data"})
        l = SingleKeyCausalLattice(vc, val)
        assert l.vector_clock == vc
        assert l.value == val

    def test_constructor_invalid_vc(self):
        try:
            SingleKeyCausalLattice("not_vc", SetLattice())
            assert False
        except ValueError:
            pass

    def test_constructor_invalid_value(self):
        try:
            SingleKeyCausalLattice(VectorClock({}), "not_set")
            assert False
        except ValueError:
            pass

    def test_reveal(self):
        vc = VectorClock({"a": MaxIntLattice(1)})
        l = SingleKeyCausalLattice(vc, SetLattice({b"data"}))
        assert l.reveal() == [b"data"]

    def test_merge_dominated(self):
        vc1 = VectorClock({"a": MaxIntLattice(2)})
        vc2 = VectorClock({"a": MaxIntLattice(1)})
        l1 = SingleKeyCausalLattice(vc1, SetLattice({b"new"}))
        l2 = SingleKeyCausalLattice(vc2, SetLattice({b"old"}))
        l1.merge(l2)
        assert l1.value.reveal() == {b"new"}

    def test_serialize(self):
        vc = VectorClock({"a": MaxIntLattice(1)})
        l = SingleKeyCausalLattice(vc, SetLattice({b"data"}))
        pb, typ = l.serialize()
        assert pb.vector_clock["a"] == 1
        assert list(pb.values) == [b"data"]


class TestMultiKeyCausalLattice:
    def test_constructor(self):
        vc = VectorClock({"a": MaxIntLattice(1)})
        deps = MapLattice({"other": VectorClock({"b": MaxIntLattice(0)})})
        val = SetLattice({b"data"})
        l = MultiKeyCausalLattice(vc, deps, val)
        assert l.vector_clock == vc
        assert l.dependencies == deps
        assert l.value == val

    def test_constructor_invalid_vc(self):
        try:
            MultiKeyCausalLattice("bad", MapLattice({}), SetLattice())
            assert False
        except ValueError:
            pass

    def test_constructor_invalid_deps(self):
        try:
            MultiKeyCausalLattice(VectorClock({}), "bad", SetLattice())
            assert False
        except ValueError:
            pass

    def test_constructor_invalid_value(self):
        try:
            MultiKeyCausalLattice(VectorClock({}), MapLattice({}), "bad")
            assert False
        except ValueError:
            pass

    def test_reveal(self):
        vc = VectorClock({"a": MaxIntLattice(1)})
        l = MultiKeyCausalLattice(vc, MapLattice({}), SetLattice({b"data"}))
        assert l.reveal() == [b"data"]


class TestPriorityLattice:
    def test_constructor(self):
        p = PriorityLattice(1.0, b"val")
        assert p.priority == 1.0
        assert p.value == b"val"

    def test_constructor_invalid_type(self):
        try:
            PriorityLattice("not_float", b"val")
            assert False
        except ValueError:
            pass

    def test_reveal(self):
        p = PriorityLattice(2.5, b"data")
        assert p.reveal() == b"data"

    def test_assign(self):
        p = PriorityLattice(1.0, b"old")
        p.assign((3.0, b"new"))
        assert p.priority == 3.0
        assert p.value == b"new"

    def test_assign_invalid_type(self):
        p = PriorityLattice(1.0, b"val")
        try:
            p.assign("invalid")
            assert False
        except ValueError:
            pass

    def test_merge_lower_priority_wins(self):
        high = PriorityLattice(5.0, b"high")
        low = PriorityLattice(1.0, b"low")
        result = high.merge(low)
        assert result.reveal() == b"low"

    def test_merge_higher_priority_keeps(self):
        low = PriorityLattice(1.0, b"low")
        high = PriorityLattice(5.0, b"high")
        result = low.merge(high)
        assert result.reveal() == b"low"

    def test_serialize(self):
        p = PriorityLattice(2.0, b"val")
        pb, typ = p.serialize()
        assert pb.priority == 2.0
        assert pb.value == b"val"

    def test_str(self):
        p = PriorityLattice(1.5, b"val")
        assert str(p) == str(b"val")
