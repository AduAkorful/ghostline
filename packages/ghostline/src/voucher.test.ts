// SPDX-License-Identifier: MIT

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { fromHex, toHex } from "./channel.ts";
import { publicKeyFromSecret, signVoucher, verifyVoucher, voucherToBytes } from "./voucher.ts";

const vectors = JSON.parse(
  readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../test-vectors/voucher.json"), "utf8"),
) as {
  payer_secret_hex: string;
  payer_public_hex: string;
  sign_nonce_secret_hex: string;
  network_tag: number;
  deposit: number;
  payer_root_hex: string;
  channel_nonce_hex: string;
  amount: number;
  channel_id_hex: string;
  voucher_hex: string;
};

const params = {
  networkTag: vectors.network_tag,
  payer: fromHex(vectors.payer_public_hex),
  deposit: BigInt(vectors.deposit),
  payerRoot: fromHex(vectors.payer_root_hex),
  nonce: fromHex(vectors.channel_nonce_hex),
};

test("seeded public key matches the Rust vector", () => {
  assert.equal(toHex(publicKeyFromSecret(fromHex(vectors.payer_secret_hex))), vectors.payer_public_hex);
});

test("signVoucher matches the Rust voucher bytes", () => {
  const voucher = signVoucher(
    fromHex(vectors.payer_secret_hex),
    params,
    BigInt(vectors.amount),
    fromHex(vectors.sign_nonce_secret_hex),
  );
  assert.equal(toHex(voucher.channelId), vectors.channel_id_hex);
  assert.equal(toHex(voucherToBytes(voucher)), vectors.voucher_hex);
  verifyVoucher(voucher, params);
});

test("a random-nonce voucher still verifies", () => {
  const voucher = signVoucher(fromHex(vectors.payer_secret_hex), params, 10n);
  verifyVoucher(voucher, params);
});
