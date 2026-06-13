FROM rust:1.75-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/src/synapsis
COPY . .
RUN cargo build --release --bin synapsis-server --bin synapsis-mcp

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/synapsis/target/release/synapsis-server /usr/local/bin/synapsis
COPY --from=builder /usr/src/synapsis/target/release/synapsis-mcp /usr/local/bin/synapsis-mcp
RUN useradd -m -u 1000 synapsis
USER synapsis
WORKDIR /home/synapsis
EXPOSE 7438
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 CMD curl -f http://localhost:7438/ || exit 1
ENTRYPOINT ["synapsis"]
