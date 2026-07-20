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

import unittest

from anna.cache_client import CacheClient, CACHE_REGISTRATION_PORT, \
    CACHE_UPDATE_PORT


class TestCacheClientConstants(unittest.TestCase):
    def test_registration_port(self):
        self.assertEqual(CACHE_REGISTRATION_PORT, 7200)

    def test_update_port(self):
        self.assertEqual(CACHE_UPDATE_PORT, 7150)


class TestCacheClient(unittest.TestCase):
    def setUp(self):
        self.client = CacheClient(
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


if __name__ == "__main__":
    unittest.main()
