import pytest

from anna.base_client import BaseAnnaClient
from anna.kvs_pb2 import LWW, ORDERED_SET, PRIORITY, SET
from anna.kvs_pb2 import KeyTuple, LWWValue, PriorityValue, SetValue
from anna.lattices import LWWPairLattice, OrderedSetLattice, PriorityLattice, SetLattice


class TestSerialize:
    def test_serialize_lww(self):
        lattice = LWWPairLattice(1, b"hello")
        data, typ = BaseAnnaClient._serialize(lattice)
        assert typ == LWW
        assert data is not None

    def test_serialize_set(self):
        lattice = SetLattice({b"a", b"b"})
        data, typ = BaseAnnaClient._serialize(lattice)
        assert typ == SET
        assert data is not None

    def test_serialize_priority(self):
        lattice = PriorityLattice(1.0, b"val")
        data, typ = BaseAnnaClient._serialize(lattice)
        assert typ == PRIORITY
        assert data is not None

    def test_serialize_non_lattice(self):
        with pytest.raises(ValueError):
            BaseAnnaClient._serialize("not a lattice")


class TestDeserialize:
    def test_deserialize_lww(self):
        tup = KeyTuple()
        tup.lattice_type = LWW
        val = LWWValue()
        val.timestamp = 1
        val.value = b"hello"
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, LWWPairLattice)
        assert result.reveal() == b"hello"

    def test_deserialize_set(self):
        tup = KeyTuple()
        tup.lattice_type = SET
        val = SetValue()
        val.values.append(b"a")
        val.values.append(b"b")
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, SetLattice)
        assert result.reveal() == {b"a", b"b"}

    def test_deserialize_priority(self):
        tup = KeyTuple()
        tup.lattice_type = PRIORITY
        val = PriorityValue()
        val.priority = 2.0
        val.value = b"data"
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, PriorityLattice)
        assert result.reveal() == b"data"

    def test_deserialize_invalid_type(self):
        tup = KeyTuple()
        tup.lattice_type = 999

        with pytest.raises(ValueError):
            BaseAnnaClient._deserialize(tup)

    def test_deserialize_ordered_set(self):
        tup = KeyTuple()
        tup.lattice_type = ORDERED_SET
        val = SetValue()
        val.values.append(b"a")
        val.values.append(b"b")
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, OrderedSetLattice)
        assert result.reveal() == [b"a", b"b"]


class TestCausalDeserialization:
    def test_multi_causal_roundtrip(self):
        from anna.lattices import MultiKeyCausalLattice, SetLattice, MapLattice, VectorClock
        from anna.base_client import BaseAnnaClient
        from anna.kvs_pb2 import KeyTuple, MULTI_CAUSAL

        vc = VectorClock({"test": 1}, True)
        dep_vc = VectorClock({"test1": 1}, True)
        deps = MapLattice({"dep1": dep_vc})
        val = SetLattice({b"hello"})
        lattice = MultiKeyCausalLattice(vc, deps, val)

        pb, typ = lattice.serialize()
        assert typ == MULTI_CAUSAL

        tup = KeyTuple()
        tup.lattice_type = MULTI_CAUSAL
        tup.payload = pb.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, MultiKeyCausalLattice)
        assert b"hello" in result.value.reveal()
        assert result.vector_clock.reveal()["test"].reveal() == 1
        dep = result.dependencies.reveal()["dep1"]
        assert dep.reveal()["test1"].reveal() == 1
