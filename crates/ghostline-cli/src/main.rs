// SPDX-License-Identifier: MIT

mod seal;
mod secrets;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ghostline_core::{CHANNEL_ID_LEN, ChannelParams, NONCE_LEN, SignedVoucher, from_hex, to_hex};
use ghostline_net::{
    DEFAULT_ACCOUNT_SIGNER_JSON, IndexerClient, TransactionEnvelope, WalletClient,
    WalletSubmitRequest, execution_accepted,
};
use ghostline_sdk::{
    MaskAndValue, OutputScan, UnsignedTransaction, inspect_transaction, unblind_output,
    unblind_transaction,
};
use serde::Deserialize;
use tari_utilities::ByteArray;

#[derive(Parser)]
#[command(
    name = "ghostline",
    about = "Sign and verify Ghostline vouchers, seal stealth transfers, inspect/unblind stealth outputs, submit via a wallet daemon or indexer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the 32-byte channel id for the given open parameters.
    ChannelId(ParamsArgs),
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
        #[arg(long)]
        budget: Option<u64>,
        #[arg(long)]
        max_per_call: Option<u64>,
        /// Satisfy a configured human gate for this sign only.
        #[arg(long)]
        attest: bool,
        /// Sign an absolute `--amount` without policy or session persistence.
        #[arg(long)]
        unsafe_no_policy: bool,
        /// Hex payer secret. Prefer `GHOSTLINE_PAYER_SECRET` or `-` on stdin.
        #[arg(long)]
        secret_hex: Option<String>,
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
    Keygen,
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
        /// Account component that pays revealed XTR fees (`pay_fee`). Required on public networks.
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
}

#[derive(Subcommand)]
enum WalletCommand {
    /// Dry-run or submit an unsigned transaction (`seal-settle --unsigned-json`).
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
        /// Extra `KeyId` JSON. Defaults to the derived account key that pays `pay_fee`.
        #[arg(long)]
        other_signer: Vec<String>,
        #[arg(long)]
        dry_run: bool,
        /// Skip `transactions.wait_result` after a live submit.
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
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
    if let Err(err) = run(Cli::parse()).await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::ChannelId(args) => {
            println!("{}", to_hex(&params(&args)?.channel_id()));
        }
        Command::Sign {
            params: args,
            price,
            amount,
            session,
            vendor_hex,
            budget,
            max_per_call,
            attest,
            unsafe_no_policy,
            secret_hex,
        } => {
            let params = params(&args)?;
            let secret = secrets::load_secret(
                secret_hex.as_deref(),
                secrets::PAYER_SECRET_ENV,
                "payer secret",
            )?;
            let unsafe_ok = unsafe_no_policy || ghostline_policy::env_flag(secrets::UNSAFE_ENV);
            if unsafe_ok {
                let amount = amount.ok_or("unsafe sign requires --amount")?;
                let voucher =
                    SignedVoucher::sign(&secret, &params, amount).map_err(|e| e.to_string())?;
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
                let vendor_hex = vendor_hex
                    .or_else(|| std::env::var(secrets::VENDOR_ENV).ok())
                    .ok_or("set --vendor-hex or GHOSTLINE_POLICY_VENDOR_HEX")?;
                let vendor = seal::parse_pk("vendor", vendor_hex.trim())?;
                let policy = match (budget, max_per_call) {
                    (Some(b), Some(m)) => ghostline_policy::Policy::new(b, m).allow_vendor(vendor),
                    (None, None) => ghostline_policy::Policy::from_env()?.ok_or(
                        "set --budget and --max-per-call, or GHOSTLINE_POLICY_BUDGET and GHOSTLINE_POLICY_MAX_PER_CALL",
                    )?,
                    _ => {
                        return Err("set both --budget and --max-per-call".into());
                    }
                };
                let mut engine = ghostline_policy::PolicyEngine::new(policy);
                if ghostline_policy::env_flag("GHOSTLINE_POLICY_REQUIRE_HUMAN") {
                    engine = engine.require_human();
                }
                let mut store =
                    ghostline_paywall::ChannelStore::open_or_create(path, params.clone())
                        .map_err(|e| e.to_string())?;
                let (voucher, _) = ghostline_paywall::gated_sign(
                    &mut store, engine, vendor, price, &secret, attest,
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
        },
        Command::Wallet { command } => match command {
            WalletCommand::Submit {
                url,
                unsigned_json,
                seal_key_id,
                seal_key_id_file,
                other_signer,
                dry_run,
                no_wait,
                timeout_secs,
            } => {
                let unsigned = read_unsigned_json(&unsigned_json)?;
                let seal_signer = read_key_id(seal_key_id.as_deref(), seal_key_id_file.as_deref())?;
                let other_signers = if other_signer.is_empty() {
                    vec![
                        serde_json::from_str(DEFAULT_ACCOUNT_SIGNER_JSON)
                            .expect("DEFAULT_ACCOUNT_SIGNER_JSON"),
                    ]
                } else {
                    other_signer
                        .iter()
                        .map(|s| {
                            serde_json::from_str(s).map_err(|e| format!("other-signer JSON: {e}"))
                        })
                        .collect::<Result<Vec<_>, _>>()?
                };
                let request = WalletSubmitRequest::new(unsigned, seal_signer, other_signers);
                let mut wallet = WalletClient::connect(&url).map_err(|e| e.to_string())?;
                wallet.auth_none_admin().await.map_err(|e| e.to_string())?;
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
                        let waited = wallet
                            .wait_result(submitted.transaction_id, Some(timeout_secs))
                            .await
                            .map_err(|e| e.to_string())?;
                        let outcome = waited.get("result").unwrap_or(&waited);
                        let status = waited
                            .get("status")
                            .and_then(|s| s.as_str())
                            .or_else(|| outcome.get("status").and_then(|s| s.as_str()));
                        if execution_accepted(outcome)
                            || execution_accepted(&waited)
                            || status == Some("Accepted")
                        {
                            println!("result=Accept");
                        } else {
                            println!("wait={}", waited);
                        }
                    }
                }
            }
        },
        Command::Keygen => {
            use tari_crypto::keys::PublicKey as _;
            use tari_crypto::ristretto::RistrettoPublicKey;
            let (secret, public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
            println!("secret={}", to_hex(secret.as_bytes()));
            println!("public={}", to_hex(public.as_bytes()));
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
                    &channel,
                    deposit,
                    &voucher,
                    channel.spec().service,
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
                let tx = seal::seal_settle(
                    &channel,
                    deposit,
                    &voucher,
                    channel.spec().service,
                    resource,
                    max_epoch,
                    &sealer,
                    scan,
                    fee,
                )?;
                println!("{tx}");
            }
        }
        Command::SealRefund {
            open,
            deposit_mask_hex,
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
            let deposit_mask = secrets::load_hex(
                deposit_mask_hex.as_deref(),
                secrets::DEPOSIT_MASK_ENV,
                "deposit mask",
            )?;
            let resource = seal::parse_resource(resource_hex.as_deref())?;
            let scan = seal::parse_scan(&ootle_network, resource)?;
            let tx = seal::seal_refund(
                &channel,
                funded(channel.params().deposit, &deposit_mask)?,
                channel.spec().payer,
                resource,
                max_epoch,
                &sealer,
                scan,
            )?;
            println!("{tx}");
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
            use tari_template_lib::types::{
                EncryptedData,
                crypto::{PedersenCommitmentBytes, UtxoTag},
                stealth::UnspentOutput,
            };
            let (commitment_hex, nonce_hex, encrypted_hex, tag, expected) =
                load_utxo(utxo_json, commitment_hex, nonce_hex, encrypted_hex, tag)?;
            let secret = secrets::load_claim_secret(secret_hex.as_deref())?;
            let encrypted_bytes = from_hex(&encrypted_hex)
                .map_err(|_| "encrypted: expected even-length hex".to_string())?;
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
            let resource = seal::parse_resource(resource_hex.as_deref())?;
            let recovered = unblind_output(
                &output,
                UtxoTag::new(tag),
                &secret,
                seal::parse_scan(&ootle_network, resource)?,
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
    }
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

fn decode_tx(tx_hex: &str) -> Result<ghostline_net::Transaction, String> {
    let bytes = from_hex(tx_hex).map_err(|_| "tx: expected even-length hex".to_string())?;
    TransactionEnvelope::from_raw(bytes.into_boxed_slice())
        .decode()
        .map_err(|e| format!("tx envelope: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tari_crypto::keys::PublicKey as _;
    use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
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
    fn sign_then_verify() {
        let (secret, params) = sample();
        let voucher = SignedVoucher::sign(&secret, &params, 30).unwrap();
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
        let voucher = SignedVoucher::sign(&payer_secret, channel.params(), 30).unwrap();
        let settle_hex = seal::seal_settle(
            &channel,
            funded(100, &deposit_mask).unwrap(),
            &voucher,
            channel.spec().service,
            STEALTH_TARI_RESOURCE_ADDRESS,
            1,
            &service_secret,
            OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS),
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
    fn sqlite_signed_tag_becomes_u32() {
        let parsed: UtxoJson = serde_json::from_str(
            r#"{"commitment":"aa","nonce":"bb","encrypted_hex":"cc","tag":-1825295126,"value":1000000}"#,
        )
        .unwrap();
        assert_eq!(parsed.tag as u32, 2469672170);
        assert_eq!(parsed.value, Some(1_000_000));
    }
}
