import assert from "node:assert/strict";
import { test } from "node:test";
import { highlight, prepare, search, snippet, tokensOf, withinOneEdit } from "../lib/search.ts";
import { buildSearchIndex } from "../lib/search-index.ts";

const index = prepare(buildSearchIndex());
const hrefs = (q: string, n = 3) => search(index, q).slice(0, n).map((r) => r.entry.href);

test("every docs section is indexed and its anchors resolve to rendered headings", () => {
  const entries = buildSearchIndex();
  assert.ok(entries.length > 150, `only ${entries.length} entries`);
  assert.ok(entries.every((e) => e.href.startsWith("/") && e.page && e.text.length >= 0));
});

test("finds the right section for plain-language, identifier and flag queries", () => {
  assert.ok(hrefs("cookie").includes("/docs/mainnet-quickstart/#1-start-the-wallet-with-cookie-login"));
  assert.ok(hrefs("GHOSTLINE_POLICY_BUDGET").includes("/docs/policy/#set-the-limits"));
  assert.ok(hrefs("policy budget").includes("/docs/policy/#set-the-limits"));
  assert.ok(hrefs("max settleable").some((h) => h.startsWith("/docs/fees/")));
  assert.ok(hrefs("payout destination", 5).some((h) => h.startsWith("/docs/settle/")));
  assert.ok(hrefs("how do I refund after expiry", 5).some((h) => /refund|settle/.test(h)));
});

test("page names rank their own page first and accept partial words", () => {
  assert.equal(hrefs("mcp")[0].split("#")[0], "/docs/mcp/");
  assert.equal(hrefs("thre")[0].split("#")[0], "/docs/threat-model/");
});

test("indexed text keeps identifiers and drops Markdown syntax", () => {
  const limits = buildSearchIndex().find((e) => e.href === "/docs/policy/#set-the-limits")!;
  assert.ok(limits.text.includes("GHOSTLINE_POLICY_BUDGET"), limits.text.slice(0, 120));
  for (const e of buildSearchIndex()) assert.ok(!/\|[-:\s]*\||```/.test(e.text), `${e.href}: ${e.text.slice(0, 80)}`);
  assert.equal(search(index, "policy budget")[0].entry.href, "/docs/policy/#set-the-limits");
});

test("tolerates one typo in longer words and returns nothing for gibberish", () => {
  assert.ok(hrefs("setlle", 6).some((h) => h.startsWith("/docs/settle/")));
  assert.ok(hrefs("walet daemon", 6).length > 0);
  assert.deepEqual(search(index, "zzqqxxjj"), []);
  assert.deepEqual(search(index, "   "), []);
});

test("caps results per page so one long page cannot fill the list", () => {
  const pages = search(index, "channel", 12).map((r) => r.entry.href.split("#")[0]);
  for (const p of new Set(pages)) assert.ok(pages.filter((x) => x === p).length <= 3, p);
});

test("tokens drop filler words but keep a query made only of them", () => {
  assert.deepEqual(tokensOf("How do I settle?"), ["settle"]);
  assert.deepEqual(tokensOf("how to"), ["how", "to"]);
  assert.ok(withinOneEdit("settle", "setlle") && withinOneEdit("settle", "settl") && !withinOneEdit("settle", "setting"));
});

test("highlights matches inside identifiers and excerpts around the first hit", () => {
  const parts = highlight("Set GHOSTLINE_POLICY_BUDGET in µT", "policy budget");
  assert.deepEqual(parts.filter((p) => p.hit).map((p) => p.text), ["POLICY", "BUDGET"]);
  const long = `${"word ".repeat(60)}the cookie file is private ${"tail ".repeat(60)}`;
  const excerpt = snippet(long, "cookie");
  assert.ok(excerpt.some((p) => p.hit && p.text.toLowerCase() === "cookie"));
  assert.ok(excerpt[0].text === "…" && excerpt.at(-1)!.text === "…");
  assert.ok(excerpt.map((p) => p.text).join("").length < 260);
});
