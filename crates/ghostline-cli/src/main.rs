// SPDX-License-Identifier: MIT

use clap::{Parser, Subcommand};
use ghostline_core::{CHANNEL_ID_LEN, ChannelParams, NONCE_LEN, SignedVoucher, from_hex, to_hex};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{Hash32, crypto::RistrettoPublicKeyBytes};
use tari_utilities::ByteArray;

#[derive(Parser)]
#[command(
    name = "ghostline",
    about = "Sign and verify Ghostline channel vouchers"
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
        payer: parse_pk(&args.payer_hex)?,
        deposit: args.deposit,
        payer_root: parse_hash(&args.payer_root_hex)?,
        nonce: parse_nonce(&args.nonce_hex)?,
    })
}

fn parse_pk(hex: &str) -> Result<RistrettoPublicKeyBytes, String> {
    RistrettoPublicKeyBytes::from_hex(hex).map_err(|e| format!("payer public key: {e}"))
}

fn parse_hash(hex: &str) -> Result<Hash32, String> {
    Hash32::from_hex(hex).map_err(|e| format!("payer root: {e}"))
}

fn parse_nonce(hex: &str) -> Result<[u8; NONCE_LEN], String> {
    let bytes = from_hex(hex).map_err(|_| "nonce: expected even-length hex".to_string())?;
    bytes
        .try_into()
        .map_err(|_| format!("nonce: expected {NONCE_LEN} bytes"))
}

fn parse_secret(hex: &str) -> Result<RistrettoSecretKey, String> {
    let bytes = from_hex(hex).map_err(|_| "secret: expected even-length hex".to_string())?;
    RistrettoSecretKey::from_canonical_bytes(&bytes).map_err(|e| format!("secret: {e}"))
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
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
            let secret = parse_secret(&secret_hex)?;
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tari_crypto::keys::PublicKey as _;
    use tari_crypto::ristretto::RistrettoPublicKey;
    use tari_template_lib::types::crypto::RistrettoPublicKeyBytes;
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
