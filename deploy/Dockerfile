# syntax=docker/dockerfile:1

FROM rust:1.97.1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang cmake libclang-dev libssl-dev libsqlite3-dev pkg-config protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --locked --release -p ghostline-cli -p ghostline-paid-api

FROM debian:bookworm-slim AS runtime
ARG OOTLE_WALLETD_VERSION=0.47.0
ARG OOTLE_WALLETD_COMMIT=9dabb2a
ARG OOTLE_WALLETD_SHA256=919579e691fa3ec4ddead30a40f96c67f1efa0a735ca77a17cc2dc53d611ba52
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl libdbus-1-3 libgcc-s1 libgcrypt20 liblz4-1 liblzma5 passwd \
    libssl3 libstdc++6 libsystemd0 libzstd1 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /home/ghostline ghostline
WORKDIR /app
COPY --from=build /src/target/release/ghostline /usr/local/bin/ghostline
COPY --from=build /src/target/release/ghostline-paid-api /usr/local/bin/ghostline-paid-api
RUN asset="tari_ootle_walletd-${OOTLE_WALLETD_VERSION}-${OOTLE_WALLETD_COMMIT}-linux-x86_64" \
    && curl --fail --location --silent --show-error \
      "https://github.com/tari-project/tari-ootle/releases/download/v${OOTLE_WALLETD_VERSION}/${asset}" \
      --output /usr/local/bin/tari_ootle_walletd \
    && printf '%s  %s\n' "$OOTLE_WALLETD_SHA256" /usr/local/bin/tari_ootle_walletd | sha256sum --check --status \
    && chmod 0755 /usr/local/bin/tari_ootle_walletd
COPY deploy/start-relay.sh /usr/local/bin/start-relay
RUN chmod 0755 /usr/local/bin/start-relay && mkdir -p /tmp/ghostline-ledger /tmp/tari-wallet \
    && chown -R ghostline:ghostline /tmp/ghostline-ledger /tmp/tari-wallet /home/ghostline
USER ghostline
ENV GHOSTLINE_LEDGER_PATH=/tmp/ghostline-ledger \
    TARI_BASE_DIR=/tmp/tari-wallet \
    GHOSTLINE_WALLET_URL=http://127.0.0.1:5100 \
    GHOSTLINE_CLI_BIN=/usr/local/bin/ghostline
EXPOSE 10000
ENTRYPOINT ["/usr/local/bin/start-relay"]
