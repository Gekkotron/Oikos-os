# syntax=docker/dockerfile:1.6
#
# Multi-stage build of the Oikos-os hub.
# Stage 1 compiles the Leptos web UI with Trunk and then the hub binary
# that embeds its `dist/` via rust-embed. Stage 2 ships only the binary
# on debian:bookworm-slim.

ARG RUST_IMAGE=rust:1-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

FROM ${RUST_IMAGE} AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        ca-certificates \
        cmake \
        clang \
        pkg-config \
        libssl-dev \
        curl \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk --locked --version "^0.21"

WORKDIR /src
COPY . .

RUN cd crates/oikos-web && trunk build --release
RUN cargo build --release -p oikos-os

FROM ${RUNTIME_IMAGE} AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        ca-certificates \
        tzdata \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/oikos-os /usr/local/bin/oikos-os

ENV OIKOS_HTTP__BIND=0.0.0.0:8081
EXPOSE 8081
ENTRYPOINT ["/usr/local/bin/oikos-os"]
CMD ["--config", "/etc/oikos/oikos.toml"]
