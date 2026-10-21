// SPDX-License-Identifier: MIT

import { verifyVoucher } from "./vendor/ghostline.js";

export const PRICE = 10n;

export class SimulatedPaywall {
  constructor(params, serviceOutputKey) {
    this.params = params;
    this.serviceOutputKey = serviceOutputKey;
    this.lastAccepted = 0n;
    this.calls = 0;
  }

  quote() {
    return {
      status: 402,
      body: {
        scheme: "ghostline.channel.voucher.v1",
        price: PRICE,
        channel_id: this.params.channelId,
      },
    };
  }

  pay(voucher) {
    verifyVoucher(voucher, this.params);
    if (!equalBytes(voucher.serviceOutputKey, this.serviceOutputKey)) {
      throw new Error("voucher recipient does not match this service");
    }
    if (voucher.amount <= this.lastAccepted) {
      throw new Error("cumulative voucher must increase");
    }
    if (voucher.amount > this.params.deposit - 1n) {
      throw new Error("voucher exceeds the fixed-shape spend limit");
    }
    if (voucher.amount - this.lastAccepted !== PRICE) {
      throw new Error("voucher does not pay the quoted price");
    }
    this.lastAccepted = voucher.amount;
    this.calls += 1;
    return { status: 200, body: { fortune: fortunes[(this.calls - 1) % fortunes.length] } };
  }
}

const fortunes = [
  "A quiet channel carries the longest conversation.",
  "The ledger remembers the close, not every whisper.",
  "A fresh key leaves fewer footprints behind.",
  "The spirit accepts only a rising toll.",
  "One sealed receipt can follow many small requests.",
  "A hidden amount is not an invisible transaction.",
  "The service knows every answer it has sold.",
  "A careful payer reads the quote before signing.",
  "The unused remainder waits beyond the expiry.",
  "Commitments keep their secrets, but fees do not.",
  "A voucher grows; it never walks backward.",
  "Two outputs leave fewer clues than one or two.",
  "The network sees the door open and close.",
  "The watcher cannot count calls from this receipt.",
  "No mask should wander away from its owner.",
  "A testnet coin is still worth treating carefully.",
  "The payer signs once for each new cumulative total.",
  "An explorer shows proof of finality, not service quality.",
  "The branch is public even when the amount is veiled.",
  "Ghostline keeps the calls off-chain and the limits honest.",
];

function equalBytes(left, right) {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) difference |= left[index] ^ right[index];
  return difference === 0;
}
