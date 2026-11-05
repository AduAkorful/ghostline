#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
script="$repo_root/deploy/bump-stealth-mask.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if ! command -v sqlite3 >/dev/null 2>&1; then
    echo 'sqlite3 is required' >&2
    exit 1
fi

schema='CREATE TABLE key_manager_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    branch_seed TEXT NOT NULL,
    "index" BIGINT NOT NULL,
    is_active BOOLEAN NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX key_manager_states_uniq_branch_seed_index
    ON key_manager_states (branch_seed, "index");'

db="$tmp/esmeralda/data/wallet.sqlite"
mkdir -p "$(dirname "$db")"
sqlite3 "$db" "$schema"
sqlite3 "$db" <<'SQL'
INSERT INTO key_manager_states (branch_seed, "index", is_active) VALUES
    ('stealth_mask', 0, 1),
    ('stealth_mask', 2, 0),
    ('account', 3, 1);
SQL

output=$(TARI_BASE_DIR="$tmp" "$script")
printf '%s\n' "$output" | grep -Fq 'previous=2 floor=10000 inserted=yes next=10001'
count=$(sqlite3 "$db" "SELECT COUNT(*) FROM key_manager_states WHERE branch_seed='stealth_mask' AND \"index\"=10000 AND is_active=0;")
[ "$count" = 1 ]
account=$(sqlite3 "$db" "SELECT \"index\" FROM key_manager_states WHERE branch_seed='account';")
[ "$account" = 3 ]

output=$(TARI_BASE_DIR="$tmp" "$script")
printf '%s\n' "$output" | grep -Fq 'previous=10000 floor=10000 inserted=no next=10001'
count=$(sqlite3 "$db" "SELECT COUNT(*) FROM key_manager_states WHERE branch_seed='stealth_mask';")
[ "$count" = 3 ]

output=$(GHOSTLINE_STEALTH_MASK_FLOOR=50 TARI_BASE_DIR="$tmp" "$script")
printf '%s\n' "$output" | grep -Fq 'inserted=no'
max=$(sqlite3 "$db" "SELECT MAX(\"index\") FROM key_manager_states WHERE branch_seed='stealth_mask';")
[ "$max" = 10000 ]

empty=$(mktemp -d)
skipped=$(TARI_BASE_DIR="$empty" "$script" 2>&1)
printf '%s\n' "$skipped" | grep -Fq 'stealth_mask bump skipped'
rmdir "$empty"

echo 'bump-stealth-mask ok'
