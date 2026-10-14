// SPDX-License-Identifier: MIT

use ghostline_core::{NONCE_LEN, SignedVoucher, from_hex, to_hex};
use ghostline_sdk::{
    Channel, ChannelSpec, MaskAndValue, OutputScan, fund_open, refund, seal_hex, settle,
};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{
    Hash32, ResourceAddress, constants::STEALTH_TARI_RESOURCE_ADDRESS,
    crypto::RistrettoPublicKeyBytes,
};
use tari_utilities::ByteArray;

pub struct OpenSpec {
    pub network_tag: u8,
    pub deposit: u64,
    pub payer: RistrettoPublicKeyBytes,
    pub service: RistrettoPublicKeyBytes,
    pub expiry_epoch: u64,
    pub nonce: [u8; NONCE_LEN],
    pub channel_template: Hash32,
}

pub fn channel(spec: OpenSpec) -> Result<Channel, String> {
    Channel::open(ChannelSpec {
        network_tag: spec.network_tag,
        payer: spec.payer,
        service: spec.service,
        deposit: spec.deposit,
        expiry_epoch: spec.expiry_epoch,
        nonce: spec.nonce,
        channel_template: spec.channel_template,
    })
    .map_err(|e| e.to_string())
}

pub fn print_ids(channel: &Channel) {
    println!(
        "payer_root={}",
        to_hex(channel.params().payer_root.as_slice())
    );
    println!("channel_id={}", to_hex(&channel.params().channel_id()));
}

pub fn parse_pk(label: &str, hex: &str) -> Result<RistrettoPublicKeyBytes, String> {
    RistrettoPublicKeyBytes::from_hex(hex).map_err(|e| format!("{label}: {e}"))
}

pub fn parse_hash(label: &str, hex: &str) -> Result<Hash32, String> {
    Hash32::from_hex(hex).map_err(|e| format!("{label}: {e}"))
}

pub fn parse_nonce(hex: &str) -> Result<[u8; NONCE_LEN], String> {
    let bytes = from_hex(hex).map_err(|_| "nonce: expected even-length hex".to_string())?;
    bytes
        .try_into()
        .map_err(|_| format!("nonce: expected {NONCE_LEN} bytes"))
}

pub fn parse_secret(label: &str, hex: &str) -> Result<RistrettoSecretKey, String> {
    let bytes = from_hex(hex).map_err(|_| format!("{label}: expected even-length hex"))?;
    RistrettoSecretKey::from_canonical_bytes(&bytes).map_err(|e| format!("{label}: {e}"))
}

pub fn parse_resource(hex: Option<&str>) -> Result<ResourceAddress, String> {
    match hex {
        None => Ok(STEALTH_TARI_RESOURCE_ADDRESS),
        Some(h) => ResourceAddress::from_hex(h).map_err(|e| format!("resource: {e}")),
    }
}

pub fn parse_scan(name: &str, resource: ResourceAddress) -> Result<OutputScan, String> {
    match name {
        "localnet" => Ok(OutputScan::localnet(resource)),
        "esmeralda" => Ok(OutputScan::esmeralda(resource)),
        other => Err(format!(
            "ootle-network: expected localnet or esmeralda, got {other}"
        )),
    }
}

pub fn seal_open(
    channel: &Channel,
    input: MaskAndValue,
    change_key: RistrettoPublicKeyBytes,
    resource: ResourceAddress,
    max_epoch: u64,
    sealer: &RistrettoSecretKey,
    scan: OutputScan,
) -> Result<(String, String), String> {
    let opened = fund_open(channel, [input], change_key, scan).map_err(|e| e.to_string())?;
    let tx = seal_hex(scan.network, resource, opened.statement, max_epoch, sealer)
        .map_err(|e| e.to_string())?;
    Ok((tx, to_hex(opened.deposit.mask.as_bytes())))
}

pub fn seal_settle(
    channel: &Channel,
    deposit: MaskAndValue,
    voucher: &SignedVoucher,
    service_output: RistrettoPublicKeyBytes,
    resource: ResourceAddress,
    max_epoch: u64,
    sealer: &RistrettoSecretKey,
    scan: OutputScan,
) -> Result<String, String> {
    let statement = settle(channel, deposit, voucher, service_output, scan)
        .map_err(|e| e.to_string())?;
    seal_hex(scan.network, resource, statement, max_epoch, sealer).map_err(|e| e.to_string())
}

pub fn seal_refund(
    channel: &Channel,
    deposit: MaskAndValue,
    payer_output: RistrettoPublicKeyBytes,
    resource: ResourceAddress,
    max_epoch: u64,
    sealer: &RistrettoSecretKey,
    scan: OutputScan,
) -> Result<String, String> {
    let statement = refund(channel, deposit, payer_output, scan).map_err(|e| e.to_string())?;
    seal_hex(scan.network, resource, statement, max_epoch, sealer).map_err(|e| e.to_string())
}
