# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS builder

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*

COPY . .

ARG PACKAGE=agentflow-server
ARG BIN=agentflow-server

RUN cargo build --release -p "${PACKAGE}" --bin "${BIN}"

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl libssl3 \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --create-home --uid 10001 --shell /usr/sbin/nologin agentflow

ARG BIN=agentflow-server

COPY --from=builder /app/target/release/${BIN} /usr/local/bin/agentflow

USER agentflow
ENV PORT=3000
ENV RUST_LOG=info
EXPOSE 3000

ENTRYPOINT ["/usr/local/bin/agentflow"]
