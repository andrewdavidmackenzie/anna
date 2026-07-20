# Client Feature List

Features implemented per client. Each client wraps the Anna KVS protocol
(protobuf over ZeroMQ) with a language-native API.

## Value Change Subscription

All four clients provide a value change subscription API — a pub-sub
mechanism for receiving notifications when specific keys are updated
(including deletes). The subscriber registers interest in keys with the
KVS server threads via a dedicated registration port (7200). During each
gossip epoch, when a watched key changes, the KVS pushes the new value
to the subscriber on port 7150.

Applications can use this for caching, event-driven updates, replication
to external systems, or any pattern that needs to react to key changes.

### API (per language)

| Operation                  | Description                                     |
|----------------------------|-------------------------------------------------|
| Create subscriber          | Connect to the KVS and bind the update listener |
| `watch(keys)`              | Register interest in one or more keys           |
| `recv_update(timeout)`     | Block for next pushed update from gossip        |
| `get_cached(key)`          | Read the latest received value locally          |

This feature replaces the original Cloudburst management-node-based cache
registration with direct client-to-server registration, enabling
subscribers in standalone mode (`mgmt_ip: "NULL"`).

## Rust Client (`clients/rust`)

| Feature                    | Tested |
|----------------------------|--------|
| GET / PUT / DELETE (LWW)   | Yes    |
| GET_SET / PUT_SET           | Yes    |
| GET_ORDERED_SET / PUT_ORDERED_SET | Yes |
| GET_CAUSAL / PUT_CAUSAL    | Yes    |
| GET_SINGLE_CAUSAL / PUT_SINGLE_CAUSAL | Yes |
| GET_PRIORITY / PUT_PRIORITY | Yes   |
| Multi-key GET (get_multi)  | Yes    |
| Address cache invalidation | Yes    |
| WRONG_THREAD retry         | Yes    |
| Timeout retry              | Yes    |
| Dead-address eviction      | Yes    |
| Configurable timeout       | Yes    |
| Port base_offset support   | Yes    |
| Process management (start/stop/status) | Yes |
| Value change subscription (watch/recv/get_cached) | Yes |

## C++ Client (`clients/cpp`)

| Feature                    | Tested |
|----------------------------|--------|
| GET / PUT (LWW)            | Yes    |
| Address cache invalidation | Yes    |
| WRONG_THREAD auto-retry    | Yes    |
| Timeout (generate_bad_response) | Yes |
| Value change subscription (watch/recv/get_cached) | Yes |

## Go Client (`clients/go`)

| Feature                    | Tested |
|----------------------------|--------|
| GET / PUT / DELETE (LWW)   | Yes    |
| GET_SET / PUT_SET           | Yes    |
| GET_ORDERED_SET / PUT_ORDERED_SET | Yes |
| GET_SINGLE_CAUSAL / PUT_SINGLE_CAUSAL | Yes |
| GET_PRIORITY / PUT_PRIORITY | Yes   |
| Error code mapping         | Yes    |
| Timeout error code         | Yes    |
| Value change subscription (watch/recv/get_cached) | Yes |

## Python Client (`clients/python`)

| Feature                    | Tested |
|----------------------------|--------|
| GET / PUT / DELETE (LWW)   | Yes    |
| GET_SET / PUT_SET           | Yes    |
| GET_ORDERED_SET / PUT_ORDERED_SET | Yes |
| GET_SINGLE_CAUSAL / PUT_SINGLE_CAUSAL | Yes |
| GET_PRIORITY / PUT_PRIORITY | Yes   |
| Timeout (poll-based)       | Yes    |
| Process management (start/stop) | Yes |
| Value change subscription (watch/recv/get_cached) | Yes |
