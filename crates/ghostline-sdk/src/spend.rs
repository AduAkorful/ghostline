// SPDX-License-Identifier: MIT

#[cfg(feature = "trusted-local-batch-prototype")]
use ghostline_core::{BATCH_BALANCE_DOMAIN, sign_with_nonce};
use ghostline_core::{FEE_DIVISOR, SignedVoucher, VoucherError, settlement_fee};
use ootle_byte_type::ToByteType;
use ootle_network::Network;
use tari_crypto::{
    keys::{PublicKey as _, SecretKey as _},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_ootle_wallet_crypto::{
    MaskAndValue, OutputWitness, StealthCryptoApi, StealthInputWitness, StealthOutputWitness,
    WalletCryptoError,
    balance_proof::generate_stealth_balance_proof_signature,
    pay_to::PayTo,
    stealth::{self, create_transfer_statement},
};
#[cfg(feature = "trusted-local-batch-prototype")]
use tari_template_lib::types::Hash32;
use tari_template_lib::types::{
    Amount, ResourceAddress,
    crypto::RistrettoPublicKeyBytes,
    stealth::{
        SpendAuthorization, SpendWitness, StealthInput, StealthInputsStatement,
        StealthTransferStatement,
    },
};
use tari_utilities::ByteArray;

use crate::{Channel, SdkError, WalletDestination};

/// Ootle scan parameters for UTXO tags. Distinct from Ghostline's voucher `network_tag`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputScan {
    pub network: Network,
    pub resource: ResourceAddress,
}

impl OutputScan {
    /// Tags for `Transaction::builder_localnet` and the given resource.
    pub fn localnet(resource: ResourceAddress) -> Self {
        Self {
            network: Network::LocalNet,
            resource,
        }
    }

    /// Tags for Esmeralda (`Network::Esmeralda` / `0x26`) and the given resource.
    pub fn esmeralda(resource: ResourceAddress) -> Self {
        Self {
            network: Network::Esmeralda,
            resource,
        }
    }

    /// Tags for mainnet (`Network::MainNet` / `0x00`) and the given resource.
    pub fn mainnet(resource: ResourceAddress) -> Self {
        Self {
            network: Network::MainNet,
            resource,
        }
    }

    /// The voucher `network_tag` is the same byte the sealed transaction uses.
    pub fn require_channel_tag(self, network_tag: u8) -> Result<Self, String> {
        let expected = self.network.as_byte();
        if network_tag == expected {
            return Ok(self);
        }
        Err(format!(
            "network_tag {network_tag} does not match ootle network {} (byte {expected})",
            self.network.as_key_str()
        ))
    }
}

/// A funded channel UTXO plus the transfer that created it.
#[derive(Clone)]
pub struct OpenStatement {
    pub statement: StealthTransferStatement,
    pub deposit: MaskAndValue,
}

impl std::fmt::Debug for OpenStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenStatement")
            .field("deposit_value", &self.deposit.value)
            .finish_non_exhaustive()
    }
}

/// Spends key-path stealth inputs into the channel's two-leaf condition tree (`PayTo::Conditions`).
///
/// Input value above the deposit is returned as a key-path change output to `payer_change_key`.
/// Output minimum-value promises are fixed at one micro-unit so the opening amount is not published.
pub fn fund_open(
    channel: &Channel,
    inputs: impl IntoIterator<Item = MaskAndValue>,
    payer_change_key: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<OpenStatement, SdkError> {
    fund_open_with_leaves(
        channel,
        channel.leaves_vec(),
        inputs,
        payer_change_key,
        scan,
    )
}

/// Opens a batch-enabled channel with pooled settlement, individual settlement fallback, and expiry refund paths.
pub fn fund_open_batched(
    channel: &Channel,
    inputs: impl IntoIterator<Item = MaskAndValue>,
    payer_change_key: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<OpenStatement, SdkError> {
    fund_open_with_leaves(
        channel,
        channel.batched_leaves_vec(),
        inputs,
        payer_change_key,
        scan,
    )
}

fn fund_open_with_leaves(
    channel: &Channel,
    leaves: Vec<tari_template_lib::types::stealth::SpendCondition>,
    inputs: impl IntoIterator<Item = MaskAndValue>,
    payer_change_key: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<OpenStatement, SdkError> {
    let inputs: Vec<MaskAndValue> = inputs.into_iter().collect();
    let mut have: u64 = 0;
    for input in &inputs {
        if input.value == 0 {
            return Err(SdkError::ZeroInput);
        }
        have = have.checked_add(input.value).ok_or(SdkError::Overflow)?;
    }
    let need = channel.params().deposit;
    if have < need {
        return Err(SdkError::InsufficientFunds { have, need });
    }
    let change = have - need;

    let auth = stealth::pay_to_output_authorization(&PayTo::Conditions(leaves), || {
        unreachable!("PayTo::Conditions does not derive a one-time spend key")
    })?;

    let deposit_out = output(need, auth, &channel.spec().payer, scan)?;
    let deposit = MaskAndValue {
        value: need,
        mask: deposit_out.witness.mask.clone(),
    };

    let mut outputs = vec![deposit_out];
    if change > 0 {
        outputs.push(output(
            change,
            SpendAuthorization::Key(payer_change_key),
            &payer_change_key,
            scan,
        )?);
    }

    let statement = create_transfer_statement(
        inputs.into_iter().map(StealthInputWitness::new),
        Amount::zero(),
        outputs.iter(),
        None,
    )?;
    Ok(OpenStatement { statement, deposit })
}

/// Spends the channel UTXO with a voucher: service receives `amount`, the treasury receives the protocol fee
/// `amount / FEE_DIVISOR`, and the remainder returns under `payer_root`.
///
/// `treasury` must be the key currently held by the fee config component the channel template pins; any other key
/// fails the template check. Amount and mask stay in the off-chain voucher. Public outputs use a one-unit
/// minimum-value promise. One micro-unit is reserved for payer change so the settlement always has the same public
/// output count.
pub fn settle(
    channel: &Channel,
    deposit: MaskAndValue,
    voucher: &SignedVoucher,
    service_output_key: RistrettoPublicKeyBytes,
    treasury: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    settle_with_leaves(
        channel,
        channel.leaves_vec(),
        channel.settle_leaf(),
        deposit,
        voucher,
        service_output_key,
        treasury,
        None,
        scan,
    )
}

/// Settles with the service payout addressed to the service's wallet, so walletd finds and can spend it.
///
/// `payout` must be the destination whose one-time key the service quoted: the voucher binds
/// `service_output_key`, and the template pays only to that key.
pub fn settle_to_wallet(
    channel: &Channel,
    deposit: MaskAndValue,
    voucher: &SignedVoucher,
    payout: &WalletDestination,
    treasury: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    settle_with_leaves(
        channel,
        channel.leaves_vec(),
        channel.settle_leaf(),
        deposit,
        voucher,
        voucher.service_output_key,
        treasury,
        Some(payout),
        scan,
    )
}

/// Individually settles a batch-enabled channel when its depositor cannot participate in a pooled batch.
///
/// This fallback gives the service the ordinary per-channel payout path. It preserves channel liveness, but the
/// settlement no longer gets the pooled batch's within-batch payout unlinking.
pub fn settle_batched_individually(
    channel: &Channel,
    deposit: MaskAndValue,
    voucher: &SignedVoucher,
    service_output_key: RistrettoPublicKeyBytes,
    treasury: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    settle_with_leaves(
        channel,
        channel.batched_leaves_vec(),
        channel.batched_individual_settle_leaf(),
        deposit,
        voucher,
        service_output_key,
        treasury,
        None,
        scan,
    )
}

#[allow(clippy::too_many_arguments)]
fn settle_with_leaves(
    channel: &Channel,
    leaves: Vec<tari_template_lib::types::stealth::SpendCondition>,
    settle_leaf: &tari_template_lib::types::stealth::SpendCondition,
    deposit: MaskAndValue,
    voucher: &SignedVoucher,
    service_output_key: RistrettoPublicKeyBytes,
    treasury: RistrettoPublicKeyBytes,
    payout: Option<&WalletDestination>,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    voucher.verify(channel.params())?;
    if voucher.service_output_key != service_output_key {
        return Err(SdkError::InvalidServiceOutputKey);
    }
    if let Some(payout) = payout
        && payout.owner_key() != service_output_key
    {
        return Err(SdkError::InvalidServiceOutputKey);
    }
    if deposit.value != channel.params().deposit {
        return Err(SdkError::DepositMismatch {
            expected: channel.params().deposit,
            actual: deposit.value,
        });
    }

    let fee = settlement_fee(voucher.amount)?;
    let fee_output = FeeOutput {
        amount: fee,
        mask: fee_mask(&voucher.mask),
        auth: Some(SpendAuthorization::Key(treasury)),
        recipient: treasury,
    };
    settle_statement(
        channel,
        &leaves,
        settle_leaf,
        deposit,
        voucher,
        service_output_key,
        fee_output,
        payout,
        scan,
    )
}

struct FeeOutput {
    amount: u64,
    mask: RistrettoSecretKey,
    /// `None` omits the fee output.
    auth: Option<SpendAuthorization>,
    recipient: RistrettoPublicKeyBytes,
}

/// A deliberately malformed fee output for template rejection tests.
#[cfg(feature = "adversarial-test-builders")]
pub struct FeeOverride {
    pub amount: u64,
    /// `None` uses the honest mask derived from the voucher.
    pub mask: Option<RistrettoSecretKey>,
    /// `None` omits the fee output.
    pub auth: Option<SpendAuthorization>,
    pub recipient: RistrettoPublicKeyBytes,
}

/// Builds a settlement with a caller-chosen fee output and no fee validation. Test use only.
#[cfg(feature = "adversarial-test-builders")]
pub fn settle_with_fee_override(
    channel: &Channel,
    deposit: MaskAndValue,
    voucher: &SignedVoucher,
    service_output_key: RistrettoPublicKeyBytes,
    fee: FeeOverride,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    voucher.verify(channel.params())?;
    let fee_output = FeeOutput {
        amount: fee.amount,
        mask: fee.mask.unwrap_or_else(|| fee_mask(&voucher.mask)),
        auth: fee.auth,
        recipient: fee.recipient,
    };
    settle_statement(
        channel,
        &channel.leaves_vec(),
        channel.settle_leaf(),
        deposit,
        voucher,
        service_output_key,
        fee_output,
        None,
        scan,
    )
}

#[allow(clippy::too_many_arguments)]
fn settle_statement(
    channel: &Channel,
    leaves: &[tari_template_lib::types::stealth::SpendCondition],
    settle_leaf: &tari_template_lib::types::stealth::SpendCondition,
    deposit: MaskAndValue,
    voucher: &SignedVoucher,
    service_output_key: RistrettoPublicKeyBytes,
    fee: FeeOutput,
    payout: Option<&WalletDestination>,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    let fee_paid = if fee.auth.is_some() { fee.amount } else { 0 };
    let remainder = payer_remainder(channel.params().deposit, voucher.amount, fee_paid)?;

    let (witness, _) =
        stealth::script_path_witness_with_data(leaves, settle_leaf, voucher.witness_data())?;

    let mut outputs = vec![output(
        remainder,
        SpendAuthorization::Script(channel.params().payer_root),
        &channel.spec().payer,
        scan,
    )?];
    outputs.push(match payout {
        Some(payout) => output_to_wallet(voucher.amount, voucher.mask.clone(), payout, scan)?,
        None => output_with_mask(
            voucher.amount,
            voucher.mask.clone(),
            SpendAuthorization::Key(service_output_key),
            &service_output_key,
            scan,
        )?,
    });
    if let Some(auth) = fee.auth {
        outputs.push(output_with_mask(
            fee.amount,
            fee.mask,
            auth,
            &fee.recipient,
            scan,
        )?);
    }

    channel_close_statement(deposit, witness, &outputs)
}

/// Closes 2–12 batch-enabled channels to one fresh pooled service output. Each payer's voucher is still signed for
/// its channel and the shared one-time batch payout key. The on-chain predicate checks the shared manifest, pooled
/// commitment sum, and a Schnorr proof that each channel's input, voucher and change amounts balance.
///
/// # Privacy warning
///
/// This feasibility prototype receives every channel's deposit value and mask in one process. That process learns all
/// participant deposit openings. It is not coordinator-private and must not be used by a hosted multi-user service.
/// The API is hidden unless the `trusted-local-batch-prototype` feature is explicitly enabled.
#[cfg(feature = "trusted-local-batch-prototype")]
pub fn settle_batch(
    channels: &[(&Channel, MaskAndValue, &SignedVoucher)],
    service_output_key: RistrettoPublicKeyBytes,
    treasury: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    if !(2..=12).contains(&channels.len()) {
        return Err(SdkError::InvalidBatchSize);
    }

    let mut sum_value = 0u64;
    let mut sum_mask = RistrettoSecretKey::default();
    let mut input_mask = RistrettoSecretKey::default();
    let mut outputs = Vec::with_capacity(channels.len() + 2);

    for (channel, deposit, voucher) in channels {
        voucher.verify(channel.params())?;
        if voucher.service_output_key != service_output_key {
            return Err(SdkError::InvalidServiceOutputKey);
        }
        if deposit.value != channel.params().deposit {
            return Err(SdkError::DepositMismatch {
                expected: channel.params().deposit,
                actual: deposit.value,
            });
        }
        let fee = settlement_fee(voucher.amount)?;
        let remainder = payer_remainder(channel.params().deposit, voucher.amount, fee)?;
        sum_value = sum_value
            .checked_add(voucher.amount)
            .ok_or(SdkError::Overflow)?;
        sum_mask = sum_mask + &voucher.mask;
        input_mask = input_mask + &deposit.mask;

        outputs.push(output(
            remainder,
            SpendAuthorization::Script(channel.params().payer_root),
            &channel.spec().payer,
            scan,
        )?);
    }

    for i in 0..channels.len() {
        for j in 0..i {
            let (channel_i, deposit_i, voucher_i) = &channels[i];
            let (channel_j, deposit_j, voucher_j) = &channels[j];
            if voucher_i.channel_id == voucher_j.channel_id
                || deposit_i.to_commitment() == deposit_j.to_commitment()
                || channel_i.params().payer_root == channel_j.params().payer_root
            {
                return Err(SdkError::DuplicateBatchEntry);
            }
        }
    }

    let pooled = output_with_mask(
        sum_value,
        sum_mask.clone(),
        SpendAuthorization::Key(service_output_key),
        &service_output_key,
        scan,
    )?;
    outputs.insert(0, pooled);
    // Each voucher is a multiple of the divisor, so the pooled fee is the exact sum of per-channel fees.
    let pooled_fee = output_with_mask(
        sum_value / FEE_DIVISOR,
        fee_mask(&sum_mask),
        SpendAuthorization::Key(treasury),
        &treasury,
        scan,
    )?;

    let mut entries = Vec::with_capacity(channels.len());
    let mut aggregate_excess = RistrettoSecretKey::default();
    for (index, (channel, deposit, voucher)) in channels.iter().enumerate() {
        let change = &outputs[index + 1];
        let change_commitment =
            MaskAndValue::new(change.witness.amount, change.witness.mask.clone())
                .to_commitment()
                .to_byte_type();
        let input_commitment = deposit.to_commitment().to_byte_type();
        let channel_id = Hash32::from_array(voucher.channel_id);
        let mask_excess =
            &deposit.mask - &voucher.mask - &fee_mask(&voucher.mask) - &change.witness.mask;
        if mask_excess == RistrettoSecretKey::default() {
            return Err(SdkError::DegenerateBatchProof);
        }
        aggregate_excess = aggregate_excess + &mask_excess;
        let proof_message = batch_balance_message(
            &channel_id,
            &input_commitment,
            &channel.params().payer_root,
            &service_output_key,
            &voucher.output_commitment,
            &change_commitment,
        );
        let (proof_nonce, _) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let balance_proof = sign_with_nonce(
            &mask_excess,
            proof_nonce,
            channel.params().network_tag,
            BATCH_BALANCE_DOMAIN,
            &proof_message,
        );
        entries.push((
            channel_id,
            input_commitment,
            channel.spec().payer,
            channel.params().payer_root,
            voucher.output_commitment,
            tari_template_lib::types::crypto::Signature::<
                tari_template_lib::types::crypto::NoSignatureDomain,
            >::new(voucher.signature.payload().clone()),
            balance_proof,
        ));
    }
    if aggregate_excess == RistrettoSecretKey::default() {
        return Err(SdkError::DegenerateBatchProof);
    }
    outputs.push(pooled_fee);

    let mut input_witnesses = Vec::with_capacity(channels.len());
    for (index, (channel, deposit, _voucher)) in channels.iter().enumerate() {
        let data = tari_bor::encode(&(index as u32, service_output_key, &entries))
            .map_err(|_| SdkError::Encode)?;
        let (witness, _) = stealth::script_path_witness_with_data(
            &channel.batched_leaves_vec(),
            channel.batch_settle_leaf(),
            tari_template_lib::types::bytes::Bytes::from_vec(data),
        )?;
        input_witnesses.push(StealthInput {
            commitment: deposit.to_commitment().to_byte_type(),
            witness: witness.clone(),
        });
    }

    let outputs_statement =
        stealth::create_outputs_statement(outputs.iter(), None).map_err(WalletCryptoError::from)?;
    let inputs_statement = StealthInputsStatement {
        inputs: input_witnesses,
        revealed_amount: Amount::zero(),
    };
    let output_mask = outputs
        .iter()
        .fold(RistrettoSecretKey::default(), |sum, output| {
            sum + &output.witness.mask
        });
    let balance_proof = generate_stealth_balance_proof_signature(
        &input_mask,
        &output_mask,
        &inputs_statement,
        &outputs_statement,
        &[],
    );
    Ok(StealthTransferStatement {
        inputs_statement,
        outputs_statement,
        balance_proof: Some(balance_proof),
        covenant_claims: Vec::new(),
    })
}

#[cfg(feature = "trusted-local-batch-prototype")]
fn batch_balance_message(
    channel_id: &Hash32,
    input: &tari_template_lib::types::crypto::PedersenCommitmentBytes,
    payer_root: &Hash32,
    payout_key: &RistrettoPublicKeyBytes,
    voucher: &tari_template_lib::types::crypto::PedersenCommitmentBytes,
    change: &tari_template_lib::types::crypto::PedersenCommitmentBytes,
) -> [u8; 192] {
    let mut message = [0u8; 192];
    message[..32].copy_from_slice(channel_id.as_slice());
    message[32..64].copy_from_slice(input.as_bytes());
    message[64..96].copy_from_slice(payer_root.as_slice());
    message[96..128].copy_from_slice(payout_key.as_bytes());
    message[128..160].copy_from_slice(voucher.as_bytes());
    message[160..].copy_from_slice(change.as_bytes());
    message
}

/// Spends the channel UTXO on the refund leaf after expiry, paying the full deposit to `payer_output_key`.
pub fn refund(
    channel: &Channel,
    deposit: MaskAndValue,
    payer_output_key: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    refund_with_leaves(
        channel,
        channel.leaves_vec(),
        deposit,
        RefundTo::Key(payer_output_key),
        scan,
    )
}

/// Refunds after expiry to the payer's wallet address, so walletd finds and can spend the returned deposit.
pub fn refund_to_wallet(
    channel: &Channel,
    deposit: MaskAndValue,
    destination: &WalletDestination,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    refund_with_leaves(
        channel,
        channel.leaves_vec(),
        deposit,
        RefundTo::Wallet(destination),
        scan,
    )
}

enum RefundTo<'a> {
    Key(RistrettoPublicKeyBytes),
    Wallet(&'a WalletDestination),
}

/// Refunds a batch-enabled channel after expiry using its matching committed condition tree.
pub fn refund_batched(
    channel: &Channel,
    deposit: MaskAndValue,
    payer_output_key: RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    refund_with_leaves(
        channel,
        channel.batched_leaves_vec(),
        deposit,
        RefundTo::Key(payer_output_key),
        scan,
    )
}

fn refund_with_leaves(
    channel: &Channel,
    leaves: Vec<tari_template_lib::types::stealth::SpendCondition>,
    deposit: MaskAndValue,
    refund_to: RefundTo<'_>,
    scan: OutputScan,
) -> Result<StealthTransferStatement, SdkError> {
    if deposit.value != channel.params().deposit {
        return Err(SdkError::DepositMismatch {
            expected: channel.params().deposit,
            actual: deposit.value,
        });
    }

    let (witness, _) =
        stealth::script_path_witness_with_data(&leaves, channel.refund_leaf(), Default::default())?;
    let outputs = [match refund_to {
        RefundTo::Key(key) => output(
            channel.params().deposit,
            SpendAuthorization::Key(key),
            &key,
            scan,
        )?,
        RefundTo::Wallet(destination) => output_to_wallet(
            channel.params().deposit,
            RistrettoSecretKey::random(&mut rand::rng()),
            destination,
            scan,
        )?,
    }];

    channel_close_statement(deposit, witness, &outputs)
}

/// Ghostline's close leaves do not query a covenant partition's cleartext outflow. Settlement checks the signed
/// output commitment and restricts the entire transfer shape; refund requires the payer and the expiry timelock.
/// The global balance proof still enforces conservation. The generic wallet builder would additionally publish an
/// unused covenant claim revealing the deposit, so assemble this statement with the upstream output and global
/// proof builders and an empty claim list. The script witness is retained and verified against the actual UTXO.
fn channel_close_statement(
    deposit: MaskAndValue,
    witness: SpendWitness,
    outputs: &[StealthOutputWitness],
) -> Result<StealthTransferStatement, SdkError> {
    let outputs_statement =
        stealth::create_outputs_statement(outputs.iter(), None).map_err(WalletCryptoError::from)?;
    let inputs_statement = StealthInputsStatement {
        inputs: vec![StealthInput {
            commitment: deposit.to_commitment().to_byte_type(),
            witness,
        }],
        revealed_amount: Amount::zero(),
    };
    let output_mask = outputs
        .iter()
        .fold(RistrettoSecretKey::default(), |sum, output| {
            sum + &output.witness.mask
        });
    let covenant_claims = Vec::new();
    let balance_proof = generate_stealth_balance_proof_signature(
        &deposit.mask,
        &output_mask,
        &inputs_statement,
        &outputs_statement,
        &covenant_claims,
    );
    Ok(StealthTransferStatement {
        inputs_statement,
        outputs_statement,
        balance_proof: Some(balance_proof),
        covenant_claims,
    })
}

/// Payer change after the service payout and protocol fee. At least one micro-unit must remain so every settlement
/// has the same output count.
fn payer_remainder(deposit: u64, payout: u64, fee: u64) -> Result<u64, SdkError> {
    let spent = payout.checked_add(fee).ok_or(SdkError::Overflow)?;
    match deposit.checked_sub(spent) {
        Some(remainder) if remainder > 0 => Ok(remainder),
        _ => Err(VoucherError::ExceedsDeposit {
            amount: spent,
            deposit: deposit.saturating_sub(ghostline_core::SETTLEMENT_RESERVE),
        }
        .into()),
    }
}

/// The template checks `FEE_DIVISOR * C_fee == C_service`, which needs the fee mask to be the service mask divided by
/// the divisor in the scalar field.
fn fee_mask(service_mask: &RistrettoSecretKey) -> RistrettoSecretKey {
    let inverse = RistrettoSecretKey::from(FEE_DIVISOR)
        .invert()
        .expect("fee divisor is a non-zero scalar");
    service_mask * &inverse
}

fn output(
    amount: u64,
    auth: SpendAuthorization,
    recipient: &RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<StealthOutputWitness, SdkError> {
    let mask = RistrettoSecretKey::random(&mut rand::rng());
    output_with_mask(amount, mask, auth, recipient, scan)
}

fn output_with_mask(
    amount: u64,
    mask: RistrettoSecretKey,
    auth: SpendAuthorization,
    recipient: &RistrettoPublicKeyBytes,
    scan: OutputScan,
) -> Result<StealthOutputWitness, SdkError> {
    let (nonce_secret, nonce_public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
    let recipient_pk = RistrettoPublicKey::from_canonical_bytes(recipient.as_bytes())
        .map_err(|_| SdkError::InvalidKey)?;
    let encrypted_data = StealthCryptoApi::new()
        .encrypt_value_and_mask(amount, &mask, &recipient_pk, &nonce_secret, None)
        .map_err(|err| SdkError::Encrypt(err.to_string()))?;
    let tag = StealthCryptoApi::new().derive_stealth_output_tag(
        scan.network,
        &nonce_secret,
        &recipient_pk,
        &scan.resource,
    );

    Ok(StealthOutputWitness {
        witness: OutputWitness {
            amount,
            mask,
            sender_public_nonce: nonce_public,
            minimum_value_promise: 1,
            encrypted_data,
            resource_view_key: None,
        },
        auth,
        tag,
    })
}

/// An output walletd recognises as its own: data and tag keyed to the address's view key, locked to the one-time
/// spend key `H(r·V)·G + A`. Mirrors upstream `build_stealth_output_witness` for `StealthPublicKey`.
fn output_to_wallet(
    amount: u64,
    mask: RistrettoSecretKey,
    destination: &WalletDestination,
    scan: OutputScan,
) -> Result<StealthOutputWitness, SdkError> {
    if destination.network() != scan.network {
        return Err(SdkError::Address(format!(
            "destination is for {} but the output is on {}",
            destination.network(),
            scan.network
        )));
    }
    let view = destination.view_public_key();
    let encrypted_data = StealthCryptoApi::new()
        .encrypt_value_and_mask(amount, &mask, view, destination.nonce(), None)
        .map_err(|err| SdkError::Encrypt(err.to_string()))?;
    let tag = StealthCryptoApi::new().derive_stealth_output_tag(
        scan.network,
        destination.nonce(),
        view,
        &scan.resource,
    );
    Ok(StealthOutputWitness {
        witness: OutputWitness {
            amount,
            mask,
            sender_public_nonce: destination.public_nonce(),
            minimum_value_promise: 1,
            encrypted_data,
            resource_view_key: None,
        },
        auth: SpendAuthorization::Key(destination.owner_key()),
        tag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, ChannelSpec};
    use ghostline_core::{NONCE_LEN, SignedVoucher};
    use ootle_byte_type::ToByteType;
    use tari_ootle_common_types::crypto::create_key_pair_from_seed;
    use tari_template_lib::types::constants::STEALTH_TARI_RESOURCE_ADDRESS;
    use tari_template_lib::types::{ComponentAddress, Hash32};

    fn scan() -> OutputScan {
        OutputScan::localnet(STEALTH_TARI_RESOURCE_ADDRESS)
    }

    fn treasury() -> RistrettoPublicKeyBytes {
        create_key_pair_from_seed(202).1.to_byte_type()
    }

    fn channel_and_keys() -> (RistrettoSecretKey, Channel, RistrettoPublicKeyBytes) {
        let (payer_secret, payer_public) = create_key_pair_from_seed(11);
        let (_, service_public) = create_key_pair_from_seed(13);
        let payer: RistrettoPublicKeyBytes = payer_public.to_byte_type();
        let service: RistrettoPublicKeyBytes = service_public.to_byte_type();
        let channel = Channel::open(ChannelSpec {
            network_tag: 1,
            payer,
            service,
            deposit: 1_000,
            expiry_epoch: u64::MAX,
            nonce: [42u8; NONCE_LEN],
            channel_template: Hash32::from_array([7u8; 32]),
            fee_config: ComponentAddress::from_array([8u8; 32]),
        })
        .unwrap();
        (payer_secret, channel, service)
    }

    fn funded(value: u64) -> MaskAndValue {
        MaskAndValue {
            value,
            mask: RistrettoSecretKey::random(&mut rand::rng()),
        }
    }

    #[test]
    fn settle_outputs_use_non_disclosing_promises() {
        let (secret, channel, service) = channel_and_keys();
        let voucher = SignedVoucher::sign(&secret, channel.params(), 300, service).unwrap();
        let statement = settle(
            &channel,
            funded(1_000),
            &voucher,
            service,
            treasury(),
            scan(),
        )
        .unwrap();
        let promises: Vec<u64> = statement
            .outputs_statement
            .outputs
            .iter()
            .map(|o| o.output.minimum_value_promise)
            .collect();
        assert_eq!(promises, vec![1, 1, 1]);
    }

    #[test]
    fn settle_fee_output_opens_to_the_quotient_and_balances() {
        let (secret, channel, service) = channel_and_keys();
        let voucher = SignedVoucher::sign(&secret, channel.params(), 300, service).unwrap();
        let statement = settle(
            &channel,
            funded(1_000),
            &voucher,
            service,
            treasury(),
            scan(),
        )
        .unwrap();
        let fee = &statement.outputs_statement.outputs[2];
        assert_eq!(fee.auth, SpendAuthorization::Key(treasury()));
        let expected = MaskAndValue::new(3, fee_mask(&voucher.mask))
            .to_commitment()
            .to_byte_type();
        assert_eq!(fee.output.commitment, expected);
        tari_engine_types::stealth::validate_transfer(&statement, None).unwrap();
    }

    /// The receive side as walletd runs it (upstream `ootle_sdk_core::stealth::scan_stealth_output`): tag and AEAD
    /// keyed by the view secret, then the one-time spend key re-derived from the view secret and account key.
    /// Returns the opened funds and the spend secret walletd would derive.
    fn wallet_scan(
        output: &tari_template_lib::types::stealth::StealthUnspentOutput,
        account: &(RistrettoSecretKey, RistrettoPublicKey),
        view: &(RistrettoSecretKey, RistrettoPublicKey),
    ) -> Option<(MaskAndValue, RistrettoSecretKey)> {
        let funds = crate::unblind_output(&output.output, output.tag, &view.0, scan()).ok()?;
        let nonce =
            RistrettoPublicKey::from_canonical_bytes(output.output.sender_public_nonce.as_bytes())
                .unwrap();
        let api = StealthCryptoApi::new();
        let expected = api.derive_stealth_owner_public_key_from_view_key(
            scan().network,
            &account.1,
            &view.0,
            &nonce,
        );
        if output.auth != SpendAuthorization::Key(expected.to_byte_type()) {
            return None;
        }
        let spend_secret =
            api.derive_stealth_owner_secret(scan().network, &account.0, &view.0, &nonce);
        Some((funds, spend_secret))
    }

    fn service_wallet() -> (
        (RistrettoSecretKey, RistrettoPublicKey),
        (RistrettoSecretKey, RistrettoPublicKey),
        WalletDestination,
    ) {
        let account = create_key_pair_from_seed(21);
        let view = create_key_pair_from_seed(22);
        let destination = WalletDestination::new(
            scan().network,
            account.1.to_byte_type(),
            view.1.to_byte_type(),
            RistrettoSecretKey::random(&mut rand::rng()),
        )
        .unwrap();
        (account, view, destination)
    }

    #[test]
    fn wallet_payout_is_found_and_spendable_by_the_service_wallet() {
        let (secret, channel, _) = channel_and_keys();
        let (account, view, payout) = service_wallet();
        let voucher =
            SignedVoucher::sign(&secret, channel.params(), 300, payout.owner_key()).unwrap();
        let statement = settle_to_wallet(
            &channel,
            funded(1_000),
            &voucher,
            &payout,
            treasury(),
            scan(),
        )
        .unwrap();
        tari_engine_types::stealth::validate_transfer(&statement, None).unwrap();

        let service_output = &statement.outputs_statement.outputs[1];
        assert_eq!(
            service_output.auth,
            SpendAuthorization::Key(payout.owner_key())
        );
        let (funds, spend_secret) =
            wallet_scan(service_output, &account, &view).expect("walletd finds the payout");
        assert_eq!(funds.value, 300);
        assert_eq!(funds.mask, voucher.mask);
        assert_eq!(
            RistrettoPublicKey::from_secret_key(&spend_secret).to_byte_type(),
            payout.owner_key()
        );

        // The template still sees the voucher commitment, the fee quotient and the treasury key.
        let fee = &statement.outputs_statement.outputs[2];
        assert_eq!(fee.auth, SpendAuthorization::Key(treasury()));
        assert_eq!(
            fee.output.commitment,
            MaskAndValue::new(3, fee_mask(&voucher.mask))
                .to_commitment()
                .to_byte_type()
        );
    }

    #[test]
    fn key_addressed_payout_is_invisible_to_the_wallet_scan() {
        // The pre-fix shape that stranded mainnet payouts: encrypted to the bare spend key.
        let (secret, channel, _) = channel_and_keys();
        let (account, view, _) = service_wallet();
        let bare: RistrettoPublicKeyBytes = account.1.to_byte_type();
        let voucher = SignedVoucher::sign(&secret, channel.params(), 300, bare).unwrap();
        let statement =
            settle(&channel, funded(1_000), &voucher, bare, treasury(), scan()).unwrap();
        assert!(wallet_scan(&statement.outputs_statement.outputs[1], &account, &view).is_none());
    }

    #[test]
    fn wallet_payout_must_be_the_destination_the_voucher_names() {
        let (secret, channel, _) = channel_and_keys();
        let (_, _, quoted) = service_wallet();
        let (_, _, other) = service_wallet();
        let voucher =
            SignedVoucher::sign(&secret, channel.params(), 300, quoted.owner_key()).unwrap();
        let err = settle_to_wallet(
            &channel,
            funded(1_000),
            &voucher,
            &other,
            treasury(),
            scan(),
        )
        .unwrap_err();
        assert!(matches!(err, SdkError::InvalidServiceOutputKey), "{err}");
    }

    #[test]
    fn wallet_payout_refuses_a_destination_on_another_network() {
        let (secret, channel, _) = channel_and_keys();
        let account = create_key_pair_from_seed(21);
        let view = create_key_pair_from_seed(22);
        let mainnet = WalletDestination::new(
            Network::MainNet,
            account.1.to_byte_type(),
            view.1.to_byte_type(),
            RistrettoSecretKey::random(&mut rand::rng()),
        )
        .unwrap();
        let voucher =
            SignedVoucher::sign(&secret, channel.params(), 300, mainnet.owner_key()).unwrap();
        let err = settle_to_wallet(
            &channel,
            funded(1_000),
            &voucher,
            &mainnet,
            treasury(),
            scan(),
        )
        .unwrap_err();
        assert!(matches!(err, SdkError::Address(_)), "{err}");
    }

    #[test]
    fn wallet_refund_is_found_and_spendable_by_the_payer_wallet() {
        let (_, channel, _) = channel_and_keys();
        let (account, view, destination) = service_wallet();
        let statement = refund_to_wallet(&channel, funded(1_000), &destination, scan()).unwrap();
        tari_engine_types::stealth::validate_transfer(&statement, None).unwrap();
        let (funds, spend_secret) =
            wallet_scan(&statement.outputs_statement.outputs[0], &account, &view)
                .expect("walletd finds the refund");
        assert_eq!(funds.value, 1_000);
        assert_eq!(
            SpendAuthorization::Key(
                RistrettoPublicKey::from_secret_key(&spend_secret).to_byte_type()
            ),
            statement.outputs_statement.outputs[0].auth
        );
    }

    #[cfg(feature = "trusted-local-batch-prototype")]
    #[test]
    fn maximum_batch_fits_witness_and_output_limits() {
        let (_, service_public) = create_key_pair_from_seed(80);
        let (_, payout_public) = create_key_pair_from_seed(81);
        let mut channels = Vec::new();
        let mut deposits = Vec::new();
        let mut vouchers = Vec::new();

        for index in 0..12u8 {
            let (payer, payer_public) = create_key_pair_from_seed(100 + index);
            let channel = Channel::open(ChannelSpec {
                network_tag: 1,
                payer: payer_public.to_byte_type(),
                service: service_public.to_byte_type(),
                deposit: 1_000,
                expiry_epoch: u64::MAX,
                nonce: [index; NONCE_LEN],
                channel_template: Hash32::from_array([7u8; 32]),
                fee_config: ComponentAddress::from_array([8u8; 32]),
            })
            .unwrap();
            let deposit = funded(1_000);
            let voucher =
                SignedVoucher::sign(&payer, channel.params(), 300, payout_public.to_byte_type())
                    .unwrap();
            channels.push(channel);
            deposits.push(deposit);
            vouchers.push(voucher);
        }

        let participants: Vec<_> = channels
            .iter()
            .zip(deposits)
            .zip(vouchers.iter())
            .map(|((channel, deposit), voucher)| (channel, deposit, voucher))
            .collect();
        let statement = settle_batch(
            &participants,
            payout_public.to_byte_type(),
            treasury(),
            scan(),
        )
        .unwrap();
        assert_eq!(statement.inputs_statement.inputs.len(), 12);
        assert_eq!(statement.outputs_statement.outputs.len(), 14);
        for input in &statement.inputs_statement.inputs {
            let (_, _, data) = input.witness.as_script_path().unwrap();
            assert!(data.len() <= 4096);
        }
    }

    #[test]
    fn maximum_spend_keeps_reserved_payer_change_output() {
        let (secret, channel, service) = channel_and_keys();
        // 900 + 9 fee leaves 91; the next aligned payout, 1_000, exceeds the deposit.
        let voucher = SignedVoucher::sign(&secret, channel.params(), 900, service).unwrap();
        let statement = settle(
            &channel,
            funded(1_000),
            &voucher,
            service,
            treasury(),
            scan(),
        )
        .unwrap();
        assert_eq!(statement.outputs_statement.outputs.len(), 3);
    }

    #[test]
    fn settle_refuses_a_payout_whose_fee_consumes_the_reserve() {
        let (secret, channel, service) = channel_and_keys();
        let channel = Channel::open(ChannelSpec {
            deposit: 101,
            ..channel.spec().clone()
        })
        .unwrap();
        let voucher = SignedVoucher::sign(&secret, channel.params(), 100, service).unwrap();
        let err = settle(&channel, funded(101), &voucher, service, treasury(), scan()).unwrap_err();
        assert!(matches!(
            err,
            SdkError::Voucher(VoucherError::ExceedsDeposit {
                amount: 101,
                deposit: 100
            })
        ));
    }

    #[test]
    fn voucher_cannot_spend_the_reserved_micro_unit() {
        let (secret, channel, service) = channel_and_keys();
        let err = SignedVoucher::sign(&secret, channel.params(), 1_000, service).unwrap_err();
        assert!(matches!(
            err,
            ghostline_core::VoucherError::ExceedsDeposit {
                amount: 1_000,
                deposit: 999
            }
        ));
    }

    #[test]
    fn zero_value_voucher_cannot_be_signed_for_settlement() {
        let (secret, channel, service) = channel_and_keys();
        let err = SignedVoucher::sign(&secret, channel.params(), 0, service).unwrap_err();
        assert!(matches!(err, ghostline_core::VoucherError::ZeroAmount));
    }

    #[test]
    fn refund_returns_the_deposit() {
        let (_secret, channel, _service) = channel_and_keys();
        let statement = refund(&channel, funded(1_000), channel.spec().payer, scan()).unwrap();
        assert_eq!(statement.outputs_statement.outputs.len(), 1);
        assert_eq!(
            statement.outputs_statement.outputs[0]
                .output
                .minimum_value_promise,
            1
        );
    }

    #[test]
    fn settle_rejects_wrong_utxo_value() {
        let (secret, channel, service) = channel_and_keys();
        let voucher = SignedVoucher::sign(&secret, channel.params(), 300, service).unwrap();
        let err = settle(&channel, funded(999), &voucher, service, treasury(), scan()).unwrap_err();
        assert!(matches!(
            err,
            SdkError::DepositMismatch {
                expected: 1_000,
                actual: 999
            }
        ));
    }

    #[test]
    fn settle_rejects_invalid_voucher_before_building_transaction() {
        let (_secret, channel, service) = channel_and_keys();
        let (wrong_secret, _) = create_key_pair_from_seed(12);
        let voucher = SignedVoucher::sign(&wrong_secret, channel.params(), 300, service).unwrap();
        assert!(matches!(
            settle(
                &channel,
                funded(1_000),
                &voucher,
                service,
                treasury(),
                scan()
            ),
            Err(SdkError::Voucher(
                ghostline_core::VoucherError::InvalidSignature
            ))
        ));
    }

    #[test]
    fn open_commits_the_channel_tree() {
        let (_secret, channel, _service) = channel_and_keys();
        let opened = fund_open(&channel, [funded(1_000)], channel.spec().payer, scan()).unwrap();
        assert_eq!(opened.statement.outputs_statement.outputs.len(), 1);
        assert_eq!(
            opened.statement.outputs_statement.outputs[0]
                .output
                .minimum_value_promise,
            1
        );
        let expected = stealth::pay_to_output_authorization(
            &PayTo::Conditions(channel.leaves_vec()),
            || unreachable!(),
        )
        .unwrap();
        assert_eq!(opened.statement.outputs_statement.outputs[0].auth, expected);
        assert_eq!(opened.deposit.value, 1_000);
    }

    #[test]
    fn open_outputs_use_non_disclosing_promises() {
        let (_secret, channel, _service) = channel_and_keys();
        let opened = fund_open(&channel, [funded(1_030)], channel.spec().payer, scan()).unwrap();
        let promises: Vec<u64> = opened
            .statement
            .outputs_statement
            .outputs
            .iter()
            .map(|o| o.output.minimum_value_promise)
            .collect();
        assert_eq!(promises, vec![1, 1]);
    }

    #[test]
    fn open_rejects_short_inputs() {
        let (_secret, channel, _service) = channel_and_keys();
        let err = fund_open(
            &channel,
            [funded(400), funded(500)],
            channel.spec().payer,
            scan(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            SdkError::InsufficientFunds {
                have: 900,
                need: 1_000
            }
        ));
    }

    #[test]
    fn open_statement_validates() {
        let (_secret, channel, _service) = channel_and_keys();
        let opened = fund_open(&channel, [funded(1_000)], channel.spec().payer, scan()).unwrap();
        tari_engine_types::stealth::validate_transfer(&opened.statement, None).unwrap();
    }

    #[test]
    fn payer_wallet_recomputes_the_deposit_tag() {
        let (secret, channel, _service) = channel_and_keys();
        let opened = fund_open(&channel, [funded(1_000)], channel.spec().payer, scan()).unwrap();
        let out = &opened.statement.outputs_statement.outputs[0];
        let nonce =
            RistrettoPublicKey::from_canonical_bytes(out.output.sender_public_nonce.as_bytes())
                .unwrap();
        let expected = StealthCryptoApi::new().derive_stealth_output_tag(
            Network::LocalNet,
            &secret,
            &nonce,
            &STEALTH_TARI_RESOURCE_ADDRESS,
        );
        assert_eq!(out.tag, expected);
    }
}
