// SPDX-License-Identifier: MIT

use std::str::FromStr;

use ghostline_core::{NONCE_LEN, SignedVoucher, from_hex, to_hex};
use ghostline_sdk::{
    AccountFee, Channel, ChannelSpec, MaskAndValue, OutputScan, UnsignedTransaction, fund_open,
    refund, seal_hex, seal_hex_with_fee, settle, unsigned_stealth_transfer_with_fee,
};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_template_lib::types::{
    ComponentAddress, Hash32, ResourceAddress, UtxoAddress, UtxoId,
    constants::STEALTH_TARI_RESOURCE_ADDRESS, crypto::PedersenCommitmentBytes,
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

pub struct SettleArgs<'a> {
    pub channel: &'a Channel,
    pub deposit: MaskAndValue,
    pub voucher: &'a SignedVoucher,
    pub service_output: RistrettoPublicKeyBytes,
    pub resource: ResourceAddress,
    pub max_epoch: u64,
    pub scan: OutputScan,
}

pub struct RefundArgs<'a> {
    pub channel: &'a Channel,
    pub deposit: MaskAndValue,
    pub payer_output: RistrettoPublicKeyBytes,
    pub resource: ResourceAddress,
    pub max_epoch: u64,
    pub scan: OutputScan,
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

pub fn parse_component(label: &str, addr: &str) -> Result<ComponentAddress, String> {
    ComponentAddress::from_str(addr).map_err(|e| format!("{label}: {e}"))
}

/// Indexer substate id for a stealth output: `utxo_{resource_key}_{commitment}`.
pub fn utxo_substate_id(resource: ResourceAddress, commitment_hex: &str) -> Result<String, String> {
    let commitment = PedersenCommitmentBytes::from_hex(commitment_hex)
        .map_err(|e| format!("commitment: {e}"))?;
    Ok(UtxoAddress::new(resource, UtxoId::from(commitment)).to_string())
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
    args: SettleArgs<'_>,
    sealer: &RistrettoSecretKey,
    fee: Option<AccountFee<'_>>,
) -> Result<String, String> {
    let statement = settle(
        args.channel,
        args.deposit,
        args.voucher,
        args.service_output,
        args.scan,
    )
    .map_err(|e| e.to_string())?;
    seal_hex_with_fee(
        args.scan.network,
        args.resource,
        statement,
        args.max_epoch,
        sealer,
        fee,
    )
    .map_err(|e| e.to_string())
}

pub fn unsigned_settle(
    args: SettleArgs<'_>,
    fee_account: Option<(ComponentAddress, u64)>,
) -> Result<UnsignedTransaction, String> {
    let statement = settle(
        args.channel,
        args.deposit,
        args.voucher,
        args.service_output,
        args.scan,
    )
    .map_err(|e| e.to_string())?;
    Ok(unsigned_stealth_transfer_with_fee(
        args.scan.network,
        args.resource,
        statement,
        args.max_epoch,
        fee_account,
    ))
}

pub fn unsigned_refund(
    channel: &Channel,
    deposit: MaskAndValue,
    payer_output: RistrettoPublicKeyBytes,
    resource: ResourceAddress,
    max_epoch: u64,
    scan: OutputScan,
    fee_account: Option<(ComponentAddress, u64)>,
) -> Result<UnsignedTransaction, String> {
    let statement = refund(channel, deposit, payer_output, scan).map_err(|e| e.to_string())?;
    Ok(unsigned_stealth_transfer_with_fee(
        scan.network,
        resource,
        statement,
        max_epoch,
        fee_account,
    ))
}

pub fn seal_refund(
    args: RefundArgs<'_>,
    sealer: &RistrettoSecretKey,
    fee: Option<AccountFee<'_>>,
) -> Result<String, String> {
    let statement = refund(args.channel, args.deposit, args.payer_output, args.scan)
        .map_err(|e| e.to_string())?;
    seal_hex_with_fee(
        args.scan.network,
        args.resource,
        statement,
        args.max_epoch,
        sealer,
        fee,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utxo_id_is_resource_key_and_commitment() {
        let commitment = "11".repeat(32);
        let id = utxo_substate_id(STEALTH_TARI_RESOURCE_ADDRESS, &commitment).unwrap();
        let resource_key = STEALTH_TARI_RESOURCE_ADDRESS.as_object_key().to_string();
        assert_eq!(id, format!("utxo_{resource_key}_{commitment}"));
    }
}
