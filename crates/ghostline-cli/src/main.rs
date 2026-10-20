// SPDX-License-Identifier: MIT

mod channel_file;
mod seal;
mod secrets;

use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use clap::{Parser, Subcommand};
#[cfg(test)]
use ghostline_core::{CHANNEL_ID_LEN, NONCE_LEN};
use ghostline_core::{ChannelParams, SignedVoucher, from_hex, to_hex};
use ghostline_net::{
    AccountsCreateRequest, ChannelOpenParams, IndexerClient, StealthTransferRequest,
    SubstatePresence, TransactionEnvelope, TransactionId, UtxoInputSelection, WalletClient,
    WalletStealthUtxo, WalletSubmitRequest, channel_open_utxo, execution_accepted,
    indexer_result_status, settle_fee_signers, stealth_utxos_from_wallet_tx,
};
#[cfg(test)]
use ghostline_sdk::OutputScan;
use ghostline_sdk::{
    MaskAndValue, UnsignedTransaction, inspect_transaction, unblind_output, unblind_transaction,
};
use serde::Deserialize;
use tari_utilities::ByteArray;

#[derive(Parser)]
#[command(
    name = "ghostline",
    about = "Sign and verify Ghostline vouchers, seal stealth transfers, inspect/unblind stealth outputs, operator status/watch, open/settle via a wallet daemon or indexer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the 32-byte channel id for the given open parameters.
    ChannelId(ParamsArgs),
    /// One channel record for open, status, settle, and refund.
    Channel {
        #[command(subcommand)]
        command: ChannelCommand,
    },
    /// Sign the next voucher. Requires a session file and policy unless `--unsafe-no-policy`.
    Sign {
        #[command(flatten)]
        params: ParamsArgs,
        /// Per-call increment (policy path). Mutually exclusive with `--amount`.
        #[arg(long)]
        price: Option<u64>,
        /// Absolute cumulative amount. Only with `--unsafe-no-policy`.
        #[arg(long)]
        amount: Option<u64>,
        #[arg(long)]
        session: Option<PathBuf>,
        #[arg(long)]
        vendor_hex: Option<String>,
        /// Optional one-time service payout key advertised by the service. Defaults to the vendor key.
        #[arg(long)]
        service_output_key_hex: Option<String>,
        #[arg(long)]
        budget: Option<u64>,
        #[arg(long)]
        max_per_call: Option<u64>,
        /// Sign an absolute `--amount` without policy or session persistence.
        #[arg(long)]
        unsafe_no_policy: bool,
        /// Hex payer secret. Prefer `GHOSTLINE_PAYER_SECRET` or `-` on stdin.
        #[arg(long)]
        secret_hex: Option<String>,
        /// Private JSON key file created by `ghostline keygen --out`.
        #[arg(long, conflicts_with = "secret_hex")]
        secret_file: Option<PathBuf>,
    },
    /// Verify a hex-encoded voucher against the given channel parameters.
    Verify {
        #[command(flatten)]
        params: ParamsArgs,
        /// Hex-encoded CBOR voucher from `ghostline sign`.
        #[arg(long)]
        voucher_hex: String,
    },
    /// Talk to a local Ootle indexer REST API (do not point this at public networks for attack tests).
    Indexer {
        #[command(subcommand)]
        command: IndexerCommand,
    },
    /// Talk to a local `tari_walletd` JSON-RPC (`--authentication none`).
    Wallet {
        #[command(subcommand)]
        command: WalletCommand,
    },
    /// Print payer_root and channel_id from a full channel spec (computes the tree).
    Spec(OpenArgs),
    /// Print `PayTo::Conditions` JSON for `accounts.stealth_transfer`.
    PayTo(OpenArgs),
    /// Print a random Ristretto secret and matching public key as hex.
    Keygen {
        /// Save the generated keypair in an owner-only JSON file instead of printing its secret.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Label the key's intended use. Service settlement keys are created by `wallet key-create`.
        #[arg(long, value_parser = ["payer"])]
        role: Option<String>,
    },
    /// Seal an open (key-path input → PayTo::Conditions). Prints tx= and deposit_mask=.
    SealOpen {
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long)]
        input_value: u64,
        #[arg(long)]
        input_mask_hex: Option<String>,
        #[arg(long)]
        seal_secret_hex: Option<String>,
        #[arg(long, default_value_t = 1)]
        max_epoch: u64,
        #[arg(long)]
        resource_hex: Option<String>,
        /// Ootle network used for UTXO tags and the sealed transaction (`localnet` or `esmeralda`).
        #[arg(long, default_value = "localnet")]
        ootle_network: String,
    },
    /// Seal a settle of a voucher against the channel UTXO.
    SealSettle {
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long)]
        voucher_hex: String,
        #[arg(long)]
        deposit_mask_hex: Option<String>,
        #[arg(long)]
        seal_secret_hex: Option<String>,
        #[arg(long, default_value_t = 1)]
        max_epoch: u64,
        #[arg(long)]
        resource_hex: Option<String>,
        #[arg(long, default_value = "localnet")]
        ootle_network: String,
        /// Account component that pays revealed XTR fees (`pay_fee`). On settle this should be a service-owned account, not the opener.
        #[arg(long)]
        fee_account: Option<String>,
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
        #[arg(long)]
        fee_secret_hex: Option<String>,
        /// Print unsigned JSON for `transactions.submit` / `submit_dry_run` instead of a sealed hex.
        #[arg(long)]
        unsigned_json: bool,
    },
    /// Seal an unsigned JSON after `indexer resolve-inputs` (indexer path; versions must be set).
    SealUnsigned {
        #[arg(long)]
        unsigned_json: PathBuf,
        #[arg(long)]
        seal_secret_hex: Option<String>,
        #[arg(long)]
        fee_secret_hex: Option<String>,
        /// Set `dry_run` on the unsigned body (required for `indexer dry-run`).
        #[arg(long)]
        dry_run: bool,
    },
    /// Seal a refund of the channel UTXO after expiry.
    SealRefund {
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long)]
        deposit_mask_hex: Option<String>,
        #[arg(long)]
        seal_secret_hex: Option<String>,
        #[arg(long, default_value_t = 1)]
        max_epoch: u64,
        #[arg(long)]
        resource_hex: Option<String>,
        #[arg(long, default_value = "localnet")]
        ootle_network: String,
        /// Account component that pays revealed XTR fees (`pay_fee`). On refund this is the payer's fee account.
        #[arg(long)]
        fee_account: Option<String>,
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
        #[arg(long)]
        fee_secret_hex: Option<String>,
        /// Print unsigned JSON for `wallet submit` / `indexer resolve-inputs` instead of a sealed hex.
        #[arg(long)]
        unsigned_json: bool,
    },
    /// Recover value and mask from a stealth output encrypted to the claim secret.
    Unblind {
        #[arg(long)]
        commitment_hex: Option<String>,
        #[arg(long)]
        nonce_hex: Option<String>,
        #[arg(long)]
        encrypted_hex: Option<String>,
        #[arg(long)]
        tag: Option<i64>,
        /// JSON with `commitment`, `nonce`, `encrypted_hex`, `tag` (u32 or signed sqlite INTEGER), optional `value`.
        #[arg(long)]
        utxo_json: Option<PathBuf>,
        #[arg(long)]
        resource_hex: Option<String>,
        #[arg(long)]
        secret_hex: Option<String>,
        #[arg(long, default_value = "localnet")]
        ootle_network: String,
    },
    /// Recover every stealth output in a sealed tx hex that matches the claim secret.
    UnblindTx {
        #[arg(long)]
        tx_hex: String,
        #[arg(long)]
        secret_hex: Option<String>,
    },
    /// Print stealth output fields from a sealed tx hex (no secret).
    InspectTx {
        #[arg(long)]
        tx_hex: String,
    },
    /// Off-chain channel totals plus the observer panel (not a chain confirmation).
    Status {
        /// Payer session JSON (`ChannelStore`).
        #[arg(long)]
        session: Option<PathBuf>,
        /// Service ledger JSON (`ChannelStore`). Combined with `--session` when both are set.
        #[arg(long)]
        ledger: Option<PathBuf>,
        /// Single store file (session or ledger). Do not combine with `--session` / `--ledger`.
        #[arg(long)]
        store: Option<PathBuf>,
        #[arg(long)]
        expiry_epoch: Option<u64>,
        /// Fetch `current_epoch` from this indexer (read-only). Also used for `--commitment-hex`.
        #[arg(long)]
        indexer_url: Option<String>,
        /// Include payer_hex, nonce, and payer_root (operator-only; omit from public reports).
        #[arg(long)]
        include_params: bool,
        /// Channel output commitment. With `--indexer-url`, `GET /substates` for that UTXO.
        #[arg(long)]
        commitment_hex: Option<String>,
        /// Wallet `utxo-json` whose `commitment` is the channel output.
        #[arg(long)]
        utxo_json: Option<PathBuf>,
        #[arg(long)]
        resource_hex: Option<String>,
    },
    /// Same JSON as `status`. Alias for operator export.
    Report {
        #[arg(long)]
        session: Option<PathBuf>,
        #[arg(long)]
        ledger: Option<PathBuf>,
        #[arg(long)]
        store: Option<PathBuf>,
        #[arg(long)]
        expiry_epoch: Option<u64>,
        #[arg(long)]
        indexer_url: Option<String>,
        #[arg(long)]
        include_params: bool,
        #[arg(long)]
        commitment_hex: Option<String>,
        #[arg(long)]
        utxo_json: Option<PathBuf>,
        #[arg(long)]
        resource_hex: Option<String>,
    },
    /// Compare current epoch to channel expiry (settlement-window heartbeat).
    Watch {
        #[arg(long)]
        expiry_epoch: u64,
        #[arg(long)]
        indexer_url: String,
        /// `within_margin` is true when epochs until expiry is at most this value (including overdue).
        #[arg(long, default_value_t = 0)]
        margin_epochs: u64,
    },
    /// Build (and optionally submit) a settle of the ledger's latest voucher before expiry.
    ///
    /// Does nothing at or after `expiry_epoch` (a live settle there is `OnlyFeeAccepted`).
    /// Default is dry: print the decision and, with `--out`, write unsigned JSON. `--submit` sends it.
    SettleDue {
        #[arg(long)]
        ledger: PathBuf,
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long)]
        indexer_url: String,
        /// Settle when this many epochs (or fewer) remain, and the epoch is still strictly before expiry.
        #[arg(long, default_value_t = 10)]
        margin_epochs: u64,
        /// Settle now while the window is still open, even outside `--margin-epochs`. Does not override expiry.
        #[arg(long)]
        force: bool,
        /// If set with the indexer, refuse to build a settle when this output is down.
        #[arg(long)]
        commitment_hex: Option<String>,
        #[arg(long)]
        utxo_json: Option<PathBuf>,
        #[arg(long)]
        deposit_mask_hex: Option<String>,
        #[arg(long)]
        max_epoch: u64,
        #[arg(long)]
        resource_hex: Option<String>,
        #[arg(long, default_value = "localnet")]
        ootle_network: String,
        #[arg(long)]
        fee_account: String,
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
        /// Write unsigned JSON here when the decision is `due`.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Submit the unsigned settle to a wallet daemon. Without this flag nothing is sent.
        #[arg(long)]
        submit: bool,
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        wallet_url: String,
        #[arg(long)]
        seal_key_id_file: Option<PathBuf>,
        #[arg(long)]
        fee_account_name: Option<String>,
        /// With `--submit`, dry-run the wallet instead of broadcasting.
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 90)]
        timeout_secs: u64,
    },
    /// Build (and optionally submit) a refund once `current_epoch >= expiry_epoch`.
    ///
    /// Before that epoch it does not submit, even with `--submit`. A spent output (`--utxo-json`) is `output_spent`.
    RefundDue {
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long)]
        indexer_url: String,
        #[arg(long)]
        commitment_hex: Option<String>,
        #[arg(long)]
        utxo_json: Option<PathBuf>,
        #[arg(long)]
        deposit_mask_hex: Option<String>,
        #[arg(long)]
        max_epoch: u64,
        #[arg(long)]
        resource_hex: Option<String>,
        #[arg(long, default_value = "localnet")]
        ootle_network: String,
        /// Payer account component that pays revealed XTR fees. Required when the refund is due.
        #[arg(long)]
        fee_account: Option<String>,
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        submit: bool,
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        wallet_url: String,
        /// Payer key imported into the wallet (`GeneralPurpose`).
        #[arg(long)]
        seal_key_id_file: Option<PathBuf>,
        #[arg(long)]
        fee_account_name: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 90)]
        timeout_secs: u64,
    },
}

#[derive(Subcommand)]
enum ChannelCommand {
    /// Write a channel file. No network call. The file has no keys until `channel open` or `channel bind`.
    Init {
        #[arg(long)]
        out: PathBuf,
        #[command(flatten)]
        open: Box<OpenArgs>,
        #[arg(long)]
        indexer_url: String,
        #[arg(long, default_value = "localnet")]
        ootle_network: String,
        #[arg(long)]
        max_epoch: u64,
        #[arg(long, default_value = "ghostline")]
        opener_account: String,
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        opener_wallet_url: String,
        #[arg(long)]
        fee_account: Option<String>,
        #[arg(long)]
        fee_account_name: Option<String>,
        #[arg(long)]
        fee_wallet_url: Option<String>,
        #[arg(long)]
        payer_fee_account: Option<String>,
        #[arg(long)]
        payer_fee_account_name: Option<String>,
        #[arg(long)]
        payer_wallet_url: Option<String>,
        #[arg(long)]
        ledger: Option<PathBuf>,
        /// Payer session JSON. Used by MCP when `GHOSTLINE_SESSION` is unset.
        #[arg(long)]
        session: Option<PathBuf>,
    },
    /// Wallet-open the channel, unblind the output, and store the mask on the file.
    Open {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        dry_run: bool,
        /// Maximum µT fee. Required for a live open; dry-runs default to 100000 µT.
        #[arg(long)]
        max_fee: Option<u64>,
        #[arg(long, default_value_t = 90)]
        timeout_secs: u64,
    },
    /// Create the payer session from this file, or keep it when the channel id already matches.
    Session {
        #[arg(long)]
        file: PathBuf,
    },
    /// Unblind an existing `--utxo-json` into the channel file.
    Bind {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        utxo_json: PathBuf,
    },
    /// `status` using the file's ledger, expiry, indexer, and UTXO.
    Status {
        #[arg(long)]
        file: PathBuf,
    },
    /// `settle-due` using the file. `--seal-key-id-file` is the service key in the fee wallet.
    Settle {
        #[arg(long)]
        file: PathBuf,
        #[arg(long, default_value_t = 10)]
        margin_epochs: u64,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        submit: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        seal_key_id_file: Option<PathBuf>,
        /// Maximum µT fee. Review the dry-run requirement and set an acceptable cap before submitting.
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
        #[arg(long, default_value_t = 90)]
        timeout_secs: u64,
    },
    /// One row per channel file in a directory. Skips JSON that is not a channel record.
    List {
        #[arg(long)]
        dir: PathBuf,
    },
    /// Accounting export of channel files. Default columns omit channel id, keys, and the deposit mask.
    ///
    /// `--disclose` adds `channel_id`, `cumulative`, `has_latest_voucher`, `epochs_until_expiry`, or `current_epoch`.
    Report {
        #[arg(long)]
        dir: PathBuf,
        /// `csv` or `json`.
        #[arg(long, default_value = "csv")]
        format: String,
        #[arg(long)]
        disclose: Vec<String>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Per-call prices from each channel file's ledger. No voucher hex.
    Calls {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Create a shareable privacy receipt from the local call log and the chain transactions you name.
    /// No mask or voucher hex is included; the receipt is an operator summary, not a cryptographic proof.
    Trace {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        open_tx: Option<String>,
        #[arg(long)]
        settle_tx: Option<String>,
        #[arg(long)]
        refund_tx: Option<String>,
        /// `text` or `json`.
        #[arg(long, default_value = "text")]
        format: String,
        /// Write the receipt to this path instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Poll until the settle window is due, then run `settle`. Stops at expiry, on a spent output,
    /// or when nothing is owed. Does not broadcast unless `--submit`.
    Watch {
        #[arg(long)]
        file: PathBuf,
        #[arg(long, default_value_t = 10)]
        margin_epochs: u64,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        submit: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        seal_key_id_file: Option<PathBuf>,
        #[arg(long, default_value_t = 90)]
        timeout_secs: u64,
        /// Seconds between indexer polls while the settle is not yet due.
        #[arg(long, default_value_t = 30)]
        interval_secs: u64,
        /// Print one decision and return, including `wait`.
        #[arg(long)]
        once: bool,
    },
    /// `refund-due` using the file. `--seal-key-id-file` is the payer key in the opener wallet.
    Refund {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        submit: bool,
        #[arg(long)]
        dry_run: bool,
        /// Maximum revealed fee in micro-TARI (default: 100000).
        #[arg(long, default_value_t = 100_000)]
        max_fee: u64,
        #[arg(long)]
        seal_key_id_file: Option<PathBuf>,
        #[arg(long, default_value_t = 90)]
        timeout_secs: u64,
    },
}

#[derive(Subcommand)]
enum WalletCommand {
    /// Allocate a fresh wallet-managed account signing key. Use one per service/channel settlement.
    KeyCreate {
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        url: String,
        /// Save the new wallet KeyId JSON as an owner-only file for `--seal-key-id-file`.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Dry-run or submit an unsigned transaction (`seal-settle` / `seal-refund --unsigned-json`).
    Submit {
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        url: String,
        /// Path to unsigned JSON, or `-` for stdin.
        #[arg(long)]
        unsigned_json: PathBuf,
        /// `KeyId` JSON for the sealer (service key imported into the wallet).
        #[arg(long)]
        seal_key_id: Option<String>,
        #[arg(long)]
        seal_key_id_file: Option<PathBuf>,
        /// Extra `KeyId` JSON. Not implied: pass this or `--fee-account-name`.
        #[arg(long)]
        other_signer: Vec<String>,
        /// Wallet account whose `owner_key_id` co-signs `pay_fee` (service fee account, not the opener).
        #[arg(long)]
        fee_account_name: Option<String>,
        #[arg(long)]
        dry_run: bool,
        /// Skip `transactions.wait_result` after a live submit.
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Print stealth outputs from a wallet-stored transaction (`transactions.get`) as `unblind --utxo-json`.
    Utxos {
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        url: String,
        #[arg(long)]
        transaction_id: String,
        /// Write the channel deposit (`Script` auth) JSON for `unblind --utxo-json`.
        #[arg(long)]
        utxo_json_out: Option<PathBuf>,
    },
    /// Open a channel UTXO via `accounts.stealth_transfer` (`PayTo::Conditions`).
    Open {
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        url: String,
        /// Wallet account name used as `owner_account`.
        #[arg(long)]
        account_name: String,
        /// Ootle address that receives (and encrypts) the output. Default: `accounts.get_default`.
        #[arg(long)]
        destination_address: Option<String>,
        #[arg(long, default_value = "PreferConfidential")]
        input_selection: String,
        #[arg(long, default_value = "PreferRevealed")]
        fee_input_selection: String,
        /// Maximum µT fee. Required for a live open; dry-runs default to 100000 µT.
        #[arg(long)]
        max_fee: Option<u64>,
        #[arg(long)]
        resource_hex: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
        /// Write the channel deposit (`Script` auth) JSON for `unblind --utxo-json` after Accept.
        #[arg(long)]
        utxo_json_out: Option<PathBuf>,
    },
    /// Create a named wallet account (`accounts.create`), derive its keys, associate stealth tTARI. Fund it from an unlinked source before using it for settle fees.
    AccountCreate {
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        url: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        key_index: Option<u64>,
        #[arg(long)]
        is_default: bool,
    },
    /// Print a wallet account and its `owner_key_id` (`accounts.get`).
    AccountGet {
        #[arg(long, default_value = "http://127.0.0.1:5100")]
        url: String,
        #[arg(long)]
        name: String,
    },
}

#[derive(Deserialize)]
struct UtxoJson {
    commitment: String,
    nonce: String,
    encrypted_hex: String,
    tag: i64,
    #[serde(default)]
    value: Option<u64>,
}

#[derive(Subcommand)]
enum IndexerCommand {
    /// GET /wait-until-ready
    Wait {
        #[arg(long)]
        url: String,
    },
    /// GET /epoch-manager/stats — prints current_epoch
    Epoch {
        #[arg(long)]
        url: String,
    },
    /// POST /transactions — hex is BOR-encoded sealed Transaction bytes
    Submit {
        #[arg(long)]
        url: String,
        #[arg(long)]
        tx_hex: String,
    },
    /// POST /transactions/dry-run
    DryRun {
        #[arg(long)]
        url: String,
        #[arg(long)]
        tx_hex: String,
    },
    /// GET /transactions/{id}/result — Pending, Finalized, or Rejected.
    Result {
        #[arg(long)]
        url: String,
        #[arg(long)]
        transaction_id: String,
    },
    /// Fill `version: null` inputs and append fee-account vaults via `GET /substates/{id}`.
    ResolveInputs {
        #[arg(long)]
        url: String,
        #[arg(long)]
        unsigned_json: PathBuf,
        /// Write resolved unsigned JSON here. Stdout is only the filled=/down=/added= line.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(clap::Args)]
struct OpenArgs {
    #[arg(long)]
    network_tag: u8,
    #[arg(long)]
    deposit: u64,
    #[arg(long)]
    payer_hex: String,
    #[arg(long)]
    service_hex: String,
    #[arg(long)]
    expiry_epoch: u64,
    #[arg(long)]
    nonce_hex: String,
    #[arg(long)]
    template_hex: String,
}

#[derive(clap::Args)]
struct ParamsArgs {
    #[arg(long)]
    network_tag: u8,
    /// Deposit in integer micro-units.
    #[arg(long)]
    deposit: u64,
    /// Hex-encoded payer public key (32 bytes).
    #[arg(long)]
    payer_hex: String,
    /// Hex-encoded payer condition-tree root (32 bytes).
    #[arg(long)]
    payer_root_hex: String,
    /// Hex-encoded 32-byte channel nonce.
    #[arg(long)]
    nonce_hex: String,
}

fn params(args: &ParamsArgs) -> Result<ChannelParams, String> {
    Ok(ChannelParams {
        network_tag: args.network_tag,
        payer: seal::parse_pk("payer", &args.payer_hex)?,
        deposit: args.deposit,
        payer_root: seal::parse_hash("payer root", &args.payer_root_hex)?,
        nonce: seal::parse_nonce(&args.nonce_hex)?,
    })
}

fn open_spec(args: &OpenArgs) -> Result<seal::OpenSpec, String> {
    Ok(seal::OpenSpec {
        network_tag: args.network_tag,
        deposit: args.deposit,
        payer: seal::parse_pk("payer", &args.payer_hex)?,
        service: seal::parse_pk("service", &args.service_hex)?,
        expiry_epoch: args.expiry_epoch,
        nonce: seal::parse_nonce(&args.nonce_hex)?,
        channel_template: seal::parse_hash("template", &args.template_hex)?,
    })
}

fn funded(value: u64, mask_hex: &str) -> Result<MaskAndValue, String> {
    Ok(MaskAndValue {
        value,
        mask: seal::parse_secret("mask", mask_hex)?,
    })
}

#[tokio::main]
async fn main() {
    init_logging();
    if let Err(err) = run(Cli::parse()).await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn record_channel_operation(operation: &'static str, outcome: &'static str) {
    tracing::info!(
        event = "channel_operation",
        metric_name = "ghostline_channel_operation_outcomes_total",
        metric_type = "counter",
        operation,
        outcome,
        value = 1u64,
    );
}

async fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::ChannelId(args) => {
            println!("{}", to_hex(&params(&args)?.channel_id()));
        }
        Command::Channel { command } => channel_command(command).await?,
        Command::Sign {
            params: args,
            price,
            amount,
            session,
            vendor_hex,
            budget,
            max_per_call,
            unsafe_no_policy,
            secret_hex,
            secret_file,
            service_output_key_hex,
        } => {
            let params = params(&args)?;
            let secret = if let Some(path) = secret_file.as_deref() {
                secrets::load_key_file(path, "payer")?
            } else {
                secrets::load_secret(
                    secret_hex.as_deref(),
                    secrets::PAYER_SECRET_ENV,
                    "payer secret",
                )?
            };
            let vendor_hex = vendor_hex
                .or_else(|| std::env::var(secrets::VENDOR_ENV).ok())
                .ok_or("set --vendor-hex or GHOSTLINE_POLICY_VENDOR_HEX")?;
            let vendor = seal::parse_pk("vendor", vendor_hex.trim())?;
            let service_output_key = match service_output_key_hex {
                Some(hex) => seal::parse_pk("service output key", hex.trim())?,
                None => vendor,
            };
            let unsafe_ok = unsafe_no_policy || ghostline_policy::env_flag(secrets::UNSAFE_ENV);
            if unsafe_ok {
                let amount = amount.ok_or("unsafe sign requires --amount")?;
                let voucher = SignedVoucher::sign(&secret, &params, amount, service_output_key)
                    .map_err(|e| e.to_string())?;
                println!("{}", to_hex(&voucher.to_bytes()));
            } else {
                if amount.is_some() {
                    return Err(
                        "use --price with a session; --amount is only for --unsafe-no-policy"
                            .into(),
                    );
                }
                let price =
                    price.ok_or("sign requires --price (or --unsafe-no-policy --amount)")?;
                let path = session
                    .or_else(|| std::env::var_os(secrets::SESSION_ENV).map(PathBuf::from))
                    .ok_or("set --session or GHOSTLINE_SESSION")?;
                let policy = match (budget, max_per_call) {
                    (Some(b), Some(m)) => ghostline_policy::Policy::new(b, m).allow_vendor(vendor),
                    (None, None) => ghostline_policy::Policy::from_env()?.ok_or(
                        "set --budget and --max-per-call, or GHOSTLINE_POLICY_BUDGET and GHOSTLINE_POLICY_MAX_PER_CALL",
                    )?,
                    _ => {
                        return Err("set both --budget and --max-per-call".into());
                    }
                };
                let engine = ghostline_policy::PolicyEngine::new(policy)
                    .with_audit_file_from_env()
                    .map_err(|e| e.to_string())?
                    .with_human_rules_from_env()
                    .map_err(|e| e.to_string())?;
                let require_human = engine.requires_human_approval(&vendor, price);
                let attested = if require_human {
                    confirm_human_approval(price, &params.channel_id())?
                } else {
                    false
                };
                let mut store =
                    ghostline_paywall::ChannelStore::open_or_create(path, params.clone())
                        .map_err(|e| e.to_string())?;
                let mut quote =
                    ghostline_paywall::PaymentRequired::new(price, &params.channel_id());
                quote.service_output_key = Some(to_hex(service_output_key.as_bytes()));
                let (voucher, _) = ghostline_paywall::gated_sign_quote(
                    &mut store, engine, vendor, &quote, &secret, attested,
                )
                .map_err(|e| e.to_string())?;
                println!("{}", to_hex(&voucher.to_bytes()));
            }
        }
        Command::Verify {
            params: args,
            voucher_hex,
        } => {
            let params = params(&args)?;
            let bytes = from_hex(&voucher_hex)
                .map_err(|_| "voucher: expected even-length hex".to_string())?;
            let voucher = SignedVoucher::from_bytes(&bytes).map_err(|e| e.to_string())?;
            voucher.verify(&params).map_err(|e| e.to_string())?;
            println!(
                "ok amount={} channel_id={}",
                voucher.amount,
                to_hex(&voucher.channel_id)
            );
        }
        Command::Indexer { command } => match command {
            IndexerCommand::Wait { url } => {
                IndexerClient::connect(&url)
                    .map_err(|e| e.to_string())?
                    .wait_until_ready()
                    .await
                    .map_err(|e| e.to_string())?;
                println!("ready");
            }
            IndexerCommand::Epoch { url } => {
                let stats = IndexerClient::connect(&url)
                    .map_err(|e| e.to_string())?
                    .epoch_stats()
                    .await
                    .map_err(|e| e.to_string())?;
                println!("{}", stats.current_epoch.as_u64());
            }
            IndexerCommand::Submit { url, tx_hex } => {
                let id = IndexerClient::connect(&url)
                    .map_err(|e| e.to_string())?
                    .submit(decode_tx(&tx_hex)?)
                    .await
                    .map_err(|e| e.to_string())?;
                println!("{id}");
            }
            IndexerCommand::DryRun { url, tx_hex } => {
                let id = IndexerClient::connect(&url)
                    .map_err(|e| e.to_string())?
                    .dry_run(decode_tx(&tx_hex)?)
                    .await
                    .map_err(|e| e.to_string())?;
                println!("{id}");
            }
            IndexerCommand::Result {
                url,
                transaction_id,
            } => {
                let id = parse_transaction_id(&transaction_id)?;
                let body = IndexerClient::connect(&url)
                    .map_err(|e| e.to_string())?
                    .transaction_result(id)
                    .await
                    .map_err(|e| e.to_string())?;
                println!("{}", indexer_result_status(&body));
            }
            IndexerCommand::ResolveInputs {
                url,
                unsigned_json,
                out,
            } => {
                let mut unsigned = serde_json::to_value(read_unsigned_json(&unsigned_json)?)
                    .map_err(|e| e.to_string())?;
                let report = IndexerClient::connect(&url)
                    .map_err(|e| e.to_string())?
                    .resolve_unsigned_input_versions(&mut unsigned)
                    .await
                    .map_err(|e| e.to_string())?;
                println!(
                    "filled={} down={} added={}",
                    report.filled, report.down, report.added
                );
                let json = serde_json::to_string(&unsigned).map_err(|e| e.to_string())?;
                if let Some(path) = out {
                    ghostline_core::write_private_file(&path, json.as_bytes())
                        .map_err(|e| format!("resolve-inputs --out: {e}"))?;
                    println!("wrote={}", path.display());
                } else {
                    println!("{json}");
                }
            }
        },
        Command::Wallet { command } => match command {
            WalletCommand::KeyCreate { url, out } => {
                if let Some(path) = &out
                    && path.exists()
                {
                    return Err(format!(
                        "refusing to overwrite key id file: {}",
                        path.display()
                    ));
                }
                let wallet = connect_wallet(&url).await?;
                let before = wallet
                    .keys_list(ghostline_net::ACCOUNT_KEY_BRANCH)
                    .await
                    .map_err(|e| e.to_string())?;
                let key = wallet
                    .keys_create(ghostline_net::ACCOUNT_KEY_BRANCH)
                    .await
                    .map_err(|e| e.to_string())?;
                let keys = wallet
                    .keys_list(ghostline_net::ACCOUNT_KEY_BRANCH)
                    .await
                    .map_err(|e| e.to_string())?;
                let (key_id, public_key_hex) = newly_created_account_key(&before, &keys)?;
                if let Some(path) = &out {
                    let json = serde_json::to_vec_pretty(&key_id).map_err(|e| e.to_string())?;
                    ghostline_core::write_private_file(path, &json)
                        .map_err(|e| format!("key id file: {e}"))?;
                }
                let output = serde_json::json!({
                    "created": key,
                    "new_key_id": key_id,
                    "public_key_hex": public_key_hex,
                    "key_id_file": out.as_ref().map(|path| path.display().to_string()),
                    "account_keys": keys
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
                );
                eprintln!(
                    "Use public_key_hex for this channel's service key; use new_key_id (or the saved file) to sign its one settlement. Do not reuse the key for another channel."
                );
            }
            WalletCommand::Submit {
                url,
                unsigned_json,
                seal_key_id,
                seal_key_id_file,
                other_signer,
                fee_account_name,
                dry_run,
                no_wait,
                timeout_secs,
            } => {
                let unsigned = read_unsigned_json(&unsigned_json)?;
                submit_unsigned(SubmitUnsignedArgs {
                    url: &url,
                    unsigned,
                    seal_key_id: seal_key_id.as_deref(),
                    seal_key_id_file: seal_key_id_file.as_deref(),
                    other_signer: &other_signer,
                    fee_account_name: fee_account_name.as_deref(),
                    dry_run,
                    no_wait,
                    timeout_secs,
                })
                .await?;
            }
            WalletCommand::Utxos {
                url,
                transaction_id,
                utxo_json_out,
            } => {
                let wallet = connect_wallet(&url).await?;
                dump_wallet_utxos(
                    &wallet,
                    parse_transaction_id(&transaction_id)?,
                    utxo_json_out.as_deref(),
                )
                .await?;
            }
            WalletCommand::Open {
                open,
                url,
                account_name,
                destination_address,
                input_selection,
                fee_input_selection,
                max_fee,
                resource_hex,
                dry_run,
                no_wait,
                timeout_secs,
                utxo_json_out,
            } => {
                let max_fee = open_max_fee(dry_run, max_fee)?;
                let channel = seal::channel(open_spec(&open)?)?;
                let pay_to = serde_json::json!({ "Conditions": channel.leaves_vec() });
                let resource = seal::parse_resource(resource_hex.as_deref())?;
                let wallet = connect_wallet(&url).await?;
                let destination = match destination_address {
                    Some(addr) => addr,
                    None => wallet
                        .default_account_address()
                        .await
                        .map_err(|e| e.to_string())?,
                };
                let request = StealthTransferRequest::channel_open(ChannelOpenParams {
                    account_name,
                    destination_address: destination,
                    resource_address: resource.to_string(),
                    deposit: open.deposit,
                    pay_to,
                    input_selection: input_selection.parse::<UtxoInputSelection>()?,
                    fee_input_selection: fee_input_selection.parse::<UtxoInputSelection>()?,
                    max_fee,
                    dry_run,
                });
                seal::print_ids(&channel);
                let submitted = wallet
                    .stealth_transfer(&request)
                    .await
                    .map_err(|e| wallet_open_error(e.to_string()))?;
                println!("transaction_id={}", submitted.transaction_id);
                if dry_run {
                    print_wallet_open_fee_quote(&wallet, submitted.transaction_id).await?;
                    println!("max_fee={max_fee}");
                    println!("dry_run=true");
                    println!("utxo dump needs an Accepted live open (dry-run is not stored)");
                } else if !no_wait {
                    print_wait(&wallet, submitted.transaction_id, timeout_secs).await?;
                    dump_wallet_utxos(&wallet, submitted.transaction_id, utxo_json_out.as_deref())
                        .await?;
                }
            }
            WalletCommand::AccountCreate {
                url,
                name,
                key_index,
                is_default,
            } => {
                let wallet = connect_wallet(&url).await?;
                let created = wallet
                    .account_create(&AccountsCreateRequest {
                        account_name: Some(name.clone()),
                        is_default: Some(is_default),
                        key_index,
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                print_wallet_account(&created);
                let index = key_index.unwrap_or(0);
                wallet
                    .ensure_derived_key(ghostline_net::ACCOUNT_KEY_BRANCH, index)
                    .await
                    .map_err(|e| e.to_string())?;
                wallet
                    .ensure_derived_key(ghostline_net::VIEW_ONLY_KEY_BRANCH, index)
                    .await
                    .map_err(|e| e.to_string())?;
                let resource =
                    tari_template_lib::types::constants::STEALTH_TARI_RESOURCE_ADDRESS.to_string();
                wallet
                    .associate_stealth_resource(&name, &resource)
                    .await
                    .map_err(|e| e.to_string())?;
                println!("derived_keys={index}");
                println!("associated_resource={resource}");
            }
            WalletCommand::AccountGet { url, name } => {
                let wallet = connect_wallet(&url).await?;
                let got = wallet.account_get(&name).await.map_err(|e| e.to_string())?;
                print_wallet_account(&got);
            }
        },
        Command::Keygen { out, role } => {
            use tari_crypto::keys::PublicKey as _;
            use tari_crypto::ristretto::RistrettoPublicKey;
            let (secret, public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
            let secret_hex = to_hex(secret.as_bytes());
            let public_hex = to_hex(public.as_bytes());
            if let Some(path) = out {
                let role = role.unwrap_or_else(|| "payer".to_string());
                let json = serde_json::json!({
                    "format": "ghostline-key-v1",
                    "role": role,
                    "secret_hex": secret_hex,
                    "public_hex": public_hex,
                });
                ghostline_core::write_private_file(
                    &path,
                    serde_json::to_string_pretty(&json)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                )
                .map_err(|e| format!("key file: {e}"))?;
                println!("wrote={}", path.display());
                println!("public={public_hex}");
            } else {
                println!("secret={secret_hex}");
                println!("public={public_hex}");
            }
        }
        Command::Spec(args) => {
            seal::print_ids(&seal::channel(open_spec(&args)?)?);
        }
        Command::PayTo(args) => {
            let channel = seal::channel(open_spec(&args)?)?;
            let pay_to = serde_json::json!({ "Conditions": channel.leaves_vec() });
            println!(
                "{}",
                serde_json::to_string(&pay_to).map_err(|e| e.to_string())?
            );
            seal::print_ids(&channel);
        }
        Command::SealOpen {
            open,
            input_value,
            input_mask_hex,
            seal_secret_hex,
            max_epoch,
            resource_hex,
            ootle_network,
        } => {
            let channel = seal::channel(open_spec(&open)?)?;
            let sealer = secrets::load_secret(
                seal_secret_hex.as_deref(),
                secrets::SEAL_SECRET_ENV,
                "seal secret",
            )?;
            let input_mask = secrets::load_hex(
                input_mask_hex.as_deref(),
                secrets::INPUT_MASK_ENV,
                "input mask",
            )?;
            let resource = seal::parse_resource(resource_hex.as_deref())?;
            let scan = seal::parse_scan(&ootle_network, resource)?;
            let (tx, mask) = seal::seal_open(
                &channel,
                funded(input_value, &input_mask)?,
                channel.spec().payer,
                resource,
                max_epoch,
                &sealer,
                scan,
            )?;
            println!("tx={tx}");
            println!("deposit_mask={mask}");
        }
        Command::SealSettle {
            open,
            voucher_hex,
            deposit_mask_hex,
            seal_secret_hex,
            max_epoch,
            resource_hex,
            ootle_network,
            fee_account,
            max_fee,
            fee_secret_hex,
            unsigned_json,
        } => {
            let channel = seal::channel(open_spec(&open)?)?;
            let bytes = from_hex(&voucher_hex)
                .map_err(|_| "voucher: expected even-length hex".to_string())?;
            let voucher = SignedVoucher::from_bytes(&bytes).map_err(|e| e.to_string())?;
            let deposit_mask = secrets::load_hex(
                deposit_mask_hex.as_deref(),
                secrets::DEPOSIT_MASK_ENV,
                "deposit mask",
            )?;
            let resource = seal::parse_resource(resource_hex.as_deref())?;
            let scan = seal::parse_scan(&ootle_network, resource)?;
            let deposit = funded(channel.params().deposit, &deposit_mask)?;
            if unsigned_json {
                let fee = match &fee_account {
                    Some(addr) => Some((seal::parse_component("fee-account", addr)?, max_fee)),
                    None => None,
                };
                let unsigned = seal::unsigned_settle(
                    seal::SettleArgs {
                        channel: &channel,
                        deposit,
                        voucher: &voucher,
                        service_output: voucher.service_output_key,
                        resource,
                        max_epoch,
                        scan,
                    },
                    fee,
                )?;
                println!(
                    "{}",
                    serde_json::to_string(&unsigned).map_err(|e| e.to_string())?
                );
            } else {
                let sealer = secrets::load_secret(
                    seal_secret_hex.as_deref(),
                    secrets::SEAL_SECRET_ENV,
                    "seal secret",
                )?;
                let fee_secret = if fee_account.is_some() {
                    Some(secrets::load_secret(
                        fee_secret_hex.as_deref(),
                        secrets::FEE_SECRET_ENV,
                        "fee secret",
                    )?)
                } else {
                    None
                };
                let fee = match (&fee_account, &fee_secret) {
                    (Some(addr), Some(secret)) => Some(ghostline_sdk::AccountFee {
                        account: seal::parse_component("fee-account", addr)?,
                        max_fee,
                        signer: secret,
                    }),
                    (None, None) => None,
                    _ => return Err(
                        "fee-account and fee secret must be set together (GHOSTLINE_FEE_SECRET)"
                            .into(),
                    ),
                };
                let tx = seal::seal_settle(
                    seal::SettleArgs {
                        channel: &channel,
                        deposit,
                        voucher: &voucher,
                        service_output: voucher.service_output_key,
                        resource,
                        max_epoch,
                        scan,
                    },
                    &sealer,
                    fee,
                )?;
                println!("{tx}");
            }
        }
        Command::SealUnsigned {
            unsigned_json,
            seal_secret_hex,
            fee_secret_hex,
            dry_run,
        } => {
            use tari_crypto::keys::PublicKey as _;
            use tari_crypto::ristretto::RistrettoPublicKey;
            use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;
            let unsigned = {
                let u = read_unsigned_json(&unsigned_json)?;
                if dry_run { u.with_dry_run(true) } else { u }
            };
            let sealer = secrets::load_secret(
                seal_secret_hex.as_deref(),
                secrets::SEAL_SECRET_ENV,
                "seal secret",
            )?;
            let pk = RistrettoPublicKey::from_secret_key(&sealer);
            let sealer_pk = RistrettoPublicKeyBytes::from_bytes(pk.as_bytes())
                .map_err(|e| format!("seal public key: {e}"))?;
            let fee_secret = if fee_secret_hex.is_some()
                || std::env::var(secrets::FEE_SECRET_ENV)
                    .ok()
                    .is_some_and(|v| !v.trim().is_empty())
            {
                Some(secrets::load_secret(
                    fee_secret_hex.as_deref(),
                    secrets::FEE_SECRET_ENV,
                    "fee secret",
                )?)
            } else {
                None
            };
            let tx = match &fee_secret {
                Some(fee) => unsigned.add_signer(&sealer_pk, fee).seal(&sealer),
                None => unsigned.finish().seal(&sealer),
            };
            println!(
                "{}",
                ghostline_sdk::transaction_envelope_hex(tx).map_err(|e| e.to_string())?
            );
        }
        Command::SealRefund {
            open,
            deposit_mask_hex,
            seal_secret_hex,
            max_epoch,
            resource_hex,
            ootle_network,
            fee_account,
            max_fee,
            fee_secret_hex,
            unsigned_json,
        } => {
            let channel = seal::channel(open_spec(&open)?)?;
            let deposit_mask = secrets::load_hex(
                deposit_mask_hex.as_deref(),
                secrets::DEPOSIT_MASK_ENV,
                "deposit mask",
            )?;
            let resource = seal::parse_resource(resource_hex.as_deref())?;
            let scan = seal::parse_scan(&ootle_network, resource)?;
            let deposit = funded(channel.params().deposit, &deposit_mask)?;
            if unsigned_json {
                let fee = match &fee_account {
                    Some(addr) => Some((seal::parse_component("fee-account", addr)?, max_fee)),
                    None => None,
                };
                let unsigned = seal::unsigned_refund(
                    &channel,
                    deposit,
                    channel.spec().payer,
                    resource,
                    max_epoch,
                    scan,
                    fee,
                )?;
                println!(
                    "{}",
                    serde_json::to_string(&unsigned).map_err(|e| e.to_string())?
                );
            } else {
                let sealer = secrets::load_secret(
                    seal_secret_hex.as_deref(),
                    secrets::SEAL_SECRET_ENV,
                    "seal secret",
                )?;
                let fee_secret = if fee_account.is_some() {
                    Some(secrets::load_secret(
                        fee_secret_hex.as_deref(),
                        secrets::FEE_SECRET_ENV,
                        "fee secret",
                    )?)
                } else {
                    None
                };
                let fee = match (&fee_account, &fee_secret) {
                    (Some(addr), Some(secret)) => Some(ghostline_sdk::AccountFee {
                        account: seal::parse_component("fee-account", addr)?,
                        max_fee,
                        signer: secret,
                    }),
                    (None, None) => None,
                    _ => return Err(
                        "fee-account and fee secret must be set together (GHOSTLINE_FEE_SECRET)"
                            .into(),
                    ),
                };
                let tx = seal::seal_refund(
                    seal::RefundArgs {
                        channel: &channel,
                        deposit,
                        payer_output: channel.spec().payer,
                        resource,
                        max_epoch,
                        scan,
                    },
                    &sealer,
                    fee,
                )?;
                println!("{tx}");
            }
        }
        Command::Unblind {
            commitment_hex,
            nonce_hex,
            encrypted_hex,
            tag,
            utxo_json,
            resource_hex,
            secret_hex,
            ootle_network,
        } => {
            let recovered = recover_utxo(RecoverUtxoArgs {
                utxo_json,
                commitment_hex,
                nonce_hex,
                encrypted_hex,
                tag,
                resource_hex: resource_hex.as_deref(),
                secret_hex: secret_hex.as_deref(),
                ootle_network: &ootle_network,
            })?;
            println!("value={}", recovered.value);
            println!("mask={}", to_hex(recovered.mask.as_bytes()));
        }
        Command::UnblindTx { tx_hex, secret_hex } => {
            let secret = secrets::load_secret(
                secret_hex.as_deref(),
                secrets::PAYER_SECRET_ENV,
                "claim secret",
            )?;
            let found =
                unblind_transaction(&decode_tx(&tx_hex)?, &secret).map_err(|e| e.to_string())?;
            println!("outputs={}", found.len());
            for (i, out) in found.iter().enumerate() {
                println!(
                    "output[{i}] value={} tag={} resource={} mask={}",
                    out.funds.value,
                    out.tag.value(),
                    out.resource,
                    to_hex(out.funds.mask.as_bytes())
                );
            }
        }
        Command::InspectTx { tx_hex } => {
            let views = inspect_transaction(&decode_tx(&tx_hex)?);
            println!("outputs={}", views.len());
            for (i, view) in views.iter().enumerate() {
                println!(
                    "output[{i}] tag={} resource={} commitment={} nonce={} encrypted={} promise={}",
                    view.tag.value(),
                    view.resource,
                    to_hex(view.output.commitment.as_bytes()),
                    to_hex(view.output.sender_public_nonce.as_bytes()),
                    to_hex(view.output.encrypted_data.as_bytes()),
                    view.output.minimum_value_promise
                );
            }
        }
        Command::Status {
            session,
            ledger,
            store,
            expiry_epoch,
            indexer_url,
            include_params,
            commitment_hex,
            utxo_json,
            resource_hex,
        }
        | Command::Report {
            session,
            ledger,
            store,
            expiry_epoch,
            indexer_url,
            include_params,
            commitment_hex,
            utxo_json,
            resource_hex,
        } => {
            print_channel_status(ChannelStatusArgs {
                session,
                ledger,
                store,
                expiry_epoch,
                indexer_url: indexer_url.as_deref(),
                include_params,
                commitment_hex,
                utxo_json,
                resource_hex: resource_hex.as_deref(),
            })
            .await?;
        }
        Command::Watch {
            expiry_epoch,
            indexer_url,
            margin_epochs,
        } => {
            let current_epoch = fetch_epoch(&indexer_url).await?;
            let epochs_until_expiry = i128::from(expiry_epoch) - i128::from(current_epoch);
            let settle_window = if current_epoch < expiry_epoch {
                ghostline_paywall::SettleWindow::SettleOpen
            } else {
                ghostline_paywall::SettleWindow::RefundOpen
            };
            let within_margin =
                current_epoch >= expiry_epoch || expiry_epoch - current_epoch <= margin_epochs;
            let body = serde_json::json!({
                "current_epoch": current_epoch,
                "expiry_epoch": expiry_epoch,
                "epochs_until_expiry": epochs_until_expiry,
                "margin_epochs": margin_epochs,
                "within_margin": within_margin,
                "settle_window": settle_window,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
            );
        }
        Command::SettleDue {
            ledger,
            open,
            indexer_url,
            margin_epochs,
            force,
            commitment_hex,
            utxo_json,
            deposit_mask_hex,
            max_epoch,
            resource_hex,
            ootle_network,
            fee_account,
            max_fee,
            out,
            submit,
            wallet_url,
            seal_key_id_file,
            fee_account_name,
            dry_run,
            timeout_secs,
        } => {
            settle_due(SettleDueArgs {
                ledger,
                open,
                indexer_url,
                margin_epochs,
                force,
                commitment_hex,
                utxo_json,
                deposit_mask_hex,
                deposit_mask_from_channel_file: false,
                max_epoch,
                resource_hex,
                ootle_network,
                fee_account,
                max_fee,
                out,
                submit,
                wallet_url,
                seal_key_id_file,
                fee_account_name,
                dry_run,
                timeout_secs,
            })
            .await?;
        }
        Command::RefundDue {
            open,
            indexer_url,
            commitment_hex,
            utxo_json,
            deposit_mask_hex,
            max_epoch,
            resource_hex,
            ootle_network,
            fee_account,
            max_fee,
            out,
            submit,
            wallet_url,
            seal_key_id_file,
            fee_account_name,
            dry_run,
            timeout_secs,
        } => {
            refund_due(RefundDueArgs {
                open,
                indexer_url,
                commitment_hex,
                utxo_json,
                deposit_mask_hex,
                deposit_mask_from_channel_file: false,
                max_epoch,
                resource_hex,
                ootle_network,
                fee_account,
                max_fee,
                out,
                submit,
                wallet_url,
                seal_key_id_file,
                fee_account_name,
                dry_run,
                timeout_secs,
            })
            .await?;
        }
    }
    Ok(())
}

fn confirm_human_approval(price: u64, channel_id: &[u8; 32]) -> Result<bool, String> {
    if !io::stdin().is_terminal() {
        return Err("human approval requires an interactive terminal".into());
    }
    eprint!(
        "Approve signing a {price} µT voucher for channel {}? [y/N] ",
        to_hex(channel_id)
    );
    io::stderr().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    Ok(approval_answer(&answer))
}

fn approval_answer(answer: &str) -> bool {
    answer.trim().eq_ignore_ascii_case("y") || answer.trim().eq_ignore_ascii_case("yes")
}

fn open_args_from(file: &channel_file::ChannelFile) -> OpenArgs {
    OpenArgs {
        network_tag: file.network_tag,
        deposit: file.deposit,
        payer_hex: file.payer_hex.clone(),
        service_hex: file.service_hex.clone(),
        expiry_epoch: file.expiry_epoch,
        nonce_hex: file.nonce_hex.clone(),
        template_hex: file.template_hex.clone(),
    }
}

struct RecoverUtxoArgs<'a> {
    utxo_json: Option<PathBuf>,
    commitment_hex: Option<String>,
    nonce_hex: Option<String>,
    encrypted_hex: Option<String>,
    tag: Option<i64>,
    resource_hex: Option<&'a str>,
    secret_hex: Option<&'a str>,
    ootle_network: &'a str,
}

fn recover_utxo(args: RecoverUtxoArgs<'_>) -> Result<MaskAndValue, String> {
    use tari_template_lib::types::{
        EncryptedData,
        crypto::{PedersenCommitmentBytes, UtxoTag},
        stealth::UnspentOutput,
    };
    let (commitment_hex, nonce_hex, encrypted_hex, tag, expected) = load_utxo(
        args.utxo_json,
        args.commitment_hex,
        args.nonce_hex,
        args.encrypted_hex,
        args.tag,
    )?;
    let secret = secrets::load_claim_secret(args.secret_hex)?;
    let encrypted_bytes =
        from_hex(&encrypted_hex).map_err(|_| "encrypted: expected even-length hex".to_string())?;
    let encrypted = EncryptedData::try_from(encrypted_bytes)
        .map_err(|len| format!("encrypted: invalid length {len}"))?;
    let output = UnspentOutput {
        commitment: PedersenCommitmentBytes::from_hex(&commitment_hex)
            .map_err(|e| format!("commitment: {e}"))?,
        sender_public_nonce: seal::parse_pk("nonce", &nonce_hex)?,
        encrypted_data: encrypted,
        minimum_value_promise: 0,
        viewable_balance_proof: None,
    };
    let resource = seal::parse_resource(args.resource_hex)?;
    let recovered = unblind_output(
        &output,
        UtxoTag::new(tag),
        &secret,
        seal::parse_scan(args.ootle_network, resource)?,
    )
    .map_err(|e| e.to_string())?;
    if let Some(expected) = expected
        && recovered.value != expected
    {
        return Err(format!(
            "unblinded value {} does not match expected {expected}",
            recovered.value
        ));
    }
    Ok(recovered)
}

fn list_channel_calls(dir: &std::path::Path) -> Result<Vec<channel_file::CallRow>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("channel calls: {e}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut rows = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("channel.json")
            .to_string();
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<channel_file::ChannelFile>(&text) else {
            continue;
        };
        let Some(ledger) = record.ledger else {
            continue;
        };
        let store = ghostline_paywall::ChannelStore::open(&ledger)
            .map_err(|e| format!("channel calls: {name}: {e}"))?;
        if store.calls().is_empty() {
            if store.accepted() > 0 {
                rows.push(channel_file::CallRow {
                    file: name,
                    price: None,
                    accepted: Some(store.accepted()),
                    note: "no call log; accepted total predates it".into(),
                });
            }
            continue;
        }
        for call in store.calls() {
            rows.push(channel_file::CallRow {
                file: name.clone(),
                price: Some(call.price),
                accepted: Some(call.accepted),
                note: String::new(),
            });
        }
    }
    Ok(rows)
}

async fn list_channel_files(
    dir: &std::path::Path,
) -> Result<Vec<channel_file::ChannelListRow>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("channel list: {e}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut epochs: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut rows = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("channel.json")
            .to_string();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                rows.push(channel_file::ChannelListRow {
                    file: name,
                    channel_id: String::new(),
                    deposit: 0,
                    expiry_epoch: 0,
                    accepted: None,
                    cumulative: None,
                    remainder_at_accepted: None,
                    pending_settlement: None,
                    has_latest_voucher: None,
                    settle_window: None,
                    epochs_until_expiry: None,
                    current_epoch: None,
                    output_presence: None,
                    output_version: None,
                    note: Some(format!("unreadable: {e}")),
                });
                continue;
            }
        };
        let record: channel_file::ChannelFile = match serde_json::from_str(&text) {
            Ok(record) => record,
            Err(_) if text.contains("\"opener_account\"") => {
                rows.push(channel_file::ChannelListRow {
                    file: name,
                    channel_id: String::new(),
                    deposit: 0,
                    expiry_epoch: 0,
                    accepted: None,
                    cumulative: None,
                    remainder_at_accepted: None,
                    pending_settlement: None,
                    has_latest_voucher: None,
                    settle_window: None,
                    epochs_until_expiry: None,
                    current_epoch: None,
                    output_presence: None,
                    output_version: None,
                    note: Some("channel file did not parse".into()),
                });
                continue;
            }
            Err(_) => continue,
        };
        let channel = seal::channel(open_spec(&open_args_from(&record))?)?;
        let mut row = channel_file::ChannelListRow::identity(
            &name,
            to_hex(&channel.params().channel_id()),
            &record,
        );
        if let Some(ledger) = &record.ledger {
            match ghostline_paywall::ChannelStore::open(ledger) {
                Ok(store) => {
                    let current = if let Some(epoch) = epochs.get(&record.indexer_url).copied() {
                        Some(epoch)
                    } else {
                        match fetch_epoch(&record.indexer_url).await {
                            Ok(epoch) => {
                                epochs.insert(record.indexer_url.clone(), epoch);
                                Some(epoch)
                            }
                            Err(e) => {
                                row.note = Some(format!("indexer: {e}"));
                                None
                            }
                        }
                    };
                    match ghostline_paywall::ChannelStatus::from_store(
                        &store,
                        Some(record.expiry_epoch),
                        current,
                    ) {
                        Ok(view) => {
                            row.accepted = Some(view.accepted);
                            row.cumulative = Some(view.cumulative);
                            row.remainder_at_accepted = Some(view.remainder_at_accepted);
                            row.pending_settlement = Some(view.pending_settlement);
                            row.has_latest_voucher = Some(view.has_latest_voucher);
                            row.settle_window = Some(
                                match view.settle_window {
                                    ghostline_paywall::SettleWindow::Unknown => "unknown",
                                    ghostline_paywall::SettleWindow::SettleOpen => "settle_open",
                                    ghostline_paywall::SettleWindow::RefundOpen => "refund_open",
                                }
                                .to_string(),
                            );
                            row.epochs_until_expiry = view.epochs_until_expiry;
                            row.current_epoch = view.current_epoch;
                        }
                        Err(e) => row.note = Some(e.to_string()),
                    }
                }
                Err(e) => row.note = Some(format!("ledger: {e}")),
            }
        } else {
            row.note = Some("no ledger".into());
        }
        if let Some(utxo) = &record.utxo_json {
            match commitment_arg(None, Some(utxo.clone())) {
                Ok(Some(hex)) => {
                    match lookup_channel_output(&record.indexer_url, None, &hex).await {
                        Ok((_, presence)) => match presence {
                            SubstatePresence::Up { version } => {
                                row.output_presence = Some("up".into());
                                row.output_version = Some(version);
                            }
                            SubstatePresence::Down { version } => {
                                row.output_presence = Some("down".into());
                                row.output_version = Some(version);
                            }
                        },
                        Err(e) => {
                            row.note = Some(match &row.note {
                                Some(prev) => format!("{prev}; output: {e}"),
                                None => format!("output: {e}"),
                            });
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => row.note = Some(format!("utxo: {e}")),
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

fn default_session_path(channel_file: &std::path::Path) -> PathBuf {
    let mut name = channel_file
        .file_stem()
        .unwrap_or(std::ffi::OsStr::new("channel"))
        .to_os_string();
    name.push(".session.json");
    channel_file.with_file_name(name)
}

/// Create an empty payer session, or leave an existing one whose channel id matches.
fn ensure_payer_session(file_path: &std::path::Path) -> Result<(PathBuf, bool, u64), String> {
    let mut record = channel_file::ChannelFile::load(file_path)?;
    let params = seal::channel(open_spec(&open_args_from(&record))?)?
        .params()
        .clone();
    let session_path = record
        .session
        .clone()
        .unwrap_or_else(|| default_session_path(file_path));
    let created = if session_path.exists() {
        let existing = ghostline_paywall::ChannelStore::open(&session_path)
            .map_err(|e| format!("session: {e}"))?;
        if existing.channel_id().map_err(|e| e.to_string())? != params.channel_id() {
            return Err("session channel id does not match the channel file".into());
        }
        false
    } else {
        ghostline_paywall::ChannelStore::create(&session_path, params)
            .map_err(|e| format!("session: {e}"))?;
        true
    };
    let store = ghostline_paywall::ChannelStore::open(&session_path)
        .map_err(|e| format!("session: {e}"))?;
    if record.session.as_deref() != Some(session_path.as_path()) {
        record.session = Some(session_path.clone());
        record.save(file_path)?;
    }
    Ok((session_path, created, store.cumulative()))
}

async fn channel_command(command: ChannelCommand) -> Result<(), String> {
    match command {
        ChannelCommand::Init {
            out,
            open,
            indexer_url,
            ootle_network,
            max_epoch,
            opener_account,
            opener_wallet_url,
            fee_account,
            fee_account_name,
            fee_wallet_url,
            payer_fee_account,
            payer_fee_account_name,
            payer_wallet_url,
            ledger,
            session,
        } => {
            let file = channel_file::ChannelFile {
                network_tag: open.network_tag,
                deposit: open.deposit,
                payer_hex: open.payer_hex,
                service_hex: open.service_hex,
                expiry_epoch: open.expiry_epoch,
                nonce_hex: open.nonce_hex,
                template_hex: open.template_hex,
                indexer_url,
                ootle_network,
                max_epoch,
                opener_account,
                opener_wallet_url,
                fee_account,
                fee_account_name,
                fee_wallet_url,
                payer_fee_account,
                payer_fee_account_name,
                payer_wallet_url,
                ledger,
                session,
                utxo_json: None,
                deposit_mask_hex: None,
            };
            file.save(&out)?;
            println!("wrote={}", out.display());
        }
        ChannelCommand::Session { file: path } => {
            let (session, created, cumulative) = ensure_payer_session(&path)?;
            println!("session={}", session.display());
            println!("created={created}");
            println!("cumulative={cumulative}");
        }
        ChannelCommand::Open {
            file: path,
            dry_run,
            max_fee,
            timeout_secs,
        } => {
            let max_fee = open_max_fee(dry_run, max_fee)?;
            let mut record = channel_file::ChannelFile::load(&path)?;
            let open = open_args_from(&record);
            let channel = seal::channel(open_spec(&open)?)?;
            if !dry_run {
                secrets::load_secret(None, secrets::CLAIM_SECRET_ENV, "wallet claim secret")?;
            }
            let pay_to = serde_json::json!({ "Conditions": channel.leaves_vec() });
            let resource = seal::parse_resource(None)?;
            let wallet = connect_wallet(&record.opener_wallet_url).await?;
            let destination = wallet
                .default_account_address()
                .await
                .map_err(|e| e.to_string())?;
            let request = StealthTransferRequest::channel_open(ChannelOpenParams {
                account_name: record.opener_account.clone(),
                destination_address: destination,
                resource_address: resource.to_string(),
                deposit: open.deposit,
                pay_to,
                input_selection: "PreferConfidential".parse::<UtxoInputSelection>()?,
                fee_input_selection: "PreferRevealed".parse::<UtxoInputSelection>()?,
                max_fee,
                dry_run,
            });
            seal::print_ids(&channel);
            let submitted = wallet
                .stealth_transfer(&request)
                .await
                .map_err(|e| wallet_open_error(e.to_string()))?;
            println!("transaction_id={}", submitted.transaction_id);
            if dry_run {
                print_wallet_open_fee_quote(&wallet, submitted.transaction_id).await?;
                println!("max_fee={max_fee}");
                println!("dry_run=true");
                println!("utxo dump needs an Accepted live open (dry-run is not stored)");
                return Ok(());
            }
            print_wait(&wallet, submitted.transaction_id, timeout_secs).await?;
            let utxo_path = path.with_extension("utxo.json");
            dump_wallet_utxos(&wallet, submitted.transaction_id, Some(&utxo_path)).await?;
            let recovered = recover_utxo(RecoverUtxoArgs {
                utxo_json: Some(utxo_path.clone()),
                commitment_hex: None,
                nonce_hex: None,
                encrypted_hex: None,
                tag: None,
                resource_hex: None,
                secret_hex: None,
                ootle_network: &record.ootle_network,
            })?;
            if recovered.value != record.deposit {
                return Err(format!(
                    "unblinded value {} does not match deposit {}",
                    recovered.value, record.deposit
                ));
            }
            record.utxo_json = Some(utxo_path);
            record.deposit_mask_hex = Some(to_hex(recovered.mask.as_bytes()));
            record.save(&path)?;
            println!("value={}", recovered.value);
            println!("mask_stored=true");
            let (session, created, cumulative) = ensure_payer_session(&path)?;
            println!("session={}", session.display());
            println!("session_created={created}");
            println!("cumulative={cumulative}");
            println!("wrote={}", path.display());
        }
        ChannelCommand::Bind {
            file: path,
            utxo_json,
        } => {
            let mut record = channel_file::ChannelFile::load(&path)?;
            let recovered = recover_utxo(RecoverUtxoArgs {
                utxo_json: Some(utxo_json.clone()),
                commitment_hex: None,
                nonce_hex: None,
                encrypted_hex: None,
                tag: None,
                resource_hex: None,
                secret_hex: None,
                ootle_network: &record.ootle_network,
            })?;
            if recovered.value != record.deposit {
                return Err(format!(
                    "unblinded value {} does not match deposit {}",
                    recovered.value, record.deposit
                ));
            }
            record.utxo_json = Some(utxo_json);
            record.deposit_mask_hex = Some(to_hex(recovered.mask.as_bytes()));
            record.save(&path)?;
            println!("value={}", recovered.value);
            println!("mask_stored=true");
            let (session, created, cumulative) = ensure_payer_session(&path)?;
            println!("session={}", session.display());
            println!("session_created={created}");
            println!("cumulative={cumulative}");
            println!("wrote={}", path.display());
        }
        ChannelCommand::List { dir } => {
            let rows = list_channel_files(&dir).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?
            );
        }
        ChannelCommand::Report {
            dir,
            format,
            disclose,
            out,
        } => {
            let rows = list_channel_files(&dir).await?;
            let columns = channel_file::report_columns(&disclose)?;
            let body = match format.as_str() {
                "csv" => channel_file::report_csv(&rows, &columns),
                "json" => serde_json::to_string_pretty(&channel_file::report_json(&rows, &columns))
                    .map_err(|e| e.to_string())?,
                other => return Err(format!("format: expected csv or json, got {other}")),
            };
            if let Some(path) = out {
                ghostline_core::write_private_file(&path, body.as_bytes())
                    .map_err(|e| format!("channel report: {e}"))?;
                println!("wrote={}", path.display());
            } else {
                print!("{body}");
            }
        }
        ChannelCommand::Calls { dir, out } => {
            let body = channel_file::calls_csv(&list_channel_calls(&dir)?);
            if let Some(path) = out {
                ghostline_core::write_private_file(&path, body.as_bytes())
                    .map_err(|e| format!("channel calls: {e}"))?;
                println!("wrote={}", path.display());
            } else {
                print!("{body}");
            }
        }
        ChannelCommand::Status { file: path } => {
            let record = channel_file::ChannelFile::load(&path)?;
            let ledger = record
                .ledger
                .clone()
                .ok_or("channel file has no ledger path")?;
            print_channel_status(ChannelStatusArgs {
                session: record.session.clone(),
                ledger: Some(ledger),
                store: None,
                expiry_epoch: Some(record.expiry_epoch),
                indexer_url: Some(&record.indexer_url),
                include_params: false,
                commitment_hex: None,
                utxo_json: record.utxo_json.clone(),
                resource_hex: None,
            })
            .await?;
        }
        ChannelCommand::Settle {
            file: path,
            margin_epochs,
            force,
            submit,
            dry_run,
            seal_key_id_file,
            max_fee,
            timeout_secs,
        } => {
            let record = channel_file::ChannelFile::load(&path)?;
            let ledger = record
                .ledger
                .clone()
                .ok_or("channel file has no ledger path")?;
            let fee_account = record
                .fee_account
                .clone()
                .ok_or("channel file has no fee_account")?;
            settle_due(SettleDueArgs {
                ledger,
                open: open_args_from(&record),
                indexer_url: record.indexer_url.clone(),
                margin_epochs,
                force,
                commitment_hex: None,
                utxo_json: record.utxo_json.clone(),
                deposit_mask_hex: record.deposit_mask_hex.clone(),
                deposit_mask_from_channel_file: true,
                max_epoch: record.max_epoch,
                resource_hex: None,
                ootle_network: record.ootle_network.clone(),
                fee_account,
                max_fee,
                out: None,
                submit,
                wallet_url: record
                    .fee_wallet_url
                    .clone()
                    .unwrap_or(record.opener_wallet_url.clone()),
                seal_key_id_file,
                fee_account_name: record.fee_account_name.clone(),
                dry_run,
                timeout_secs,
            })
            .await?;
        }
        ChannelCommand::Trace {
            file: path,
            open_tx,
            settle_tx,
            refund_tx,
            format,
            out,
        } => {
            let record = channel_file::ChannelFile::load(&path)?;
            let ledger = record
                .ledger
                .clone()
                .ok_or("channel file has no ledger path")?;
            let store =
                ghostline_paywall::ChannelStore::open(&ledger).map_err(|e| e.to_string())?;
            let calls: Vec<(u64, u64)> = store
                .calls()
                .iter()
                .map(|call| (call.price, call.accepted))
                .collect();
            let off_chain = channel_file::off_chain_panel(&calls, store.accepted());
            let indexer = IndexerClient::connect(&record.indexer_url).map_err(|e| e.to_string())?;
            let mut on_chain = Vec::new();
            for (role, tx) in [
                ("open", open_tx),
                ("settle", settle_tx),
                ("refund", refund_tx),
            ] {
                let Some(tx) = tx else { continue };
                let id = parse_transaction_id(&tx)?;
                let body = indexer
                    .transaction_result(id)
                    .await
                    .map_err(|e| e.to_string())?;
                on_chain.push(channel_file::OnChainTx {
                    role: role.to_string(),
                    transaction_id: tx.trim().to_string(),
                    status: indexer_result_status(&body).to_string(),
                });
            }
            let view = channel_file::trace_view(&record.ootle_network, off_chain, on_chain);
            let body = match format.as_str() {
                "text" => channel_file::trace_text(&view),
                "json" => format!(
                    "{}\n",
                    serde_json::to_string_pretty(&view).map_err(|e| e.to_string())?
                ),
                other => return Err(format!("format: expected text or json, got {other}")),
            };
            if let Some(path) = out {
                ghostline_core::write_private_file(&path, body.as_bytes())
                    .map_err(|e| format!("channel trace: {e}"))?;
                println!("wrote={}", path.display());
            } else {
                print!("{body}");
            }
        }
        ChannelCommand::Watch {
            file: path,
            margin_epochs,
            force,
            submit,
            dry_run,
            seal_key_id_file,
            timeout_secs,
            interval_secs,
            once,
        } => {
            watch_channel(WatchChannelArgs {
                path: &path,
                margin_epochs,
                force,
                submit,
                dry_run,
                seal_key_id_file,
                timeout_secs,
                interval_secs,
                once,
            })
            .await?;
        }
        ChannelCommand::Refund {
            file: path,
            submit,
            dry_run,
            max_fee,
            seal_key_id_file,
            timeout_secs,
        } => {
            let record = channel_file::ChannelFile::load(&path)?;
            refund_due(RefundDueArgs {
                open: open_args_from(&record),
                indexer_url: record.indexer_url.clone(),
                commitment_hex: None,
                utxo_json: record.utxo_json.clone(),
                deposit_mask_hex: record.deposit_mask_hex.clone(),
                deposit_mask_from_channel_file: true,
                max_epoch: record.max_epoch,
                resource_hex: None,
                ootle_network: record.ootle_network.clone(),
                fee_account: record.payer_fee_account.clone(),
                max_fee,
                out: None,
                submit,
                wallet_url: record
                    .payer_wallet_url
                    .clone()
                    .unwrap_or(record.opener_wallet_url.clone()),
                seal_key_id_file,
                fee_account_name: record.payer_fee_account_name.clone(),
                dry_run,
                timeout_secs,
            })
            .await?;
        }
    }
    Ok(())
}

struct SettleDueArgs {
    ledger: PathBuf,
    open: OpenArgs,
    indexer_url: String,
    margin_epochs: u64,
    force: bool,
    commitment_hex: Option<String>,
    utxo_json: Option<PathBuf>,
    deposit_mask_hex: Option<String>,
    deposit_mask_from_channel_file: bool,
    max_epoch: u64,
    resource_hex: Option<String>,
    ootle_network: String,
    fee_account: String,
    max_fee: u64,
    out: Option<PathBuf>,
    submit: bool,
    wallet_url: String,
    seal_key_id_file: Option<PathBuf>,
    fee_account_name: Option<String>,
    dry_run: bool,
    timeout_secs: u64,
}

async fn settle_due(args: SettleDueArgs) -> Result<(), String> {
    let store = ghostline_paywall::ChannelStore::open(&args.ledger).map_err(|e| e.to_string())?;
    let params = store.params().map_err(|e| e.to_string())?;
    let spec = open_spec(&args.open)?;
    if spec.network_tag != params.network_tag
        || spec.deposit != params.deposit
        || spec.payer != params.payer
        || spec.nonce != params.nonce
    {
        return Err("ledger channel does not match open args".into());
    }
    if let Some(hex) = store.latest_voucher_hex() {
        let bytes = from_hex(hex).map_err(|_| "stored voucher: expected even-length hex")?;
        let voucher = SignedVoucher::from_bytes(&bytes).map_err(|e| e.to_string())?;
        voucher.verify(&params).map_err(|e| e.to_string())?;
        if voucher.amount != store.accepted() {
            return Err(format!(
                "stored voucher amount {} does not match accepted {}",
                voucher.amount,
                store.accepted()
            ));
        }
    }
    let current_epoch = fetch_epoch(&args.indexer_url).await?;
    let output = match commitment_arg(args.commitment_hex, args.utxo_json)? {
        Some(hex) => Some(
            lookup_channel_output(&args.indexer_url, args.resource_hex.as_deref(), &hex).await?,
        ),
        None => None,
    };
    let decision = ghostline_paywall::decide_settle(
        store.accepted(),
        store.latest_voucher_hex().is_some(),
        current_epoch,
        args.open.expiry_epoch,
        args.margin_epochs,
        args.force,
    );
    let due = matches!(decision, ghostline_paywall::SettleDecision::Due { .. });
    let decision_outcome = match &decision {
        ghostline_paywall::SettleDecision::NothingOwed => "nothing_owed",
        ghostline_paywall::SettleDecision::MissingVoucher { .. } => "missing_voucher",
        ghostline_paywall::SettleDecision::Wait { .. } => "wait",
        ghostline_paywall::SettleDecision::Due { .. } => "due",
        ghostline_paywall::SettleDecision::Expired { .. } => "expired",
    };
    let mut report = serde_json::json!({
        "decision": decision,
        "current_epoch": current_epoch,
        "unsigned_written": false,
        "submitted": false,
    });
    if let Some((id, presence)) = &output {
        report["channel_output"] = presence_json(id, presence);
        if matches!(presence, SubstatePresence::Down { .. }) {
            record_channel_operation("settle", "output_spent");
            report["decision"] = serde_json::json!({ "action": "output_spent" });
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
    }
    if !due {
        record_channel_operation("settle", decision_outcome);
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    ensure_transaction_epoch_valid(current_epoch, args.max_epoch)?;
    let voucher_hex = store
        .latest_voucher_hex()
        .ok_or("due settle is missing a stored voucher")?
        .to_string();
    let bytes = from_hex(&voucher_hex).map_err(|_| "stored voucher: expected even-length hex")?;
    let voucher = SignedVoucher::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let deposit_mask = if args.deposit_mask_from_channel_file {
        args.deposit_mask_hex
            .clone()
            .ok_or("channel file has no deposit mask")?
    } else {
        secrets::load_hex(
            args.deposit_mask_hex.as_deref(),
            secrets::DEPOSIT_MASK_ENV,
            "deposit mask",
        )?
    };
    let channel = seal::channel(spec)?;
    let resource = seal::parse_resource(args.resource_hex.as_deref())?;
    let scan = seal::parse_scan(&args.ootle_network, resource)?;
    let unsigned = seal::unsigned_settle(
        seal::SettleArgs {
            channel: &channel,
            deposit: funded(channel.params().deposit, &deposit_mask)?,
            voucher: &voucher,
            service_output: voucher.service_output_key,
            resource,
            max_epoch: args.max_epoch,
            scan,
        },
        Some((
            seal::parse_component("fee-account", &args.fee_account)?,
            args.max_fee,
        )),
    )?;
    if !args.submit {
        record_channel_operation("settle", "unsigned_prepared");
    }
    if let Some(path) = &args.out {
        let json = serde_json::to_string(&unsigned).map_err(|e| e.to_string())?;
        ghostline_core::write_private_file(path, json.as_bytes())
            .map_err(|e| format!("settle-due --out: {e}"))?;
        report["unsigned_written"] = serde_json::json!(true);
        report["out"] = serde_json::json!(path.display().to_string());
    }
    if args.submit {
        if args.fee_account_name.is_none() || args.seal_key_id_file.is_none() {
            return Err("submit requires --fee-account-name and --seal-key-id-file".into());
        }
        if let Err(err) = submit_unsigned(SubmitUnsignedArgs {
            url: &args.wallet_url,
            unsigned,
            seal_key_id: None,
            seal_key_id_file: args.seal_key_id_file.as_deref(),
            other_signer: &[],
            fee_account_name: args.fee_account_name.as_deref(),
            dry_run: args.dry_run,
            no_wait: false,
            timeout_secs: args.timeout_secs,
        })
        .await
        {
            record_channel_operation("settle", "submission_error");
            return Err(err);
        }
        record_channel_operation(
            "settle",
            if args.dry_run {
                "dry_run_accepted"
            } else {
                "wallet_accepted"
            },
        );
        report["submitted"] = serde_json::json!(true);
        report["dry_run"] = serde_json::json!(args.dry_run);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

struct WatchChannelArgs<'a> {
    path: &'a std::path::Path,
    margin_epochs: u64,
    force: bool,
    submit: bool,
    dry_run: bool,
    seal_key_id_file: Option<PathBuf>,
    timeout_secs: u64,
    interval_secs: u64,
    once: bool,
}

async fn watch_channel(args: WatchChannelArgs<'_>) -> Result<(), String> {
    let WatchChannelArgs {
        path,
        margin_epochs,
        force,
        submit,
        dry_run,
        seal_key_id_file,
        timeout_secs,
        interval_secs,
        once,
    } = args;
    if !once && interval_secs == 0 {
        return Err("channel watch: interval_secs must be non-zero".into());
    }
    loop {
        let record = channel_file::ChannelFile::load(path)?;
        let ledger = record
            .ledger
            .clone()
            .ok_or("channel file has no ledger path")?;
        let store = ghostline_paywall::ChannelStore::open(&ledger).map_err(|e| e.to_string())?;
        let commitment_hex = record
            .utxo_json
            .as_ref()
            .map(|path| commitment_arg(None, Some(path.clone())))
            .transpose()?
            .flatten();
        let retry_delay = (!once).then_some(std::time::Duration::from_secs(interval_secs));
        let (current_epoch, output_down) = retry_indexer_read(
            || read_chain_state_once(&record.indexer_url, commitment_hex.as_deref()),
            retry_delay,
        )
        .await?;
        let step = ghostline_paywall::watch_step(
            store.accepted(),
            store.latest_voucher_hex().is_some(),
            current_epoch,
            record.expiry_epoch,
            margin_epochs,
            force,
            output_down,
        );
        let outcome = match &step {
            ghostline_paywall::WatchStep::Wait { .. } => "wait",
            ghostline_paywall::WatchStep::Settle { .. } => "settle_due",
            ghostline_paywall::WatchStep::Refund { .. } => "refund_due",
            ghostline_paywall::WatchStep::OutputSpent => "output_spent",
            ghostline_paywall::WatchStep::NothingOwed => "nothing_owed",
            ghostline_paywall::WatchStep::MissingVoucher { .. } => "missing_voucher",
        };
        record_channel_operation("watch", outcome);
        println!(
            "{}",
            serde_json::to_string(&step).map_err(|e| e.to_string())?
        );
        match &step {
            ghostline_paywall::WatchStep::Wait { .. } if !once => {
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
                continue;
            }
            ghostline_paywall::WatchStep::Settle { .. } => {
                let fee_account = record
                    .fee_account
                    .clone()
                    .ok_or("channel file has no fee_account")?;
                return settle_due(SettleDueArgs {
                    ledger,
                    open: open_args_from(&record),
                    indexer_url: record.indexer_url.clone(),
                    margin_epochs,
                    force,
                    commitment_hex: None,
                    utxo_json: record.utxo_json.clone(),
                    deposit_mask_hex: record.deposit_mask_hex.clone(),
                    deposit_mask_from_channel_file: true,
                    max_epoch: record.max_epoch,
                    resource_hex: None,
                    ootle_network: record.ootle_network.clone(),
                    fee_account,
                    max_fee: 100_000,
                    out: None,
                    submit,
                    wallet_url: record
                        .fee_wallet_url
                        .clone()
                        .unwrap_or(record.opener_wallet_url.clone()),
                    seal_key_id_file,
                    fee_account_name: record.fee_account_name.clone(),
                    dry_run,
                    timeout_secs,
                })
                .await;
            }
            _ => return Ok(()),
        }
    }
}

async fn read_chain_state_once(
    indexer_url: &str,
    commitment_hex: Option<&str>,
) -> Result<(u64, bool), String> {
    let current_epoch = fetch_epoch(indexer_url).await?;
    let output_down = match commitment_hex {
        Some(hex) => matches!(
            lookup_channel_output(indexer_url, None, hex).await?.1,
            SubstatePresence::Down { .. }
        ),
        None => false,
    };
    Ok((current_epoch, output_down))
}

async fn retry_indexer_read<T, F, Fut>(
    mut read: F,
    retry_delay: Option<std::time::Duration>,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    loop {
        match read().await {
            Ok(value) => return Ok(value),
            Err(error) => match retry_delay {
                Some(delay) => {
                    eprintln!("channel watch: indexer read failed ({error}); retrying");
                    tokio::time::sleep(delay).await;
                }
                None => return Err(error),
            },
        }
    }
}

struct RefundDueArgs {
    open: OpenArgs,
    indexer_url: String,
    commitment_hex: Option<String>,
    utxo_json: Option<PathBuf>,
    deposit_mask_hex: Option<String>,
    deposit_mask_from_channel_file: bool,
    max_epoch: u64,
    resource_hex: Option<String>,
    ootle_network: String,
    fee_account: Option<String>,
    max_fee: u64,
    out: Option<PathBuf>,
    submit: bool,
    wallet_url: String,
    seal_key_id_file: Option<PathBuf>,
    fee_account_name: Option<String>,
    dry_run: bool,
    timeout_secs: u64,
}

async fn refund_due(args: RefundDueArgs) -> Result<(), String> {
    let current_epoch = fetch_epoch(&args.indexer_url).await?;
    let output = match commitment_arg(args.commitment_hex, args.utxo_json)? {
        Some(hex) => Some(
            lookup_channel_output(&args.indexer_url, args.resource_hex.as_deref(), &hex).await?,
        ),
        None => None,
    };
    let decision = ghostline_paywall::decide_refund(current_epoch, args.open.expiry_epoch);
    let due = matches!(decision, ghostline_paywall::RefundDecision::Due { .. });
    let decision_outcome = match &decision {
        ghostline_paywall::RefundDecision::Wait { .. } => "wait",
        ghostline_paywall::RefundDecision::Due { .. } => "due",
    };
    let mut report = serde_json::json!({
        "decision": decision,
        "current_epoch": current_epoch,
        "unsigned_written": false,
        "submitted": false,
    });
    if let Some((id, presence)) = &output {
        report["channel_output"] = presence_json(id, presence);
        if matches!(presence, SubstatePresence::Down { .. }) {
            record_channel_operation("refund", "output_spent");
            report["decision"] = serde_json::json!({ "action": "output_spent" });
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            return Ok(());
        }
    }
    if !due {
        record_channel_operation("refund", decision_outcome);
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    ensure_transaction_epoch_valid(current_epoch, args.max_epoch)?;
    let spec = open_spec(&args.open)?;
    let deposit_mask = if args.deposit_mask_from_channel_file {
        args.deposit_mask_hex
            .clone()
            .ok_or("channel file has no deposit mask")?
    } else {
        secrets::load_hex(
            args.deposit_mask_hex.as_deref(),
            secrets::DEPOSIT_MASK_ENV,
            "deposit mask",
        )?
    };
    let channel = seal::channel(spec)?;
    let resource = seal::parse_resource(args.resource_hex.as_deref())?;
    let scan = seal::parse_scan(&args.ootle_network, resource)?;
    let unsigned = seal::unsigned_refund(
        &channel,
        funded(channel.params().deposit, &deposit_mask)?,
        channel.spec().payer,
        resource,
        args.max_epoch,
        scan,
        Some((
            seal::parse_component(
                "fee-account",
                args.fee_account
                    .as_deref()
                    .ok_or("due refund requires --fee-account")?,
            )?,
            args.max_fee,
        )),
    )?;
    if !args.submit {
        record_channel_operation("refund", "unsigned_prepared");
    }
    if let Some(path) = &args.out {
        let json = serde_json::to_string(&unsigned).map_err(|e| e.to_string())?;
        ghostline_core::write_private_file(path, json.as_bytes())
            .map_err(|e| format!("refund-due --out: {e}"))?;
        report["unsigned_written"] = serde_json::json!(true);
        report["out"] = serde_json::json!(path.display().to_string());
    }
    if args.submit {
        if args.fee_account_name.is_none() || args.seal_key_id_file.is_none() {
            return Err("submit requires --fee-account-name and --seal-key-id-file".into());
        }
        if let Err(err) = submit_unsigned(SubmitUnsignedArgs {
            url: &args.wallet_url,
            unsigned,
            seal_key_id: None,
            seal_key_id_file: args.seal_key_id_file.as_deref(),
            other_signer: &[],
            fee_account_name: args.fee_account_name.as_deref(),
            dry_run: args.dry_run,
            no_wait: false,
            timeout_secs: args.timeout_secs,
        })
        .await
        {
            record_channel_operation("refund", "submission_error");
            return Err(err);
        }
        record_channel_operation(
            "refund",
            if args.dry_run {
                "dry_run_accepted"
            } else {
                "wallet_accepted"
            },
        );
        report["submitted"] = serde_json::json!(true);
        report["dry_run"] = serde_json::json!(args.dry_run);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

struct SubmitUnsignedArgs<'a> {
    url: &'a str,
    unsigned: UnsignedTransaction,
    seal_key_id: Option<&'a str>,
    seal_key_id_file: Option<&'a std::path::Path>,
    other_signer: &'a [String],
    fee_account_name: Option<&'a str>,
    dry_run: bool,
    no_wait: bool,
    timeout_secs: u64,
}

async fn submit_unsigned(args: SubmitUnsignedArgs<'_>) -> Result<(), String> {
    let SubmitUnsignedArgs {
        url,
        unsigned,
        seal_key_id,
        seal_key_id_file,
        other_signer,
        fee_account_name,
        dry_run,
        no_wait,
        timeout_secs,
    } = args;
    let seal_signer = read_key_id(seal_key_id, seal_key_id_file)?;
    let explicit = other_signer
        .iter()
        .map(|s| serde_json::from_str(s).map_err(|e| format!("other-signer JSON: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    let wallet = connect_wallet(url).await?;
    let from_account = if let Some(name) = fee_account_name {
        let got = wallet.account_get(name).await.map_err(|e| e.to_string())?;
        println!("fee_account={}", got.account.component_address_display());
        Some(
            got.account
                .fee_signer_key_id()
                .map_err(|e| e.to_string())?
                .clone(),
        )
    } else {
        None
    };
    let other_signers = settle_fee_signers(explicit, from_account).map_err(|e| e.to_string())?;
    let request = WalletSubmitRequest::new(unsigned, seal_signer, other_signers);
    if dry_run {
        let dry = wallet
            .submit_dry_run(&request)
            .await
            .map_err(|e| e.to_string())?;
        println!("transaction_id={}", dry.transaction_id);
        println!("required_fees={}", dry.required_fees);
        if execution_accepted(&dry.result) {
            println!("result=Accept");
        } else {
            return Err(format!("dry-run did not Accept: {}", dry.result));
        }
    } else {
        let submitted = wallet.submit(&request).await.map_err(|e| e.to_string())?;
        println!("transaction_id={}", submitted.transaction_id);
        if !no_wait {
            print_wait(&wallet, submitted.transaction_id, timeout_secs).await?;
        }
    }
    Ok(())
}

fn commitment_arg(
    commitment_hex: Option<String>,
    utxo_json: Option<PathBuf>,
) -> Result<Option<String>, String> {
    match (commitment_hex, utxo_json) {
        (Some(_), Some(_)) => Err("pass only one of --commitment-hex and --utxo-json".into()),
        (Some(hex), None) => Ok(Some(hex)),
        (None, Some(path)) => {
            let parsed: UtxoJson = serde_json::from_str(
                &std::fs::read_to_string(&path).map_err(|e| format!("utxo-json: {e}"))?,
            )
            .map_err(|e| format!("utxo-json: {e}"))?;
            Ok(Some(parsed.commitment))
        }
        (None, None) => Ok(None),
    }
}

async fn lookup_channel_output(
    indexer_url: &str,
    resource_hex: Option<&str>,
    commitment_hex: &str,
) -> Result<(String, SubstatePresence), String> {
    let id = seal::utxo_substate_id(seal::parse_resource(resource_hex)?, commitment_hex)?;
    let presence = IndexerClient::connect(indexer_url)
        .map_err(|e| e.to_string())?
        .substate_presence(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok((id, presence))
}

fn presence_json(substate_id: &str, presence: &SubstatePresence) -> serde_json::Value {
    match presence {
        SubstatePresence::Up { version } => serde_json::json!({
            "substate_id": substate_id,
            "presence": "up",
            "version": version,
        }),
        SubstatePresence::Down { version } => serde_json::json!({
            "substate_id": substate_id,
            "presence": "down",
            "version": version,
        }),
    }
}

async fn fetch_epoch(indexer_url: &str) -> Result<u64, String> {
    Ok(IndexerClient::connect(indexer_url)
        .map_err(|e| e.to_string())?
        .epoch_stats()
        .await
        .map_err(|e| e.to_string())?
        .current_epoch
        .as_u64())
}

fn ensure_transaction_epoch_valid(current_epoch: u64, max_epoch: u64) -> Result<(), String> {
    if current_epoch > max_epoch {
        return Err(format!(
            "transaction max epoch {max_epoch} is before current epoch {current_epoch}; set --max-epoch to at least the current epoch before building or submitting"
        ));
    }
    Ok(())
}

struct ChannelStatusArgs<'a> {
    session: Option<PathBuf>,
    ledger: Option<PathBuf>,
    store: Option<PathBuf>,
    expiry_epoch: Option<u64>,
    indexer_url: Option<&'a str>,
    include_params: bool,
    commitment_hex: Option<String>,
    utxo_json: Option<PathBuf>,
    resource_hex: Option<&'a str>,
}

async fn print_channel_status(args: ChannelStatusArgs<'_>) -> Result<(), String> {
    let ChannelStatusArgs {
        session,
        ledger,
        store,
        expiry_epoch,
        indexer_url,
        include_params,
        commitment_hex,
        utxo_json,
        resource_hex,
    } = args;
    let current_epoch = match indexer_url {
        Some(url) => Some(fetch_epoch(url).await?),
        None => None,
    };
    let params_path = store
        .clone()
        .or_else(|| session.clone())
        .or_else(|| ledger.clone());
    let view = match (store, session, ledger) {
        (Some(path), None, None) => {
            let opened = ghostline_paywall::ChannelStore::open(path).map_err(|e| e.to_string())?;
            ghostline_paywall::ChannelStatus::from_store(&opened, expiry_epoch, current_epoch)
                .map_err(|e| e.to_string())?
        }
        (None, Some(session_path), Some(ledger_path)) => {
            let session_store =
                ghostline_paywall::ChannelStore::open(session_path).map_err(|e| e.to_string())?;
            let ledger_store =
                ghostline_paywall::ChannelStore::open(ledger_path).map_err(|e| e.to_string())?;
            ghostline_paywall::ChannelStatus::from_session_and_ledger(
                &session_store,
                &ledger_store,
                expiry_epoch,
                current_epoch,
            )
            .map_err(|e| e.to_string())?
        }
        (None, Some(path), None) | (None, None, Some(path)) => {
            let opened = ghostline_paywall::ChannelStore::open(path).map_err(|e| e.to_string())?;
            ghostline_paywall::ChannelStatus::from_store(&opened, expiry_epoch, current_epoch)
                .map_err(|e| e.to_string())?
        }
        (None, None, None) => {
            return Err("set --store, or --session and/or --ledger".into());
        }
        _ => {
            return Err("use --store alone, or --session/--ledger without --store".into());
        }
    };
    let mut value = serde_json::to_value(&view).map_err(|e| e.to_string())?;
    if include_params {
        let path = params_path.ok_or("set --store, or --session and/or --ledger")?;
        let opened = ghostline_paywall::ChannelStore::open(path).map_err(|e| e.to_string())?;
        let params = opened.params().map_err(|e| e.to_string())?;
        value["network_tag"] = serde_json::json!(params.network_tag);
        value["payer_hex"] = serde_json::json!(to_hex(params.payer.as_bytes()));
        value["payer_root_hex"] = serde_json::json!(to_hex(params.payer_root.as_slice()));
        value["nonce_hex"] = serde_json::json!(to_hex(&params.nonce));
    }
    value["channel_output"] = match commitment_arg(commitment_hex, utxo_json)? {
        None => serde_json::json!("not_checked"),
        Some(hex) => {
            let url = indexer_url.ok_or("commitment lookup needs --indexer-url")?;
            let (id, presence) = lookup_channel_output(url, resource_hex, &hex).await?;
            presence_json(&id, &presence)
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn load_utxo(
    utxo_json: Option<PathBuf>,
    commitment_hex: Option<String>,
    nonce_hex: Option<String>,
    encrypted_hex: Option<String>,
    tag: Option<i64>,
) -> Result<(String, String, String, u32, Option<u64>), String> {
    if let Some(path) = utxo_json {
        let parsed: UtxoJson = serde_json::from_str(
            &std::fs::read_to_string(&path).map_err(|e| format!("utxo-json: {e}"))?,
        )
        .map_err(|e| format!("utxo-json: {e}"))?;
        return Ok((
            parsed.commitment,
            parsed.nonce,
            parsed.encrypted_hex,
            parsed.tag as u32,
            parsed.value,
        ));
    }
    Ok((
        commitment_hex.ok_or("unblind requires --utxo-json or --commitment-hex")?,
        nonce_hex.ok_or("unblind requires --nonce-hex")?,
        encrypted_hex.ok_or("unblind requires --encrypted-hex")?,
        tag.ok_or("unblind requires --tag")? as u32,
        None,
    ))
}

fn print_wallet_account(got: &ghostline_net::AccountWithAddress) {
    if let Some(name) = &got.account.name {
        println!("name={name}");
    }
    println!(
        "component_address={}",
        got.account.component_address_display()
    );
    println!("is_default={}", got.account.is_default);
    match &got.account.owner_key_id {
        Some(key_id) => println!("owner_key_id={key_id}"),
        None => println!("owner_key_id="),
    }
    println!("address={}", got.address_display());
}

fn open_max_fee(dry_run: bool, configured: Option<u64>) -> Result<u64, String> {
    match configured {
        Some(0) => Err("max-fee must be greater than zero".into()),
        Some(fee) => Ok(fee),
        None if dry_run => Ok(100_000),
        None => Err("live channel open requires an explicit --max-fee; dry-run first".into()),
    }
}

fn wallet_open_error(message: String) -> String {
    if message.to_ascii_lowercase().contains("insufficient funds") {
        format!(
            "{message}\nHint: Check that this account has enough available tTARI for the deposit and fee cap. If you just funded or created the account, walletd may still be scanning for its output; wait for the scan to find it, then retry. This error alone cannot distinguish a real shortfall from an output that has not been discovered yet."
        )
    } else {
        message
    }
}

async fn print_wallet_open_fee_quote(
    wallet: &WalletClient,
    transaction_id: TransactionId,
) -> Result<(), String> {
    let transaction = wallet
        .transaction_get(transaction_id)
        .await
        .map_err(|e| format!("open fee quote: {e}"))?;
    let estimated = transaction
        .get("final_fee")
        .and_then(serde_json::Value::as_u64)
        .ok_or("open fee quote is missing final_fee")?;
    let required = transaction
        .get("result")
        .and_then(|result| result.get("total_fees_required"))
        .and_then(serde_json::Value::as_u64)
        .ok_or("open fee quote is missing total_fees_required")?;
    println!("estimated_fee={estimated}");
    println!("required_fee={required}");
    Ok(())
}

async fn connect_wallet(url: &str) -> Result<WalletClient, String> {
    let mut wallet = WalletClient::connect(url).map_err(|e| e.to_string())?;
    wallet.auth_none_admin().await.map_err(|e| e.to_string())?;
    Ok(wallet)
}

fn parse_transaction_id(hex: &str) -> Result<TransactionId, String> {
    TransactionId::from_hex(hex.trim()).map_err(|e| format!("transaction-id: {e}"))
}

async fn dump_wallet_utxos(
    wallet: &WalletClient,
    transaction_id: TransactionId,
    utxo_json_out: Option<&std::path::Path>,
) -> Result<(), String> {
    let got = wallet
        .transaction_get(transaction_id)
        .await
        .map_err(|e| e.to_string())?;
    let utxos = stealth_utxos_from_wallet_tx(&got);
    print_stealth_utxos(&utxos);
    if let Some(path) = utxo_json_out {
        let Some(channel) = channel_open_utxo(&utxos) else {
            return Err("no body stealth output to write as utxo-json".into());
        };
        ghostline_core::write_private_file(
            path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&channel.unblind_json()).map_err(|e| e.to_string())?
            )
            .as_bytes(),
        )
        .map_err(|e| format!("utxo-json-out: {e}"))?;
        println!("wrote={}", path.display());
    }
    Ok(())
}

fn print_stealth_utxos(utxos: &[WalletStealthUtxo]) {
    println!("outputs={}", utxos.len());
    for (i, utxo) in utxos.iter().enumerate() {
        println!(
            "output[{i}] auth={} tag={} from_fee={} resource={}",
            utxo.auth,
            utxo.tag,
            utxo.from_fee_instruction,
            utxo.resource.as_deref().unwrap_or("")
        );
        println!(
            "utxo_json={}",
            serde_json::to_string(&utxo.unblind_json()).expect("utxo json")
        );
    }
}

async fn print_wait(
    wallet: &WalletClient,
    transaction_id: ghostline_net::TransactionId,
    timeout_secs: u64,
) -> Result<(), String> {
    let waited = wallet
        .wait_result(transaction_id, Some(timeout_secs))
        .await
        .map_err(|e| e.to_string())?;
    let outcome = waited.get("result").unwrap_or(&waited);
    let status = waited
        .get("status")
        .and_then(|s| s.as_str())
        .or_else(|| outcome.get("status").and_then(|s| s.as_str()));
    if execution_accepted(outcome) || execution_accepted(&waited) || status == Some("Accepted") {
        println!("result=Accept");
        Ok(())
    } else {
        Err(format!("wait did not Accept: {waited}"))
    }
}

fn read_unsigned_json(path: &std::path::Path) -> Result<UnsignedTransaction, String> {
    let raw = if path.as_os_str() == "-" {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .map_err(|e| format!("unsigned-json stdin: {e}"))?;
        buf
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("unsigned-json: {e}"))?
    };
    serde_json::from_str(raw.trim()).map_err(|e| format!("unsigned-json: {e}"))
}

fn read_key_id(
    json: Option<&str>,
    file: Option<&std::path::Path>,
) -> Result<serde_json::Value, String> {
    let raw = match (json, file) {
        (Some(s), None) => s.to_string(),
        (None, Some(path)) => {
            std::fs::read_to_string(path).map_err(|e| format!("seal-key-id-file: {e}"))?
        }
        (Some(_), Some(_)) => {
            return Err("pass either --seal-key-id or --seal-key-id-file, not both".into());
        }
        (None, None) => {
            return Err("set --seal-key-id JSON or --seal-key-id-file".into());
        }
    };
    serde_json::from_str(raw.trim()).map_err(|e| format!("seal-key-id: {e}"))
}

/// Find the single account key allocated between two `keys.list` snapshots.
/// The wallet's documented/live response is `[KeyId, public_key_hex, is_active]`.
fn newly_created_account_key(
    before: &[serde_json::Value],
    after: &[serde_json::Value],
) -> Result<(serde_json::Value, String), String> {
    let new_rows: Vec<_> = after
        .iter()
        .filter(|row| {
            let Some(fields) = row.as_array() else {
                return false;
            };
            let Some(key_id) = fields.first() else {
                return false;
            };
            !before.iter().any(|old| {
                old.as_array()
                    .and_then(|old_fields| old_fields.first())
                    .is_some_and(|old_id| old_id == key_id)
            })
        })
        .collect();
    let [row] = new_rows.as_slice() else {
        return Err(format!(
            "expected exactly one new account key, found {}",
            new_rows.len()
        ));
    };
    let fields = row
        .as_array()
        .ok_or_else(|| "wallet keys.list returned a malformed key row".to_string())?;
    let key_id = fields
        .first()
        .cloned()
        .ok_or_else(|| "wallet keys.list key row has no KeyId".to_string())?;
    let public_key_hex = fields
        .get(1)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "wallet keys.list key row has no public key".to_string())?
        .to_string();
    let public_key = from_hex(&public_key_hex)
        .map_err(|_| "wallet keys.list returned a malformed public key".to_string())?;
    if public_key.len() != 32 {
        return Err("wallet keys.list public key must be 32 bytes".into());
    }
    Ok((key_id, public_key_hex))
}

fn decode_tx(tx_hex: &str) -> Result<ghostline_net::Transaction, String> {
    let bytes = from_hex(tx_hex).map_err(|_| "tx: expected even-length hex".to_string())?;
    TransactionEnvelope::from_raw(bytes.into_boxed_slice())
        .decode()
        .map_err(|e| format!("tx envelope: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_settle_accepts_reviewed_fee_cap() {
        let cli = Cli::try_parse_from([
            "ghostline",
            "channel",
            "settle",
            "--file",
            "channel.json",
            "--max-fee",
            "23456",
        ])
        .unwrap();
        match cli.command {
            Command::Channel {
                command: ChannelCommand::Settle { max_fee, .. },
            } => assert_eq!(max_fee, 23_456),
            _ => panic!("expected channel settle command"),
        }
    }

    #[test]
    fn wallet_key_create_finds_new_key_id_and_public_key() {
        let before = serde_json::json!([
            [{"Derived":{"index":0,"key_branch":"account"}}, "11".repeat(32), true]
        ]);
        let after = serde_json::json!([
            [{"Derived":{"index":0,"key_branch":"account"}}, "11".repeat(32), true],
            [{"Derived":{"index":1,"key_branch":"account"}}, "22".repeat(32), true]
        ]);
        let (key_id, public_hex) =
            newly_created_account_key(before.as_array().unwrap(), after.as_array().unwrap())
                .unwrap();
        assert_eq!(key_id["Derived"]["index"], 1);
        assert_eq!(public_hex, "22".repeat(32));
    }

    #[test]
    fn wallet_key_create_fails_closed_when_new_key_is_ambiguous_or_malformed() {
        let before = serde_json::json!([]);
        let no_new_key = serde_json::json!([]);
        assert!(
            newly_created_account_key(before.as_array().unwrap(), no_new_key.as_array().unwrap())
                .is_err()
        );

        let two_new_keys = serde_json::json!([
            [{"Derived":{"index":0,"key_branch":"account"}}, "11".repeat(32), true],
            [{"Derived":{"index":1,"key_branch":"account"}}, "22".repeat(32), true]
        ]);
        assert!(
            newly_created_account_key(before.as_array().unwrap(), two_new_keys.as_array().unwrap())
                .is_err()
        );
    }

    #[test]
    fn live_open_requires_an_explicit_fee_cap() {
        assert_eq!(open_max_fee(true, None).unwrap(), 100_000);
        assert_eq!(open_max_fee(true, Some(15_217)).unwrap(), 15_217);
        assert!(open_max_fee(false, None).unwrap_err().contains("--max-fee"));
        assert!(open_max_fee(false, Some(0)).is_err());
    }

    #[test]
    fn due_close_rejects_a_transaction_max_epoch_in_the_past() {
        assert!(
            ensure_transaction_epoch_valid(12403, 12402)
                .unwrap_err()
                .contains("set --max-epoch to at least the current epoch")
        );
        assert!(ensure_transaction_epoch_valid(12403, 12403).is_ok());
    }

    #[test]
    fn insufficient_funds_open_error_explains_wallet_scan_lag() {
        let message = wallet_open_error(
            "wallet daemon RPC accounts.stealth_transfer failed: Insufficient funds: Available: 0, Required: 110000".into(),
        );
        assert!(message.contains("Available: 0, Required: 110000"));
        assert!(message.contains("walletd may still be scanning"));
        assert!(message.contains("cannot distinguish a real shortfall"));
    }

    #[test]
    fn unrelated_open_errors_are_preserved() {
        let message = "wallet daemon RPC failed: connection refused";
        assert_eq!(wallet_open_error(message.into()), message);
    }
    use ootle_byte_type::ToByteType;
    use tari_crypto::keys::PublicKey as _;
    use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};
    use tari_utilities::ByteArray;

    fn sample() -> (RistrettoSecretKey, ChannelParams) {
        let (secret, public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let payer = RistrettoPublicKeyBytes::from_bytes(public.as_bytes()).unwrap();
        let params = ChannelParams {
            network_tag: 1,
            payer,
            deposit: 100,
            payer_root: Hash32::from_array([9u8; 32]),
            nonce: [3u8; NONCE_LEN],
        };
        (secret, params)
    }

    #[test]
    fn human_approval_requires_an_explicit_yes() {
        assert!(approval_answer("y\n"));
        assert!(approval_answer(" YES "));
        assert!(!approval_answer(""));
        assert!(!approval_answer("no"));
        assert!(!approval_answer("true"));
    }

    #[tokio::test]
    async fn continuous_watcher_retries_transient_indexer_read_errors() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let attempts = Arc::new(AtomicUsize::new(0));
        let observed_attempts = Arc::clone(&attempts);
        let value = retry_indexer_read(
            move || {
                let attempt = observed_attempts.fetch_add(1, Ordering::SeqCst);
                async move {
                    if attempt == 0 {
                        Err("temporary indexer error".to_string())
                    } else {
                        Ok(attempt)
                    }
                }
            },
            Some(std::time::Duration::ZERO),
        )
        .await
        .unwrap();

        assert_eq!(value, 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn one_shot_watcher_returns_indexer_error_without_retrying() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let attempts = Arc::new(AtomicUsize::new(0));
        let observed_attempts = Arc::clone(&attempts);
        let result: Result<(), String> = retry_indexer_read(
            move || {
                observed_attempts.fetch_add(1, Ordering::SeqCst);
                async { Err("indexer unavailable".to_string()) }
            },
            None,
        )
        .await;

        assert_eq!(result.unwrap_err(), "indexer unavailable");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn session_is_created_once_and_keeps_cumulative() {
        let (secret, payer_pk) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let (_, service_pk) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let dir = std::env::temp_dir().join(format!(
            "ghostline-session-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("channel.json");
        let mask = "55".repeat(32);
        channel_file::ChannelFile {
            network_tag: 1,
            deposit: 100,
            payer_hex: to_hex(payer_pk.as_bytes()),
            service_hex: to_hex(service_pk.as_bytes()),
            expiry_epoch: 50,
            nonce_hex: to_hex(&[3u8; 32]),
            template_hex: to_hex(&[9u8; 32]),
            indexer_url: "http://127.0.0.1:1/".into(),
            ootle_network: "esmeralda".into(),
            max_epoch: 10,
            opener_account: "ghostline".into(),
            opener_wallet_url: "http://127.0.0.1:5100".into(),
            fee_account: None,
            fee_account_name: None,
            fee_wallet_url: None,
            payer_fee_account: None,
            payer_fee_account_name: None,
            payer_wallet_url: None,
            ledger: None,
            session: None,
            utxo_json: None,
            deposit_mask_hex: Some(mask.clone()),
        }
        .save(&path)
        .unwrap();
        let (session_path, created, cumulative) = ensure_payer_session(&path).unwrap();
        assert!(created);
        assert_eq!(cumulative, 0);
        let loaded = channel_file::ChannelFile::load(&path).unwrap();
        assert_eq!(loaded.session.as_deref(), Some(session_path.as_path()));
        assert_eq!(loaded.deposit_mask_hex.as_deref(), Some(mask.as_str()));
        let mut store = ghostline_paywall::ChannelStore::open(&session_path).unwrap();
        let mut session = store.session().unwrap();
        session
            .sign_next(&secret, 10, create_key_pair_from_seed(13).1.to_byte_type())
            .unwrap();
        store.save_session(&session).unwrap();
        let (_, again, cumulative) = ensure_payer_session(&path).unwrap();
        assert!(!again);
        assert_eq!(cumulative, 10);
        let other = dir.join("other.json");
        let mut mismatched = loaded;
        mismatched.nonce_hex = to_hex(&[4u8; 32]);
        mismatched.session = Some(session_path);
        mismatched.save(&other).unwrap();
        let err = ensure_payer_session(&other).unwrap_err();
        assert!(err.contains("does not match"));
    }

    #[test]
    fn sign_then_verify() {
        let (secret, params) = sample();
        let voucher = SignedVoucher::sign(
            &secret,
            &params,
            30,
            create_key_pair_from_seed(13).1.to_byte_type(),
        )
        .unwrap();
        voucher.verify(&params).unwrap();
        assert_eq!(voucher.channel_id.len(), CHANNEL_ID_LEN);
    }
}

#[cfg(test)]
mod seal_tests {
    use super::*;
    use ootle_byte_type::ToByteType;
    use tari_crypto::keys::SecretKey as _;
    use tari_crypto::ristretto::RistrettoSecretKey;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::Hash32;
    use tari_template_lib::types::constants::STEALTH_TARI_RESOURCE_ADDRESS;

    #[test]
    fn seal_open_then_settle_round_trip_hex() {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (service_secret, service_public) = create_key_pair_from_seed(13);
        let (_, payout_public) = create_key_pair_from_seed(14);
        let channel = seal::channel(seal::OpenSpec {
            network_tag: 1,
            deposit: 100,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let input = MaskAndValue {
            value: 100,
            mask: RistrettoSecretKey::random(&mut rand::rng()),
        };
        let (open_hex, deposit_mask) = seal::seal_open(
            &channel,
            input,
            channel.spec().payer,
            STEALTH_TARI_RESOURCE_ADDRESS,
            1,
            &payer_secret,
            OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
        )
        .unwrap();
        decode_tx(&open_hex).unwrap();
        let voucher = SignedVoucher::sign(
            &payer_secret,
            channel.params(),
            30,
            payout_public.to_byte_type(),
        )
        .unwrap();
        let settle_hex = seal::seal_settle(
            seal::SettleArgs {
                channel: &channel,
                deposit: funded(100, &deposit_mask).unwrap(),
                voucher: &voucher,
                service_output: voucher.service_output_key,
                resource: STEALTH_TARI_RESOURCE_ADDRESS,
                max_epoch: 1,
                scan: OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
            },
            &service_secret,
            None,
        )
        .unwrap();
        decode_tx(&settle_hex).unwrap();
    }

    #[test]
    fn unblind_recovers_deposit_mask_from_open_output() {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let channel = seal::channel(seal::OpenSpec {
            network_tag: 1,
            deposit: 100,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let opened = ghostline_sdk::fund_open(
            &channel,
            [MaskAndValue {
                value: 100,
                mask: RistrettoSecretKey::random(&mut rand::rng()),
            }],
            channel.spec().payer,
            OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
        )
        .unwrap();
        let out = &opened.statement.outputs_statement.outputs[0];
        let recovered = unblind_output(
            &out.output,
            out.tag,
            &payer_secret,
            OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
        )
        .unwrap();
        assert_eq!(recovered.value, 100);
        assert_eq!(recovered.mask, opened.deposit.mask);
    }

    #[test]
    fn unblind_tx_recovers_the_same_deposit() {
        let open_hex = {
            let (payer_secret, payer_public) = create_key_pair_from_seed(11);
            let (_, service_public) = create_key_pair_from_seed(13);
            let channel = seal::channel(seal::OpenSpec {
                network_tag: 1,
                deposit: 100,
                payer: payer_public.to_byte_type(),
                service: service_public.to_byte_type(),
                expiry_epoch: u64::MAX,
                nonce: [42u8; NONCE_LEN],
                channel_template: Hash32::from_array([7u8; 32]),
            })
            .unwrap();
            let input = MaskAndValue {
                value: 100,
                mask: RistrettoSecretKey::random(&mut rand::rng()),
            };
            let (hex, _) = seal::seal_open(
                &channel,
                input,
                channel.spec().payer,
                STEALTH_TARI_RESOURCE_ADDRESS,
                1,
                &payer_secret,
                OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
            )
            .unwrap();
            hex
        };
        let (payer_secret, _) = create_key_pair_from_seed(11);
        let found = unblind_transaction(&decode_tx(&open_hex).unwrap(), &payer_secret).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].funds.value, 100);
        let views = inspect_transaction(&decode_tx(&open_hex).unwrap());
        assert_eq!(views.len(), 1);
        let recovered = unblind_output(
            &views[0].output,
            views[0].tag,
            &payer_secret,
            OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
        )
        .unwrap();
        assert_eq!(recovered.mask, found[0].funds.mask);
    }

    #[test]
    fn unsigned_settle_with_dry_run_sets_the_flag() {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (_service_secret, service_public) = create_key_pair_from_seed(13);
        let channel = seal::channel(seal::OpenSpec {
            network_tag: 1,
            deposit: 100,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let voucher =
            SignedVoucher::sign(&payer_secret, channel.params(), 30, channel.spec().service)
                .unwrap();
        let unsigned = seal::unsigned_settle(
            seal::SettleArgs {
                channel: &channel,
                deposit: MaskAndValue {
                    value: 100,
                    mask: RistrettoSecretKey::random(&mut rand::rng()),
                },
                voucher: &voucher,
                service_output: voucher.service_output_key,
                resource: STEALTH_TARI_RESOURCE_ADDRESS,
                max_epoch: 1,
                scan: OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
            },
            None,
        )
        .unwrap();
        let off = serde_json::to_value(&unsigned).unwrap();
        assert_eq!(off["V1"]["dry_run"], false);
        let on = serde_json::to_value(unsigned.with_dry_run(true)).unwrap();
        assert_eq!(on["V1"]["dry_run"], true);
    }

    #[test]
    fn unsigned_refund_serializes_like_settle() {
        let (_, payer_public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let channel = seal::channel(seal::OpenSpec {
            network_tag: 1,
            deposit: 100,
            payer: payer_public.to_byte_type(),
            service: service_public.to_byte_type(),
            expiry_epoch: 0,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
        })
        .unwrap();
        let unsigned = seal::unsigned_refund(
            &channel,
            MaskAndValue {
                value: 100,
                mask: RistrettoSecretKey::random(&mut rand::rng()),
            },
            channel.spec().payer,
            STEALTH_TARI_RESOURCE_ADDRESS,
            1,
            OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
            None,
        )
        .unwrap();
        let json = serde_json::to_value(&unsigned).unwrap();
        assert_eq!(json["V1"]["dry_run"], false);
        assert!(
            json["V1"]["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| { i.get("version").is_some() && i["version"].is_null() })
        );
    }

    #[test]
    fn sqlite_signed_tag_becomes_u32() {
        let parsed: UtxoJson = serde_json::from_str(
            r#"{"commitment":"aa","nonce":"bb","encrypted_hex":"cc","tag":-1825295126,"value":1000000}"#,
        )
        .unwrap();
        assert_eq!(parsed.tag as u32, 2469672170);
        assert_eq!(parsed.value, Some(1_000_000));
    }
}
