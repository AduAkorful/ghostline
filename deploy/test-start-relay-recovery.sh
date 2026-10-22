#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

write_fake_cli() {
    cat > "$tmp_root/ghostline" <<'FAKE'
#!/bin/sh
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
            exit 0
        fi
        exit 1
        ;;
    account-create)
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
if tail -n 1 "$FAKE_WALLETD_ARGS" | grep -Fq -- '--indexer-api-url'; then
    echo "walletd default indexer set should be preserved when no override is configured" >&2
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

echo "relay recovery startup checks passed"
