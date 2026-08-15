# syntax=docker/dockerfile:1

FROM rust:1.97-bookworm AS build

RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake clang perl \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
 && cargo build --release \
 && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

COPY --from=build /src/target/release/capstone-analyzer /usr/local/bin/capstone-analyzer

EXPOSE 8092

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -fsS "http://127.0.0.1:${ANALYZER_PORT:-8092}/healthz" >/dev/null

ENTRYPOINT ["/usr/local/bin/capstone-analyzer"]
