// SPDX-License-Identifier: MIT

import assert from "node:assert/strict";
import test from "node:test";
import { createEphemeralChannelKeys } from "../../../demo/ui/ephemeral-keys.js";
import { publicKeyFromSecret, scalarFromWide } from "./voucher.ts";

test("separate browser channels receive fresh payer and settlement-output keys", () => {
  let sample = 1;
  const randomBytes = (length: number) => new Uint8Array(length).fill(sample++);
  const first = createEphemeralChannelKeys(randomBytes, scalarFromWide, publicKeyFromSecret);
  const second = createEphemeralChannelKeys(randomBytes, scalarFromWide, publicKeyFromSecret);

  assert.notDeepEqual(first.payerPublic, first.serviceOutputPublic);
  assert.notDeepEqual(first.payerPublic, second.payerPublic);
  assert.notDeepEqual(first.serviceOutputPublic, second.serviceOutputPublic);
});
