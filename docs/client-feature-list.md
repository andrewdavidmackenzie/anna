# Client Feature List

Features implemented per client. Each client wraps the Anna KVS protocol
(protobuf over ZeroMQ) with a language-native API.

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

## C++ Client (`clients/cpp`)

| Feature                    | Tested |
|----------------------------|--------|
| GET / PUT (LWW)            | Yes    |
| Address cache invalidation | Yes    |
| WRONG_THREAD auto-retry    | Yes    |
| Timeout (generate_bad_response) | Yes |

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
