// SPDX-License-Identifier: MIT

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { channelId, fromHex, toHex, voucherMessage } from "./channel.ts";

const vectors = JSON.parse(
  readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../test-vectors/vectors.json"), "utf8"),
) as {
  network_tag: number;
  payer_hex: string;
  deposit: number;
  payer_root_hex: string;
  nonce_hex: string;
  amount: number;
  channel_id_hex: string;
  voucher_message_hex: string;
};

test("channel id matches the Rust vector", () => {
  const id = channelId({
    networkTag: vectors.network_tag,
    payer: fromHex(vectors.payer_hex),
    deposit: BigInt(vectors.deposit),
    payerRoot: fromHex(vectors.payer_root_hex),
    nonce: fromHex(vectors.nonce_hex),
  });
  assert.equal(toHex(id), vectors.channel_id_hex);
});

test("voucher message matches the Rust vector", () => {
  const message = voucherMessage(fromHex(vectors.channel_id_hex), BigInt(vectors.amount));
  assert.equal(toHex(message), vectors.voucher_message_hex);
});

test("hex decoding rejects malformed bytes instead of coercing them", () => {
  assert.throws(() => fromHex("0g"), /invalid hex/);
  assert.throws(() => fromHex("abc"), /odd-length hex/);
});

test("channel id rejects a non-integer network tag", () => {
  assert.throws(
    () =>
      channelId({
        networkTag: Number.NaN,
        payer: fromHex(vectors.payer_hex),
        deposit: BigInt(vectors.deposit),
        payerRoot: fromHex(vectors.payer_root_hex),
        nonce: fromHex(vectors.nonce_hex),
      }),
    /networkTag must fit in a u8/,
  );
});
