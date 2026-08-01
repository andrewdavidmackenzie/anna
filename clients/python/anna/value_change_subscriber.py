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

"""Subscribe to value changes for specific keys via the KVS gossip mechanism."""

import logging

import zmq

from .kvs_pb2 import KeyResponse
from .shared_pb2 import StringSet

CACHE_REGISTRATION_PORT = 6900
CACHE_UPDATE_PORT = 6850

logger = logging.getLogger(__name__)


class ValueChangeSubscriber:
    """A client that receives key updates pushed from the KVS during gossip.

    The cache client registers with KVS server threads to watch specific keys.
    When those keys are updated, the KVS pushes the new values during its
    gossip epoch.

    Args:
        server_ip: IP address of the KVS server.
        cache_ip: IP address of this cache client (for receiving updates).
        memory_threads: Number of memory threads on the server (default 1).
        offset: Port base offset (default 0).
        tid: Thread ID for port selection (default 0).
    """

    def __init__(self, server_ip, cache_ip="127.0.0.1", memory_threads=1,
                 offset=0, tid=0):
        self.server_ip = server_ip
        self.cache_ip = cache_ip
        self.memory_threads = memory_threads
        self.offset = offset
        self.tid = tid
        self.local_cache = {}
        self.watched_keys = []

        self.context = zmq.Context()

        self.update_puller = self.context.socket(zmq.PULL)
        bind_addr = "tcp://{}:{}".format(
            cache_ip, tid + CACHE_UPDATE_PORT + offset)
        self.update_puller.bind(bind_addr)
        logger.info("Cache client listening for updates on %s", bind_addr)

        self.push_sockets = {}

    def watch(self, keys):
        """Register interest in keys with all KVS server threads.

        Args:
            keys: List of key names to watch.
        """
        self.watched_keys.extend(keys)

        msg = StringSet()
        msg.keys.append(self.cache_ip)
        for key in keys:
            msg.keys.append(key)

        payload = msg.SerializeToString()

        for tid in range(self.memory_threads):
            addr = "tcp://{}:{}".format(
                self.server_ip, tid + CACHE_REGISTRATION_PORT + self.offset)
            if addr not in self.push_sockets:
                sock = self.context.socket(zmq.PUSH)
                sock.connect(addr)
                self.push_sockets[addr] = sock

            self.push_sockets[addr].send(payload)
            logger.debug("Registered %d keys with KVS thread %d at %s",
                         len(keys), tid, addr)

        logger.info("Registered %d keys with %d KVS threads",
                     len(keys), self.memory_threads)

    def recv_update(self, timeout_ms=15000):
        """Receive the next update pushed from the KVS.

        Args:
            timeout_ms: Timeout in milliseconds (default 15000).

        Returns:
            Tuple of (key, payload_bytes) or None if timeout.
        """
        if self.update_puller.poll(timeout_ms):
            data = self.update_puller.recv()
            response = KeyResponse()
            response.ParseFromString(data)

            for tuple_msg in response.tuples:
                key = tuple_msg.key
                payload = tuple_msg.payload
                if payload:
                    self.local_cache[key] = payload
                    logger.debug("Cache updated for key: %s", key)
                    return (key, payload)

        return None

    def get_cached(self, key):
        """Read a value from the local cache.

        Args:
            key: The key to look up.

        Returns:
            Raw payload bytes, or None if not cached.
        """
        return self.local_cache.get(key)

    def close(self):
        """Clean up ZMQ sockets."""
        self.update_puller.close()
        for sock in self.push_sockets.values():
            sock.close()
        self.push_sockets.clear()
