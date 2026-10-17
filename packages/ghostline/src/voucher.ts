// SPDX-License-Identifier: MIT

import { ristretto255 } from "@noble/curves/ed25519.js";
import { bytesToNumberLE, numberToBytesLE } from "@noble/curves/utils.js";
import { blake2b } from "@noble/hashes/blake2.js";
import { type ChannelParams, VOUCHER_DOMAIN, channelId, fromHex, voucherMessage } from "./channel.ts";
import { encodeVoucherCbor } from "./cbor.ts";

const { Point } = ristretto255;
const Fn = Point.Fn;

export type SignedVoucher = {
  channelId: Uint8Array;
  amount: bigint;
  mask: Uint8Array;
  serviceOutputKey: Uint8Array;
  outputCommitment: Uint8Array;
  publicNonce: Uint8Array;
  scalar: Uint8Array;
};

// Tari's Pedersen H generator is the first NUMS point in tari_crypto 0.24.0.
const PEDERSEN_H = fromHex("ce389841c0c8698ab95b70242aeea64840b1eac5f644b7d008ac0587cf471d70");

function concat(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function scalarFromCanonical(bytes: Uint8Array): bigint {
  if (bytes.length !== 32) {
    throw new Error("scalar: expected 32 bytes");
  }
  return Fn.fromBytes(bytes);
}

/** curve25519-dalek `Scalar::from_bytes_mod_order_wide` (64-byte little-endian). */
export function scalarFromWide(bytes: Uint8Array): bigint {
  if (bytes.length !== 64) {
    throw new Error("wide scalar: expected 64 bytes");
  }
  return Fn.create(bytesToNumberLE(bytes));
}

export function publicKeyFromSecret(secret: Uint8Array): Uint8Array {
  return Point.BASE.multiply(scalarFromCanonical(secret)).toBytes();
}

/** Blake2b-512(domain || nonce_public || public_key || message), as in `RistrettoSchnorrBlake2bVerifier`. */
export function computeChallenge(
  domain: Uint8Array,
  message: Uint8Array,
  publicKey: Uint8Array,
  noncePublic: Uint8Array,
): Uint8Array {
  return blake2b(concat([domain, noncePublic, publicKey, message]), { dkLen: 64 });
}

function randomNonceScalar(): bigint {
  const wide = new Uint8Array(64);
  crypto.getRandomValues(wide);
  const scalar = scalarFromWide(wide);
  if (Fn.is0(scalar)) {
    throw new Error("signing nonce was zero");
  }
  return scalar;
}

function randomMaskScalar(): bigint {
  const wide = new Uint8Array(64);
  crypto.getRandomValues(wide);
  const scalar = scalarFromWide(wide);
  if (Fn.is0(scalar)) {
    throw new Error("output mask was zero");
  }
  return scalar;
}

/** Tari's commit_u64_amount(mask, amount) = mask*G + amount*H. */
export function outputCommitment(amount: bigint, mask: Uint8Array): Uint8Array {
  if (amount <= 0n || amount > 0xffff_ffff_ffff_ffffn) {
    throw new Error("amount out of u64 range");
  }
  const maskScalar = scalarFromCanonical(mask);
  const valueScalar = Fn.create(amount);
  return Point.BASE.multiply(maskScalar).add(Point.fromBytes(PEDERSEN_H).multiply(valueScalar)).toBytes();
}

export function signVoucher(
  secret: Uint8Array,
  params: ChannelParams,
  amount: bigint,
  serviceOutputKey: Uint8Array,
  maskSecret?: Uint8Array,
  nonceSecret?: Uint8Array,
): SignedVoucher {
  if (amount === 0n) {
    throw new Error("amount must be greater than zero");
  }
  if (amount > params.deposit) {
    throw new Error("amount exceeds deposit");
  }
  if (serviceOutputKey.length !== 32) {
    throw new Error("service output key must be 32 bytes");
  }
  const mask = maskSecret ?? numberToBytesLE(randomMaskScalar(), 32);
  const commitment = outputCommitment(amount, mask);
  const k = scalarFromCanonical(secret);
  const r = nonceSecret === undefined ? randomNonceScalar() : scalarFromCanonical(nonceSecret);
  const publicKey = Point.BASE.multiply(k).toBytes();
  const publicNonce = Point.BASE.multiply(r).toBytes();
  const id = channelId(params);
  const message = voucherMessage(id, serviceOutputKey, commitment);
  const e = scalarFromWide(computeChallenge(VOUCHER_DOMAIN, message, publicKey, publicNonce));
  const s = Fn.add(Fn.mul(e, k), r);
  return {
    channelId: id,
    amount,
    mask,
    serviceOutputKey,
    outputCommitment: commitment,
    publicNonce,
    scalar: numberToBytesLE(s, 32),
  };
}

export function verifyVoucher(voucher: SignedVoucher, params: ChannelParams): void {
  if (voucher.amount === 0n) {
    throw new Error("amount must be greater than zero");
  }
  if (voucher.amount > params.deposit) {
    throw new Error("amount exceeds deposit");
  }
  const id = channelId(params);
  if (toHexCmp(voucher.channelId, id) !== 0) {
    throw new Error("channel id mismatch");
  }
  if (toHexCmp(outputCommitment(voucher.amount, voucher.mask), voucher.outputCommitment) !== 0) {
    throw new Error("voucher amount and mask do not open its commitment");
  }
  const publicKey = params.payer;
  const R = Point.fromBytes(voucher.publicNonce);
  if (R.is0()) {
    throw new Error("invalid signature");
  }
  const message = voucherMessage(id, voucher.serviceOutputKey, voucher.outputCommitment);
  const e = scalarFromWide(computeChallenge(VOUCHER_DOMAIN, message, publicKey, voucher.publicNonce));
  const sG = Point.BASE.multiply(scalarFromCanonical(voucher.scalar));
  const eP = Point.fromBytes(publicKey).multiply(e);
  if (!sG.equals(R.add(eP))) {
    throw new Error("invalid signature");
  }
}

export function voucherToBytes(voucher: SignedVoucher): Uint8Array {
  return encodeVoucherCbor(
    voucher.channelId,
    voucher.amount,
    voucher.mask,
    voucher.serviceOutputKey,
    voucher.outputCommitment,
    voucher.publicNonce,
    voucher.scalar,
  );
}

function toHexCmp(a: Uint8Array, b: Uint8Array): number {
  if (a.length !== b.length) {
    return 1;
  }
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a[i]! ^ b[i]!;
  }
  return diff;
}

