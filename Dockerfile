# Multi-stage build; fallback for anyone without Nix (§6). The Nix path
# (`nix build .#container`) is the preferred one but could not be verified
# in this sandbox -- see flake.nix and README.md "Status".
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs CHANGELOG.md ./
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/target/release/ripae /app/ripae
COPY static ./static
VOLUME ["/data"]
EXPOSE 8080
ENTRYPOINT ["/app/ripae"]
