// SPDX-License-Identifier: MIT

mod seal;

use clap::{Parser, Subcommand};
use ghostline_core::{CHANNEL_ID_LEN, ChannelParams, NONCE_LEN, SignedVoucher, from_hex, to_hex};
use ghostline_net::{IndexerClient, TransactionEnvelope};
use ghostline_sdk::MaskAndValue;

#[derive(Parser)]
#[command(
    name = "ghostline",
    about = "Sign and verify Ghostline channel vouchers, and submit sealed transactions to a local indexer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the 32-byte channel id for the given open parameters.
    ChannelId(ParamsArgs),
    /// Sign a cumulative voucher. Prints hex-encoded CBOR of (channel_id, amount, signature).
    Sign {
        #[command(flatten)]
        params: ParamsArgs,
        /// Cumulative amount owed, in integer micro-units.
        #[arg(long)]
        amount: u64,
        /// Hex-encoded 32-byte payer secret. Prefer piping; never log this value.
        #[arg(long)]
        secret_hex: String,
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
    /// Print payer_root and channel_id from a full channel spec (computes the tree).
    Spec(OpenArgs),
    /// Seal a localnet open (key-path input → PayTo::Conditions). Prints tx= and deposit_mask=.
    SealOpen {
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long)]
        input_value: u64,
        #[arg(long)]
        input_mask_hex: String,
        #[arg(long)]
        seal_secret_hex: String,
        #[arg(long, default_value_t = 1)]
        max_epoch: u64,
        #[arg(long)]
        resource_hex: Option<String>,
    },
    /// Seal a localnet settle of a voucher against the channel UTXO.
    SealSettle {
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long)]
        voucher_hex: String,
        #[arg(long)]
        deposit_mask_hex: String,
        #[arg(long)]
        seal_secret_hex: String,
        #[arg(long, default_value_t = 1)]
        max_epoch: u64,
        #[arg(long)]
        resource_hex: Option<String>,
    },
    /// Seal a localnet refund of the channel UTXO after expiry.
    SealRefund {
        #[command(flatten)]
        open: OpenArgs,
        #[arg(long)]
        deposit_mask_hex: String,
        #[arg(long)]
        seal_secret_hex: String,
        #[arg(long, default_value_t = 1)]
        max_epoch: u64,
        #[arg(long)]
        resource_hex: Option<String>,
    },
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
            amount,
            secret_hex,
        } => {
            let params = params(&args)?;
            let secret = seal::parse_secret("secret", &secret_hex)?;
            let voucher =
                SignedVoucher::sign(&secret, &params, amount).map_err(|e| e.to_string())?;
            println!("{}", to_hex(&voucher.to_bytes()));
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
        Command::Spec(args) => {
            seal::print_ids(&seal::channel(open_spec(&args)?)?);
        }
        Command::SealOpen {
            open,
            input_value,
            input_mask_hex,
            seal_secret_hex,
            max_epoch,
            resource_hex,
        } => {
            let channel = seal::channel(open_spec(&open)?)?;
            let sealer = seal::parse_secret("seal secret", &seal_secret_hex)?;
            let resource = seal::parse_resource(resource_hex.as_deref())?;
            let (tx, mask) = seal::seal_open(
                &channel,
                funded(input_value, &input_mask_hex)?,
                channel.spec().payer,
                resource,
                max_epoch,
                &sealer,
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
        } => {
            let channel = seal::channel(open_spec(&open)?)?;
            let bytes = from_hex(&voucher_hex)
                .map_err(|_| "voucher: expected even-length hex".to_string())?;
            let voucher = SignedVoucher::from_bytes(&bytes).map_err(|e| e.to_string())?;
            let sealer = seal::parse_secret("seal secret", &seal_secret_hex)?;
            let resource = seal::parse_resource(resource_hex.as_deref())?;
            let tx = seal::seal_settle(
                &channel,
                funded(channel.params().deposit, &deposit_mask_hex)?,
                &voucher,
                channel.spec().service,
                resource,
                max_epoch,
                &sealer,
            )?;
            println!("{tx}");
        }
        Command::SealRefund {
            open,
            deposit_mask_hex,
            seal_secret_hex,
            max_epoch,
            resource_hex,
        } => {
            let channel = seal::channel(open_spec(&open)?)?;
            let sealer = seal::parse_secret("seal secret", &seal_secret_hex)?;
            let resource = seal::parse_resource(resource_hex.as_deref())?;
            let tx = seal::seal_refund(
                &channel,
                funded(channel.params().deposit, &deposit_mask_hex)?,
                channel.spec().payer,
                resource,
                max_epoch,
                &sealer,
            )?;
            println!("{tx}");
        }
    }
    Ok(())
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
        )
        .unwrap();
        decode_tx(&settle_hex).unwrap();
    }
}
