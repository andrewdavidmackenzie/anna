#  Copyright 2019 U.C. Berkeley RISE Lab
#
#  Licensed under the Apache License, Version 2.0 (the "License");
#  you may not use this file except in compliance with the License.
#  You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.

import time
import unittest

import zmq

from anna.kvs_pb2 import KeyResponse, KeyTuple
from anna.shared_pb2 import StringSet
from anna.value_change_subscriber import ValueChangeSubscriber, \
    CACHE_REGISTRATION_PORT, CACHE_UPDATE_PORT


class TestValueChangeSubscriberConstants(unittest.TestCase):
    def test_registration_port(self):
        self.assertEqual(CACHE_REGISTRATION_PORT, 7200)

    def test_update_port(self):
        self.assertEqual(CACHE_UPDATE_PORT, 7150)


class TestValueChangeSubscriber(unittest.TestCase):
    def setUp(self):
        self.client = ValueChangeSubscriber(
            server_ip="127.0.0.1",
            cache_ip="127.0.0.1",
            memory_threads=1,
            offset=50000,
            tid=0,
        )

    def tearDown(self):
        self.client.close()

    def test_initial_state(self):
        self.assertEqual(self.client.watched_keys, [])
        self.assertEqual(self.client.local_cache, {})

    def test_get_cached_missing_key(self):
        self.assertIsNone(self.client.get_cached("nonexistent"))

    def test_get_cached_present_key(self):
        self.client.local_cache["test-key"] = b"test-value"
        self.assertEqual(self.client.get_cached("test-key"), b"test-value")

    def test_recv_update_timeout(self):
        result = self.client.recv_update(timeout_ms=100)
        self.assertIsNone(result)

    def test_close_cleans_up(self):
        self.client.close()
        self.assertEqual(self.client.push_sockets, {})

    def test_watched_keys_tracking(self):
        self.client.watched_keys.extend(["key1", "key2"])
        self.assertEqual(self.client.watched_keys, ["key1", "key2"])
        self.client.watched_keys.extend(["key3"])
        self.assertEqual(len(self.client.watched_keys), 3)

    def test_recv_update_receives_pushed_value(self):
        ctx = zmq.Context()
        pusher = ctx.socket(zmq.PUSH)
        pusher.connect("tcp://127.0.0.1:57150")
        time.sleep(0.1)

        response = KeyResponse()
        t = response.tuples.add()
        t.key = "py_test_key"
        t.payload = b"py_test_value"
        pusher.send(response.SerializeToString())

        result = self.client.recv_update(timeout_ms=5000)
        self.assertIsNotNone(result)
        key, payload = result
        self.assertEqual(key, "py_test_key")
        self.assertEqual(payload, b"py_test_value")
        self.assertEqual(self.client.get_cached("py_test_key"), b"py_test_value")

        pusher.close()
        ctx.term()

    def test_recv_update_skips_empty_payload(self):
        ctx = zmq.Context()
        pusher = ctx.socket(zmq.PUSH)
        pusher.connect("tcp://127.0.0.1:57150")
        time.sleep(0.1)

        response = KeyResponse()
        t = response.tuples.add()
        t.key = "empty_key"
        t.payload = b""
        pusher.send(response.SerializeToString())

        result = self.client.recv_update(timeout_ms=2000)
        self.assertIsNone(result)
        self.assertIsNone(self.client.get_cached("empty_key"))

        pusher.close()
        ctx.term()


class TestWatch(unittest.TestCase):
    """Test watch() method using mocked ZMQ sockets to avoid blocking sends."""

    def setUp(self):
        from unittest.mock import MagicMock, patch

        # Create a real subscriber but with mocked context for watch tests
        self.mock_context = MagicMock()
        self.mock_push_socket = MagicMock()
        self.mock_context.socket.return_value = self.mock_push_socket

        # Create a subscriber with a real update_puller (bound to high offset)
        self.client = ValueChangeSubscriber(
            server_ip="127.0.0.1",
            cache_ip="127.0.0.1",
            memory_threads=2,
            offset=51000,
            tid=0,
        )
        # Replace the context used for push sockets with our mock
        self.client.context = self.mock_context

    def tearDown(self):
        # The update_puller is a real socket, so close it manually
        try:
            self.client.update_puller.close()
        except Exception:
            pass
        self.client.push_sockets.clear()

    def test_watch_extends_watched_keys(self):
        self.client.watch(["key1", "key2"])
        self.assertEqual(self.client.watched_keys, ["key1", "key2"])

    def test_watch_sends_to_all_threads(self):
        self.client.watch(["keyA"])
        # With 2 memory_threads, should have 2 push sockets
        self.assertEqual(len(self.client.push_sockets), 2)

    def test_watch_creates_push_sockets_with_correct_addresses(self):
        self.client.watch(["keyB"])
        expected_addrs = [
            "tcp://127.0.0.1:{}".format(0 + CACHE_REGISTRATION_PORT + 51000),
            "tcp://127.0.0.1:{}".format(1 + CACHE_REGISTRATION_PORT + 51000),
        ]
        for addr in expected_addrs:
            self.assertIn(addr, self.client.push_sockets)

    def test_watch_reuses_existing_sockets(self):
        self.client.watch(["key1"])
        socket_count_after_first = len(self.client.push_sockets)
        self.client.watch(["key2"])
        # Should reuse existing sockets, not create new ones
        self.assertEqual(len(self.client.push_sockets), socket_count_after_first)

    def test_watch_multiple_keys_at_once(self):
        self.client.watch(["a", "b", "c"])
        self.assertEqual(self.client.watched_keys, ["a", "b", "c"])

    def test_watch_sends_serialized_message(self):
        self.client.watch(["test_key"])
        # Verify send was called on each push socket
        self.assertEqual(self.mock_push_socket.send.call_count, 2)

        # Verify the payload contains the cache_ip and key
        payload = self.mock_push_socket.send.call_args[0][0]
        msg = StringSet()
        msg.ParseFromString(payload)
        self.assertIn("127.0.0.1", list(msg.keys))
        self.assertIn("test_key", list(msg.keys))


class TestCloseWithPushSockets(unittest.TestCase):
    def test_close_cleans_push_sockets(self):
        from unittest.mock import MagicMock

        client = ValueChangeSubscriber(
            server_ip="127.0.0.1",
            cache_ip="127.0.0.1",
            memory_threads=1,
            offset=52000,
            tid=0,
        )
        # Add a mock push socket
        mock_sock = MagicMock()
        client.push_sockets["tcp://127.0.0.1:59200"] = mock_sock

        client.close()
        self.assertEqual(client.push_sockets, {})
        mock_sock.close.assert_called_once()


if __name__ == "__main__":
    unittest.main()
