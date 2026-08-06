# Multi-stage Dockerfile for the Anna KVS server.
#
# Builds the two server binaries (anna-kvs, anna-monitor)
# in a build stage, then copies them into a minimal runtime image.
#
# Usage:
#   docker build -t anna .
#   docker run --rm --network host anna  # Linux only (ZMQ binds 127.0.0.1)

# ---------------------------------------------------------------------------
# Build stage
# ---------------------------------------------------------------------------
FROM ubuntu:24.04 AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    pkg-config \
    protobuf-compiler \
    libprotobuf-dev \
    libzmq3-dev \
    cppzmq-dev \
    libspdlog-dev \
    libfmt-dev \
    libyaml-cpp-dev \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Rust (needed for anna-hashring shared hash library).
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain stable --no-modify-path \
    && chmod -R a+w $RUSTUP_HOME $CARGO_HOME

COPY Cargo.toml Cargo.lock /src/
COPY server/rust/ /src/server/rust/
COPY server/protobuf/ /src/server/protobuf/
COPY clients/rust/Cargo.toml /src/clients/rust/Cargo.toml
COPY clients/rust/build.rs /src/clients/rust/build.rs
# Create stub sources so workspace resolves without copying full client.
RUN mkdir -p /src/clients/rust/src/lib && \
    touch /src/clients/rust/src/lib/lib.rs && \
    mkdir -p /src/clients/rust/src && \
    echo 'fn main() {}' > /src/clients/rust/src/main.rs

# Build the Rust hash ring library (static .a used by C++ server).
RUN cd /src && cargo build --release -p anna-hashring

COPY server/cpp/ /src/server/cpp/

RUN mkdir -p /src/server/cpp/build \
    && cd /src/server/cpp/build \
    && cmake -G "Unix Makefiles" \
        -DCMAKE_BUILD_TYPE=Release \
        -DBUILD_TEST=OFF \
        -DANNA_HASHRING_LIB=/src/target/release/libanna_hashring.a \
        -DANNA_HASHRING_INCLUDE=/src/server/rust/anna-hashring \
        .. \
    && make -j$(nproc)

# ---------------------------------------------------------------------------
# Runtime stage
# ---------------------------------------------------------------------------
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    libprotobuf-dev \
    libzmq5 \
    libspdlog-dev \
    libfmt-dev \
    libyaml-cpp-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy server binaries
COPY --from=builder /src/server/cpp/build/target/kvs/anna-kvs /usr/local/bin/
COPY --from=builder /src/server/cpp/build/target/kvs/anna-monitor /usr/local/bin/

# Copy default config and entrypoint
COPY server/conf/anna-config.yml /etc/anna/anna-config.yml
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Run as non-root user
RUN useradd --system --create-home anna \
    && mkdir -p /data \
    && chown anna:anna /data
USER anna

# Anna uses ports 6000-6956 (see docs/ports.md)
EXPOSE 6000-6956

ENTRYPOINT ["docker-entrypoint.sh"]
