# Ghostline CLI

Command reference generated from the executable's `--help`. Use the [mainnet quickstart](mainnet-quickstart.md) for wallet setup and a full channel walkthrough.

## ghostline

```text
Sign and verify Ghostline vouchers, seal stealth transfers, inspect/unblind stealth outputs, operator status/watch, open/settle via a wallet daemon or indexer

Usage: ghostline <COMMAND>

Commands:
  demo           Run a bounded local quote → voucher → paid-call simulation (no wallet or chain access)
  channel-id     Print the 32-byte channel id for the given open parameters
  channel        One channel record for open, status, settle, and refund
  sign           Sign the next voucher. Requires a session file and policy unless `--unsafe-no-policy`
  verify         Verify a hex-encoded voucher against the given channel parameters
  indexer        Talk to a local Ootle indexer REST API (do not point this at public networks for attack tests)
  wallet         Talk to a local `tari_walletd` JSON-RPC (`--authentication none`)
  spec           Print payer_root and channel_id from a full channel spec (computes the tree)
  pay-to         Print `PayTo::Conditions` JSON for `accounts.stealth_transfer`
  keygen         Print a random Ristretto secret and matching public key as hex
  seal-open      Seal an open (key-path input → PayTo::Conditions). Prints tx= and deposit_mask=
  seal-settle    Seal a settle of a voucher against the channel UTXO
  seal-unsigned  Seal an unsigned JSON after `indexer resolve-inputs` (indexer path; versions must be set)
  seal-refund    Seal a refund of the channel UTXO after expiry
  unblind        Recover value and mask from a stealth output encrypted to the claim secret
  unblind-tx     Recover every stealth output in a sealed tx hex that matches the claim secret
  inspect-tx     Print stealth output fields from a sealed tx hex (no secret)
  status         Off-chain channel totals plus the observer panel (not a chain confirmation)
  report         Same JSON as `status`. Alias for operator export
  watch          Compare current epoch to channel expiry (settlement-window heartbeat)
  settle-due     Build (and optionally submit) a settle of the ledger's latest voucher before expiry
  refund-due     Build (and optionally submit) a refund once `current_epoch >= expiry_epoch`
  help           Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## ghostline demo

```text
Run a bounded local quote → voucher → paid-call simulation (no wallet or chain access)

Usage: ghostline demo [OPTIONS]

Options:
      --calls <CALLS>        Number of simulated paid calls (1–50) [default: 3]
      --show-chain-evidence  Print explorer links for the separate, recorded Esmeralda evidence
  -h, --help                 Print help
```

## ghostline channel-id

```text
Print the 32-byte channel id for the given open parameters

Usage: ghostline channel-id --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --payer-root-hex <PAYER_ROOT_HEX> --nonce-hex <NONCE_HEX>

Options:
      --network-tag <NETWORK_TAG>        
      --deposit <DEPOSIT>                Deposit in integer micro-units
      --payer-hex <PAYER_HEX>            Hex-encoded payer public key (32 bytes)
      --payer-root-hex <PAYER_ROOT_HEX>  Hex-encoded payer condition-tree root (32 bytes)
      --nonce-hex <NONCE_HEX>            Hex-encoded 32-byte channel nonce
  -h, --help                             Print help
```

## ghostline channel

```text
One channel record for open, status, settle, and refund

Usage: ghostline channel <COMMAND>

Commands:
  init     Write a channel file. No network call. The file has no keys until `channel open` or `channel bind`
  open     Wallet-open the channel, unblind the output, and store the mask on the file
  session  Create the payer session from this file, or keep it when the channel id already matches
  bind     Unblind an existing `--utxo-json` into the channel file
  status   `status` using the file's ledger, expiry, indexer, and UTXO
  settle   `settle-due` using the file. `--seal-key-id-file` is the service key in the fee wallet
  list     One row per channel file in a directory. Skips JSON that is not a channel record
  report   Accounting export of channel files. Default columns omit channel id, keys, and the deposit mask
  calls    Per-call prices from each channel file's ledger. No voucher hex
  trace    Create a shareable privacy receipt from the local call log and the chain transactions you name. No mask or voucher hex is included; the receipt is an operator summary, not a cryptographic proof
  watch    Poll until the settle window is due, then run `settle`. Stops at expiry, on a spent output, or when nothing is owed. Does not broadcast unless `--submit`
  refund   `refund-due` using the file. `--seal-key-id-file` is the payer key in the opener wallet
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## ghostline channel init

```text
Write a channel file. No network call. The file has no keys until `channel open` or `channel bind`

Usage: ghostline channel init [OPTIONS] --out <OUT> --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG> --indexer-url <INDEXER_URL> --max-epoch <MAX_EPOCH>

Options:
      --out <OUT>
          
      --network-tag <NETWORK_TAG>
          
      --deposit <DEPOSIT>
          
      --payer-hex <PAYER_HEX>
          
      --service-hex <SERVICE_HEX>
          
      --refund-hex <REFUND_HEX>
          Optional public key for the expiry-refund branch. Defaults to the payer key
      --expiry-epoch <EXPIRY_EPOCH>
          
      --nonce-hex <NONCE_HEX>
          
      --template-hex <TEMPLATE_HEX>
          
      --fee-config <FEE_CONFIG>
          Fee config component the channel template pins (`component_<hex>` or raw hex)
      --indexer-url <INDEXER_URL>
          
      --ootle-network <OOTLE_NETWORK>
          Ootle network for UTXO tags and the sealed transaction (`localnet`, `esmeralda`, or `mainnet`) [default: localnet]
      --max-epoch <MAX_EPOCH>
          
      --opener-account <OPENER_ACCOUNT>
          [default: ghostline]
      --opener-wallet-url <OPENER_WALLET_URL>
          [default: http://127.0.0.1:5100]
      --fee-account <FEE_ACCOUNT>
          
      --fee-account-name <FEE_ACCOUNT_NAME>
          
      --fee-wallet-url <FEE_WALLET_URL>
          
      --payer-fee-account <PAYER_FEE_ACCOUNT>
          
      --payer-fee-account-name <PAYER_FEE_ACCOUNT_NAME>
          
      --payer-wallet-url <PAYER_WALLET_URL>
          
      --ledger <LEDGER>
          
      --session <SESSION>
          Payer session JSON. Used by MCP when `GHOSTLINE_SESSION` is unset
      --payout-destination <PAYOUT_DESTINATION>
          Service payout destination (`wallet payout-destination`). `channel settle` pays the service wallet
      --claim-key-file <CLAIM_KEY_FILE>
          Claim key file (`keygen --role claim`) used to address and unblind the deposit instead of the wallet's view key, which walletd only exports to a browser session
  -h, --help
          Print help
```

## ghostline channel open

```text
Wallet-open the channel, unblind the output, and store the mask on the file

Usage: ghostline channel open [OPTIONS] --file <FILE>

Options:
      --file <FILE>
          
      --dry-run
          
      --max-fee <MAX_FEE>
          Maximum µT fee. Required for a live open; dry-runs default to 100000 µT
      --funding-commitment <FUNDING_COMMITMENT>
          Commitment hex of a stealth output this open must spend. Repeat for each output. The broadcast is refused unless these are the account's only unspent stealth outputs
      --timeout-secs <TIMEOUT_SECS>
          [default: 90]
  -h, --help
          Print help
```

## ghostline channel session

```text
Create the payer session from this file, or keep it when the channel id already matches

Usage: ghostline channel session --file <FILE>

Options:
      --file <FILE>  
  -h, --help         Print help
```

## ghostline channel bind

```text
Unblind an existing `--utxo-json` into the channel file

Usage: ghostline channel bind --file <FILE> --utxo-json <UTXO_JSON>

Options:
      --file <FILE>            
      --utxo-json <UTXO_JSON>  
  -h, --help                   Print help
```

## ghostline channel status

```text
`status` using the file's ledger, expiry, indexer, and UTXO

Usage: ghostline channel status --file <FILE>

Options:
      --file <FILE>  
  -h, --help         Print help
```

## ghostline channel settle

```text
`settle-due` using the file. `--seal-key-id-file` is the service key in the fee wallet

Usage: ghostline channel settle [OPTIONS] --file <FILE>

Options:
      --file <FILE>
          
      --margin-epochs <MARGIN_EPOCHS>
          [default: 10]
      --force
          
      --submit
          
      --dry-run
          
      --seal-key-id-file <SEAL_KEY_ID_FILE>
          
      --max-fee <MAX_FEE>
          Maximum µT fee. Review the dry-run requirement and set an acceptable cap before submitting [default: 100000]
      --timeout-secs <TIMEOUT_SECS>
          [default: 90]
  -h, --help
          Print help
```

## ghostline channel list

```text
One row per channel file in a directory. Skips JSON that is not a channel record

Usage: ghostline channel list --dir <DIR>

Options:
      --dir <DIR>  
  -h, --help       Print help
```

## ghostline channel report

```text
Accounting export of channel files. Default columns omit channel id, keys, and the deposit mask.

`--disclose` adds `channel_id`, `cumulative`, `has_latest_voucher`, `epochs_until_expiry`, or `current_epoch`.

Usage: ghostline channel report [OPTIONS] --dir <DIR>

Options:
      --dir <DIR>
          

      --format <FORMAT>
          `csv` or `json`
          
          [default: csv]

      --disclose <DISCLOSE>
          

      --out <OUT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## ghostline channel calls

```text
Per-call prices from each channel file's ledger. No voucher hex

Usage: ghostline channel calls [OPTIONS] --dir <DIR>

Options:
      --dir <DIR>  
      --out <OUT>  
  -h, --help       Print help
```

## ghostline channel trace

```text
Create a shareable privacy receipt from the local call log and the chain transactions you name. No mask or voucher hex is included; the receipt is an operator summary, not a cryptographic proof

Usage: ghostline channel trace [OPTIONS] --file <FILE>

Options:
      --file <FILE>            
      --open-tx <OPEN_TX>      
      --settle-tx <SETTLE_TX>  
      --refund-tx <REFUND_TX>  
      --format <FORMAT>        `text` or `json` [default: text]
      --out <OUT>              Write the receipt to this path instead of stdout
  -h, --help                   Print help
```

## ghostline channel watch

```text
Poll until the settle window is due, then run `settle`. Stops at expiry, on a spent output, or when nothing is owed. Does not broadcast unless `--submit`

Usage: ghostline channel watch [OPTIONS] --file <FILE>

Options:
      --file <FILE>
          
      --margin-epochs <MARGIN_EPOCHS>
          [default: 10]
      --force
          
      --submit
          
      --dry-run
          
      --seal-key-id-file <SEAL_KEY_ID_FILE>
          
      --timeout-secs <TIMEOUT_SECS>
          [default: 90]
      --interval-secs <INTERVAL_SECS>
          Seconds between indexer polls while the settle is not yet due [default: 30]
      --once
          Print one decision and return, including `wait`
  -h, --help
          Print help
```

## ghostline channel refund

```text
`refund-due` using the file. `--seal-key-id-file` is the payer key in the opener wallet

Usage: ghostline channel refund [OPTIONS] --file <FILE>

Options:
      --file <FILE>
          
      --submit
          
      --dry-run
          
      --max-fee <MAX_FEE>
          Maximum revealed fee in micro-TARI (default: 100000) [default: 100000]
      --seal-key-id-file <SEAL_KEY_ID_FILE>
          
      --refund-output-hex <REFUND_OUTPUT_HEX>
          Optional public key that receives the expiry refund; defaults to the payer key
      --timeout-secs <TIMEOUT_SECS>
          [default: 90]
  -h, --help
          Print help
```

## ghostline sign

```text
Sign the next voucher. Requires a session file and policy unless `--unsafe-no-policy`

Usage: ghostline sign [OPTIONS] --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --payer-root-hex <PAYER_ROOT_HEX> --nonce-hex <NONCE_HEX>

Options:
      --network-tag <NETWORK_TAG>
          
      --deposit <DEPOSIT>
          Deposit in integer micro-units
      --payer-hex <PAYER_HEX>
          Hex-encoded payer public key (32 bytes)
      --payer-root-hex <PAYER_ROOT_HEX>
          Hex-encoded payer condition-tree root (32 bytes)
      --nonce-hex <NONCE_HEX>
          Hex-encoded 32-byte channel nonce
      --price <PRICE>
          Per-call increment (policy path). Mutually exclusive with `--amount`
      --amount <AMOUNT>
          Absolute cumulative amount. Only with `--unsafe-no-policy`
      --session <SESSION>
          
      --vendor-hex <VENDOR_HEX>
          
      --service-output-key-hex <SERVICE_OUTPUT_KEY_HEX>
          Optional one-time service payout key advertised by the service. Defaults to the vendor key
      --budget <BUDGET>
          
      --max-per-call <MAX_PER_CALL>
          
      --unsafe-no-policy
          Sign an absolute `--amount` without policy or session persistence
      --secret-hex <SECRET_HEX>
          Hex payer secret. Prefer `GHOSTLINE_PAYER_SECRET` or `-` on stdin
      --secret-file <SECRET_FILE>
          Private JSON key file created by `ghostline keygen --out`
  -h, --help
          Print help
```

## ghostline verify

```text
Verify a hex-encoded voucher against the given channel parameters

Usage: ghostline verify --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --payer-root-hex <PAYER_ROOT_HEX> --nonce-hex <NONCE_HEX> --voucher-hex <VOUCHER_HEX>

Options:
      --network-tag <NETWORK_TAG>        
      --deposit <DEPOSIT>                Deposit in integer micro-units
      --payer-hex <PAYER_HEX>            Hex-encoded payer public key (32 bytes)
      --payer-root-hex <PAYER_ROOT_HEX>  Hex-encoded payer condition-tree root (32 bytes)
      --nonce-hex <NONCE_HEX>            Hex-encoded 32-byte channel nonce
      --voucher-hex <VOUCHER_HEX>        Hex-encoded CBOR voucher from `ghostline sign`
  -h, --help                             Print help
```

## ghostline indexer

```text
Talk to a local Ootle indexer REST API (do not point this at public networks for attack tests)

Usage: ghostline indexer <COMMAND>

Commands:
  wait            GET /wait-until-ready
  epoch           GET /epoch-manager/stats — prints current_epoch
  submit          POST /transactions — hex is BOR-encoded sealed Transaction bytes
  dry-run         POST /transactions/dry-run
  result          GET /transactions/{id}/result — Pending, Finalized, or Rejected
  resolve-inputs  Fill `version: null` inputs and append fee-account vaults via `GET /substates/{id}`
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## ghostline indexer wait

```text
GET /wait-until-ready

Usage: ghostline indexer wait --url <URL>

Options:
      --url <URL>  
  -h, --help       Print help
```

## ghostline indexer epoch

```text
GET /epoch-manager/stats — prints current_epoch

Usage: ghostline indexer epoch --url <URL>

Options:
      --url <URL>  
  -h, --help       Print help
```

## ghostline indexer submit

```text
POST /transactions — hex is BOR-encoded sealed Transaction bytes

Usage: ghostline indexer submit --url <URL> --tx-hex <TX_HEX>

Options:
      --url <URL>        
      --tx-hex <TX_HEX>  
  -h, --help             Print help
```

## ghostline indexer dry-run

```text
POST /transactions/dry-run

Usage: ghostline indexer dry-run --url <URL> --tx-hex <TX_HEX>

Options:
      --url <URL>        
      --tx-hex <TX_HEX>  
  -h, --help             Print help
```

## ghostline indexer result

```text
GET /transactions/{id}/result — Pending, Finalized, or Rejected

Usage: ghostline indexer result --url <URL> --transaction-id <TRANSACTION_ID>

Options:
      --url <URL>                        
      --transaction-id <TRANSACTION_ID>  
  -h, --help                             Print help
```

## ghostline indexer resolve-inputs

```text
Fill `version: null` inputs and append fee-account vaults via `GET /substates/{id}`

Usage: ghostline indexer resolve-inputs [OPTIONS] --url <URL> --unsigned-json <UNSIGNED_JSON>

Options:
      --url <URL>                      
      --unsigned-json <UNSIGNED_JSON>  
      --out <OUT>                      Write resolved unsigned JSON here. Stdout is only the filled=/down=/added= line
  -h, --help                           Print help
```

## ghostline wallet

```text
Talk to a local `tari_walletd` JSON-RPC (`--authentication none`)

Usage: ghostline wallet <COMMAND>

Commands:
  payout-destination  Create a one-time payout destination for a service wallet address
  key-create          Allocate a fresh wallet-managed account signing key. Use one per service/channel settlement
  submit              Dry-run or submit an unsigned transaction (`seal-settle` / `seal-refund --unsigned-json`)
  utxos               Print stealth outputs from a wallet-stored transaction (`transactions.get`) as `unblind --utxo-json`
  open                Open a channel UTXO via `accounts.stealth_transfer` (`PayTo::Conditions`)
  account-create      Create a named wallet account (`accounts.create`), derive its keys, associate stealth tTARI. Fund it from an unlinked source before using it for settle fees
  account-ensure      Create or reuse a deterministic account after seed recovery, keeping its address and funds
  transfer            Move revealed XTR from one wallet account into another
  faucet-claim        Publish an account that is not on chain and fund it from the Esmeralda faucet
  account-get         Print a wallet account and its `owner_key_id` (`accounts.get`)
  help                Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## ghostline wallet payout-destination

```text
Create a one-time payout destination for a service wallet address.

Quote the printed `owner_key_hex` as `--service-output-key-hex` for one channel, and give the file to `settle-due --payout-destination-file` (or `channel init --payout-destination`). The settled payout is then addressed so that wallet's walletd finds and can spend it.

Usage: ghostline wallet payout-destination --address <ADDRESS> --ootle-network <OOTLE_NETWORK> --out <OUT>

Options:
      --address <ADDRESS>
          The service wallet's `otl_…` address (`wallet account-get` prints it)

      --ootle-network <OOTLE_NETWORK>
          `mainnet`, `esmeralda` or `localnet`; must match the address

      --out <OUT>
          Owner-only output file. It holds the payout's secret nonce

  -h, --help
          Print help (see a summary with '-h')
```

## ghostline wallet key-create

```text
Allocate a fresh wallet-managed account signing key. Use one per service/channel settlement

Usage: ghostline wallet key-create [OPTIONS]

Options:
      --url <URL>  [default: http://127.0.0.1:5100]
      --out <OUT>  Save the new wallet KeyId JSON as an owner-only file for `--seal-key-id-file`
  -h, --help       Print help
```

## ghostline wallet submit

```text
Dry-run or submit an unsigned transaction (`seal-settle` / `seal-refund --unsigned-json`)

Usage: ghostline wallet submit [OPTIONS] --unsigned-json <UNSIGNED_JSON>

Options:
      --url <URL>
          [default: http://127.0.0.1:5100]
      --unsigned-json <UNSIGNED_JSON>
          Path to unsigned JSON, or `-` for stdin
      --seal-key-id <SEAL_KEY_ID>
          `KeyId` JSON for the sealer (service key imported into the wallet)
      --seal-key-id-file <SEAL_KEY_ID_FILE>
          
      --other-signer <OTHER_SIGNER>
          Extra `KeyId` JSON. Not implied: pass this or `--fee-account-name`
      --fee-account-name <FEE_ACCOUNT_NAME>
          Wallet account whose `owner_key_id` co-signs `pay_fee` (service fee account, not the opener)
      --dry-run
          
      --no-wait
          Skip `transactions.wait_result` after a live submit
      --timeout-secs <TIMEOUT_SECS>
          [default: 60]
  -h, --help
          Print help
```

## ghostline wallet utxos

```text
Print stealth outputs from a wallet-stored transaction (`transactions.get`) as `unblind --utxo-json`

Usage: ghostline wallet utxos [OPTIONS] --transaction-id <TRANSACTION_ID>

Options:
      --url <URL>
          [default: http://127.0.0.1:5100]
      --transaction-id <TRANSACTION_ID>
          
      --utxo-json-out <UTXO_JSON_OUT>
          Write the channel deposit (`Script` auth) JSON for `unblind --utxo-json`
  -h, --help
          Print help
```

## ghostline wallet open

```text
Open a channel UTXO via `accounts.stealth_transfer` (`PayTo::Conditions`)

Usage: ghostline wallet open [OPTIONS] --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG> --account-name <ACCOUNT_NAME>

Options:
      --network-tag <NETWORK_TAG>
          
      --deposit <DEPOSIT>
          
      --payer-hex <PAYER_HEX>
          
      --service-hex <SERVICE_HEX>
          
      --refund-hex <REFUND_HEX>
          Optional public key for the expiry-refund branch. Defaults to the payer key
      --expiry-epoch <EXPIRY_EPOCH>
          
      --nonce-hex <NONCE_HEX>
          
      --template-hex <TEMPLATE_HEX>
          
      --fee-config <FEE_CONFIG>
          Fee config component the channel template pins (`component_<hex>` or raw hex)
      --url <URL>
          [default: http://127.0.0.1:5100]
      --account-name <ACCOUNT_NAME>
          Wallet account name used as `owner_account`
      --destination-address <DESTINATION_ADDRESS>
          Ootle address that receives (and encrypts) the output. Default: `accounts.get_default`
      --input-selection <INPUT_SELECTION>
          [default: PreferConfidential]
      --fee-input-selection <FEE_INPUT_SELECTION>
          [default: PreferRevealed]
      --max-fee <MAX_FEE>
          Maximum µT fee. Required for a live open; dry-runs default to 100000 µT
      --resource-hex <RESOURCE_HEX>
          
      --dry-run
          
      --no-wait
          
      --timeout-secs <TIMEOUT_SECS>
          [default: 60]
      --utxo-json-out <UTXO_JSON_OUT>
          Write the channel deposit (`Script` auth) JSON for `unblind --utxo-json` after Accept
  -h, --help
          Print help
```

## ghostline wallet account-create

```text
Create a named wallet account (`accounts.create`), derive its keys, associate stealth tTARI. Fund it from an unlinked source before using it for settle fees

Usage: ghostline wallet account-create [OPTIONS] --name <NAME>

Options:
      --url <URL>              [default: http://127.0.0.1:5100]
      --name <NAME>            
      --key-index <KEY_INDEX>  
      --is-default             
  -h, --help                   Print help
```

## ghostline wallet account-ensure

```text
Create or reuse a deterministic account after seed recovery, keeping its address and funds

Usage: ghostline wallet account-ensure [OPTIONS] --name <NAME> --key-index <KEY_INDEX>

Options:
      --url <URL>              [default: http://127.0.0.1:5100]
      --name <NAME>            
      --key-index <KEY_INDEX>  
      --is-default             
  -h, --help                   Print help
```

## ghostline wallet transfer

```text
Move revealed XTR from one wallet account into another.

`accounts.transfer` also publishes the destination component when it is not on chain. Use this when the Esmeralda faucet cannot fund the fee account.

Usage: ghostline wallet transfer [OPTIONS] --from <FROM> --to <TO> --amount <AMOUNT> --max-fee <MAX_FEE>

Options:
      --url <URL>
          [default: http://127.0.0.1:5100]

      --from <FROM>
          

      --to <TO>
          

      --amount <AMOUNT>
          Revealed µT to deposit in the destination vault

      --max-fee <MAX_FEE>
          Maximum µT fee, paid from the source account

  -h, --help
          Print help (see a summary with '-h')
```

## ghostline wallet faucet-claim

```text
Publish an account that is not on chain and fund it from the Esmeralda faucet.

Calls walletd 0.47 `accounts.create_free_test_coins`. The handler creates the component, takes the faucet's fixed 1,000 TARI, and pays `max_fee` from that new account. One claim per account key. The claim and its fee are public.

Usage: ghostline wallet faucet-claim [OPTIONS] --name <NAME> --max-fee <MAX_FEE>

Options:
      --url <URL>
          [default: http://127.0.0.1:5100]

      --name <NAME>
          

      --max-fee <MAX_FEE>
          Maximum µT fee paid from the new account. The hosted relay uses 100000

  -h, --help
          Print help (see a summary with '-h')
```

## ghostline wallet account-get

```text
Print a wallet account and its `owner_key_id` (`accounts.get`)

Usage: ghostline wallet account-get [OPTIONS] --name <NAME>

Options:
      --url <URL>    [default: http://127.0.0.1:5100]
      --name <NAME>  
  -h, --help         Print help
```

## ghostline spec

```text
Print payer_root and channel_id from a full channel spec (computes the tree)

Usage: ghostline spec [OPTIONS] --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG>

Options:
      --network-tag <NETWORK_TAG>    
      --deposit <DEPOSIT>            
      --payer-hex <PAYER_HEX>        
      --service-hex <SERVICE_HEX>    
      --refund-hex <REFUND_HEX>      Optional public key for the expiry-refund branch. Defaults to the payer key
      --expiry-epoch <EXPIRY_EPOCH>  
      --nonce-hex <NONCE_HEX>        
      --template-hex <TEMPLATE_HEX>  
      --fee-config <FEE_CONFIG>      Fee config component the channel template pins (`component_<hex>` or raw hex)
  -h, --help                         Print help
```

## ghostline pay-to

```text
Print `PayTo::Conditions` JSON for `accounts.stealth_transfer`

Usage: ghostline pay-to [OPTIONS] --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG>

Options:
      --network-tag <NETWORK_TAG>    
      --deposit <DEPOSIT>            
      --payer-hex <PAYER_HEX>        
      --service-hex <SERVICE_HEX>    
      --refund-hex <REFUND_HEX>      Optional public key for the expiry-refund branch. Defaults to the payer key
      --expiry-epoch <EXPIRY_EPOCH>  
      --nonce-hex <NONCE_HEX>        
      --template-hex <TEMPLATE_HEX>  
      --fee-config <FEE_CONFIG>      Fee config component the channel template pins (`component_<hex>` or raw hex)
  -h, --help                         Print help
```

## ghostline keygen

```text
Print a random Ristretto secret and matching public key as hex

Usage: ghostline keygen [OPTIONS]

Options:
      --out <OUT>    Save the generated keypair in an owner-only JSON file instead of printing its secret
      --role <ROLE>  Label the key's intended use: `payer` signs vouchers, `claim` unblinds a channel deposit (`channel init --claim-key-file`). Service settlement keys are created by `wallet key-create` [possible values: payer, claim]
  -h, --help         Print help
```

## ghostline seal-open

```text
Seal an open (key-path input → PayTo::Conditions). Prints tx= and deposit_mask=

Usage: ghostline seal-open [OPTIONS] --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG> --input-value <INPUT_VALUE>

Options:
      --network-tag <NETWORK_TAG>
          
      --deposit <DEPOSIT>
          
      --payer-hex <PAYER_HEX>
          
      --service-hex <SERVICE_HEX>
          
      --refund-hex <REFUND_HEX>
          Optional public key for the expiry-refund branch. Defaults to the payer key
      --expiry-epoch <EXPIRY_EPOCH>
          
      --nonce-hex <NONCE_HEX>
          
      --template-hex <TEMPLATE_HEX>
          
      --fee-config <FEE_CONFIG>
          Fee config component the channel template pins (`component_<hex>` or raw hex)
      --input-value <INPUT_VALUE>
          
      --input-mask-hex <INPUT_MASK_HEX>
          
      --seal-secret-hex <SEAL_SECRET_HEX>
          
      --max-epoch <MAX_EPOCH>
          [default: 1]
      --resource-hex <RESOURCE_HEX>
          
      --ootle-network <OOTLE_NETWORK>
          Ootle network for UTXO tags and the sealed transaction (`localnet`, `esmeralda`, or `mainnet`) [default: localnet]
  -h, --help
          Print help
```

## ghostline seal-settle

```text
Seal a settle of a voucher against the channel UTXO

Usage: ghostline seal-settle [OPTIONS] --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG> --voucher-hex <VOUCHER_HEX>

Options:
      --network-tag <NETWORK_TAG>
          
      --deposit <DEPOSIT>
          
      --payer-hex <PAYER_HEX>
          
      --service-hex <SERVICE_HEX>
          
      --refund-hex <REFUND_HEX>
          Optional public key for the expiry-refund branch. Defaults to the payer key
      --expiry-epoch <EXPIRY_EPOCH>
          
      --nonce-hex <NONCE_HEX>
          
      --template-hex <TEMPLATE_HEX>
          
      --fee-config <FEE_CONFIG>
          Fee config component the channel template pins (`component_<hex>` or raw hex)
      --voucher-hex <VOUCHER_HEX>
          
      --deposit-mask-hex <DEPOSIT_MASK_HEX>
          
      --seal-secret-hex <SEAL_SECRET_HEX>
          
      --max-epoch <MAX_EPOCH>
          [default: 1]
      --resource-hex <RESOURCE_HEX>
          
      --ootle-network <OOTLE_NETWORK>
          Ootle network for UTXO tags and the sealed transaction (`localnet`, `esmeralda`, or `mainnet`) [default: localnet]
      --fee-account <FEE_ACCOUNT>
          Account component that pays revealed XTR fees (`pay_fee`). On settle this should be a service-owned account, not the opener
      --max-fee <MAX_FEE>
          [default: 100000]
      --fee-secret-hex <FEE_SECRET_HEX>
          
      --unsigned-json
          Print unsigned JSON for `transactions.submit` / `submit_dry_run` instead of a sealed hex
  -h, --help
          Print help
```

## ghostline seal-unsigned

```text
Seal an unsigned JSON after `indexer resolve-inputs` (indexer path; versions must be set)

Usage: ghostline seal-unsigned [OPTIONS] --unsigned-json <UNSIGNED_JSON>

Options:
      --unsigned-json <UNSIGNED_JSON>
          
      --seal-secret-hex <SEAL_SECRET_HEX>
          
      --fee-secret-hex <FEE_SECRET_HEX>
          
      --dry-run
          Set `dry_run` on the unsigned body (required for `indexer dry-run`)
  -h, --help
          Print help
```

## ghostline seal-refund

```text
Seal a refund of the channel UTXO after expiry

Usage: ghostline seal-refund [OPTIONS] --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG>

Options:
      --network-tag <NETWORK_TAG>
          
      --deposit <DEPOSIT>
          
      --payer-hex <PAYER_HEX>
          
      --service-hex <SERVICE_HEX>
          
      --refund-hex <REFUND_HEX>
          Optional public key for the expiry-refund branch. Defaults to the payer key
      --expiry-epoch <EXPIRY_EPOCH>
          
      --nonce-hex <NONCE_HEX>
          
      --template-hex <TEMPLATE_HEX>
          
      --fee-config <FEE_CONFIG>
          Fee config component the channel template pins (`component_<hex>` or raw hex)
      --deposit-mask-hex <DEPOSIT_MASK_HEX>
          
      --seal-secret-hex <SEAL_SECRET_HEX>
          
      --max-epoch <MAX_EPOCH>
          [default: 1]
      --resource-hex <RESOURCE_HEX>
          
      --ootle-network <OOTLE_NETWORK>
          Ootle network for UTXO tags and the sealed transaction (`localnet`, `esmeralda`, or `mainnet`) [default: localnet]
      --fee-account <FEE_ACCOUNT>
          Account component that pays revealed XTR fees (`pay_fee`). On refund this is the payer's fee account
      --max-fee <MAX_FEE>
          [default: 100000]
      --fee-secret-hex <FEE_SECRET_HEX>
          
      --unsigned-json
          Print unsigned JSON for `wallet submit` / `indexer resolve-inputs` instead of a sealed hex
  -h, --help
          Print help
```

## ghostline unblind

```text
Recover value and mask from a stealth output encrypted to the claim secret

Usage: ghostline unblind [OPTIONS]

Options:
      --commitment-hex <COMMITMENT_HEX>
          
      --nonce-hex <NONCE_HEX>
          
      --encrypted-hex <ENCRYPTED_HEX>
          
      --tag <TAG>
          
      --utxo-json <UTXO_JSON>
          JSON with `commitment`, `nonce`, `encrypted_hex`, `tag` (u32 or signed sqlite INTEGER), optional `value`
      --resource-hex <RESOURCE_HEX>
          
      --secret-hex <SECRET_HEX>
          
      --ootle-network <OOTLE_NETWORK>
          Ootle network for UTXO tags and the sealed transaction (`localnet`, `esmeralda`, or `mainnet`) [default: localnet]
  -h, --help
          Print help
```

## ghostline unblind-tx

```text
Recover every stealth output in a sealed tx hex that matches the claim secret

Usage: ghostline unblind-tx [OPTIONS] --tx-hex <TX_HEX>

Options:
      --tx-hex <TX_HEX>          
      --secret-hex <SECRET_HEX>  
  -h, --help                     Print help
```

## ghostline inspect-tx

```text
Print stealth output fields from a sealed tx hex (no secret)

Usage: ghostline inspect-tx --tx-hex <TX_HEX>

Options:
      --tx-hex <TX_HEX>  
  -h, --help             Print help
```

## ghostline status

```text
Off-chain channel totals plus the observer panel (not a chain confirmation)

Usage: ghostline status [OPTIONS]

Options:
      --session <SESSION>
          Payer session JSON (`ChannelStore`)
      --ledger <LEDGER>
          Service ledger JSON (`ChannelStore`). Combined with `--session` when both are set
      --store <STORE>
          Single store file (session or ledger). Do not combine with `--session` / `--ledger`
      --expiry-epoch <EXPIRY_EPOCH>
          
      --indexer-url <INDEXER_URL>
          Fetch `current_epoch` from this indexer (read-only). Also used for `--commitment-hex`
      --include-params
          Include payer_hex, nonce, and payer_root (operator-only; omit from public reports)
      --commitment-hex <COMMITMENT_HEX>
          Channel output commitment. With `--indexer-url`, `GET /substates` for that UTXO
      --utxo-json <UTXO_JSON>
          Wallet `utxo-json` whose `commitment` is the channel output
      --resource-hex <RESOURCE_HEX>
          
  -h, --help
          Print help
```

## ghostline report

```text
Same JSON as `status`. Alias for operator export

Usage: ghostline report [OPTIONS]

Options:
      --session <SESSION>                
      --ledger <LEDGER>                  
      --store <STORE>                    
      --expiry-epoch <EXPIRY_EPOCH>      
      --indexer-url <INDEXER_URL>        
      --include-params                   
      --commitment-hex <COMMITMENT_HEX>  
      --utxo-json <UTXO_JSON>            
      --resource-hex <RESOURCE_HEX>      
  -h, --help                             Print help
```

## ghostline watch

```text
Compare current epoch to channel expiry (settlement-window heartbeat)

Usage: ghostline watch [OPTIONS] --expiry-epoch <EXPIRY_EPOCH> --indexer-url <INDEXER_URL>

Options:
      --expiry-epoch <EXPIRY_EPOCH>    
      --indexer-url <INDEXER_URL>      
      --margin-epochs <MARGIN_EPOCHS>  `within_margin` is true when epochs until expiry is at most this value (including overdue) [default: 0]
  -h, --help                           Print help
```

## ghostline settle-due

```text
Build (and optionally submit) a settle of the ledger's latest voucher before expiry.

Does nothing at or after `expiry_epoch` (a live settle there is `OnlyFeeAccepted`). Default is dry: print the decision and, with `--out`, write unsigned JSON. `--submit` sends it.

Usage: ghostline settle-due [OPTIONS] --ledger <LEDGER> --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG> --indexer-url <INDEXER_URL> --max-epoch <MAX_EPOCH> --fee-account <FEE_ACCOUNT>

Options:
      --ledger <LEDGER>
          

      --network-tag <NETWORK_TAG>
          

      --deposit <DEPOSIT>
          

      --payer-hex <PAYER_HEX>
          

      --service-hex <SERVICE_HEX>
          

      --refund-hex <REFUND_HEX>
          Optional public key for the expiry-refund branch. Defaults to the payer key

      --expiry-epoch <EXPIRY_EPOCH>
          

      --nonce-hex <NONCE_HEX>
          

      --template-hex <TEMPLATE_HEX>
          

      --fee-config <FEE_CONFIG>
          Fee config component the channel template pins (`component_<hex>` or raw hex)

      --indexer-url <INDEXER_URL>
          

      --margin-epochs <MARGIN_EPOCHS>
          Settle when this many epochs (or fewer) remain, and the epoch is still strictly before expiry
          
          [default: 10]

      --force
          Settle now while the window is still open, even outside `--margin-epochs`. Does not override expiry

      --commitment-hex <COMMITMENT_HEX>
          If set with the indexer, refuse to build a settle when this output is down

      --utxo-json <UTXO_JSON>
          

      --deposit-mask-hex <DEPOSIT_MASK_HEX>
          

      --max-epoch <MAX_EPOCH>
          

      --resource-hex <RESOURCE_HEX>
          

      --ootle-network <OOTLE_NETWORK>
          Ootle network for UTXO tags and the sealed transaction (`localnet`, `esmeralda`, or `mainnet`)
          
          [default: localnet]

      --fee-account <FEE_ACCOUNT>
          

      --max-fee <MAX_FEE>
          [default: 100000]

      --out <OUT>
          Write unsigned JSON here when the decision is `due`

      --submit
          Submit the unsigned settle to a wallet daemon. Without this flag nothing is sent

      --wallet-url <WALLET_URL>
          [default: http://127.0.0.1:5100]

      --seal-key-id-file <SEAL_KEY_ID_FILE>
          

      --fee-account-name <FEE_ACCOUNT_NAME>
          

      --dry-run
          With `--submit`, dry-run the wallet instead of broadcasting

      --timeout-secs <TIMEOUT_SECS>
          [default: 90]

      --payout-destination-file <PAYOUT_DESTINATION_FILE>
          Address the payout to the service wallet in this `wallet payout-destination` file

  -h, --help
          Print help (see a summary with '-h')
```

## ghostline refund-due

```text
Build (and optionally submit) a refund once `current_epoch >= expiry_epoch`.

Before that epoch it does not submit, even with `--submit`. A spent output (`--utxo-json`) is `output_spent`.

Usage: ghostline refund-due [OPTIONS] --network-tag <NETWORK_TAG> --deposit <DEPOSIT> --payer-hex <PAYER_HEX> --service-hex <SERVICE_HEX> --expiry-epoch <EXPIRY_EPOCH> --nonce-hex <NONCE_HEX> --template-hex <TEMPLATE_HEX> --fee-config <FEE_CONFIG> --indexer-url <INDEXER_URL> --max-epoch <MAX_EPOCH>

Options:
      --network-tag <NETWORK_TAG>
          

      --deposit <DEPOSIT>
          

      --payer-hex <PAYER_HEX>
          

      --service-hex <SERVICE_HEX>
          

      --refund-hex <REFUND_HEX>
          Optional public key for the expiry-refund branch. Defaults to the payer key

      --expiry-epoch <EXPIRY_EPOCH>
          

      --nonce-hex <NONCE_HEX>
          

      --template-hex <TEMPLATE_HEX>
          

      --fee-config <FEE_CONFIG>
          Fee config component the channel template pins (`component_<hex>` or raw hex)

      --indexer-url <INDEXER_URL>
          

      --commitment-hex <COMMITMENT_HEX>
          

      --utxo-json <UTXO_JSON>
          

      --deposit-mask-hex <DEPOSIT_MASK_HEX>
          

      --max-epoch <MAX_EPOCH>
          

      --resource-hex <RESOURCE_HEX>
          

      --ootle-network <OOTLE_NETWORK>
          Ootle network for UTXO tags and the sealed transaction (`localnet`, `esmeralda`, or `mainnet`)
          
          [default: localnet]

      --fee-account <FEE_ACCOUNT>
          Payer account component that pays revealed XTR fees. Required when the refund is due

      --max-fee <MAX_FEE>
          [default: 100000]

      --out <OUT>
          

      --submit
          

      --wallet-url <WALLET_URL>
          [default: http://127.0.0.1:5100]

      --seal-key-id-file <SEAL_KEY_ID_FILE>
          Payer key imported into the wallet (`GeneralPurpose`)

      --fee-account-name <FEE_ACCOUNT_NAME>
          

      --dry-run
          

      --timeout-secs <TIMEOUT_SECS>
          [default: 90]

      --refund-address <REFUND_ADDRESS>
          Pay the refunded deposit to this payer wallet `otl_…` address so its walletd finds it

  -h, --help
          Print help (see a summary with '-h')
```
