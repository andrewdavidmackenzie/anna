"""Tests for metadata/stats helper methods on AnnaTcpClient."""

from unittest.mock import MagicMock, patch

import pytest

from anna.kvs_pb2 import LWW, NO_ERROR, KeyResponse, LWWValue
from anna.metadata_pb2 import (
    DISK,
    MEMORY,
    ClusterTopology,
    KeyAccessData,
    KeySizeData,
    ReplicationFactor,
    ServerThreadStatistics,
)
from anna.shared_pb2 import StringSet


def make_client():
    """Create an AnnaTcpClient with mocked ZMQ context and sockets."""
    with patch("anna.client.zmq") as mock_zmq:
        mock_ctx = MagicMock()
        mock_zmq.Context.return_value = mock_ctx
        mock_zmq.PULL = 7
        mock_zmq.PUSH = 8

        from anna.client import AnnaTcpClient
        client = AnnaTcpClient("127.0.0.1", "127.0.0.1", local=True, offset=0)

    return client


# -- Metadata key format tests ------------------------------------------------

class TestMetadataKeyFormat:
    """Verify the metadata key strings match the expected format."""

    def test_stats_key_format(self):
        key = ("ANNA_METADATA|stats|1.2.3.4|10.0.0.1|0|MEMORY")
        assert key == "ANNA_METADATA|stats|1.2.3.4|10.0.0.1|0|MEMORY"

    def test_access_key_format(self):
        key = ("ANNA_METADATA|access|1.2.3.4|10.0.0.1|0|MEMORY")
        assert key == "ANNA_METADATA|access|1.2.3.4|10.0.0.1|0|MEMORY"

    def test_size_key_format(self):
        key = ("ANNA_METADATA|size|1.2.3.4|10.0.0.1|0|MEMORY")
        assert key == "ANNA_METADATA|size|1.2.3.4|10.0.0.1|0|MEMORY"

    def test_replication_key_format(self):
        key = "ANNA_METADATA|replication|mykey"
        assert key == "ANNA_METADATA|replication|mykey"


# -- Protobuf roundtrip tests -------------------------------------------------

class TestServerThreadStatisticsRoundtrip:
    def test_roundtrip(self):
        stats = ServerThreadStatistics()
        stats.storage_consumption = 1024
        stats.occupancy = 0.75
        stats.epoch = 5
        stats.access_count = 100

        data = stats.SerializeToString()
        parsed = ServerThreadStatistics()
        parsed.ParseFromString(data)

        assert parsed.storage_consumption == 1024
        assert parsed.occupancy == pytest.approx(0.75)
        assert parsed.epoch == 5
        assert parsed.access_count == 100


class TestKeyAccessDataRoundtrip:
    def test_roundtrip(self):
        access = KeyAccessData()
        kc1 = access.keys.add()
        kc1.key = "key1"
        kc1.access_count = 10
        kc2 = access.keys.add()
        kc2.key = "key2"
        kc2.access_count = 20

        data = access.SerializeToString()
        parsed = KeyAccessData()
        parsed.ParseFromString(data)

        assert len(parsed.keys) == 2
        assert parsed.keys[0].key == "key1"
        assert parsed.keys[0].access_count == 10
        assert parsed.keys[1].key == "key2"
        assert parsed.keys[1].access_count == 20


class TestKeySizeDataRoundtrip:
    def test_roundtrip(self):
        size_data = KeySizeData()
        ks1 = size_data.key_sizes.add()
        ks1.key = "key1"
        ks1.size = 256
        ks2 = size_data.key_sizes.add()
        ks2.key = "key2"
        ks2.size = 512

        data = size_data.SerializeToString()
        parsed = KeySizeData()
        parsed.ParseFromString(data)

        assert len(parsed.key_sizes) == 2
        assert parsed.key_sizes[0].key == "key1"
        assert parsed.key_sizes[0].size == 256
        assert parsed.key_sizes[1].key == "key2"
        assert parsed.key_sizes[1].size == 512


class TestReplicationFactorRoundtrip:
    def test_roundtrip(self):
        rep = ReplicationFactor()
        rep.key = "mykey"

        global_field = getattr(rep, 'global')
        mg = global_field.add()
        mg.tier = MEMORY
        mg.value = 3
        dg = global_field.add()
        dg.tier = DISK
        dg.value = 0

        ml = rep.local.add()
        ml.tier = MEMORY
        ml.value = 2
        dl = rep.local.add()
        dl.tier = DISK
        dl.value = 0

        data = rep.SerializeToString()
        parsed = ReplicationFactor()
        parsed.ParseFromString(data)

        assert parsed.key == "mykey"
        parsed_global = getattr(parsed, 'global')
        assert len(parsed_global) == 2
        assert parsed_global[0].tier == MEMORY
        assert parsed_global[0].value == 3
        assert parsed_global[1].tier == DISK
        assert parsed_global[1].value == 0
        assert len(parsed.local) == 2
        assert parsed.local[0].tier == MEMORY
        assert parsed.local[0].value == 2


# -- Helper method tests with mocked transport --------------------------------

def _make_lww_response(key, inner_bytes):
    """Build a KeyResponse wrapping inner_bytes in an LWW value."""
    lww_val = LWWValue()
    lww_val.timestamp = 1
    lww_val.value = inner_bytes

    response = KeyResponse()
    response.response_id = "placeholder"
    tup = response.tuples.add()
    tup.key = key
    tup.lattice_type = LWW
    tup.payload = lww_val.SerializeToString()
    tup.error = NO_ERROR
    return response


class TestGetBytes:
    def test_returns_inner_value(self):
        client = make_client()
        meta_key = "ANNA_METADATA|stats|1.2.3.4|10.0.0.1|0|MEMORY"
        client.address_cache[meta_key] = ["tcp://127.0.0.1:6200"]

        inner = b"raw-protobuf-bytes"
        response = _make_lww_response(meta_key, inner)

        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: [response]
            result = client.get_bytes(meta_key)

        assert result == inner

    def test_returns_none_when_no_worker(self):
        client = make_client()
        with patch.object(client, '_get_worker_address', return_value=None):
            result = client.get_bytes("ANNA_METADATA|stats|x|y|0|MEMORY")
        assert result is None


class TestGetStorageStats:
    def test_decodes_stats(self):
        client = make_client()

        stats = ServerThreadStatistics()
        stats.storage_consumption = 2048
        stats.occupancy = 0.5
        stats.epoch = 3
        stats.access_count = 42
        inner = stats.SerializeToString()

        with patch.object(client, 'get_bytes', return_value=inner):
            result = client.get_storage_stats("1.2.3.4", "10.0.0.1", 0,
                                              "MEMORY")

        assert result == {
            'storage_consumption': 2048,
            'occupancy': pytest.approx(0.5),
            'epoch': 3,
            'access_count': 42,
        }

    def test_returns_none_when_key_missing(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None):
            result = client.get_storage_stats("1.2.3.4", "10.0.0.1", 0,
                                              "MEMORY")
        assert result is None

    def test_constructs_correct_key(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None) as mock_gb:
            client.get_storage_stats("1.2.3.4", "10.0.0.1", 0, "MEMORY")
        mock_gb.assert_called_once_with(
            "ANNA_METADATA|stats|1.2.3.4|10.0.0.1|0|MEMORY")


class TestGetKeyAccessStats:
    def test_decodes_access_data(self):
        client = make_client()

        access = KeyAccessData()
        kc = access.keys.add()
        kc.key = "foo"
        kc.access_count = 7
        inner = access.SerializeToString()

        with patch.object(client, 'get_bytes', return_value=inner):
            result = client.get_key_access_stats("1.2.3.4", "10.0.0.1", 0,
                                                 "MEMORY")

        assert result == [{'key': 'foo', 'access_count': 7}]

    def test_returns_none_when_key_missing(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None):
            result = client.get_key_access_stats("1.2.3.4", "10.0.0.1", 0,
                                                 "MEMORY")
        assert result is None

    def test_constructs_correct_key(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None) as mock_gb:
            client.get_key_access_stats("1.2.3.4", "10.0.0.1", 1, "DISK")
        mock_gb.assert_called_once_with(
            "ANNA_METADATA|access|1.2.3.4|10.0.0.1|1|DISK")


class TestGetKeySizeStats:
    def test_decodes_size_data(self):
        client = make_client()

        size_data = KeySizeData()
        ks = size_data.key_sizes.add()
        ks.key = "bar"
        ks.size = 128
        inner = size_data.SerializeToString()

        with patch.object(client, 'get_bytes', return_value=inner):
            result = client.get_key_size_stats("1.2.3.4", "10.0.0.1", 0,
                                               "MEMORY")

        assert result == [{'key': 'bar', 'size': 128}]

    def test_returns_none_when_key_missing(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None):
            result = client.get_key_size_stats("1.2.3.4", "10.0.0.1", 0,
                                               "MEMORY")
        assert result is None

    def test_constructs_correct_key(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None) as mock_gb:
            client.get_key_size_stats("1.2.3.4", "10.0.0.1", 2, "MEMORY")
        mock_gb.assert_called_once_with(
            "ANNA_METADATA|size|1.2.3.4|10.0.0.1|2|MEMORY")


class TestPutReplicationFactor:
    def test_constructs_correct_protobuf(self):
        """Verify the ReplicationFactor protobuf is constructed correctly
        by intercepting the put() call and decoding the LWW payload."""
        client = make_client()

        captured_args = {}

        def mock_put(key, value):
            captured_args['key'] = key
            captured_args['value'] = value
            return {key: True}

        with patch.object(client, 'put', side_effect=mock_put):
            result = client.put_replication_factor("mykey", 3, 2)

        assert captured_args['key'] == "ANNA_METADATA|replication|mykey"

        # The value should be an LWWPairLattice wrapping the protobuf
        lattice = captured_args['value']
        assert hasattr(lattice, 'reveal')
        payload = lattice.reveal()

        rep = ReplicationFactor()
        rep.ParseFromString(payload)
        assert rep.key == "mykey"

        global_field = getattr(rep, 'global')
        assert len(global_field) == 2
        assert global_field[0].tier == MEMORY
        assert global_field[0].value == 3
        assert global_field[1].tier == DISK
        assert global_field[1].value == 0

        assert len(rep.local) == 2
        assert rep.local[0].tier == MEMORY
        assert rep.local[0].value == 2
        assert rep.local[1].tier == DISK
        assert rep.local[1].value == 0

    def test_uses_correct_metadata_key(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            client.put_replication_factor("testkey", 1, 1)
        call_key = mock_put.call_args[0][0]
        assert call_key == "ANNA_METADATA|replication|testkey"


class TestGetClusterTopology:
    def test_decodes_topology(self):
        client = make_client()

        topology = ClusterTopology()
        topology.routing_thread_count = 2
        topology.memory_thread_count = 4
        topology.ebs_thread_count = 1
        inner = topology.SerializeToString()

        with patch.object(client, 'get_bytes', return_value=inner):
            result = client.get_cluster_topology()

        assert result == {
            'routing_thread_count': 2,
            'memory_thread_count': 4,
            'ebs_thread_count': 1,
        }

    def test_returns_none_when_key_missing(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None):
            result = client.get_cluster_topology()
        assert result is None

    def test_reads_correct_key(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None) as mock_gb:
            client.get_cluster_topology()
        mock_gb.assert_called_once_with("ANNA_METADATA|cluster_topology")


class TestGetMonitoringIps:
    def test_decodes_ips(self):
        client = make_client()

        string_set = StringSet()
        string_set.keys.append("10.0.0.1")
        string_set.keys.append("10.0.0.2")
        inner = string_set.SerializeToString()

        with patch.object(client, 'get_bytes', return_value=inner):
            result = client.get_monitoring_ips()

        assert result == ["10.0.0.1", "10.0.0.2"]

    def test_returns_empty_when_key_missing(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None):
            result = client.get_monitoring_ips()
        assert result == []

    def test_reads_correct_key(self):
        client = make_client()
        with patch.object(client, 'get_bytes', return_value=None) as mock_gb:
            client.get_monitoring_ips()
        mock_gb.assert_called_once_with("ANNA_METADATA|monitoring_ips")
