import assert from "node:assert/strict";
import { test } from "node:test";
import { stats } from "../lib/data.ts";

test("proof stats are derived from the recorded transactions and add up", () => {
  assert.equal(stats.transactions, 13);
  assert.equal(stats.channelsOpened, 5);
  assert.equal(stats.settles, 4);
  assert.equal(stats.refunds, 1);
  assert.equal(stats.paidCalls, 25);
  assert.equal(stats.refusedCalls, 1);
  // The four settled channels each used one open and one settle; the refunded channel's records are not counted.
  assert.equal(stats.onChainRecordsForCalls, 8);
  // Fee figures are deliberately not part of the stats.
  assert.deepEqual(Object.keys(stats).filter((k) => /fee/i.test(k)), []);
});
