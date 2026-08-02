# Multi-stage Dockerfile for the Anna KVS server.
#
# Builds the three server binaries (anna-kvs, anna-route, anna-monitor)
# in a build stage, then copies them into a minimal runtime image.
#
# Usage:
#   docker build -t anna .
#   docker run --rm -p 6000-6956:6000-6956 anna

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
    && rm -rf /var/lib/apt/lists/*

COPY server/ /src/server/

RUN mkdir -p /src/server/cpp/build \
    && cd /src/server/cpp/build \
    && cmake -G "Unix Makefiles" \
        -DCMAKE_BUILD_TYPE=Release \
        -DBUILD_TEST=OFF \
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
COPY --from=builder /src/server/cpp/build/target/kvs/anna-route /usr/local/bin/
COPY --from=builder /src/server/cpp/build/target/kvs/anna-monitor /usr/local/bin/

# Copy default config and entrypoint
COPY server/conf/anna-config.yml /etc/anna/anna-config.yml
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Data directory for disk-tier storage
RUN mkdir -p /data

# Anna uses ports 6000-6956 (see docs/ports.md)
EXPOSE 6000-6956

ENTRYPOINT ["docker-entrypoint.sh"]
