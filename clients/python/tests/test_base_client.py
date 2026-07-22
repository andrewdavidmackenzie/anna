import pytest

from anna.base_client import BaseAnnaClient
from anna.kvs_pb2 import LWW, ORDERED_SET, PRIORITY, SET, SINGLE_CAUSAL, MULTI_CAUSAL
from anna.kvs_pb2 import KeyTuple, LWWValue, PriorityValue, SetValue
from anna.lattices import LWWPairLattice, OrderedSetLattice, PriorityLattice, SetLattice


class TestBaseClientNotImplemented:
    def test_get_raises(self):
        client = BaseAnnaClient()
        with pytest.raises(NotImplementedError):
            client.get("key")

    def test_get_all_raises(self):
        client = BaseAnnaClient()
        with pytest.raises(NotImplementedError):
            client.get_all(["key"])

    def test_put_raises(self):
        client = BaseAnnaClient()
        with pytest.raises(NotImplementedError):
            client.put("key", "value")

    def test_put_all_raises(self):
        client = BaseAnnaClient()
        with pytest.raises(NotImplementedError):
            client.put_all("key", "value")

    def test_response_address_raises(self):
        client = BaseAnnaClient()
        with pytest.raises(NotImplementedError):
            _ = client.response_address


class TestDeserializeCausalTuple:
    def test_deserialize_causal_tuple(self):
        """Test deserialization of CausalTuple (the isinstance(tup, CausalTuple) branch)."""
        from anna.causal_pb2 import CausalTuple
        from anna.kvs_pb2 import MultiKeyCausalValue
        from anna.lattices import MultiKeyCausalLattice

        # Build the inner protobuf
        val = MultiKeyCausalValue()
        val.vector_clock["node1"] = 5

        dep = val.dependencies.add()
        dep.key = "dep_key"
        dep.vector_clock["node2"] = 3

        val.values.append(b"causal_value")

        # Wrap in CausalTuple
        tup = CausalTuple()
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, MultiKeyCausalLattice)
        assert b"causal_value" in result.value.reveal()
        assert result.vector_clock.reveal()["node1"].reveal() == 5
        deps = result.dependencies.reveal()
        assert "dep_key" in deps
        assert deps["dep_key"].reveal()["node2"].reveal() == 3


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


class TestOrderedSetDeserialization:
    def test_ordered_set_roundtrip(self):
        from anna.lattices import OrderedSetLattice, ListBasedOrderedSet
        from anna.base_client import BaseAnnaClient
        from anna.kvs_pb2 import KeyTuple, ORDERED_SET

        oset = ListBasedOrderedSet([b"alpha", b"beta", b"gamma"])
        lattice = OrderedSetLattice(oset)

        pb, typ = lattice.serialize()
        assert typ == ORDERED_SET

        tup = KeyTuple()
        tup.lattice_type = ORDERED_SET
        tup.payload = pb.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, OrderedSetLattice)
        assert result.reveal() == [b"alpha", b"beta", b"gamma"]


class TestSingleCausalDeserialization:
    def test_single_causal_roundtrip(self):
        from anna.lattices import SingleKeyCausalLattice, SetLattice, VectorClock
        from anna.base_client import BaseAnnaClient
        from anna.kvs_pb2 import KeyTuple, SINGLE_CAUSAL

        vc = VectorClock({"node1": 3}, True)
        val = SetLattice({b"value1"})
        lattice = SingleKeyCausalLattice(vc, val)

        pb, typ = lattice.serialize()
        assert typ == SINGLE_CAUSAL

        tup = KeyTuple()
        tup.lattice_type = SINGLE_CAUSAL
        tup.payload = pb.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, SingleKeyCausalLattice)
        assert b"value1" in result.value.reveal()
        assert result.vector_clock.reveal()["node1"].reveal() == 3


class TestPriorityDeserialization:
    def test_priority_roundtrip(self):
        from anna.lattices import PriorityLattice
        from anna.base_client import BaseAnnaClient
        from anna.kvs_pb2 import KeyTuple, PRIORITY

        lattice = PriorityLattice(5.0, b"high-pri")

        pb, typ = lattice.serialize()
        assert typ == PRIORITY

        tup = KeyTuple()
        tup.lattice_type = PRIORITY
        tup.payload = pb.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, PriorityLattice)
        assert result.reveal() == b"high-pri"
        assert result.priority == 5.0


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
