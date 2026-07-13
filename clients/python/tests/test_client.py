from unittest.mock import MagicMock, patch, PropertyMock
import pytest

from anna.kvs_pb2 import LWW, SET, NO_ERROR, KeyResponse, KeyTuple, LWWValue, SetValue
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


class TestResponseAddress:
    def test_returns_connect_address(self):
        client = make_client()
        addr = client.response_address
        assert "127.0.0.1" in addr
        assert "tcp://" in addr
