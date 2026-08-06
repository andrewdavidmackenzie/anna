# API Comparison: Anna vs. Major KV Stores

## What Anna has that's unique

Anna's lattice type system (17 types with CRDT-style merge semantics) is genuinely distinctive. Among the stores compared in this document, Anna is the only one with built-in conflict-free replicated data types at the storage layer:

- **LWW, Set, OrderedSet, Priority, Causal variants, Counter, OR-Set** -- merge-able without coordination
- **Gossip-based replication with automatic conflict resolution** -- no other store in this list does this natively
- **Per-key replication factor control** -- DynamoDB has table-level, Cassandra has keyspace-level, but per-key is rare

## Feature comparison vs. competitors

| Capability | Anna | Redis | etcd | TiKV | DynamoDB | Cassandra | FoundationDB |
|---|---|---|---|---|---|---|---|
| **TTL/Expiration** | EXPIRE (per-key, server-enforced) | Per-key | Per-lease | RawKV only | Per-item | Per-value | No |
| **Key scan/list** | SCAN (prefix filter, paginated) | SCAN/KEYS | Range | scan() | Scan/Query | SELECT | get_range() |
| **Watch/subscribe** | SUBSCRIBE (gossip-based; CLI: Rust only, library: all 4) | Pub/Sub + Streams | Watch API | CDC | Streams | CDC | watch(key) |
| **Atomic CAS** | No | WATCH+MULTI | Txn if/then | CAS (RawKV) | ConditionExpression | LWT (IF) | Native (all txns) |
| **Batch read/write** | MGET + MSET | MGET/MSET, pipeline | Range + Txn | batch_get/batch_put/scan | BatchGetItem/BatchWriteItem | SELECT IN + BATCH | get_range + Txn |
| **Counters/Incr** | INCR/DECR (PN-Counter CRDT) | INCR/DECR | No | No | Atomic counters | Counter type | add() atomic |
| **OR-Set** | SADD/SREM/SMEMBERS (add-wins CRDT) | SADD/SREM | No | No | No | No | No |
| **Secondary indexes** | No | Sorted sets | No | No | GSI/LSI | Secondary indexes | Layer (manual) |
| **Range queries** | No | ZRANGEBYSCORE | Range | scan(start,end) | Query (sort key) | Clustering cols | get_range() |

## Anna's current API surface (4 clients: Rust, C++, Python, Go)

| Category | Operations |
|---|---|
| **Core KV** | GET, PUT, DEL — get, get_bytes, get_value (auto-detect type), put, put_value (type-tagged), delete |
| **Batch** | MGET, MSET — get_multi, put_multi (groups by worker for efficiency) |
| **17 lattice types** | LWW, Set, OrderedSet, SingleCausal, MultiCausal, Priority, LwwSet, LwwOrderedSet, UnionScalar, PrioritySet, PriorityOrderedSet, CausalSet, CausalOrderedSet, MultiCausalSet, MultiCausalOrderedSet, Counter, OrSet |
| **Transactions** | begin/put/get/commit/rollback (client-side buffering, read-committed + repeatable-read isolation) |
| **Watch/subscribe** | SUBSCRIBE — ValueChangeSubscriber: watch(keys), recv_update(timeout), get_cached(key) — library: all 4 clients; CLI: Rust only |
| **TTL/Expiration** | EXPIRE — put_with_ttl(key, value, ttl_seconds); per-key, server-enforced |
| **Counter** | INCR, DECR, GET_COUNTER — increment(key, amount), decrement(key, amount), get_counter(key) |
| **OR-Set** | SADD, SREM, SMEMBERS — or_set_add(key, element), or_set_remove(key, element), get_or_set(key) |
| **Cluster introspection** | get_cluster_topology, get_monitoring_ips, get_key_addresses |
| **Replication control** | put_replication_factor(key, memory_rep, local_rep) |
| **Stats/monitoring** | get_storage_stats, get_key_access_stats, get_key_size_stats, LatencyReporter |
| **Key scan/list** | SCAN — scan(prefix), fans out to all threads, cursor-paginated, returns key + type + size + expiry |

## Remaining gaps to close

All items from the original comparison have been implemented or classified as architecture mismatches (below). The remaining items (CAS, range queries, secondary indexes) are fundamental architecture mismatches that cannot be addressed without changing Anna's core design.

## Hardest to emulate (architecture mismatch)

- **Conditional writes / CAS**: Anna's eventual consistency model conflicts with strong CAS. The causal lattice types provide partial ordering but not compare-and-swap semantics.
- **Range queries**: Anna uses hash-based key distribution (consistent hashing). Range queries require sequential key layout, which would need a fundamentally different key distribution strategy.
- **Secondary indexes**: Would require a server-side index maintenance layer that doesn't exist.

## Which store's API is closest to Anna?

**Redis** is the closest match in spirit -- both are in-memory, both support multiple data types (sets, sorted sets), both have pub/sub. If Anna wanted to emulate one API, a subset of the Redis command protocol (RESP) would be the most natural fit:

| Redis command | Redis args | Anna CLI | Rust API | Notes |
|---|---|---|---|---|
| `GET` | `GET key` | `GET {key}` | `get(key)` / `get_value(key)` | Auto-detects lattice type |
| `SET` | `SET key value` | `PUT {key} {value}` | `put(key, value)` | LWW (last-writer-wins) |
| `DEL` | `DEL key [key...]` | `DEL {key}` | `delete(key)` | Single key per call |
| `MGET` | `MGET key [key...]` | `MGET {k1} {k2} ...` | `get_multi(keys)` | Returns all values |
| `MSET` | `MSET key value [key value...]` | `MSET {k1} {v1} {k2} {v2} ...` | `put_multi(pairs)` | Groups by worker for efficiency |
| `SADD` | `SADD key member [member...]` | `SADD {key} {m1} [m2 ...]` | `or_set_add(key, element)` | OR-Set with add-wins semantics |
| `SREM` | `SREM key member [member...]` | `SREM {key} {m1} [m2 ...]` | `or_set_remove(key, element)` | Tombstone-based removal |
| `SMEMBERS` | `SMEMBERS key` | `SMEMBERS {key}` | `get_or_set(key)` | Returns live elements (not tombstoned) |
| `INCR`/`INCRBY` | `INCR key` / `INCRBY key increment` | `INCR {key} [amount]` | `increment(key)` / `increment_by(key, n)` | PN-Counter CRDT |
| `DECR`/`DECRBY` | `DECR key` / `DECRBY key decrement` | `DECR {key} [amount]` | `decrement(key)` / `decrement_by(key, n)` | PN-Counter CRDT |
| (no equivalent) | — | `GET_COUNTER {key}` | `get_counter(key)` | Returns net counter value (incr - decr) |
| `SETEX` | `SETEX key seconds value` | `EXPIRE {key} {value} {seconds}` | `put_with_ttl(key, value, ttl)` | Server-enforced per-key TTL |
| `SUBSCRIBE` | `SUBSCRIBE channel [channel...]` | `SUBSCRIBE {key1} [key2 ...]` | `ValueChangeSubscriber::watch(keys)` | Gossip-based; CLI: Rust only, library: all 4 |
| `SCAN` | `SCAN cursor [MATCH pattern] [COUNT count]` | `SCAN [prefix]` | `scan(prefix)` | Fans out to all KVS threads |

**Differences from Redis**: Anna's `SADD`/`SREM` use OR-Set (tombstone-based CRDT) rather than Redis's simple in-memory set. Anna's `INCR`/`DECR` use a PN-Counter CRDT that converges across replicas. Anna's `SUBSCRIBE` is gossip-based (eventual delivery) rather than Redis's synchronous pub/sub. Anna's `SCAN` fans out to all KVS threads since keys are hash-distributed, while Redis scans a single hash table.
