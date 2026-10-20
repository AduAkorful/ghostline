#!/usr/bin/env bash
set -Eeuo pipefail
umask 077

# Guided, user-controlled Esmeralda demo. The operator must provide a local
# wallet daemon and its view-only claim key; no key is fetched or transmitted.
set +x

INDEXER_URL_DEFAULT="https://ootle-indexer-a.tari.com"
NETWORK_TAG=38 # Tari Ootle Network::Esmeralda (0x26), per tari-project/ootle.ts.
TEMPLATE_HEX="61bad5eb1b599ba6351e1573958fb84c4aeb4dd47415e60c0b44b1c50d7157a1"
TEMPLATE_PUBLISH_TX="59fc0b3c52b36b6e0a8c7f3e44c8219e1d4777950dd6d9d4cd862aa1498c9741"
TEMPLATE_EVIDENCE_DATE="2026-10-19"
WALLET_URL_DEFAULT="http://127.0.0.1:5100"

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$1" >&2
    exit 2
  }
}

for tool in ghostline ghostline-paid-api curl openssl python3; do
  require_command "$tool"
done
if ! ghostline channel settle --help | grep -q -- '--max-fee'; then
  echo 'This runner needs a Ghostline release with reviewed channel settlement fee caps. Install the current release, then retry.' >&2
  exit 2
fi

if [[ ! -t 0 && ! -r /dev/tty ]]; then
  echo 'This guided run needs an interactive terminal.' >&2
  exit 2
fi
read_tty() {
  local prompt="$1" default="${2:-}" value
  if [[ -n "$default" ]]; then
    printf '%s [%s]: ' "$prompt" "$default" >/dev/tty
  else
    printf '%s: ' "$prompt" >/dev/tty
  fi
  IFS= read -r value </dev/tty || exit 2
  REPLY="${value:-$default}"
}

confirm_tty() {
  local prompt="$1" reply
  printf '%s [y/N]: ' "$prompt" >/dev/tty
  IFS= read -r reply </dev/tty || exit 2
  [[ "$reply" == y || "$reply" == Y || "$reply" == yes || "$reply" == YES ]]
}

is_uint() { [[ "$1" =~ ^(0|[1-9][0-9]*)$ ]]; }

read_tty 'Wallet daemon URL (keep it on loopback)' "$WALLET_URL_DEFAULT"
OPENER_WALLET_URL="$REPLY"
case "$OPENER_WALLET_URL" in
  http://127.0.0.1:*|http://localhost:*) ;;
  *) echo 'For safety, the opener wallet daemon must use a loopback URL.' >&2; exit 2 ;;
esac
read_tty 'Fee wallet URL (may be the same local wallet)' "$OPENER_WALLET_URL"
FEE_WALLET_URL="$REPLY"
case "$FEE_WALLET_URL" in
  http://127.0.0.1:*|http://localhost:*) ;;
  *) echo 'For safety, the fee wallet daemon must use a loopback URL.' >&2; exit 2 ;;
esac
read_tty 'Esmeralda indexer URL' "$INDEXER_URL_DEFAULT"
INDEXER_URL="${REPLY%/}"
read_tty 'Funded opener account name' 'ghostline'
OPENER_ACCOUNT="$REPLY"
read_tty 'Funded service fee account name (same account is simplest)' "$OPENER_ACCOUNT"
FEE_ACCOUNT_NAME="$REPLY"
read_tty 'Calls to make' '3'
CALLS="$REPLY"
read_tty 'Price per call in integer micro-Tari' '10'
PRICE="$REPLY"
read_tty 'Channel deposit in integer micro-Tari' '100000'
DEPOSIT="$REPLY"

check_wallet_compatibility() {
  local url="$1" info
  info="$(curl -fsS --max-time 10 \
    -H 'content-type: application/json' \
    -d '{"jsonrpc":"2.0","method":"wallet.get_info","params":{},"id":1}' \
    "${url%/}/json_rpc")" || {
    printf 'Could not query wallet info at %s. Check the local daemon and its JSON-RPC auth mode.\n' "$url" >&2
    return 1
  }
  if ! python3 -c 'import json,re,sys; d=json.load(sys.stdin).get("result",{}); v=d.get("version",""); n=d.get("network","").lower(); m=re.match(r"^(\d+)\.(\d+)\.(\d+)",v); ok=m and tuple(map(int,m.groups())) >= (0,47,0) and n=="esmeralda"; print(f"walletd={v} network={n}"); sys.exit(0 if ok else 1)' <<<"$info"; then
    echo 'This run needs Ootle walletd 0.47.0 or newer on Esmeralda.' >&2
    echo 'Walletd 0.46.1 cannot decode the current Esmeralda account/resource substates during settlement.' >&2
    echo 'Upgrade the local wallet daemon before opening a channel; no funds have been moved.' >&2
    return 1
  fi
}

echo
echo 'Preflight: checking wallet daemon compatibility before any transaction.'
check_wallet_compatibility "$OPENER_WALLET_URL"
if [[ "$FEE_WALLET_URL" != "$OPENER_WALLET_URL" ]]; then
  check_wallet_compatibility "$FEE_WALLET_URL"
fi

for label in calls price deposit; do
  case "$label" in
    calls) value="$CALLS" ;;
    price) value="$PRICE" ;;
    deposit) value="$DEPOSIT" ;;
  esac
  if ! is_uint "$value" || (( value == 0 )); then
    printf '%s must be a positive integer in micro-Tari.\n' "$label" >&2
    exit 2
  fi
done
if (( CALLS > 100 || PRICE > 1000000 || DEPOSIT > 1000000000 )); then
  echo 'Demo bounds exceeded (maximum 100 calls, 1,000,000 µT/call, 1,000,000,000 µT deposit).' >&2
  exit 2
fi
TOTAL=$((CALLS * PRICE))
if (( TOTAL >= DEPOSIT )); then
  echo 'The deposit must exceed the call total to preserve the fixed-shape payer-change output.' >&2
  exit 2
fi

echo
echo 'Preflight: checking the configured indexer and the documented Ghostline template.'
ghostline indexer wait --url "$INDEXER_URL" >/dev/null
template_state="$(curl -fsS "$INDEXER_URL/substates/template_$TEMPLATE_HEX")" || {
  echo 'The documented Ghostline template is not available from this Esmeralda indexer.' >&2
  echo 'Stop here and use a current, independently verified Esmeralda deployment profile.' >&2
  exit 2
}
if ! python3 -c 'import json,sys; d=json.load(sys.stdin); t=d.get("state",{}).get("up",{}).get("substate",{}).get("Template"); sys.exit(0 if t and t.get("template_name")=="GhostlineChannel" else 1)' <<<"$template_state"; then
  echo 'The address resolved, but it is not an active GhostlineChannel template.' >&2
  echo 'Stop here and use a current, independently verified Esmeralda deployment profile.' >&2
  exit 2
fi
echo "Verified active template substate: template_$TEMPLATE_HEX (publish tx evidence: $TEMPLATE_PUBLISH_TX)"
CURRENT_EPOCH="$(ghostline indexer epoch --url "$INDEXER_URL")"
if ! is_uint "$CURRENT_EPOCH"; then
  echo 'Could not read the current Esmeralda epoch.' >&2
  exit 2
fi
EXPIRY_EPOCH=$((CURRENT_EPOCH + 200))
MAX_EPOCH=$((CURRENT_EPOCH + 100))

opener_account="$(ghostline wallet account-get --url "$OPENER_WALLET_URL" --name "$OPENER_ACCOUNT")"
OPENER_ADDRESS="$(sed -n 's/^address=//p' <<<"$opener_account" | tail -n 1)"
OPENER_IS_DEFAULT="$(sed -n 's/^is_default=//p' <<<"$opener_account" | tail -n 1)"
fee_account="$(ghostline wallet account-get --url "$FEE_WALLET_URL" --name "$FEE_ACCOUNT_NAME")"
FEE_COMPONENT="$(sed -n 's/^component_address=//p' <<<"$fee_account" | tail -n 1)"
[[ -n "$OPENER_ADDRESS" && -n "$FEE_COMPONENT" ]] || {
  echo 'Could not resolve both configured wallet accounts.' >&2
  exit 2
}
if [[ "$OPENER_IS_DEFAULT" != true ]]; then
  echo 'The opener account must be the wallet default account because the open output is encrypted to its default address.' >&2
  exit 2
fi

STATE_ROOT="${XDG_STATE_HOME:-${HOME:?HOME is not set}/.local/state}/ghostline"
mkdir -p "$STATE_ROOT"
STATE_DIR="$(mktemp -d "$STATE_ROOT/esmeralda-demo.XXXXXXXX")"
chmod 700 "$STATE_DIR"
API_PID=""
KEEP_STATE=0
cleanup() {
  if [[ -n "$API_PID" ]]; then
    kill "$API_PID" 2>/dev/null || true
    wait "$API_PID" 2>/dev/null || true
  fi
  if [[ "$KEEP_STATE" == 0 ]]; then
    rm -rf "$STATE_DIR"
  else
    printf '\nPrivate run state kept at: %s\n' "$STATE_DIR" >&2
    echo 'It includes the channel deposit mask and local payment state. Remove it only after you have saved the transaction links.' >&2
  fi
}
trap cleanup EXIT INT TERM

PAYER_KEY_FILE="$STATE_DIR/payer-key.json"
SERVICE_KEY_ID_FILE="$STATE_DIR/service-key-id.json"
CHANNEL_FILE="$STATE_DIR/channel.json"
LEDGER_FILE="$STATE_DIR/service-ledger.json"
SESSION_FILE="$STATE_DIR/payer-session.json"

echo
echo 'Preparing fresh, local channel state and a one-time service key.'
keygen_out="$(ghostline keygen --role payer --out "$PAYER_KEY_FILE")"
PAYER_PUBLIC="$(sed -n 's/^public=//p' <<<"$keygen_out" | tail -n 1)"
[[ "$PAYER_PUBLIC" =~ ^[0-9a-fA-F]{64}$ ]] || { echo 'Could not read generated payer public key.' >&2; exit 2; }
service_key_out="$(ghostline wallet key-create --url "$FEE_WALLET_URL" --out "$SERVICE_KEY_ID_FILE")"
SERVICE_PUBLIC="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["public_key_hex"])' <<<"$service_key_out")"
[[ "$SERVICE_PUBLIC" =~ ^[0-9a-fA-F]{64}$ ]] || { echo 'Could not read generated service public key.' >&2; exit 2; }
NONCE_HEX="$(openssl rand -hex 32)"

spec_out="$(ghostline spec \
  --network-tag "$NETWORK_TAG" \
  --deposit "$DEPOSIT" \
  --payer-hex "$PAYER_PUBLIC" \
  --service-hex "$SERVICE_PUBLIC" \
  --expiry-epoch "$EXPIRY_EPOCH" \
  --nonce-hex "$NONCE_HEX" \
  --template-hex "$TEMPLATE_HEX")"
PAYER_ROOT="$(sed -n 's/^payer_root=//p' <<<"$spec_out" | tail -n 1)"
[[ "$PAYER_ROOT" =~ ^[0-9a-fA-F]{64}$ ]] || { echo 'Could not compute payer root.' >&2; exit 2; }

ghostline-paid-api init \
  --ledger "$LEDGER_FILE" \
  --session "$SESSION_FILE" \
  --network-tag "$NETWORK_TAG" \
  --deposit "$DEPOSIT" \
  --payer-hex "$PAYER_PUBLIC" \
  --payer-root-hex "$PAYER_ROOT" \
  --nonce-hex "$NONCE_HEX" >/dev/null

ghostline channel init \
  --out "$CHANNEL_FILE" \
  --network-tag "$NETWORK_TAG" \
  --deposit "$DEPOSIT" \
  --payer-hex "$PAYER_PUBLIC" \
  --service-hex "$SERVICE_PUBLIC" \
  --expiry-epoch "$EXPIRY_EPOCH" \
  --nonce-hex "$NONCE_HEX" \
  --template-hex "$TEMPLATE_HEX" \
  --indexer-url "$INDEXER_URL" \
  --ootle-network esmeralda \
  --max-epoch "$MAX_EPOCH" \
  --opener-account "$OPENER_ACCOUNT" \
  --opener-wallet-url "$OPENER_WALLET_URL" \
  --fee-account "$FEE_COMPONENT" \
  --fee-account-name "$FEE_ACCOUNT_NAME" \
  --fee-wallet-url "$FEE_WALLET_URL" \
  --ledger "$LEDGER_FILE" \
  --session "$SESSION_FILE" >/dev/null

echo
echo 'Run summary (using one account for both roles is simplest; it links payer funding and fee activity):'
printf '  Network: Esmeralda (network tag %s)\n' "$NETWORK_TAG"
printf '  Template: template_%s (publish record dated %s)\n' "$TEMPLATE_HEX" "$TEMPLATE_EVIDENCE_DATE"
printf '  Opener account: %s\n' "$OPENER_ACCOUNT"
printf '  Fee account: %s\n' "$FEE_ACCOUNT_NAME"
printf '  Deposit: %s µT; calls: %s × %s µT = %s µT\n' "$DEPOSIT" "$CALLS" "$PRICE" "$TOTAL"
printf '  Opening transaction fee cap and settlement fee cap are reviewed separately.\n'
echo 'The service API binds only to 127.0.0.1. The website receives no wallet data.'
confirm_tty 'Continue to the open dry-run?'

open_dry="$(ghostline channel open --file "$CHANNEL_FILE" --dry-run --max-fee 100000)"
printf '%s\n' "$open_dry"
OPEN_REQUIRED="$(sed -n 's/^required_fee=//p' <<<"$open_dry" | tail -n 1)"
if ! is_uint "$OPEN_REQUIRED"; then
  echo 'Dry-run did not provide a required opening fee; no transaction was submitted.' >&2
  exit 2
fi
OPEN_SUGGESTED="$OPEN_REQUIRED"
echo 'The open fee cap is a maximum; excess may not be returned. Choose a cap only after reviewing the dry-run.'
read_tty "Opening fee cap in µT (must be at least $OPEN_REQUIRED)" "$OPEN_SUGGESTED"
OPEN_CAP="$REPLY"
if ! is_uint "$OPEN_CAP" || (( OPEN_CAP < OPEN_REQUIRED )); then
  echo 'Opening fee cap must be an integer at least equal to the dry-run requirement.' >&2
  exit 2
fi
confirm_tty "Submit channel open with deposit $DEPOSIT µT and fee cap $OPEN_CAP µT?"
printf 'Wallet view-only key for unblinding the channel output (input hidden): ' >/dev/tty
IFS= read -r -s CLAIM_SECRET </dev/tty || exit 2
printf '\n' >/dev/tty
if [[ -z "$CLAIM_SECRET" ]]; then
  echo 'A view-only key is required to unblind the channel output. No open was submitted.' >&2
  exit 2
fi
KEEP_STATE=1
if open_out="$(GHOSTLINE_CLAIM_SECRET="$CLAIM_SECRET" ghostline channel open --file "$CHANNEL_FILE" --max-fee "$OPEN_CAP" 2>&1)"; then
  :
else
  open_status=$?
  printf '%s\n' "$open_out" >&2
  echo 'Channel open did not complete cleanly. The private state is preserved; inspect the wallet/indexer before retrying.' >&2
  exit "$open_status"
fi
unset CLAIM_SECRET
printf '%s\n' "$open_out"
OPEN_TX="$(sed -n 's/^transaction_id=//p' <<<"$open_out" | head -n 1)"
[[ -n "$OPEN_TX" ]] || { echo 'Open returned no transaction id; preserve the state directory and check the wallet.' >&2; exit 2; }

API_PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
API_URL="http://127.0.0.1:$API_PORT/fortune"
ghostline-paid-api serve \
  --bind "127.0.0.1:$API_PORT" \
  --ledger "$LEDGER_FILE" \
  --price "$PRICE" \
  --service-key-hex "$SERVICE_PUBLIC" \
  --min-expiry-epoch "$EXPIRY_EPOCH" \
  --max-expiry-epoch "$EXPIRY_EPOCH" >"$STATE_DIR/api.log" 2>&1 &
API_PID=$!
ready=0
for _ in $(seq 1 100); do
  status="$(curl -sS -o /dev/null -w '%{http_code}' "$API_URL" 2>/dev/null || true)"
  if [[ "$status" == 402 ]]; then ready=1; break; fi
  sleep 0.1
done
if [[ "$ready" != 1 ]]; then
  cat "$STATE_DIR/api.log" >&2
  echo 'Local paid API did not become ready; the channel is open. Keep state and inspect before retrying.' >&2
  exit 2
fi

echo
echo "Making $CALLS paid calls to the local API. The call record stays in the local service ledger."
PAYER_SECRET="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1],encoding="utf-8"))["secret_hex"])' "$PAYER_KEY_FILE")"
GHOSTLINE_PAYER_SECRET="$PAYER_SECRET" ghostline-paid-api call \
  --url "$API_URL" \
  --session "$SESSION_FILE" \
  --calls "$CALLS" \
  --vendor-hex "$SERVICE_PUBLIC" \
  --budget "$TOTAL" \
  --max-per-call "$PRICE"
unset PAYER_SECRET

echo
echo 'Settlement dry-run (no transaction submitted):'
settle_dry="$(ghostline channel settle \
  --file "$CHANNEL_FILE" --force --submit --dry-run --max-fee 100000 \
  --seal-key-id-file "$SERVICE_KEY_ID_FILE")"
printf '%s\n' "$settle_dry"
SETTLE_REQUIRED="$(sed -n 's/^required_fees=//p' <<<"$settle_dry" | tail -n 1)"
if ! is_uint "$SETTLE_REQUIRED"; then
  echo 'Settlement dry-run did not provide a fee requirement; no settlement was submitted.' >&2
  exit 2
fi
SETTLE_SUGGESTED="$SETTLE_REQUIRED"
read_tty "Settlement fee cap in µT (must be at least $SETTLE_REQUIRED)" "$SETTLE_SUGGESTED"
SETTLE_CAP="$REPLY"
if ! is_uint "$SETTLE_CAP" || (( SETTLE_CAP < SETTLE_REQUIRED )); then
  echo 'Settlement fee cap must be an integer at least equal to the dry-run requirement.' >&2
  exit 2
fi
echo 'The settlement consumes this channel output. The run cannot settle it again or refund it afterward.'
confirm_tty "Submit one settlement with fee cap $SETTLE_CAP µT?"
if settle_out="$(ghostline channel settle \
  --file "$CHANNEL_FILE" --force --submit --max-fee "$SETTLE_CAP" \
  --seal-key-id-file "$SERVICE_KEY_ID_FILE" 2>&1)"; then
  :
else
  settle_status=$?
  printf '%s\n' "$settle_out" >&2
  echo 'Settlement did not complete cleanly. The private state is preserved; inspect this wallet/indexer before retrying.' >&2
  exit "$settle_status"
fi
printf '%s\n' "$settle_out"
SETTLE_TX="$(sed -n 's/^transaction_id=//p' <<<"$settle_out" | head -n 1)"
[[ -n "$SETTLE_TX" ]] || { echo 'Settlement returned no transaction id; inspect wallet and indexer before retrying.' >&2; exit 2; }

RESULT=""
for _ in $(seq 1 30); do
  RESULT="$(ghostline indexer result --url "$INDEXER_URL" --transaction-id "$SETTLE_TX" 2>/dev/null || true)"
  [[ "$RESULT" == Finalized ]] && break
  [[ "$RESULT" == Rejected ]] && { echo 'Indexer reports the settlement rejected; do not retry blindly.' >&2; exit 2; }
  sleep 2
done
if [[ "$RESULT" != Finalized ]]; then
  printf 'Settlement result is %s. Do not retry; check this transaction id with the indexer.\n' "${RESULT:-unknown}" >&2
  printf 'Transaction ID: %s\n' "$SETTLE_TX" >&2
  exit 2
fi

kill "$API_PID" 2>/dev/null || true
wait "$API_PID" 2>/dev/null || true
API_PID=""
echo
echo 'Finalized Esmeralda run:'
printf '  Channel open: https://explorer.tari.mw/tx/%s\n' "$OPEN_TX"
printf '  Settlement:   https://explorer.tari.mw/tx/%s\n' "$SETTLE_TX"
printf '  Service log:  %s (local evidence for %s calls and %s µT accepted)\n' "$LEDGER_FILE" "$CALLS" "$TOTAL"
echo 'The explorer proves chain finality, not the off-chain call count or prices. Fees, timing, and transaction metadata remain public.'
echo 'Once you have saved the explorer links and no longer need local accounting, remove the private state directory printed below.'
KEEP_STATE=1
