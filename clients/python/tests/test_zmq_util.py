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
        sock.recv.return_value = b"response_bytes"

        responses = recv_response(["req-1"], sock, resp_class)

        assert len(responses) == 1
        assert responses[0] == resp_obj

    def test_skips_non_matching_then_matches(self):
        resp_class = MagicMock()

        wrong_resp = MagicMock()
        wrong_resp.response_id = "wrong-id"
        wrong_resp.Clear = MagicMock()
        wrong_resp.ParseFromString = MagicMock()

        right_resp = MagicMock()
        right_resp.response_id = "req-1"
        right_resp.ParseFromString = MagicMock()

        call_count = [0]
        def make_resp():
            obj = MagicMock()
            if call_count[0] == 0:
                obj.response_id = "wrong-id"
                obj.Clear = MagicMock()
                def update_id(data):
                    obj.response_id = "req-1"
                obj.ParseFromString = MagicMock(side_effect=update_id)
            else:
                obj.response_id = "req-1"
            call_count[0] += 1
            return obj

        resp_class.side_effect = make_resp

        sock = MagicMock()
        sock.recv.return_value = b"data"

        responses = recv_response(["req-1"], sock, resp_class)
        assert len(responses) == 1

    def test_collects_multiple_responses(self):
        resp_class = MagicMock()

        resp1 = MagicMock()
        resp1.response_id = "req-1"
        resp2 = MagicMock()
        resp2.response_id = "req-2"

        resp_class.side_effect = [resp1, resp2]

        sock = MagicMock()
        sock.recv.return_value = b"data"

        responses = recv_response(["req-1", "req-2"], sock, resp_class)
        assert len(responses) == 2


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
