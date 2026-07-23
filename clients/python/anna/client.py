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

import random
import socket
import time
import zmq

from .kvs_pb2 import (
    GET, PUT,  # Anna's request types
    LWW,  # Anna's lattice types
    NO_ERROR, WRONG_THREAD,  # Anna's error modes
    KeyAddressRequest,
    KeyAddressResponse,
    KeyResponse,
    KeyRequest,
    LWWValue,
)
from .metadata_pb2 import (
    MEMORY, DISK,
    ClusterTopology,
    KeyAccessData,
    KeySizeData,
    ReplicationFactor,
    ServerThreadStatistics,
)
from .shared_pb2 import StringSet
from .base_client import BaseAnnaClient
from .common import UserThread
from .lattices import (
    LWWPairLattice,
    ListBasedOrderedSet,
    MultiKeyCausalLattice,
    OrderedSetLattice,
    PriorityLattice,
    SetLattice,
    SingleKeyCausalLattice,
    MapLattice,
    VectorClock,
)
from .zmq_util import (
    recv_response,
    send_request,
    SocketCache
)


class AnnaTcpClient(BaseAnnaClient):
    def __init__(self, elb_addr, ip, local=False, offset=0):
        """
        The AnnaTcpClientTcpAnnaClient allows you to interact with a local
        copy of Anna or with a remote cluster running on AWS.

        elb_addr: Either 127.0.0.1 (local mode) or the address of an AWS ELB
        for the routing tier
        ip: The IP address of the machine being used -- if None is provided,
        one is inferred by using socket.gethostbyname(); WARNING: this does not
        always work
        elb_ports: The ports on which the routing tier will listen; use 6450 if
        running in local mode, otherwise do not change
        offset: A port numbering offset, which is only needed if multiple
        clients are running on the same machine
        """

        super().__init__()
        self.elb_addr = elb_addr

        if local:
            self.elb_ports = [6450]
        else:
            self.elb_ports = list(range(6450, 6454))

        if ip:
            self.ut = UserThread(ip, offset)
        else:  # If the IP is not provided, we attempt to infer it.
            self.ut = UserThread(socket.gethostbyname(socket.gethostname()),
                                 offset)

        self.context = zmq.Context(1)

        self.address_cache = {}
        self.pusher_cache = SocketCache(self.context, zmq.PUSH)

        self.response_puller = self.context.socket(zmq.PULL)
        self.response_puller.bind(self.ut.get_request_pull_bind_addr())

        self.key_address_puller = self.context.socket(zmq.PULL)
        self.key_address_puller.bind(self.ut.get_key_address_bind_addr())

        self.rid = 0
        self._max_retries = 5

    def get(self, keys):
        if type(keys) != list:
            keys = [keys]

        # Initialize all KV pairs to None. Only change a value if we get a
        # valid response from the server.
        kv_pairs = {}
        for key in keys:
            kv_pairs[key] = None

        pending = list(keys)
        for attempt in range(self._max_retries + 1):
            if not pending:
                break

            worker_addresses = {}
            for key in pending:
                worker_addresses[key] = self._get_worker_address(key)

            request_ids = []
            for key in worker_addresses:
                if worker_addresses[key]:
                    send_sock = self.pusher_cache.get(worker_addresses[key])

                    req, _ = self._prepare_data_request([key])
                    req.type = GET

                    send_request(req, send_sock)
                    request_ids.append(req.request_id)

            # Wait for all responses to return.
            responses = recv_response(request_ids, self.response_puller,
                                      KeyResponse)

            retry_keys = []
            for response in responses:
                for tup in response.tuples:
                    if tup.invalidate:
                        self._invalidate_cache(tup.key)

                    if tup.error == WRONG_THREAD and attempt < self._max_retries:
                        if tup.key in self.address_cache:
                            self._invalidate_cache(tup.key)
                        retry_keys.append(tup.key)
                    elif tup.error == NO_ERROR:
                        kv_pairs[tup.key] = self._deserialize(tup)

            pending = retry_keys

        return kv_pairs

    def get_all(self, keys):
        if type(keys) != list or not keys:
            raise ValueError('`get_all` expects a list of keys')

        worker_addresses = {}
        for key in keys:
            worker_addresses[key] = self._get_worker_address(key, False)

        # Initialize all KV pairs to 0. Only change a value if we get a valid
        # response from the server.
        kv_pairs = {}
        for key in keys:
            kv_pairs[key] = None

        for key in keys:
            if worker_addresses[key]:
                req, _ = self._prepare_data_request(key)
                req.type = GET

                req_ids = []
                for address in worker_addresses[key]:
                    req.request_id = self._get_request_id()

                    send_sock = self.pusher_cache.get(address)
                    send_request(req, send_sock)

                req_ids.append(req.request_id)

        responses = recv_response(req_ids, self.response_puller, KeyResponse)

        for resp in responses:
            for tup in resp.tuples:
                if tup.invalidate:
                    self._invalidate_cache(tup.key)

                if tup.error == NO_ERROR:
                    val = self._deserialize(tup)

                    if kv_pairs[tup.key]:
                        kv_pairs[tup.key].merge(val)
                    else:
                        kv_pairs[tup.key] = val

        return kv_pairs

    def put(self, keys, values):
        if type(keys) != list:
            keys = [keys]
        if type(values) != list:
            values = [values]

        kv_map = dict(zip(keys, values))
        pending = list(kv_map.keys())
        results = {}

        for attempt in range(self._max_retries + 1):
            if not pending:
                break

            request_ids = []
            for key in pending:
                value = kv_map[key]
                worker_address = self._get_worker_address(key)

                if not worker_address:
                    return False

                send_sock = self.pusher_cache.get(worker_address)

                req, tup = self._prepare_data_request([key])
                req.type = PUT
                request_ids.append(req.request_id)

                tup = tup[0]
                tup.payload, tup.lattice_type = self._serialize(value)

                send_request(req, send_sock)

            responses = recv_response(request_ids, self.response_puller,
                                      KeyResponse)

            retry_keys = []
            for response in responses:
                tup = response.tuples[0]

                if tup.invalidate:
                    self._invalidate_cache(tup.key)

                if tup.error == WRONG_THREAD and attempt < self._max_retries:
                    if tup.key in self.address_cache:
                        self._invalidate_cache(tup.key)
                    retry_keys.append(tup.key)
                else:
                    results[tup.key] = (tup.error == NO_ERROR)

            pending = retry_keys

        return results

    def put_all(self, key, value):
        worker_addresses = self._get_worker_address(key, False)

        if not worker_addresses:
            return False

        req, tup = self._prepare_data_request(key)
        req.type = PUT
        tup.payload, tup.lattice_type = self._serialize(value)
        tup.timestamp = 0

        req_ids = []
        for address in worker_addresses:
            req.request_id = self._get_request_id()

            send_sock = self.pusher_cache.get(address)
            send_request(req, send_sock)

            req_ids.append(req.request_id)

        responses = recv_response(req_ids, self.response_puller, KeyResponse)

        for resp in responses:
            tup = resp.tuples[0]
            if tup.invalidate:
                # reissue the request
                self._invalidate_cache(tup.key)
                return self.put(key, value)

            if tup.error != NO_ERROR:
                return False

        return True

    # Returns the worker address for a particular key. If worker addresses for
    # that key are not cached locally, a query is synchronously issued to the
    # routing tier, and the address cache is updated.
    def _get_worker_address(self, key, pick=True):
        if key not in self.address_cache or len(self.address_cache[key]) == 0:
            port = random.choice(self.elb_ports)
            addresses = self._query_routing(key, port)
            self.address_cache[key] = addresses

        if len(self.address_cache[key]) == 0:
            return None

        if pick:
            return random.choice(self.address_cache[key])
        else:
            return self.address_cache[key]

    # Invalidates the address cache for a particular key when the server tells
    # the client that its cache is out of date.
    def _invalidate_cache(self, key):
        del self.address_cache[key]

    def get_causal(self, key):
        result = self.get([key])
        return result.get(key)

    def put_causal(self, key, value):
        vc = VectorClock({"test": 1}, True)
        dep_vc = VectorClock({"test1": 1}, True)
        deps = MapLattice({"dep1": dep_vc})
        val = SetLattice({value.encode() if isinstance(value, str) else value})
        lattice = MultiKeyCausalLattice(vc, deps, val)
        return self.put(key, lattice)


    def delete(self, key):
        ts = time.time_ns()
        val = LWWPairLattice(ts, b"")
        return self.put(key, val)

    def get_ordered_set(self, key):
        result = self.get([key])
        return result.get(key)

    def put_ordered_set(self, key, values):
        encoded = [v.encode("utf-8") if isinstance(v, str) else v for v in values]
        ordered_set = ListBasedOrderedSet(encoded)
        lattice = OrderedSetLattice(ordered_set)
        return self.put(key, lattice)

    def get_single_causal(self, key):
        result = self.get([key])
        return result.get(key)

    def put_single_causal(self, key, value):
        vc = VectorClock({"test": 1}, True)
        val = SetLattice({value.encode() if isinstance(value, str) else value})
        lattice = SingleKeyCausalLattice(vc, val)
        return self.put(key, lattice)

    def get_priority(self, key):
        result = self.get([key])
        return result.get(key)

    def put_priority(self, key, priority, value):
        lattice = PriorityLattice(float(priority), value.encode()
                                  if isinstance(value, str) else value)
        return self.put(key, lattice)

    def get_bytes(self, key):
        """
        Performs a GET for the given key and returns the raw inner value bytes
        from the LWW wrapper, without lattice deserialization.

        This is used internally by metadata/stats helpers where the payload
        is a domain-specific protobuf rather than a lattice type.

        Returns None if the key does not exist or an error occurs.
        """
        for attempt in range(self._max_retries + 1):
            worker_address = self._get_worker_address(key)
            if not worker_address:
                return None

            send_sock = self.pusher_cache.get(worker_address)
            req, _ = self._prepare_data_request([key])
            req.type = GET

            send_request(req, send_sock)
            responses = recv_response([req.request_id], self.response_puller,
                                      KeyResponse)

            for response in responses:
                for tup in response.tuples:
                    if tup.invalidate:
                        self._invalidate_cache(tup.key)
                    if tup.error == WRONG_THREAD and attempt < self._max_retries:
                        if tup.key in self.address_cache:
                            self._invalidate_cache(tup.key)
                        break
                    if tup.error == NO_ERROR:
                        lww_val = LWWValue()
                        lww_val.ParseFromString(tup.payload)
                        return lww_val.value
                else:
                    # Inner loop completed without break (no WRONG_THREAD)
                    return None
                # WRONG_THREAD was encountered, continue outer retry loop
                continue

        return None

    def get_storage_stats(self, public_ip, private_ip, tid, tier):
        """
        Retrieves storage statistics for a server thread.

        Returns a dict with storage_consumption, occupancy, epoch,
        access_count, or None if the key does not exist.
        """
        key = (f"ANNA_METADATA|stats|{public_ip}|{private_ip}"
               f"|{tid}|{tier}")
        raw = self.get_bytes(key)
        if raw is None:
            return None

        stats = ServerThreadStatistics()
        stats.ParseFromString(raw)
        return {
            'storage_consumption': stats.storage_consumption,
            'occupancy': stats.occupancy,
            'epoch': stats.epoch,
            'access_count': stats.access_count,
        }

    def get_key_access_stats(self, public_ip, private_ip, tid, tier):
        """
        Retrieves per-key access frequency data for a server thread.

        Returns a list of dicts with key and access_count, or None if the
        key does not exist.
        """
        key = (f"ANNA_METADATA|access|{public_ip}|{private_ip}"
               f"|{tid}|{tier}")
        raw = self.get_bytes(key)
        if raw is None:
            return None

        data = KeyAccessData()
        data.ParseFromString(raw)
        return [{'key': kc.key, 'access_count': kc.access_count}
                for kc in data.keys]

    def get_key_size_stats(self, public_ip, private_ip, tid, tier):
        """
        Retrieves per-key size data for a server thread.

        Returns a list of dicts with key and size, or None if the key does
        not exist.
        """
        key = (f"ANNA_METADATA|size|{public_ip}|{private_ip}"
               f"|{tid}|{tier}")
        raw = self.get_bytes(key)
        if raw is None:
            return None

        data = KeySizeData()
        data.ParseFromString(raw)
        return [{'key': ks.key, 'size': ks.size}
                for ks in data.key_sizes]

    def put_replication_factor(self, key, memory_rep, local_rep):
        """
        Sets the replication factor for a key by writing a ReplicationFactor
        protobuf wrapped in an LWW value to the metadata key.

        Returns the result dict from put() (key -> bool).
        """
        rep = ReplicationFactor()
        rep.key = key

        # 'global' is a Python keyword, so we access the repeated field
        # via getattr.
        global_field = getattr(rep, 'global')

        mem_global = global_field.add()
        mem_global.tier = MEMORY
        mem_global.value = memory_rep

        disk_global = global_field.add()
        disk_global.tier = DISK
        disk_global.value = 0

        mem_local = rep.local.add()
        mem_local.tier = MEMORY
        mem_local.value = local_rep

        disk_local = rep.local.add()
        disk_local.tier = DISK
        disk_local.value = 0

        meta_key = f"ANNA_METADATA|replication|{key}"
        payload = rep.SerializeToString()

        ts = time.time_ns()
        val = LWWPairLattice(ts, payload)
        return self.put(meta_key, val)

    def get_cluster_topology(self):
        """
        Retrieves cluster topology (thread counts) from the metadata key
        ANNA_METADATA|cluster_topology.

        Returns a dict with routing_thread_count, memory_thread_count,
        ebs_thread_count, or None if the key does not exist.
        """
        raw = self.get_bytes("ANNA_METADATA|cluster_topology")
        if raw is None:
            return None

        topology = ClusterTopology()
        topology.ParseFromString(raw)
        return {
            'routing_thread_count': topology.routing_thread_count,
            'memory_thread_count': topology.memory_thread_count,
            'ebs_thread_count': topology.ebs_thread_count,
        }

    def get_monitoring_ips(self):
        """
        Retrieves monitoring node IP addresses from the metadata key
        ANNA_METADATA|monitoring_ips.

        Returns a list of IP address strings, or an empty list if the key
        does not exist.
        """
        raw = self.get_bytes("ANNA_METADATA|monitoring_ips")
        if raw is None:
            return []

        string_set = StringSet()
        string_set.ParseFromString(raw)
        return list(string_set.keys)

    # Returns and increments a request ID. Loops back after 10,000 requests.
    def _get_request_id(self):
        response = self.ut.get_ip() + ':' + str(self.rid)
        self.rid = (self.rid + 1) % 10000
        return response

    # Helper function to create a KeyRequest (see
    # hydro-project/common/lib.proto/anna.lib.proto). Takes in a key name and returns a
    # tuple containing a KeyRequest and a KeyTuple contained in that KeyRequest
    # with response_address, request_id, and address_cache_size automatically
    # populated.
    def _prepare_data_request(self, keys):
        req = KeyRequest()
        req.request_id = self._get_request_id()
        req.response_address = self.response_address

        tuples = []

        for key in keys:
            tup = req.tuples.add()
            tuples.append(tup)
            tup.key = key

            if self.address_cache and key in self.address_cache:
                tup.address_cache_size = len(self.address_cache[key])

        return req, tuples

    # Issues a synchronous query to the routing tier. Takes in a key and a
    # (randomly chosen) routing port to issue the request to. Returns a list of
    # addresses that the routing tier returned that correspond to the input
    # key.
    def _query_routing(self, key, port):
        key_request = KeyAddressRequest()

        key_request.response_address = self.ut.get_key_address_connect_addr()
        key_request.keys.append(key)
        key_request.request_id = self._get_request_id()

        dst_addr = 'tcp://' + self.elb_addr + ':' + str(port)
        send_sock = self.pusher_cache.get(dst_addr)

        send_request(key_request, send_sock)
        response = recv_response([key_request.request_id],
                                 self.key_address_puller,
                                 KeyAddressResponse)[0]

        if response.error != 0:
            return []

        result = []
        for t in response.addresses:
            if t.key == key:
                for a in t.ips:
                    result.append(a)

        return result

    @property
    def response_address(self):
        return self.ut.get_request_pull_connect_addr()
