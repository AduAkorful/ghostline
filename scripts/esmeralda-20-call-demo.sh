#!/usr/bin/env bash
set -euo pipefail

# Run 20 paid requests against an already-running Esmeralda demo API, settle the
# resulting voucher once, and print the public explorer URL. This intentionally
# does not open/fund a channel or start a public service.

required=(
  GHOSTLINE_DEMO_CHANNEL_FILE
  GHOSTLINE_DEMO_SESSION_FILE
  GHOSTLINE_DEMO_API_URL
  GHOSTLINE_DEMO_SERVICE_PUBLIC_KEY
  GHOSTLINE_DEMO_SERVICE_KEY_ID_FILE
  GHOSTLINE_PAYER_SECRET
)
for name in "${required[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    printf 'missing required environment variable: %s\n' "$name" >&2
    exit 2
  fi
done

for tool in ghostline ghostline-paid-api jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'required command not found: %s\n' "$tool" >&2
    exit 2
  fi
done

channel_file=$GHOSTLINE_DEMO_CHANNEL_FILE
session_file=$GHOSTLINE_DEMO_SESSION_FILE
key_id_file=$GHOSTLINE_DEMO_SERVICE_KEY_ID_FILE
calls=${GHOSTLINE_DEMO_CALLS:-20}
price=${GHOSTLINE_DEMO_PRICE_MICROTARI:-10}
budget=$((calls * price))

if [[ ! -r "$channel_file" || ! -r "$session_file" || ! -r "$key_id_file" ]]; then
  printf 'channel, payer session, and service key-id files must exist and be readable\n' >&2
  exit 2
fi
if [[ ! "$calls" =~ ^[1-9][0-9]*$ || ! "$price" =~ ^[1-9][0-9]*$ ]]; then
  printf 'calls and price must be positive integers\n' >&2
  exit 2
fi

ledger=$(jq -er '.ledger' "$channel_file")
indexer_url=$(jq -er '.indexer_url' "$channel_file")
if [[ ! -r "$ledger" ]]; then
  printf 'channel ledger is missing or unreadable: %s\n' "$ledger" >&2
  exit 2
fi

# Refuse to consume a channel that has already accepted any payment. The API
# service ledger is authoritative for its off-chain accepted total.
accepted=$(ghostline status --store "$ledger" | jq -er '.accepted')
if [[ "$accepted" != 0 ]]; then
  printf 'refusing to reuse channel: service ledger already accepted %s microTari\n' "$accepted" >&2
  exit 2
fi

printf 'Making %s paid calls at up to %s microTari each.\n' "$calls" "$price"
printf 'The channel is consumed by the settlement; fees are paid by its configured fee account.\n'
ghostline-paid-api call \
  --url "$GHOSTLINE_DEMO_API_URL" \
  --session "$session_file" \
  --calls "$calls" \
  --vendor-hex "$GHOSTLINE_DEMO_SERVICE_PUBLIC_KEY" \
  --budget "$budget" \
  --max-per-call "$price"

settle_output=$(ghostline channel settle \
  --file "$channel_file" \
  --force \
  --submit \
  --seal-key-id-file "$key_id_file")
printf '%s\n' "$settle_output"
transaction_id=$(sed -n 's/^transaction_id=//p' <<<"$settle_output" | tail -n 1)
if [[ -z "$transaction_id" ]]; then
  printf 'settlement output did not contain a transaction_id\n' >&2
  exit 1
fi

printf '\nChecking finality with the channel indexer...\n'
ghostline indexer result \
  --url "$indexer_url" \
  --transaction-id "$transaction_id"
printf '\nExplorer: https://explorer.tari.mw/tx/%s\n' "$transaction_id"
