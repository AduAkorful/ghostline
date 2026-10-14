// SPDX-License-Identifier: MIT

/** Minimal CBOR enough for Ghostline vouchers (`tari_bor` / minicbor encodings). */

function concat(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export function encodeUint(value: number): Uint8Array {
  if (!Number.isInteger(value) || value < 0) {
    throw new Error("cbor uint: expected a non-negative integer");
  }
  if (value < 24) {
    return Uint8Array.of(value);
  }
  if (value < 256) {
    return Uint8Array.of(0x18, value);
  }
  if (value < 65536) {
    return Uint8Array.of(0x19, value >> 8, value & 0xff);
  }
  throw new Error("cbor uint: value too large");
}

export function encodeBytes(bytes: Uint8Array): Uint8Array {
  if (bytes.length < 24) {
    return concat([Uint8Array.of(0x40 + bytes.length), bytes]);
  }
  if (bytes.length < 256) {
    return concat([Uint8Array.of(0x58, bytes.length), bytes]);
  }
  throw new Error("cbor bytes: too long");
}

export function encodeArray(items: Uint8Array[]): Uint8Array {
  const header =
    items.length < 24
      ? Uint8Array.of(0x80 + items.length)
      : items.length < 256
        ? Uint8Array.of(0x98, items.length)
        : (() => {
            throw new Error("cbor array: too long");
          })();
  return concat([header, ...items]);
}

/** `[u8; 32]` as a CBOR array of unsigned integers (minicbor default for `[u8; N]`). */
export function encodeByteArrayAsUints(bytes: Uint8Array): Uint8Array {
  return encodeArray(Array.from(bytes, (b) => encodeUint(b)));
}

export function encodeVoucherCbor(
  channelId: Uint8Array,
  amount: bigint,
  publicNonce: Uint8Array,
  scalar: Uint8Array,
): Uint8Array {
  if (amount < 0n || amount > 0xffff_ffff_ffff_ffffn) {
    throw new Error("amount out of u64 range");
  }
  if (amount > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error("cbor uint: amount exceeds JS safe integer encoding");
  }
  const schnorr = encodeArray([encodeBytes(publicNonce), encodeBytes(scalar)]);
  const payload = encodeArray([encodeUint(0), encodeArray([schnorr])]);
  const signature = encodeArray([payload]);
  return encodeArray([encodeByteArrayAsUints(channelId), encodeUint(Number(amount)), signature]);
}
