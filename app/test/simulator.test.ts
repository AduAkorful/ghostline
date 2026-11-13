import assert from "node:assert/strict";
import { test } from "node:test";
import { EMPTY, pay } from "../lib/simulator.ts";

test("allows the exact budget and keeps accepted totals on refusals", () => {
  const start = pay(EMPTY, 500, 1000);
  const held = pay(start, 1000, 1000);
  const filled = pay(held, 500, 1000);
  const denied = pay(filled, 100, 1000);
  assert.deepEqual(denied.calls.map(({ n, status, total }) => [n, status, total]), [
    [1, "paid", 500], [2, "held", 500], [3, "paid", 1000], [4, "held", 1000],
  ]);
  assert.deepEqual(EMPTY.calls, []);
});

test("settled channels never accept another call", () => {
  const settled = { ...pay(EMPTY, 100, 1000), settled: true };
  assert.equal(pay(settled, 100, 1000), settled);
});
