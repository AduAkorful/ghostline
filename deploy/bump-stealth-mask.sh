#!/usr/bin/env bash
# Raise walletd's stealth_mask index so the next output does not reuse a
# commitment already published by this seed.
#
# walletd 0.49.2 stores one row per (branch_seed, index) in key_manager_states.
# next_key uses MAX(index)+1, or 0 when the branch is missing. A new database
# therefore recreates mask 0. Channel outputs are 51 µT, so the same mask and
# the same amount are the same output id. Inserting a floor row skips every
# lower index. The row stores no key material.
set -euo pipefail

floor="${GHOSTLINE_STEALTH_MASK_FLOOR:-10000}"
base="${TARI_BASE_DIR:-/tmp/tari-wallet}"
case "$floor" in
    ''|*[!0-9]*)
        echo 'GHOSTLINE_STEALTH_MASK_FLOOR must be a nonnegative integer' >&2
        exit 1
        ;;
esac

if [ -n "${GHOSTLINE_WALLET_SQLITE:-}" ]; then
    db=$GHOSTLINE_WALLET_SQLITE
elif [ -f "$base/esmeralda/data/wallet.sqlite" ]; then
    db=$base/esmeralda/data/wallet.sqlite
elif [ -f "$base/data/wallet.sqlite" ]; then
    db=$base/data/wallet.sqlite
else
    db=$(find "$base" -type f -name wallet.sqlite ! -name '*.bak' -print -quit 2>/dev/null || true)
fi

if [ -z "${db:-}" ] || [ ! -f "$db" ]; then
    echo "stealth_mask bump skipped: wallet.sqlite is not under $base" >&2
    exit 0
fi

if ! command -v sqlite3 >/dev/null 2>&1; then
    echo 'sqlite3 is required to advance the stealth_mask index' >&2
    exit 1
fi

read_max() {
    sqlite3 -batch "$db" <<SQL
.timeout 8000
SELECT COALESCE(MAX("index"), -1) FROM key_manager_states WHERE branch_seed='stealth_mask';
SQL
}

insert_floor() {
    sqlite3 -batch "$db" <<SQL
.timeout 8000
BEGIN IMMEDIATE;
INSERT INTO key_manager_states (branch_seed, "index", is_active)
SELECT 'stealth_mask', $floor, 0
WHERE COALESCE((SELECT MAX("index") FROM key_manager_states WHERE branch_seed='stealth_mask'), -1) < $floor;
COMMIT;
SQL
}

previous=
attempt=0
until previous=$(read_max); do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 15 ]; then
        echo "stealth_mask bump could not read $db" >&2
        exit 1
    fi
    sleep 1
done
case "$previous" in
    ''|*[!0-9-]*)
        echo "stealth_mask bump read an unexpected index: $previous" >&2
        exit 1
        ;;
esac

inserted=no
if [ "$previous" -lt "$floor" ]; then
    insert_floor
    inserted=yes
fi

current=$(read_max)
if [ "$current" -lt "$floor" ]; then
    echo "stealth_mask index is $current after bump; expected at least $floor" >&2
    exit 1
fi
echo "stealth_mask previous=$previous floor=$floor inserted=$inserted next=$((current + 1))"
