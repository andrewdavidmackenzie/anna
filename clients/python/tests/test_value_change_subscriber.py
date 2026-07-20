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


if __name__ == "__main__":
    unittest.main()
