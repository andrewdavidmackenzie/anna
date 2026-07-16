# Anna

[![codecov](https://codecov.io/gh/hydro-project/anna/branch/master/graph/badge.svg)](https://codecov.io/gh/andrewdavidmackenzie/anna)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Anna is a low-latency, autoscaling key-value store developed in the [RISE Lab](https://rise.cs.berkeley.edu) at [UC Berkeley](https://berkeley.edu). 

## Design

The core design goal for Anna is to avoid expensive locking and lock-free atomic instructions, 
which have recently been [shown to be extremely inefficient](http://www.jmfaleiro.com/pubs/latch-free-cidr2017.pdf). 
Anna instead employs a wait-free, shared-nothing architecture, where each thread in the system is given a private memory 
buffer and is allowed to process requests unencumbered by coordination. To resolve potentially conflicting updates, 
Anna encapsulates all user data in [lattice](https://en.wikipedia.org/wiki/Lattice_(order)) data structures, which have 
associative, commutative, and idempotent merge functions. As a result, for workloads that can tolerate slightly stale 
data, Anna provides best-in-class performance.

For more details, see:

- [Key Concepts](docs/concepts.md) — actors, consistent hashing, replication, gossip, storage tiers
- [Architecture](docs/architecture.md) — system components, actor model, communication, fault tolerance
- [Lattices and Consistency](docs/lattices.md) — lattice types, consistency levels, comparisons with other systems
- [Autoscaling and Policy Engine](docs/autoscaling.md) — SLOs, elasticity, selective replication, tiering

## Research Papers

- [ICDE 2018](http://db.cs.berkeley.edu/jmh/papers/anna_ieee18.pdf) — "Anna: A KVS For Any Scale" — system design, coordination-free consistency, evaluation
- [VLDB 2019](http://www.vikrams.io/papers/anna-vldb19.pdf) — "Autoscaling Tiered Cloud Storage in Anna" — cloud-native design, policy engine, cost-performance evaluation

## Clients

Anna has four client implementations, each with a library and CLI:

| Client | Library | CLI Binary | Language |
|--------|---------|------------|----------|
| C++ | `anna-client-lib` | `anna-cli` | C++ |
| Rust | `annalib` | `anna` | Rust |
| Python | `anna` package | `anna-py` | Python |
| Go | `annalib` | `anna-go` | Go |

All clients support the same operations (GET, PUT, GET_SET, PUT_SET, GET_CAUSAL, PUT_CAUSAL)
and are tested against [shared golden files](tests/shared/cli/).

## Building

See detailed instructions in [building anna](docs/building-anna.md).

## Running Anna

See detailed instructions in [running anna](docs/running.md).

## More Information

* [Video of talk](https://www.youtube.com/watch?v=9qU1zO9wCNs&t=2036s)

## License

The Project is licensed under the [Apache v2 License](LICENSE).
