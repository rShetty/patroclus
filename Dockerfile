FROM rust:1.97 AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates && rm -rf /var/lib/apt/lists/*

RUN useradd --system --home-dir /app --create-home --shell /usr/sbin/nologin patroclus
WORKDIR /app
COPY --from=builder /app/target/release/patroclus /app/patroclus

EXPOSE 8484
VOLUME ["/app/data", "/app/keys"]

ENV PATROCLUS_DATABASE_PATH=/app/data/patroclus.db
ENV PATROCLUS_PRIVATE_KEY_PATH=/app/keys/private.pem
ENV PATROCLUS_PUBLIC_KEY_PATH=/app/keys/public.pem
ENV PATROCLUS_VAULT_KEY_PATH=/app/keys/vault.key

USER patroclus
ENTRYPOINT ["/app/patroclus"]
CMD ["serve", "--config", "/app/config.toml"]
