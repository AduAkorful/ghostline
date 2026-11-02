# syntax=docker/dockerfile:1

FROM rust:1.97.1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang cmake libclang-dev libssl-dev libsqlite3-dev pkg-config protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --locked --release -p ghostline-cli -p ghostline-paid-api

FROM debian:bookworm-slim AS runtime
# Official v0.49.2 (aa7b0c8). v0.48.0, v0.49.0 and v0.49.1 published no working
# wallet binary. v0.49.2 includes those resets. The checksum is the upstream
# tari_ootle_walletd-0.49.2-aa7b0c8-linux-x86_64.sha256 asset.
ARG OOTLE_WALLETD_URL=https://github.com/tari-project/tari-ootle/releases/download/v0.49.2/tari_ootle_walletd-0.49.2-aa7b0c8-linux-x86_64
ARG OOTLE_WALLETD_SHA256=57e753dd30f36405a0e092edbe65abd800d9e9d0718579225975b0969277914c
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl libdbus-1-3 libgcc-s1 libgcrypt20 liblz4-1 liblzma5 passwd \
    libssl3 libstdc++6 libsystemd0 libzstd1 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /home/ghostline ghostline
WORKDIR /app
COPY --from=build /src/target/release/ghostline /usr/local/bin/ghostline
COPY --from=build /src/target/release/ghostline-paid-api /usr/local/bin/ghostline-paid-api
RUN curl --fail --location --silent --show-error "$OOTLE_WALLETD_URL" \
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
