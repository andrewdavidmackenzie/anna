# The `anna` book

[README.md](README.md)

# Understanding Anna
- [Key Concepts](docs/concepts.md)
- [Architecture](docs/architecture.md)
- [Lattices and Consistency](docs/lattices.md)
- [Autoscaling and Policy Engine](docs/autoscaling.md)
- [Feature List](docs/feature-list.md)
  - [Client Feature List](docs/client-feature-list.md)
- [API Comparison vs. Other KV Stores](docs/api-comparison.md)

# Building and Running
- [Building anna](docs/building-anna.md)
- [Running anna](docs/running.md)
- [Configuration Reference](docs/config.md)
- [Port Layout](docs/ports.md)

# Clients
Anna has four client implementations (C++, Rust, Python, Go), each with a
library and CLI. All clients support the same operations (GET, PUT, GET_SET,
PUT_SET, GET_CAUSAL, PUT_CAUSAL) and are tested against the same shared
golden files.

