#!/usr/bin/env bash
set -euo pipefail
umask 077

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp_dir="$(mktemp -d)"
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

cargo build --quiet -p ghostline-cli -p ghostline-paid-api

keygen_output="$(target/debug/ghostline keygen)"
payer_secret="$(sed -n 's/^secret=//p' <<<"$keygen_output")"
payer_public="$(sed -n 's/^public=//p' <<<"$keygen_output")"
keygen_output="$(target/debug/ghostline keygen)"
service_public="$(sed -n 's/^public=//p' <<<"$keygen_output")"

deposit=100000
network_tag=1
expiry_epoch=42
nonce_hex=0101010101010101010101010101010101010101010101010101010101010101
# This zero hash is a local smoke-test sentinel only. No transaction is built or submitted.
template_hex=0000000000000000000000000000000000000000000000000000000000000000
spec_output="$(target/debug/ghostline spec \
  --network-tag "$network_tag" \
  --deposit "$deposit" \
  --payer-hex "$payer_public" \
  --service-hex "$service_public" \
  --expiry-epoch "$expiry_epoch" \
  --nonce-hex "$nonce_hex" \
  --template-hex "$template_hex")"
payer_root="$(sed -n 's/^payer_root=//p' <<<"$spec_output")"
[[ "$payer_root" =~ ^[0-9a-fA-F]{64}$ ]] || {
  echo "quickstart smoke: ghostline spec did not produce a 32-byte payer root" >&2
  exit 1
}

ledger="$tmp_dir/ledger.json"
session="$tmp_dir/session.json"
target/debug/ghostline-paid-api init \
  --ledger "$ledger" \
  --session "$session" \
  --network-tag "$network_tag" \
  --deposit "$deposit" \
  --payer-hex "$payer_public" \
  --payer-root-hex "$payer_root" \
  --nonce-hex "$nonce_hex"

port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
bind="127.0.0.1:$port"
target/debug/ghostline-paid-api serve \
  --bind "$bind" \
  --ledger "$ledger" \
  --price 10 \
  --service-key-hex "$service_public" \
  --min-expiry-epoch "$expiry_epoch" \
  --max-expiry-epoch "$expiry_epoch" \
  --metrics \
  >"$tmp_dir/server.log" 2>&1 &
server_pid=$!

url="http://$bind/fortune"
status=000
for _ in $(seq 1 100); do
  status="$(curl -sS -o "$tmp_dir/quote.json" -w '%{http_code}' "$url" 2>/dev/null || true)"
  [[ "$status" == 402 ]] && break
  sleep 0.1
done
if [[ "$status" != 402 ]]; then
  cat "$tmp_dir/server.log" >&2
  echo "quickstart smoke: expected an HTTP 402 quote, got $status" >&2
  exit 1
fi

python3 - "$tmp_dir/quote.json" "$service_public" "$expiry_epoch" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as quote_file:
    quote = json.load(quote_file)
assert quote["scheme"] == "ghostline.channel.voucher.v1", quote
assert quote["price"] == 10, quote
assert quote["basis"] == "per_request", quote
assert quote["recipient_key"] == sys.argv[2], quote
assert quote["min_expiry_epoch"] == int(sys.argv[3]), quote
assert quote["max_expiry_epoch"] == int(sys.argv[3]), quote
PY

call_output="$(GHOSTLINE_PAYER_SECRET="$payer_secret" target/debug/ghostline-paid-api call \
  --url "$url" \
  --session "$session" \
  --calls 3 \
  --vendor-hex "$service_public" \
  --budget 100 \
  --max-per-call 10)"
grep -q '^ok_calls=3$' <<<"$call_output"
grep -q '^cumulative=30$' <<<"$call_output"
grep -q '^unspent_on_channel=99969$' <<<"$call_output"
curl -sS "http://$bind/metrics" >"$tmp_dir/metrics.txt"
grep -q '^ghostline_paid_requests_total 3$' "$tmp_dir/metrics.txt"
grep -q '^ghostline_payment_required_total 4$' "$tmp_dir/metrics.txt"
grep -q '^ghostline_channels_configured 1$' "$tmp_dir/metrics.txt"
grep -q '^ghostline_ledgers_with_accepted_vouchers 1$' "$tmp_dir/metrics.txt"
if grep -Eq 'channel_id|deposit|cumulative|voucher_hex|100000' "$tmp_dir/metrics.txt"; then
  echo "quickstart smoke: metrics exposed channel or payment data" >&2
  exit 1
fi
echo "quickstart smoke passed: quote validated, 3 paid calls, cumulative 30 µT"
