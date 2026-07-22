from unittest.mock import MagicMock, patch, PropertyMock
import pytest

from anna.kvs_pb2 import (
    LWW, SET, NO_ERROR, KeyResponse, KeyTuple, LWWValue, SetValue,
    KeyAddressResponse,
)
from anna.lattices import LWWPairLattice, SetLattice


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


class TestAnnaTcpClientConstructor:
    def test_creates_client(self):
        client = make_client()
        assert client.elb_addr == "127.0.0.1"
        assert client.elb_ports == [6450]

    def test_non_local_ports(self):
        with patch("anna.client.zmq") as mock_zmq:
            mock_zmq.Context.return_value = MagicMock()
            mock_zmq.PULL = 7
            mock_zmq.PUSH = 8
            from anna.client import AnnaTcpClient
            client = AnnaTcpClient("elb.example.com", "10.0.0.1", local=False)
        assert client.elb_ports == list(range(6450, 6454))

    def test_ip_inference(self):
        with patch("anna.client.zmq") as mock_zmq, \
             patch("anna.client.socket") as mock_socket:
            mock_zmq.Context.return_value = MagicMock()
            mock_zmq.PULL = 7
            mock_zmq.PUSH = 8
            mock_socket.gethostname.return_value = "myhost"
            mock_socket.gethostbyname.return_value = "192.168.1.1"
            from anna.client import AnnaTcpClient
            client = AnnaTcpClient("127.0.0.1", None, local=True)
        assert client.ut.get_ip() == "192.168.1.1"


class TestGetRequestId:
    def test_increments(self):
        client = make_client()
        id1 = client._get_request_id()
        id2 = client._get_request_id()
        assert id1 != id2
        assert "127.0.0.1" in id1

    def test_wraps_at_10000(self):
        client = make_client()
        client.rid = 9999
        id1 = client._get_request_id()
        assert id1.endswith(":9999")
        id2 = client._get_request_id()
        assert id2.endswith(":0")


class TestPrepareDataRequest:
    def test_creates_request_with_key(self):
        client = make_client()
        req, tuples = client._prepare_data_request(["mykey"])
        assert len(tuples) == 1
        assert tuples[0].key == "mykey"
        assert req.request_id != ""

    def test_multiple_keys(self):
        client = make_client()
        req, tuples = client._prepare_data_request(["k1", "k2", "k3"])
        assert len(tuples) == 3
        assert tuples[0].key == "k1"
        assert tuples[2].key == "k3"


class TestGetWorkerAddress:
    def test_returns_none_when_no_addresses(self):
        client = make_client()
        with patch.object(client, '_query_routing', return_value=[]):
            result = client._get_worker_address("missing_key")
            assert result is None

    def test_caches_addresses(self):
        client = make_client()
        client.address_cache["cached_key"] = ["tcp://127.0.0.1:6200"]
        result = client._get_worker_address("cached_key")
        assert result == "tcp://127.0.0.1:6200"


class TestInvalidateCache:
    def test_removes_key(self):
        client = make_client()
        client.address_cache["mykey"] = ["addr1"]
        client._invalidate_cache("mykey")
        assert "mykey" not in client.address_cache


class TestGet:
    def test_get_returns_deserialized_value(self):
        client = make_client()
        client.address_cache["mykey"] = ["tcp://127.0.0.1:6200"]

        lww_val = LWWValue()
        lww_val.timestamp = 1
        lww_val.value = b"hello"

        response = KeyResponse()
        response.response_id = "placeholder"
        tup = response.tuples.add()
        tup.key = "mykey"
        tup.lattice_type = LWW
        tup.payload = lww_val.SerializeToString()
        tup.error = NO_ERROR

        mock_send_sock = MagicMock()
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = mock_send_sock

        with patch("anna.client.send_request") as mock_send, \
             patch("anna.client.recv_response") as mock_recv:
            # Make recv_response return our prepared response, matching any request ID
            def recv_side_effect(req_ids, sock, cls):
                response.response_id = req_ids[0]
                return [response]
            mock_recv.side_effect = recv_side_effect

            result = client.get("mykey")

        assert "mykey" in result
        assert isinstance(result["mykey"], LWWPairLattice)
        assert result["mykey"].reveal() == b"hello"


class TestPut:
    def test_put_returns_success(self):
        client = make_client()
        client.address_cache["mykey"] = ["tcp://127.0.0.1:6200"]

        response = KeyResponse()
        response.response_id = "placeholder"
        tup = response.tuples.add()
        tup.key = "mykey"
        tup.error = NO_ERROR

        mock_send_sock = MagicMock()
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = mock_send_sock

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            def recv_side_effect(req_ids, sock, cls):
                response.response_id = req_ids[0]
                return [response]
            mock_recv.side_effect = recv_side_effect

            val = LWWPairLattice(1, b"world")
            result = client.put("mykey", val)

        assert result["mykey"] is True

    def test_put_returns_false_when_no_worker(self):
        client = make_client()
        with patch.object(client, '_get_worker_address', return_value=None):
            val = LWWPairLattice(1, b"world")
            result = client.put("mykey", val)
        assert result is False


class TestGetWithInvalidate:
    def test_get_invalidates_cache(self):
        client = make_client()
        client.address_cache["mykey"] = ["tcp://127.0.0.1:6200"]

        lww_val = LWWValue()
        lww_val.timestamp = 1
        lww_val.value = b"hello"

        response = KeyResponse()
        response.response_id = "placeholder"
        tup = response.tuples.add()
        tup.key = "mykey"
        tup.lattice_type = LWW
        tup.payload = lww_val.SerializeToString()
        tup.error = NO_ERROR
        tup.invalidate = True

        mock_send_sock = MagicMock()
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = mock_send_sock

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: [response]
            result = client.get("mykey")

        assert "mykey" not in client.address_cache
        assert isinstance(result["mykey"], LWWPairLattice)


class TestPutWithInvalidate:
    def test_put_invalidates_cache(self):
        client = make_client()
        client.address_cache["mykey"] = ["tcp://127.0.0.1:6200"]

        response = KeyResponse()
        response.response_id = "placeholder"
        tup = response.tuples.add()
        tup.key = "mykey"
        tup.error = NO_ERROR
        tup.invalidate = True

        mock_send_sock = MagicMock()
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = mock_send_sock

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: [response]
            val = LWWPairLattice(1, b"world")
            result = client.put("mykey", val)

        assert "mykey" not in client.address_cache
        assert result["mykey"] is True


class TestGetWorkerAddressPickFalse:
    def test_returns_all_addresses(self):
        client = make_client()
        client.address_cache["mykey"] = ["tcp://addr1", "tcp://addr2"]
        result = client._get_worker_address("mykey", pick=False)
        assert result == ["tcp://addr1", "tcp://addr2"]


class TestQueryRouting:
    def test_error_returns_empty(self):
        from anna.kvs_pb2 import KeyAddressResponse
        client = make_client()
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        response = KeyAddressResponse()
        response.error = 1  # non-zero error

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.return_value = [response]
            result = client._query_routing("mykey", 6450)

        assert result == []


class TestGetBytesNoneAtEnd:
    def test_returns_none_when_error(self):
        from anna.kvs_pb2 import KeyResponse
        client = make_client()
        client.address_cache["mykey"] = ["tcp://127.0.0.1:6200"]
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        response = KeyResponse()
        response.response_id = "placeholder"
        tup = response.tuples.add()
        tup.key = "mykey"
        tup.error = 1  # error, not NO_ERROR

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: [response]
            result = client.get_bytes("mykey")

        assert result is None


class TestConvenienceMethods:
    """Test convenience methods that wrap get/put with specific lattice types."""

    def _make_get_client(self, key, lattice_response):
        """Helper: create a client whose get() returns {key: lattice_response}."""
        client = make_client()
        with patch.object(client, 'get', return_value={key: lattice_response}):
            yield client

    def test_get_causal(self):
        client = make_client()
        mock_result = MagicMock()
        with patch.object(client, 'get', return_value={"k": mock_result}) as mock_get:
            result = client.get_causal("k")
        mock_get.assert_called_once_with(["k"])
        assert result is mock_result

    def test_put_causal(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.put_causal("k", "hello")
        mock_put.assert_called_once()
        assert result == {"k": True}

    def test_put_causal_bytes(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.put_causal("k", b"raw_bytes")
        mock_put.assert_called_once()

    def test_delete(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.delete("k")
        mock_put.assert_called_once()
        # The value should be an LWWPairLattice with empty bytes
        lattice_arg = mock_put.call_args[0][1]
        assert isinstance(lattice_arg, LWWPairLattice)
        assert lattice_arg.reveal() == b""

    def test_get_ordered_set(self):
        client = make_client()
        mock_result = MagicMock()
        with patch.object(client, 'get', return_value={"k": mock_result}):
            result = client.get_ordered_set("k")
        assert result is mock_result

    def test_put_ordered_set(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.put_ordered_set("k", ["a", "b"])
        mock_put.assert_called_once()

    def test_put_ordered_set_bytes(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.put_ordered_set("k", [b"x", b"y"])
        mock_put.assert_called_once()

    def test_get_single_causal(self):
        client = make_client()
        mock_result = MagicMock()
        with patch.object(client, 'get', return_value={"k": mock_result}):
            result = client.get_single_causal("k")
        assert result is mock_result

    def test_put_single_causal(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.put_single_causal("k", "value")
        mock_put.assert_called_once()

    def test_put_single_causal_bytes(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.put_single_causal("k", b"raw")
        mock_put.assert_called_once()

    def test_get_priority(self):
        client = make_client()
        mock_result = MagicMock()
        with patch.object(client, 'get', return_value={"k": mock_result}):
            result = client.get_priority("k")
        assert result is mock_result

    def test_put_priority_str(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.put_priority("k", 1.5, "data")
        mock_put.assert_called_once()

    def test_put_priority_bytes(self):
        client = make_client()
        with patch.object(client, 'put', return_value={"k": True}) as mock_put:
            result = client.put_priority("k", 2.0, b"raw")
        mock_put.assert_called_once()


class TestGetBytesInvalidate:
    def test_get_bytes_invalidate_cache(self):
        client = make_client()
        meta_key = "test_key"
        client.address_cache[meta_key] = ["tcp://127.0.0.1:6200"]
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        lww_val = LWWValue()
        lww_val.timestamp = 1
        lww_val.value = b"data"

        response = KeyResponse()
        response.response_id = "placeholder"
        tup = response.tuples.add()
        tup.key = meta_key
        tup.error = NO_ERROR
        tup.invalidate = True
        tup.payload = lww_val.SerializeToString()

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: [response]
            result = client.get_bytes(meta_key)

        assert result == b"data"
        assert meta_key not in client.address_cache


class TestGetBytesNoResponse:
    def test_returns_none_for_empty_responses(self):
        client = make_client()
        client.address_cache["k"] = ["tcp://127.0.0.1:6200"]
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: []
            result = client.get_bytes("k")

        assert result is None


class TestQueryRoutingSuccess:
    def test_returns_addresses(self):
        client = make_client()
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        response = KeyAddressResponse()
        response.error = 0
        addr_entry = response.addresses.add()
        addr_entry.key = "mykey"
        addr_entry.ips.append("tcp://10.0.0.1:6200")
        addr_entry.ips.append("tcp://10.0.0.2:6200")

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.return_value = [response]
            result = client._query_routing("mykey", 6450)

        assert result == ["tcp://10.0.0.1:6200", "tcp://10.0.0.2:6200"]

    def test_skips_non_matching_keys(self):
        client = make_client()
        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        response = KeyAddressResponse()
        response.error = 0
        addr_entry = response.addresses.add()
        addr_entry.key = "other_key"
        addr_entry.ips.append("tcp://10.0.0.1:6200")

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.return_value = [response]
            result = client._query_routing("mykey", 6450)

        assert result == []


class TestGetAll:
    def test_get_all_returns_values(self):
        client = make_client()
        client.address_cache["k1"] = ["tcp://addr1"]

        lww_val = LWWValue()
        lww_val.timestamp = 1
        lww_val.value = b"val1"

        response = KeyResponse()
        response.response_id = "placeholder"
        tup = response.tuples.add()
        tup.key = "k1"
        tup.lattice_type = LWW
        tup.payload = lww_val.SerializeToString()
        tup.error = NO_ERROR

        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: [response]
            result = client.get_all(["k1"])

        assert "k1" in result

    def test_get_all_rejects_non_list(self):
        client = make_client()
        with pytest.raises(ValueError):
            client.get_all("single_key")

    def test_get_all_rejects_empty_list(self):
        client = make_client()
        with pytest.raises(ValueError):
            client.get_all([])

    def test_get_all_with_invalidate(self):
        client = make_client()
        client.address_cache["k1"] = ["tcp://addr1"]

        lww_val = LWWValue()
        lww_val.timestamp = 1
        lww_val.value = b"val"

        response = KeyResponse()
        response.response_id = "placeholder"
        tup = response.tuples.add()
        tup.key = "k1"
        tup.lattice_type = LWW
        tup.payload = lww_val.SerializeToString()
        tup.error = NO_ERROR
        tup.invalidate = True

        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        with patch("anna.client.send_request"), \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: [response]
            result = client.get_all(["k1"])

        assert "k1" not in client.address_cache

    def test_get_all_merge_multiple_responses(self):
        """Test get_all with multiple addresses returning multiple responses."""
        client = make_client()
        client.address_cache["k1"] = ["tcp://addr1", "tcp://addr2"]

        lww_val1 = LWWValue()
        lww_val1.timestamp = 1
        lww_val1.value = b"val1"

        lww_val2 = LWWValue()
        lww_val2.timestamp = 2
        lww_val2.value = b"val2"

        response1 = KeyResponse()
        response1.response_id = "r1"
        tup1 = response1.tuples.add()
        tup1.key = "k1"
        tup1.lattice_type = LWW
        tup1.payload = lww_val1.SerializeToString()
        tup1.error = NO_ERROR

        response2 = KeyResponse()
        response2.response_id = "r2"
        tup2 = response2.tuples.add()
        tup2.key = "k1"
        tup2.lattice_type = LWW
        tup2.payload = lww_val2.SerializeToString()
        tup2.error = NO_ERROR

        client.pusher_cache = MagicMock()
        client.pusher_cache.get.return_value = MagicMock()

        with patch("anna.client.send_request") as mock_send, \
             patch("anna.client.recv_response") as mock_recv:
            mock_recv.side_effect = lambda ids, sock, cls: [response1, response2]
            result = client.get_all(["k1"])

        assert "k1" in result
        assert isinstance(result["k1"], LWWPairLattice)
        mock_send.assert_called()
        mock_recv.assert_called_once()


class TestPutAll:
    def test_put_all_no_worker(self):
        client = make_client()
        with patch.object(client, '_get_worker_address', return_value=None):
            val = LWWPairLattice(1, b"data")
            result = client.put_all("k", val)
        assert result is False


class TestResponseAddress:
    def test_returns_connect_address(self):
        client = make_client()
        addr = client.response_address
        assert "127.0.0.1" in addr
        assert "tcp://" in addr
