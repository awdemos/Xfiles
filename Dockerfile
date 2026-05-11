FROM rust:slim-bookworm AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/src/xfiles
COPY Cargo.toml Cargo.lock ./
COPY xfiles.toml.example ./
COPY src ./src
COPY examples ./examples
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /data && chmod 777 /data
WORKDIR /data
COPY --from=builder /usr/src/xfiles/target/release/xfiles /usr/local/bin/xfiles
EXPOSE 9999
ENTRYPOINT ["xfiles"]
CMD ["serve"]
