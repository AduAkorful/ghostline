#!/bin/sh
set -eu
set +x

: "${GHOSTLINE_WALLET_SEED_WORDS:?set the dedicated Esmeralda relay wallet seed in Render secrets}"
: "${GHOSTLINE_WALLET_PASSWORD:?set a wallet database password in Render secrets}"
: "${GHOSTLINE_CLAIM_SECRET:?set the relay wallet view_only_key index 0 in Render secrets}"
: "${GHOSTLINE_TEMPLATE:?set the deployed Ghostline template address}"
: "${GHOSTLINE_INDEXER_URL:?set the Esmeralda indexer URL}"
: "${GHOSTLINE_DEMO_EXPIRY_EPOCHS:?set the demo channel lifetime in epochs}"
: "${GHOSTLINE_DEMO_OPENER_ACCOUNT:?set the deterministic opener account name}"
: "${GHOSTLINE_DEMO_FEE_ACCOUNT_NAME:?set the deterministic fee account name}"
: "${GHOSTLINE_LEDGER_PATH:?set the private relay state directory}"

# Render's bulk .env import strips these quotes, but copying the line into a single
# value field can preserve them. Normalize harmless whitespace either way; never log the
# resulting phrase. This also turns a wrapped paste into the one-line value walletd expects.
wallet_seed_words="$GHOSTLINE_WALLET_SEED_WORDS"
case "$wallet_seed_words" in
    \"*\") wallet_seed_words=${wallet_seed_words#\"}; wallet_seed_words=${wallet_seed_words%\"} ;;
    \'*\') wallet_seed_words=${wallet_seed_words#\'}; wallet_seed_words=${wallet_seed_words%\'} ;;
esac
wallet_seed_words="$(printf '%s' "$wallet_seed_words" | tr '\r\n\t' '   ' | awk '{$1=$1; print}')"
if ! printf '%s\n' "$wallet_seed_words" | awk '
    NR != 1 || NF != 24 { exit 1 }
    { for (i = 1; i <= NF; i++) if ($i !~ /^[a-z][a-z]*$/) exit 1 }
    END { if (NR != 1 || NF != 24) exit 1 }
'; then
    echo 'GHOSTLINE_WALLET_SEED_WORDS must be 24 lowercase English words; paste one line or import the .env file' >&2
    exit 1
fi

export GHOSTLINE_DEMO_MODE=1
export GHOSTLINE_NETWORK=esmeralda
export GHOSTLINE_NETWORK_TAG="${GHOSTLINE_NETWORK_TAG:-38}"
export GHOSTLINE_DEMO_DEPOSIT="${GHOSTLINE_DEMO_DEPOSIT:-51}"
export GHOSTLINE_DEMO_OPENER_KEY_INDEX="${GHOSTLINE_DEMO_OPENER_KEY_INDEX:-2}"
export GHOSTLINE_DEMO_FEE_KEY_INDEX="${GHOSTLINE_DEMO_FEE_KEY_INDEX:-3}"
export PORT="${PORT:-10000}"
recovery_retry_delay="${GHOSTLINE_WALLET_RECOVERY_RETRY_DELAY_SECS:-30}"
case "$recovery_retry_delay" in
    ''|*[!0-9]*)
        echo 'GHOSTLINE_WALLET_RECOVERY_RETRY_DELAY_SECS must be a nonnegative integer' >&2
        exit 1
        ;;
esac

wallet_bin="${TARI_WALLETD_BIN:-/usr/local/bin/tari_ootle_walletd}"
cli_bin="${GHOSTLINE_CLI_BIN:-/usr/local/bin/ghostline}"
api_bin="${GHOSTLINE_PAID_API_BIN:-/usr/local/bin/ghostline-paid-api}"
wallet_base="${TARI_BASE_DIR:-/tmp/tari-wallet}"
wallet_db="$wallet_base/esmeralda"
wallet_seed_marker="$wallet_base/.ghostline-seed-initialized"
wallet_indexers="${GHOSTLINE_WALLET_INDEXER_URLS:-}"
mkdir -p "$wallet_base" "$GHOSTLINE_LEDGER_PATH"

# `None` auth is limited to loopback inside this single container. Never expose this port.
# The wallet password protects its local database; the relay connects only over loopback.
# Do not infer successful initialization from the database directory existing: walletd can
# create it before rejecting a malformed seed, and Render may restart the process with the
# same /tmp contents. Skipping the seed in that case silently creates a different wallet.
if [ -f "$wallet_seed_marker" ]; then
    if [ ! -d "$wallet_db" ]; then
        echo 'wallet initialization marker exists but the wallet database is missing; refusing to create a different wallet' >&2
        exit 1
    fi
    set --
elif [ -e "$wallet_db" ]; then
    echo 'wallet database exists without a successful seed-initialization marker; refusing to use or replace it' >&2
    exit 1
else
    # The seed phrase is passed as one argument because walletd exposes it only as a CLI option.
    set -- --seed-words "$wallet_seed_words"
fi
# GHOSTLINE_WALLET_INDEXER_URLS is optional. Without it, walletd keeps its
# network-provided indexer set (including its built-in fallback endpoints).
if [ -n "$wallet_indexers" ]; then
    # The daemon accepts a comma-separated URL list as one option value.
    "$wallet_bin" --network esmeralda --indexer-api-url "$wallet_indexers" \
        --base-path "$wallet_base" \
        --listen-on 127.0.0.1:5100 --authentication none \
        --override-keyring-password "$GHOSTLINE_WALLET_PASSWORD" "$@" run &
else
    "$wallet_bin" --network esmeralda --base-path "$wallet_base" \
    --listen-on 127.0.0.1:5100 --authentication none \
    --override-keyring-password "$GHOSTLINE_WALLET_PASSWORD" "$@" run &
fi
wallet_pid=$!
cleanup() {
    kill -TERM "$wallet_pid" 2>/dev/null || true
    wait "$wallet_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

mark_seed_initialized() {
    [ -f "$wallet_seed_marker" ] && return 0
    marker_tmp="$wallet_seed_marker.tmp.$$"
    (umask 077 && : > "$marker_tmp")
    mv -f "$marker_tmp" "$wallet_seed_marker"
}

account_ready() {
    "$cli_bin" wallet account-get --url "$GHOSTLINE_WALLET_URL" \
        --name "$1" >/dev/null 2>&1
}
account_address() {
    "$cli_bin" wallet account-get --url "$GHOSTLINE_WALLET_URL" \
        --name "$1" | sed -n 's/^address=/account_address=/p'
}
account_create() {
    if [ "${3:-}" = yes ]; then
        "$cli_bin" wallet account-create --url "$GHOSTLINE_WALLET_URL" \
            --name "$1" --key-index "$2" --is-default >/dev/null
    else
        "$cli_bin" wallet account-create --url "$GHOSTLINE_WALLET_URL" \
            --name "$1" --key-index "$2" >/dev/null
    fi
}

account_create_or_retry() {
    account_error_file="$wallet_base/.ghostline-account-create-error.$$"
    if account_create "$@" 2>"$account_error_file"; then
        rm -f "$account_error_file"
        return 0
    fi
    if grep -Fq 'the wallet is still recovering accounts from its seed' "$account_error_file"; then
        # This response proves walletd accepted the seed and opened this database.
        # Recovery will be retried by walletd on its next start; preserve the database
        # marker and let Render restart after a brief delay instead of flooding logs.
        mark_seed_initialized
        rm -f "$account_error_file"
        echo 'wallet seed is initialized; account recovery is pending, preserving the wallet database for retry' >&2
        sleep "$recovery_retry_delay"
        exit 1
    fi
    rm -f "$account_error_file"
    return 1
}

attempt=0
while [ "$attempt" -lt 90 ]; do
    if account_ready "$GHOSTLINE_DEMO_OPENER_ACCOUNT"; then
        break
    fi
    if account_create_or_retry "$GHOSTLINE_DEMO_OPENER_ACCOUNT" "$GHOSTLINE_DEMO_OPENER_KEY_INDEX" yes; then
        break
    fi
    if ! kill -0 "$wallet_pid" 2>/dev/null; then
        echo 'wallet daemon exited before the opener account became available' >&2
        exit 1
    fi
    attempt=$((attempt + 1))
    sleep 2
done
account_ready "$GHOSTLINE_DEMO_OPENER_ACCOUNT" || {
    echo 'wallet daemon did not prepare the opener account' >&2
    exit 1
}

mark_seed_initialized

attempt=0
while [ "$attempt" -lt 90 ]; do
    if account_ready "$GHOSTLINE_DEMO_FEE_ACCOUNT_NAME"; then
        break
    fi
    if account_create_or_retry "$GHOSTLINE_DEMO_FEE_ACCOUNT_NAME" "$GHOSTLINE_DEMO_FEE_KEY_INDEX"; then
        break
    fi
    if ! kill -0 "$wallet_pid" 2>/dev/null; then
        echo 'wallet daemon exited before the fee account became available' >&2
        exit 1
    fi
    attempt=$((attempt + 1))
    sleep 2
done
account_ready "$GHOSTLINE_DEMO_FEE_ACCOUNT_NAME" || {
    echo 'wallet daemon did not prepare the fee account' >&2
    exit 1
}

# Public receive address only; key IDs and private key material stay hidden.
fee_address="$(account_address "$GHOSTLINE_DEMO_FEE_ACCOUNT_NAME")"
if [ -z "$fee_address" ]; then
    echo 'wallet account-get did not return a public fee-account address' >&2
    exit 1
fi
printf '%s\n' "$fee_address"

"$api_bin" relay --bind "0.0.0.0:$PORT" \
    --channel-dir "$GHOSTLINE_LEDGER_PATH/channels" &
api_pid=$!
stop_children() {
    kill -TERM "$api_pid" 2>/dev/null || true
    kill -TERM "$wallet_pid" 2>/dev/null || true
}
trap stop_children INT TERM
wait "$api_pid"
