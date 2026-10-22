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
        exit 1
        ;;
    account-create)
        if [ "${FAKE_CREATE_MODE:-recovering}" = recovering ]; then
            echo "wallet daemon RPC accounts.create failed: Invalid request: the wallet is still recovering accounts from its seed" >&2
            exit 1
        fi
        echo "wallet daemon RPC accounts.create failed: could not connect" >&2
        exit 1
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

write_fake_cli
write_fake_walletd

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
export GHOSTLINE_WALLET_RECOVERY_RETRY_DELAY_SECS=0
export FAKE_WALLETD_ARGS="$tmp_root/walletd-args"
export FAKE_WALLETD_MODE=running
export FAKE_CREATE_MODE=recovering

set +e
bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/recovery.stdout" 2>"$tmp_root/recovery.stderr"
recovery_status=$?
set -e

if [ "$recovery_status" -eq 0 ]; then
    echo "expected startup to stop while wallet recovery is pending" >&2
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
if ! grep -Fq 'account recovery is pending' "$tmp_root/recovery.stderr"; then
    echo "expected a concise recovery-pending diagnostic" >&2
    exit 1
fi

# A Render restart must reuse the accepted wallet database without replaying
# seed words. With no explicit override, walletd's built-in indexer set remains
# in effect.
unset GHOSTLINE_WALLET_INDEXER_URLS
set +e
bash "$repo_root/deploy/start-relay.sh" >"$tmp_root/restart.stdout" 2>"$tmp_root/restart.stderr"
restart_status=$?
set -e
if [ "$restart_status" -eq 0 ]; then
    echo "expected restarted process to remain pending recovery" >&2
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

echo "relay recovery startup checks passed"
