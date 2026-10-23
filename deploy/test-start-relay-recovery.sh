#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

write_fake_cli() {
    cat > "$tmp_root/ghostline" <<'FAKE'
#!/bin/sh
operation=$2
case "$2" in
    account-get)
        name=
        while [ "$#" -gt 0 ]; do
            case "$1" in
                --name) name=$2; shift 2 ;;
                *) shift ;;
            esac
        done
        if [ -n "$name" ] && [ -f "$FAKE_ACCOUNT_DIR/$name" ]; then
            printf 'address=%s\n' "$name"
            if [ -n "${FAKE_COMPONENT_ADDRESS:-}" ]; then
                printf 'component_address=%s\n' "$FAKE_COMPONENT_ADDRESS"
            fi
            exit 0
        fi
        exit 1
        ;;
    faucet-claim)
        name=
        max_fee=
        while [ "$#" -gt 0 ]; do
            case "$1" in
                --name) name=$2; shift 2 ;;
                --max-fee) max_fee=$2; shift 2 ;;
                *) shift ;;
            esac
        done
        if [ "${FAKE_FAUCET_MODE:-ok}" = transport-once ]; then
            count=0
            [ ! -f "$FAKE_FAUCET_ATTEMPTS" ] || count=$(cat "$FAKE_FAUCET_ATTEMPTS")
            count=$((count + 1))
            printf '%s\n' "$count" > "$FAKE_FAUCET_ATTEMPTS"
            if [ "$count" -eq 1 ]; then
                echo "indexer request failed: error sending request for url (http://127.0.0.1:5100/json_rpc)" >&2
                exit 1
            fi
        fi
        printf '%s %s\n' "$name" "$max_fee" >> "$FAKE_FAUCET_CLAIMS"
        printf 'transaction_id=abc\nfee=%s\namount=1000000000\nresult=Accept\n' "$max_fee"
        exit 0
        ;;
    account-create|account-ensure)
        name=
        key_index=0
        while [ "$#" -gt 0 ]; do
            case "$1" in
                --name) name=$2; shift 2 ;;
                --key-index) key_index=$2; shift 2 ;;
                *) shift ;;
            esac
        done
        case "${FAKE_CREATE_MODE:-recovering-once}" in
            recovered-relay-indexes)
                # Render's seed recovery now restores the relay's own indexes 2/3.
                # Recreating them fails regardless of their recovered display names.
                if [ "$operation" = account-create ]; then
                    echo "Store error: UNIQUE constraint failed: accounts.owner_public_key" >&2
                    exit 1
                fi
                printf '%s %s\n' "$name" "$key_index" >> "$FAKE_ACCOUNT_CREATES"
                : > "$FAKE_ACCOUNT_DIR/$name"
                exit 0
                ;;
            recovering)
                echo "wallet daemon RPC accounts.create failed: Invalid request: the wallet is still recovering accounts from its seed" >&2
                exit 1
                ;;
            recovering-once)
                count=0
                [ ! -f "$FAKE_RECOVERY_ATTEMPTS" ] || count=$(cat "$FAKE_RECOVERY_ATTEMPTS")
                count=$((count + 1))
                printf '%s\n' "$count" > "$FAKE_RECOVERY_ATTEMPTS"
                if [ "$count" -eq 1 ]; then
                    echo "wallet daemon RPC accounts.create failed: Invalid request: the wallet is still recovering accounts from its seed" >&2
                    exit 1
                fi
                if [ "$key_index" -lt 2 ]; then
                    echo "Store error: UNIQUE constraint failed: accounts.owner_public_key" >&2
                    exit 1
                fi
                printf '%s %s\n' "$name" "$key_index" >> "$FAKE_ACCOUNT_CREATES"
                : > "$FAKE_ACCOUNT_DIR/$name"
                exit 0
                ;;
            wallet-rpc-transport-once)
                count=0
                [ ! -f "$FAKE_RECOVERY_ATTEMPTS" ] || count=$(cat "$FAKE_RECOVERY_ATTEMPTS")
                count=$((count + 1))
                printf '%s\n' "$count" > "$FAKE_RECOVERY_ATTEMPTS"
                if [ "$count" -eq 1 ]; then
                    echo "indexer request failed: error sending request for url (http://127.0.0.1:5100/json_rpc)" >&2
                    exit 1
                fi
                printf '%s %s\n' "$name" "$key_index" >> "$FAKE_ACCOUNT_CREATES"
                : > "$FAKE_ACCOUNT_DIR/$name"
                exit 0
                ;;
            recovered)
                # The seed scan already restored accounts at indexes 0 and 1.
                if [ "$key_index" -lt 2 ]; then
                    echo "Store error: UNIQUE constraint failed: accounts.owner_public_key" >&2
                    exit 1
                fi
                printf '%s %s\n' "$name" "$key_index" >> "$FAKE_ACCOUNT_CREATES"
                : > "$FAKE_ACCOUNT_DIR/$name"
                exit 0
                ;;
            *)
                echo "wallet daemon RPC accounts.create failed: could not connect" >&2
                exit 1
                ;;
        esac
        ;;
esac
exit 2
FAKE
    chmod +x "$tmp_root/ghostline"
}

write_fake_walletd() {
    cat > "$tmp_root/walletd" <<'FAKE'
#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_WALLETD_ARGS"
mkdir -p "$TARI_BASE_DIR/esmeralda"
if [ "${FAKE_WALLETD_MODE:-running}" = running ]; then
    exec sleep 120
fi
exit 1
FAKE
    chmod +x "$tmp_root/walletd"
}

write_fake_api() {
    cat > "$tmp_root/paid-api" <<'FAKE'
#!/bin/sh
if [ -f "$GHOSTLINE_DEMO_READY_FILE" ]; then
    printf 'ready-at-start\n' > "$FAKE_API_START_STATE"
else
    printf 'not-ready-at-start\n' > "$FAKE_API_START_STATE"
fi
while [ ! -f "$GHOSTLINE_DEMO_READY_FILE" ]; do
    sleep 0.05
done
exit 0
FAKE
    chmod +x "$tmp_root/paid-api"
}

write_fake_cli
write_fake_walletd
write_fake_api

export GHOSTLINE_WALLET_SEED_WORDS="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon"
export GHOSTLINE_WALLET_PASSWORD=test-password
export GHOSTLINE_CLAIM_SECRET="$(printf 'a%.0s' {1..64})"
export GHOSTLINE_TEMPLATE=template_test
export GHOSTLINE_INDEXER_URL=https://indexer.example.test/
export GHOSTLINE_WALLET_INDEXER_URLS=https://indexer-a.example.test/,https://indexer-b.example.test/
export GHOSTLINE_DEMO_EXPIRY_EPOCHS=10
export GHOSTLINE_DEMO_OPENER_ACCOUNT=relay-opener
export GHOSTLINE_DEMO_FEE_ACCOUNT_NAME=relay-fees
export GHOSTLINE_LEDGER_PATH="$tmp_root/ledger"
export GHOSTLINE_WALLET_URL=http://127.0.0.1:5100
export TARI_BASE_DIR="$tmp_root/wallet"
export TARI_WALLETD_BIN="$tmp_root/walletd"
export GHOSTLINE_CLI_BIN="$tmp_root/ghostline"
export GHOSTLINE_PAID_API_BIN="$tmp_root/paid-api"
export GHOSTLINE_DEMO_READY_FILE="$tmp_root/demo-ready"
export GHOSTLINE_WALLET_RECOVERY_RETRY_DELAY_SECS=0
export FAKE_WALLETD_ARGS="$tmp_root/walletd-args"
export FAKE_ACCOUNT_DIR="$tmp_root/accounts"
export FAKE_ACCOUNT_CREATES="$tmp_root/account-creates"
export FAKE_RECOVERY_ATTEMPTS="$tmp_root/recovery-attempts"
export FAKE_API_START_STATE="$tmp_root/api-start-state"
mkdir -p "$FAKE_ACCOUNT_DIR"
export FAKE_WALLETD_MODE=running
export FAKE_CREATE_MODE=recovering-once

set +e
bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/recovery.stdout" 2>"$tmp_root/recovery.stderr"
recovery_status=$?
set -e

if [ "$recovery_status" -ne 0 ]; then
    echo "expected startup to wait for wallet recovery and then finish" >&2
    cat "$tmp_root/recovery.stderr" >&2
    exit 1
fi
if [ ! -f "$TARI_BASE_DIR/.ghostline-seed-initialized" ]; then
    echo "expected recovery-pending response to preserve the initialized wallet marker" >&2
    exit 1
fi
if ! grep -Fq -- "--indexer-api-url $GHOSTLINE_WALLET_INDEXER_URLS" "$FAKE_WALLETD_ARGS"; then
    echo "walletd did not receive the optional indexer URL list" >&2
    exit 1
fi
if ! grep -Fq 'waiting for wallet seed recovery/RPC readiness' "$tmp_root/recovery.stderr"; then
    echo "expected a concise recovery-pending diagnostic" >&2
    exit 1
fi
if ! grep -Fxq 'not-ready-at-start' "$FAKE_API_START_STATE"; then
    echo "expected the HTTP server to start before wallet recovery completes" >&2
    exit 1
fi
if [ ! -f "$GHOSTLINE_DEMO_READY_FILE" ]; then
    echo "expected startup to mark the service ready after both accounts exist" >&2
    exit 1
fi
if [ "$(cat "$FAKE_RECOVERY_ATTEMPTS")" -lt 2 ]; then
    echo "expected account creation to retry after wallet recovery" >&2
    exit 1
fi
if ! grep -Fxq 'relay-opener 2' "$FAKE_ACCOUNT_CREATES" || ! grep -Fxq 'relay-fees 3' "$FAKE_ACCOUNT_CREATES"; then
    echo "expected both demo accounts to be created after recovery" >&2
    exit 1
fi

# A wallet RPC transport failure while walletd is starting is also transient.
rm -rf "$TARI_BASE_DIR" "$FAKE_ACCOUNT_DIR"
mkdir -p "$FAKE_ACCOUNT_DIR"
rm -f "$FAKE_RECOVERY_ATTEMPTS"
export FAKE_CREATE_MODE=wallet-rpc-transport-once
set +e
bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/transport.stdout" 2>"$tmp_root/transport.stderr"
transport_status=$?
set -e
if [ "$transport_status" -ne 0 ]; then
    echo "expected startup to retry a wallet RPC transport failure" >&2
    cat "$tmp_root/transport.stderr" >&2
    exit 1
fi
if [ "$(cat "$FAKE_RECOVERY_ATTEMPTS")" -lt 2 ]; then
    echo "expected account creation to retry after a wallet RPC transport failure" >&2
    exit 1
fi
if [ ! -f "$TARI_BASE_DIR/.ghostline-seed-initialized" ]; then
    echo "expected a successful wallet RPC response to mark the seed initialized" >&2
    exit 1
fi

# A later process start must reuse the accepted wallet database without replaying
# seed words. With no explicit override, walletd's built-in indexer set remains
# in effect.
unset GHOSTLINE_WALLET_INDEXER_URLS
export FAKE_CREATE_MODE=recovered
set +e
bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/restart.stdout" 2>"$tmp_root/restart.stderr"
restart_status=$?
set -e
if [ "$restart_status" -ne 0 ]; then
    echo "expected restarted process to reuse the initialized wallet" >&2
    cat "$tmp_root/restart.stderr" >&2
    exit 1
fi
if tail -n 1 "$FAKE_WALLETD_ARGS" | grep -Fq -- '--seed-words'; then
    echo "restart must not pass the seed again to the existing wallet database" >&2
    exit 1
fi
if ! tail -n 1 "$FAKE_WALLETD_ARGS" | grep -Fq -- '--indexer-api-url https://ootle-indexer-a.tari.com/'; then
    echo "restart should pass the default Esmeralda indexer-a URL when no override is configured" >&2
    exit 1
fi

# An invalid-seed start exits before wallet RPC is available. It must not mark the
# partially created database as initialized.
rm -rf "$TARI_BASE_DIR"
rm -rf "$FAKE_ACCOUNT_DIR"
mkdir -p "$FAKE_ACCOUNT_DIR"
export FAKE_WALLETD_MODE=exit
export FAKE_CREATE_MODE=connection
set +e
bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/invalid.stdout" 2>"$tmp_root/invalid.stderr"
invalid_status=$?
set -e
if [ "$invalid_status" -eq 0 ]; then
    echo "expected invalid-seed startup to fail" >&2
    exit 1
fi
if [ -f "$TARI_BASE_DIR/.ghostline-seed-initialized" ]; then
    echo "invalid-seed startup must not create the initialized wallet marker" >&2
    exit 1
fi

# The Render wallet's successful seed scan recovered two existing account
# owners. Startup must choose unused key indexes instead of recreating either.
rm -rf "$TARI_BASE_DIR" "$FAKE_ACCOUNT_DIR"
mkdir -p "$FAKE_ACCOUNT_DIR"
unset GHOSTLINE_DEMO_OPENER_KEY_INDEX GHOSTLINE_DEMO_FEE_KEY_INDEX
export FAKE_WALLETD_MODE=running
export FAKE_CREATE_MODE=recovered
set +e
bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/recovered.stdout" 2>"$tmp_root/recovered.stderr"
recovered_status=$?
set -e
if [ "$recovered_status" -ne 0 ]; then
    echo "expected startup to reuse the recovered wallet and create relay accounts" >&2
    cat "$tmp_root/recovered.stderr" >&2
    exit 1
fi
if ! grep -Fxq 'relay-opener 2' "$FAKE_ACCOUNT_CREATES"; then
    echo "expected opener account to use unused key index 2" >&2
    exit 1
fi
if ! grep -Fxq 'relay-fees 3' "$FAKE_ACCOUNT_CREATES"; then
    echo "expected fee account to use unused key index 3" >&2
    exit 1
fi

# A new deployment restores the relay's own funded accounts under recovered names.
# It must reuse the same keys, rather than moving to yet another unfunded index.
rm -rf "$TARI_BASE_DIR" "$FAKE_ACCOUNT_DIR"
mkdir -p "$FAKE_ACCOUNT_DIR"
export FAKE_CREATE_MODE=recovered-relay-indexes
set +e
bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/relay-indexes.stdout" 2>"$tmp_root/relay-indexes.stderr"
relay_indexes_status=$?
set -e
if [ "$relay_indexes_status" -ne 0 ]; then
    echo "expected startup to reuse relay keys already restored under recovered account names" >&2
    cat "$tmp_root/relay-indexes.stderr" >&2
    exit 1
fi

write_fake_curl() {
    cat > "$tmp_root/curl" <<'FAKE'
#!/bin/sh
output=
url=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) output=$2; shift 2 ;;
        --write-out|--max-time) shift 2 ;;
        --silent|--show-error) shift ;;
        *) url=$1; shift ;;
    esac
done
printf '%s\n' "$url" >> "$FAKE_CURL_URLS"
mode=${FAKE_INDEXER_MODE:-not-found}
if [ "$mode" = not-found ] && [ -s "$FAKE_FAUCET_CLAIMS" ]; then
    mode=up
fi
case "$mode" in
    up)
        printf '%s\n' '{"substate":"up"}' > "$output"
        printf '200'
        ;;
    down)
        printf '%s\n' '{"error":"Input substate (v0) is down"}' > "$output"
        printf '404'
        ;;
    *)
        printf '%s\n' '{"error":"Substate not found"}' > "$output"
        printf '404'
        ;;
esac
FAKE
    chmod +x "$tmp_root/curl"
}

# A reported fee component that the indexer has never seen is published once.
# An already-up component is left alone. A down component is not claimed again.
write_fake_curl
export FAKE_CURL_URLS="$tmp_root/curl-urls"
export FAKE_FAUCET_CLAIMS="$tmp_root/faucet-claims"
export FAKE_COMPONENT_ADDRESS=component_7d3d8a5535d6087e3f27eb65c844dc3bb84fa8c7e67219af57361c7a1771e64f
export GHOSTLINE_FEE_COMPONENT_POLL_DELAY_SECS=0
export GHOSTLINE_FEE_COMPONENT_POLL_LIMIT=3
export FAKE_INDEXER_MODE=not-found
rm -rf "$TARI_BASE_DIR" "$FAKE_ACCOUNT_DIR"
mkdir -p "$FAKE_ACCOUNT_DIR"
rm -f "$FAKE_FAUCET_CLAIMS" "$FAKE_CURL_URLS" "$GHOSTLINE_DEMO_READY_FILE"
export FAKE_WALLETD_MODE=running
export FAKE_CREATE_MODE=recovered
set +e
PATH="$tmp_root:$PATH" bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/faucet.stdout" 2>"$tmp_root/faucet.stderr"
faucet_status=$?
set -e
if [ "$faucet_status" -ne 0 ]; then
    echo "expected startup to claim the faucet when the fee component is missing" >&2
    cat "$tmp_root/faucet.stderr" >&2
    exit 1
fi
if ! grep -Fxq 'relay-fees 100000' "$FAKE_FAUCET_CLAIMS"; then
    echo "expected one faucet claim for the fee account at the reviewed fee cap" >&2
    cat "$FAKE_FAUCET_CLAIMS" >&2
    exit 1
fi
if [ "$(wc -l < "$FAKE_FAUCET_CLAIMS")" -ne 1 ]; then
    echo "expected the faucet to be claimed once" >&2
    exit 1
fi
if ! grep -Fq "/substates/$FAKE_COMPONENT_ADDRESS" "$FAKE_CURL_URLS"; then
    echo "expected an indexer substate check for the fee component" >&2
    exit 1
fi
if ! grep -Fq 'claiming the Esmeralda faucet' "$tmp_root/faucet.stderr"; then
    echo "expected startup to say it is claiming the faucet" >&2
    exit 1
fi
if [ ! -f "$GHOSTLINE_DEMO_READY_FILE" ]; then
    echo "expected the relay to become ready after the fee component is up" >&2
    exit 1
fi

rm -f "$FAKE_FAUCET_CLAIMS" "$FAKE_CURL_URLS" "$GHOSTLINE_DEMO_READY_FILE"
export FAKE_INDEXER_MODE=up
set +e
PATH="$tmp_root:$PATH" bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/faucet-up.stdout" 2>"$tmp_root/faucet-up.stderr"
faucet_up_status=$?
set -e
if [ "$faucet_up_status" -ne 0 ]; then
    echo "expected startup to skip the faucet when the fee component is already up" >&2
    cat "$tmp_root/faucet-up.stderr" >&2
    exit 1
fi
if [ -s "$FAKE_FAUCET_CLAIMS" ]; then
    echo "expected no faucet claim when the fee component is already up" >&2
    exit 1
fi

rm -f "$FAKE_FAUCET_CLAIMS" "$FAKE_CURL_URLS" "$GHOSTLINE_DEMO_READY_FILE"
export FAKE_INDEXER_MODE=down
set +e
PATH="$tmp_root:$PATH" bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/faucet-down.stdout" 2>"$tmp_root/faucet-down.stderr"
faucet_down_status=$?
set -e
if [ "$faucet_down_status" -eq 0 ]; then
    echo "expected startup to stop when the fee component is down" >&2
    exit 1
fi
if [ -s "$FAKE_FAUCET_CLAIMS" ]; then
    echo "expected no faucet claim when the fee component is down" >&2
    exit 1
fi
if [ -f "$GHOSTLINE_DEMO_READY_FILE" ]; then
    echo "expected the relay to stay unready when the fee component is down" >&2
    exit 1
fi

# The live claim died on one loopback send failure while walletd was still up.
# Startup must retry that, then publish once.
rm -f "$FAKE_FAUCET_CLAIMS" "$FAKE_CURL_URLS" "$GHOSTLINE_DEMO_READY_FILE" "$tmp_root/faucet-attempts"
export FAKE_FAUCET_ATTEMPTS="$tmp_root/faucet-attempts"
export FAKE_FAUCET_MODE=transport-once
export FAKE_INDEXER_MODE=not-found
set +e
PATH="$tmp_root:$PATH" bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/faucet-retry.stdout" 2>"$tmp_root/faucet-retry.stderr"
faucet_retry_status=$?
set -e
if [ "$faucet_retry_status" -ne 0 ]; then
    echo "expected startup to retry a faucet claim that could not reach the wallet" >&2
    cat "$tmp_root/faucet-retry.stderr" >&2
    exit 1
fi
if ! grep -Fq 'fee faucet claim could not reach the wallet; retrying' "$tmp_root/faucet-retry.stderr"; then
    echo "expected a faucet claim retry after the wallet send failure" >&2
    exit 1
fi
if ! grep -Fxq 'relay-fees 100000' "$FAKE_FAUCET_CLAIMS"; then
    echo "expected the faucet claim to succeed after the wallet send failure" >&2
    exit 1
fi
if [ "$(cat "$FAKE_FAUCET_ATTEMPTS")" -lt 2 ]; then
    echo "expected two faucet claim attempts" >&2
    exit 1
fi

echo "relay recovery startup checks passed"
