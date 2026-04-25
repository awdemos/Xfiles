FROM rust:1.82-slim-bookworm AS builder
WORKDIR /usr/src/xfiles
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/xfiles/target/release/xfiles /usr/local/bin/xfiles
EXPOSE 9999
ENTRYPOINT ["xfiles"]
CMD ["serve"]
