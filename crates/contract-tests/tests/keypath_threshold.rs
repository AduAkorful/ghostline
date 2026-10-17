// SPDX-License-Identifier: MIT

//! Feasibility-only proof for the proposed deposit-confidential channel key path.
//!
//! This does not implement a Ghostline channel or approve the custom Tari FROST ciphersuite for production. It checks
//! that a threshold Schnorr seal can authorize a real Ootle key-path stealth spend with hidden output commitments and
//! no covenant balance claims.

use std::collections::{BTreeMap, HashMap};

use blake2::Blake2b;
use digest::consts::U64;
use frost_core::{
    Ciphersuite, Field, Group, Scalar,
    frost::{self, round1, round2},
};
use frost_ristretto255::Ristretto255Sha512;
use ghostline_core::{NONCE_LEN, SignedVoucher};
use ghostline_sdk::{Channel, ChannelSpec};
use ootle_byte_type::ToByteType;
use rand08::thread_rng;
use tari_crypto::keys::PublicKey as _;
use tari_crypto::{
    hashing::DomainSeparatedHasher,
    keys::SecretKey as _,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    signatures::SchnorrSigChallenge,
};
use tari_ootle_transaction::{Epoch, Transaction, TransactionSealSignature};
use tari_ootle_wallet_crypto::{
    MaskAndValue, OutputWitness, StealthCryptoApi, StealthInputWitness, StealthOutputWitness,
    stealth::create_transfer_statement,
};
use tari_template_lib::types::{
    Amount, ResourceAddress,
    crypto::RistrettoPublicKeyBytes,
    stealth::{SpendAuthorization, StealthTransferStatement},
};
use tari_template_test_tooling::{TemplateTest, support::spec::OutputAuthSpec};
use tari_utilities::ByteArray;

mod support;
use support::{CHANNEL, DEPOSIT, fund};

const NETWORK_TAG: u8 = 1;
const CHANNEL_NONCE: [u8; NONCE_LEN] = [91; NONCE_LEN];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct TariFrost;

impl Ciphersuite for TariFrost {
    const ID: &'static str = "GHOSTLINE-TARI-RISTRETTO-BLAKE2B-TEST-ONLY";
    type Group = <Ristretto255Sha512 as Ciphersuite>::Group;
    type HashOutput = <Ristretto255Sha512 as Ciphersuite>::HashOutput;
    type SignatureSerialization = <Ristretto255Sha512 as Ciphersuite>::SignatureSerialization;

    fn H1(m: &[u8]) -> Scalar<Self> {
        Ristretto255Sha512::H1(m)
    }

    fn H2(m: &[u8]) -> Scalar<Self> {
        assert!(
            m.len() >= 64,
            "Schnorr challenge preimage must include R and P"
        );
        let digest =
            DomainSeparatedHasher::<Blake2b<U64>, SchnorrSigChallenge>::new_with_label("challenge")
                .chain(&m[..32])
                .chain(&m[32..64])
                .chain(&m[64..])
                .finalize();
        let tari_scalar = RistrettoSecretKey::from_uniform_bytes(digest.as_ref())
            .expect("a 64-byte digest is a uniform scalar input");
        let encoded: [u8; 32] = tari_scalar
            .as_bytes()
            .try_into()
            .expect("Ristretto scalar encoding is 32 bytes");
        <<Self::Group as Group>::Field>::deserialize(&encoded)
            .expect("Tari and FROST use the same canonical scalar encoding")
    }

    fn H3(m: &[u8]) -> Scalar<Self> {
        Ristretto255Sha512::H3(m)
    }

    fn H4(m: &[u8]) -> Self::HashOutput {
        Ristretto255Sha512::H4(m)
    }

    fn H5(m: &[u8]) -> Self::HashOutput {
        Ristretto255Sha512::H5(m)
    }

    fn HDKG(m: &[u8]) -> Option<Scalar<Self>> {
        Ristretto255Sha512::HDKG(m)
    }
}

#[test]
fn threshold_seal_spends_keypath_output_without_a_balance_claim() {
    let mut test = TemplateTest::new(
        env!("CARGO_MANIFEST_DIR"),
        ["templates/faucet", "../../contracts/channel"],
    );
    let (threshold_public, key_packages, public_key_package) = dkg_two_of_two();
    let threshold_public_bytes: RistrettoPublicKeyBytes = threshold_public.to_byte_type();

    // Model opening by moving a payer-owned UTXO into a fresh two-party key-path escrow output.
    let (payer_key, payer_public) = create_key();
    let (service_key, service_public) = create_key();
    let (resource, payer_input_mask) = fund(&mut test, OutputAuthSpec::KeyPath(payer_public));
    let channel_template = test.get_template_address(CHANNEL);
    let channel = Channel::open(ChannelSpec {
        network_tag: NETWORK_TAG,
        payer: payer_public,
        service: service_public,
        deposit: DEPOSIT,
        expiry_epoch: 10,
        nonce: CHANNEL_NONCE,
        channel_template,
    })
    .unwrap();
    let open = hidden_spend(
        resource,
        payer_input_mask,
        DEPOSIT,
        vec![(DEPOSIT, threshold_public_bytes)],
        vec![None],
    );
    assert!(open.statement.covenant_claims.is_empty());
    assert_eq!(open.statement.outputs_statement.outputs.len(), 1);
    assert_eq!(
        open.statement.outputs_statement.outputs[0]
            .output
            .minimum_value_promise,
        1
    );
    let open_tx = Transaction::builder_localnet(Epoch(2))
        .stealth_transfer(resource, open.statement)
        .finish()
        .seal(&payer_key);
    test.execute_expect_success(open_tx, vec![]);
    let deposit_mask = open.output_masks[0].clone();

    let payer_change_public = create_key().1;
    assert_ne!(service_public, payer_change_public);
    let voucher = SignedVoucher::sign(&payer_key, channel.params(), 30, service_public).unwrap();
    voucher.verify(channel.params()).unwrap();
    let settlement = hidden_spend(
        resource,
        deposit_mask.clone(),
        DEPOSIT,
        vec![(30, service_public), (70, payer_change_public)],
        vec![Some(voucher.mask.clone()), None],
    );
    assert!(settlement.statement.covenant_claims.is_empty());
    assert_eq!(settlement.statement.outputs_statement.outputs.len(), 2);
    assert_eq!(
        settlement.statement.outputs_statement.outputs[0]
            .output
            .commitment,
        voucher.output_commitment,
        "the service output must open the commitment authorized by the voucher"
    );
    assert!(
        settlement
            .statement
            .outputs_statement
            .outputs
            .iter()
            .all(|output| output.output.minimum_value_promise == 1)
    );

    // Neither the payer nor the service can bypass the 2-of-2 authorization and take the whole channel output.
    for solo_key in [&payer_key, &service_key] {
        let unauthorized = Transaction::builder_localnet(Epoch(2))
            .stealth_transfer(resource, settlement.statement.clone())
            .finish()
            .seal(solo_key);
        let rejection = test.execute_expect_failure(unauthorized, vec![]);
        let rejection_text = format!("{rejection:?}");
        assert!(
            rejection_text.contains("required to spend the input"),
            "unexpected rejection: {rejection_text}"
        );
    }

    // The payer would receive this fully signed refund during channel opening. Ootle's transaction validator (outside
    // TemplateTest's instruction executor) must reject it before this minimum epoch.
    let refund = hidden_spend(
        resource,
        deposit_mask.clone(),
        DEPOSIT,
        vec![(DEPOSIT, payer_public)],
        vec![None],
    );
    assert!(refund.statement.covenant_claims.is_empty());
    let refund_unsealed = Transaction::builder_localnet(Epoch(2))
        .with_min_epoch(Some(Epoch(10)))
        .with_max_epoch(Epoch(100))
        .stealth_transfer(resource, refund.statement)
        .finish();
    let refund_message = TransactionSealSignature::create_message(&refund_unsealed);
    let refund_tx = refund_unsealed.seal_with_signature(sign_two_of_two(
        &refund_message,
        &key_packages,
        &public_key_package,
    ));
    assert_eq!(refund_tx.min_epoch(), Some(Epoch(10)));
    assert_eq!(refund_tx.max_epoch(), Epoch(100));

    let unsealed = Transaction::builder_localnet(Epoch(2))
        .stealth_transfer(resource, settlement.statement)
        .finish();
    let message = TransactionSealSignature::create_message(&unsealed);
    let signature = sign_two_of_two(&message, &key_packages, &public_key_package);
    let transaction = unsealed.seal_with_signature(signature);
    assert!(transaction.verify_all_signatures());
    let replay = transaction.clone();
    test.execute_expect_success(transaction, vec![]);
    test.execute_expect_failure(replay, vec![]);
}

#[test]
fn one_party_cannot_aggregate_a_two_of_two_transaction_signature() {
    let (_, key_packages, public_key_package) = dkg_two_of_two();
    let mut rng = thread_rng();
    let (id, key_package) = key_packages.iter().next().unwrap();
    let (nonces, commitment) = round1::commit(key_package.secret_share(), &mut rng);
    let message = b"refund and settlement candidates are transaction-specific";
    let signing_package =
        frost::SigningPackage::<TariFrost>::new(BTreeMap::from([(*id, commitment)]), message);
    let share = round2::sign(&signing_package, &nonces, key_package).unwrap();
    let partial_shares = HashMap::from([(*id, share)]);

    assert!(frost::aggregate(&signing_package, &partial_shares, &public_key_package).is_err());
}

fn dkg_two_of_two() -> (
    RistrettoPublicKey,
    BTreeMap<frost::Identifier<TariFrost>, frost::keys::KeyPackage<TariFrost>>,
    frost::keys::PublicKeyPackage<TariFrost>,
) {
    let mut rng = thread_rng();
    let ids = [1u16, 2].map(|n| frost::Identifier::<TariFrost>::try_from(n).unwrap());
    let mut secrets = HashMap::new();
    let mut round1_packages = HashMap::new();
    for id in ids {
        let (secret, package) =
            frost::keys::dkg::part1::<TariFrost, _>(id, 2, 2, &mut rng).unwrap();
        secrets.insert(id, secret);
        round1_packages.insert(id, package);
    }

    let mut round2_secrets = HashMap::new();
    let mut round2_packages: HashMap<_, HashMap<_, _>> = HashMap::new();
    for id in ids {
        let received = round1_packages
            .iter()
            .filter(|(other, _)| **other != id)
            .map(|(other, package)| (*other, package.clone()))
            .collect();
        let (secret, outgoing) =
            frost::keys::dkg::part2(secrets.remove(&id).unwrap(), &received).unwrap();
        round2_secrets.insert(id, secret);
        for (recipient, package) in outgoing {
            round2_packages
                .entry(recipient)
                .or_default()
                .insert(id, package);
        }
    }

    let mut key_packages = BTreeMap::new();
    let mut public_package = None;
    for id in ids {
        let received = round1_packages
            .iter()
            .filter(|(other, _)| **other != id)
            .map(|(other, package)| (*other, package.clone()))
            .collect();
        let (key_package, current_public) =
            frost::keys::dkg::part3(&round2_secrets[&id], &received, &round2_packages[&id])
                .unwrap();
        if let Some(previous) = &public_package {
            assert_eq!(previous, &current_public);
        } else {
            public_package = Some(current_public);
        }
        key_packages.insert(id, key_package);
    }
    let public_package = public_package.unwrap();
    let public_key = RistrettoPublicKey::from_canonical_bytes(
        &key_packages
            .values()
            .next()
            .unwrap()
            .group_public()
            .serialize(),
    )
    .unwrap();
    (public_key, key_packages, public_package)
}

fn sign_two_of_two(
    message: &[u8],
    key_packages: &BTreeMap<frost::Identifier<TariFrost>, frost::keys::KeyPackage<TariFrost>>,
    public_package: &frost::keys::PublicKeyPackage<TariFrost>,
) -> TransactionSealSignature {
    let mut rng = thread_rng();
    let mut nonces = BTreeMap::new();
    let mut commitments = BTreeMap::new();
    for (id, package) in key_packages {
        let (nonce, commitment) = round1::commit(package.secret_share(), &mut rng);
        nonces.insert(*id, nonce);
        commitments.insert(*id, commitment);
    }
    let signing_package = frost::SigningPackage::<TariFrost>::new(commitments, message);
    let shares = nonces
        .iter()
        .map(|(id, nonce)| {
            (
                *id,
                round2::sign(&signing_package, nonce, &key_packages[id]).unwrap(),
            )
        })
        .collect();
    let aggregate = frost::aggregate(&signing_package, &shares, public_package).unwrap();
    let bytes = aggregate.serialize();
    let nonce = RistrettoPublicKey::from_canonical_bytes(&bytes[..32]).unwrap();
    let response = RistrettoSecretKey::from_canonical_bytes(&bytes[32..]).unwrap();
    let signature = RistrettoSchnorr::new(nonce, response);
    let public_key = RistrettoPublicKey::from_canonical_bytes(
        &key_packages
            .values()
            .next()
            .unwrap()
            .group_public()
            .serialize(),
    )
    .unwrap();
    assert!(signature.verify(&public_key, message));
    TransactionSealSignature::new(public_key.to_byte_type(), signature.to_byte_type())
}

fn create_key() -> (RistrettoSecretKey, RistrettoPublicKeyBytes) {
    let secret = RistrettoSecretKey::random(&mut rand::rng());
    let public = RistrettoPublicKey::from_secret_key(&secret).to_byte_type();
    (secret, public)
}

struct HiddenSpend {
    statement: StealthTransferStatement,
    output_masks: Vec<RistrettoSecretKey>,
}

fn hidden_spend(
    resource: ResourceAddress,
    input_mask: RistrettoSecretKey,
    input_value: u64,
    recipient_outputs: Vec<(u64, RistrettoPublicKeyBytes)>,
    provided_masks: Vec<Option<RistrettoSecretKey>>,
) -> HiddenSpend {
    assert_eq!(recipient_outputs.len(), provided_masks.len());
    let output_total = recipient_outputs
        .iter()
        .try_fold(0u64, |sum, (value, _)| sum.checked_add(*value));
    assert_eq!(output_total, Some(input_value));
    let mut outputs = Vec::new();
    let mut output_masks = Vec::new();
    for ((value, recipient_bytes), provided_mask) in
        recipient_outputs.into_iter().zip(provided_masks)
    {
        let recipient =
            RistrettoPublicKey::from_canonical_bytes(recipient_bytes.as_bytes()).unwrap();
        let mask = provided_mask.unwrap_or_else(|| RistrettoSecretKey::random(&mut rand::rng()));
        let (nonce_secret, nonce_public) = RistrettoPublicKey::random_keypair(&mut rand::rng());
        let encrypted_data = StealthCryptoApi::new()
            .encrypt_value_and_mask(value, &mask, &recipient, &nonce_secret, None)
            .unwrap();
        let tag = StealthCryptoApi::new().derive_stealth_output_tag(
            ootle_network::Network::LocalNet,
            &nonce_secret,
            &recipient,
            &resource,
        );
        output_masks.push(mask.clone());
        outputs.push(StealthOutputWitness {
            witness: OutputWitness {
                amount: value,
                mask,
                sender_public_nonce: nonce_public,
                minimum_value_promise: 1,
                encrypted_data,
                resource_view_key: None,
            },
            auth: SpendAuthorization::Key(recipient_bytes),
            tag,
        });
    }
    let statement = create_transfer_statement(
        [StealthInputWitness::new(MaskAndValue {
            mask: input_mask,
            value: input_value,
        })],
        Amount::zero(),
        outputs.iter(),
        None,
    )
    .unwrap();
    HiddenSpend {
        statement,
        output_masks,
    }
}
