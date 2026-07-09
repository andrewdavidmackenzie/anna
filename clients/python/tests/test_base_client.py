import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'anna'))

from base_client import BaseAnnaClient
from lattices import LWWPairLattice, SetLattice, PriorityLattice
from kvs_pb2 import LWW, SET, PRIORITY


class TestSerialize:
    def test_serialize_lww(self):
        l = LWWPairLattice(1, b"hello")
        data, typ = BaseAnnaClient._serialize(l)
        assert typ == LWW
        assert data is not None

    def test_serialize_set(self):
        l = SetLattice({b"a", b"b"})
        data, typ = BaseAnnaClient._serialize(l)
        assert typ == SET
        assert data is not None

    def test_serialize_priority(self):
        l = PriorityLattice(1.0, b"val")
        data, typ = BaseAnnaClient._serialize(l)
        assert typ == PRIORITY
        assert data is not None

    def test_serialize_non_lattice(self):
        try:
            BaseAnnaClient._serialize("not a lattice")
            assert False
        except ValueError:
            pass


class TestDeserialize:
    def test_deserialize_lww(self):
        from kvs_pb2 import KeyTuple, LWW
        tup = KeyTuple()
        tup.lattice_type = LWW
        from kvs_pb2 import LWWValue
        val = LWWValue()
        val.timestamp = 1
        val.value = b"hello"
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, LWWPairLattice)
        assert result.reveal() == b"hello"

    def test_deserialize_set(self):
        from kvs_pb2 import KeyTuple, SET
        tup = KeyTuple()
        tup.lattice_type = SET
        from kvs_pb2 import SetValue
        val = SetValue()
        val.values.append(b"a")
        val.values.append(b"b")
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, SetLattice)
        assert result.reveal() == {b"a", b"b"}

    def test_deserialize_priority(self):
        from kvs_pb2 import KeyTuple, PRIORITY
        tup = KeyTuple()
        tup.lattice_type = PRIORITY
        from kvs_pb2 import PriorityValue
        val = PriorityValue()
        val.priority = 2.0
        val.value = b"data"
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        assert isinstance(result, PriorityLattice)
        assert result.reveal() == b"data"

    def test_deserialize_invalid_type(self):
        from kvs_pb2 import KeyTuple
        tup = KeyTuple()
        tup.lattice_type = 999

        try:
            BaseAnnaClient._deserialize(tup)
            assert False
        except ValueError:
            pass

    def test_deserialize_ordered_set(self):
        from kvs_pb2 import KeyTuple, ORDERED_SET
        tup = KeyTuple()
        tup.lattice_type = ORDERED_SET
        from kvs_pb2 import SetValue
        val = SetValue()
        val.values.append(b"a")
        val.values.append(b"b")
        tup.payload = val.SerializeToString()

        result = BaseAnnaClient._deserialize(tup)
        from lattices import OrderedSetLattice
        assert isinstance(result, OrderedSetLattice)
        assert result.reveal() == [b"a", b"b"]
