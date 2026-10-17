// SPDX-License-Identifier: MIT

import { blake2b } from "@noble/hashes/blake2.js";

export const CHANNEL_ID_DOMAIN = new TextEncoder().encode("ghostline.channel.id.v1");
export const VOUCHER_DOMAIN = new TextEncoder().encode("ghostline.channel.voucher.v1");
export const CHANNEL_ID_LEN = 32;
export const NONCE_LEN = 32;

export type ChannelParams = {
  networkTag: number;
  payer: Uint8Array;
  deposit: bigint;
  payerRoot: Uint8Array;
  nonce: Uint8Array;
};

export function fromHex(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) {
    throw new Error("odd-length hex");
  }
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    const byte = hex.slice(i * 2, i * 2 + 2);
    if (!/^[0-9a-fA-F]{2}$/.test(byte)) {
      throw new Error("invalid hex");
    }
    out[i] = Number.parseInt(byte, 16);
  }
  return out;
}

export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function u64Le(value: bigint): Uint8Array {
  if (value < 0n || value > 0xffff_ffff_ffff_ffffn) {
    throw new Error("amount out of u64 range");
  }
  const out = new Uint8Array(8);
  let n = value;
  for (let i = 0; i < 8; i++) {
    out[i] = Number(n & 0xffn);
    n >>= 8n;
  }
  return out;
}

export function channelId(params: ChannelParams): Uint8Array {
  if (params.payer.length !== 32 || params.payerRoot.length !== 32 || params.nonce.length !== NONCE_LEN) {
    throw new Error("payer, payerRoot and nonce must be 32 bytes");
  }
  if (!Number.isInteger(params.networkTag) || params.networkTag < 0 || params.networkTag > 255) {
    throw new Error("networkTag must fit in a u8");
  }
  const parts = [
    CHANNEL_ID_DOMAIN,
    Uint8Array.of(params.networkTag),
    params.payer,
    u64Le(params.deposit),
    params.payerRoot,
    params.nonce,
  ];
  const joined = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let offset = 0;
  for (const part of parts) {
    joined.set(part, offset);
    offset += part.length;
  }
  return blake2b(joined, { dkLen: 32 });
}

export function voucherMessage(
  channelIdBytes: Uint8Array,
  serviceOutputKey: Uint8Array,
  outputCommitment: Uint8Array,
): Uint8Array {
  if (channelIdBytes.length !== CHANNEL_ID_LEN) {
    throw new Error("channel id must be 32 bytes");
  }
  if (serviceOutputKey.length !== 32 || outputCommitment.length !== 32) {
    throw new Error("service output key and commitment must be 32 bytes");
  }
  const out = new Uint8Array(CHANNEL_ID_LEN + 64);
  out.set(channelIdBytes, 0);
  out.set(serviceOutputKey, CHANNEL_ID_LEN);
  out.set(outputCommitment, CHANNEL_ID_LEN + 32);
  return out;
}
