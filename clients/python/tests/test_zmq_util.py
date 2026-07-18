from unittest.mock import MagicMock, call

from anna.zmq_util import send_request, recv_response, SocketCache


class TestSendRequest:
    def test_serializes_and_sends(self):
        req = MagicMock()
        req.SerializeToString.return_value = b"serialized_data"
        sock = MagicMock()

        send_request(req, sock)

        req.SerializeToString.assert_called_once()
        sock.send.assert_called_once_with(b"serialized_data")


class TestRecvResponse:
    def test_receives_matching_response(self):
        resp_class = MagicMock()
        resp_obj = MagicMock()
        resp_obj.response_id = "req-1"
        resp_class.return_value = resp_obj
        resp_obj.ParseFromString = MagicMock()

        sock = MagicMock()
        sock.poll.return_value = 1
        sock.recv.return_value = b"response_bytes"

        responses = recv_response(["req-1"], sock, resp_class)

        assert len(responses) == 1
        assert responses[0] == resp_obj

    def test_skips_non_matching_then_matches(self):
        resp_class = MagicMock()

        obj = MagicMock()
        parse_count = [0]
        def parse_side_effect(data):
            if parse_count[0] == 0:
                obj.response_id = "wrong-id"
            else:
                obj.response_id = "req-1"
            parse_count[0] += 1

        obj.ParseFromString = MagicMock(side_effect=parse_side_effect)
        obj.Clear = MagicMock()
        resp_class.return_value = obj

        sock = MagicMock()
        sock.poll.return_value = 1
        sock.recv.return_value = b"data"

        responses = recv_response(["req-1"], sock, resp_class)
        assert len(responses) == 1
        assert obj.Clear.called

    def test_collects_multiple_responses(self):
        resp_class = MagicMock()

        resp1 = MagicMock()
        resp1.response_id = "req-1"
        resp2 = MagicMock()
        resp2.response_id = "req-2"

        resp_class.side_effect = [resp1, resp2]

        sock = MagicMock()
        sock.poll.return_value = 1
        sock.recv.return_value = b"data"

        responses = recv_response(["req-1", "req-2"], sock, resp_class)
        assert len(responses) == 2

    def test_timeout_raises_error(self):
        import pytest
        resp_class = MagicMock()
        sock = MagicMock()
        sock.poll.return_value = 0

        with pytest.raises(TimeoutError, match="Timed out"):
            recv_response(["req-1"], sock, resp_class, timeout_ms=100)

    def test_timeout_while_skipping_non_matching(self):
        import pytest
        resp_class = MagicMock()
        resp_obj = MagicMock()
        resp_obj.response_id = "wrong-id"
        resp_obj.Clear = MagicMock()
        resp_class.return_value = resp_obj

        sock = MagicMock()
        sock.poll.side_effect = [1, 0]
        sock.recv.return_value = b"data"

        with pytest.raises(TimeoutError, match="Timed out"):
            recv_response(["req-1"], sock, resp_class, timeout_ms=100)


class TestSocketCache:
    def test_creates_socket_on_first_access(self):
        ctx = MagicMock()
        sock = MagicMock()
        ctx.socket.return_value = sock

        cache = SocketCache(ctx, 8)
        result = cache.get("tcp://127.0.0.1:6200")

        assert result == sock
        ctx.socket.assert_called_once_with(8)
        sock.connect.assert_called_once_with("tcp://127.0.0.1:6200")

    def test_returns_cached_socket_on_second_access(self):
        ctx = MagicMock()
        sock = MagicMock()
        ctx.socket.return_value = sock

        cache = SocketCache(ctx, 8)
        first = cache.get("tcp://127.0.0.1:6200")
        second = cache.get("tcp://127.0.0.1:6200")

        assert first is second
        ctx.socket.assert_called_once()

    def test_different_addrs_get_different_sockets(self):
        ctx = MagicMock()
        sock1 = MagicMock()
        sock2 = MagicMock()
        ctx.socket.side_effect = [sock1, sock2]

        cache = SocketCache(ctx, 8)
        first = cache.get("tcp://127.0.0.1:6200")
        second = cache.get("tcp://127.0.0.1:6201")

        assert first is not second
        assert ctx.socket.call_count == 2
