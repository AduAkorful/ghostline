// SPDX-License-Identifier: MIT
// Static checks for the site pages: shared navigation, link conventions and evidence consistency.
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { describeRuns, explorerStatus, formatRunDate } from "./evidence-view.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const PAGES = ["index.html", "play.html", "evidence.html", "docs/install.html", "docs/quickstart.html", "docs/interfaces.html"];
const html = Object.fromEntries(PAGES.map((page) => [page, read(page)]));
const evidence = JSON.parse(read("evidence.json"));

function anchors(source) {
  return [...source.matchAll(/<a\b([^>]*)>([\s\S]*?)<\/a>/g)].map(([, attributes, body]) => ({
    href: /href="([^"]*)"/.exec(attributes)?.[1],
    attributes,
    body,
  }));
}

// Pages live at different depths, so compare where each link lands, not its spelling.
function navigation(page) {
  const nav = /<nav aria-label="Main navigation">([\s\S]*?)<\/nav>/.exec(html[page])?.[1];
  assert.ok(nav, `${page} has a main navigation`);
  return anchors(nav).map(({ href, body }) => {
    const target = new URL(href, `https://site.test/${page}`);
    const normalized = `${target.pathname.replace(/^\/$/, "/index.html")}${target.hash}`.replace(/^\/index\.html#/, "#").replace(/^\/docs\/[a-z]+\.html$/, "/docs");
    return `${body.replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim()} → ${normalized.startsWith("#") ? normalized : normalized.replace(/^\/index\.html/, "/")}`;
  });
}

test("every page uses the same main navigation", () => {
  const expected = navigation("index.html");
  assert.ok(expected.some((item) => item.startsWith("Privacy")), "the shared nav includes Privacy");
  for (const page of PAGES.slice(1)) assert.deepEqual(navigation(page), expected, `${page} navigation matches the landing page`);
});

test("the external-link arrow is used only for links that leave the site", () => {
  for (const page of PAGES) {
    for (const { href, body } of anchors(html[page])) {
      if (!href) continue;
      const external = /^https?:\/\//.test(href);
      if (!external) assert.doesNotMatch(body, /↗/, `${page}: internal link ${href} uses ↗`);
    }
  }
});

test("dates use one format: day, short month, year", () => {
  const usStyle = /\b(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) \d{1,2}, \d{4}\b/;
  for (const page of PAGES) {
    const visible = html[page].replace(/<script[\s\S]*?<\/script>/g, "").replace(/<[^>]+>/g, " ");
    assert.doesNotMatch(visible, usStyle, `${page} uses a month-first date`);
  }
  assert.equal(formatRunDate("2026-10-22"), "22 Oct 2026");
  assert.equal(formatRunDate("not a date"), "date not recorded");
});

test("landing evidence links ship real hrefs that match evidence.json", () => {
  const index = html["index.html"];
  const opened = evidence.postResetOpen;
  const expected = {
    "open-tx-link": opened.openTransactionUrl,
    "template-tx-link": evidence.templateRecord.publishTransactionUrl,
    "template-id-link": evidence.templateRecord.publishTransactionUrl,
  };
  for (const [id, url] of Object.entries(expected)) {
    const tag = new RegExp(`<a\\b[^>]*\\bid="${id}"[^>]*>`).exec(index)?.[0];
    assert.ok(tag, `landing has #${id}`);
    assert.equal(/href="([^"]*)"/.exec(tag)?.[1], url, `#${id} links to the recorded transaction`);
  }
  assert.ok(index.includes(opened.openTransactionId) && index.includes(evidence.templateRecord.publishTransactionId), "ids are present without JavaScript");
  assert.doesNotMatch(index, /9986c19d|1684aad9|61bad5eb|22 Oct 2026|23 Oct 2026/);
});

test("landing copy matches the current template record", () => {
  const index = html["index.html"];
  assert.match(index, /2 Nov 2026/);
  assert.match(index, /51 µT/);
  assert.match(index, /No paid calls and no settlement are recorded on this template/);
  assert.match(index, /Try the 5-call simulation \(no wallet\)/);
  assert.match(index, /Mediums, start here/);
  assert.doesNotMatch(index.replace(/\b(class|id|aria-labelledby)="[^"]*judge[^"]*"/g, ""), /judge/i, "visible copy never addresses judges");
  assert.match(index, /id="terms"/);
  assert.doesNotMatch(index, /did not recognize these historical records/);
});

test("every transaction id in the evidence has a checked explorer status", () => {
  const listed = describeRuns(evidence).flatMap((run) => run.transactions.map(([, id]) => id));
  const recorded = Object.keys(evidence.explorerAvailability.transactions);
  for (const id of listed) assert.notEqual(explorerStatus(evidence, id), "not checked", `${id} has an explorer status`);
  assert.deepEqual(new Set(listed), new Set(recorded), "the readable view and the status list cover the same transactions");
  for (const [id, state] of Object.entries(evidence.explorerAvailability.transactions)) {
    assert.match(id, /^[0-9a-f]{64}$/);
    assert.ok(["loads", "not known to the public indexer"].includes(state), `${id}: ${state}`);
  }
});

test("pages link to the readable evidence view, which links back to the site", () => {
  assert.match(html["play.html"], /href="evidence\.html"/);
  assert.match(html["index.html"], /href="evidence\.html"/);
  assert.match(html["evidence.html"], /href="index\.html#evidence"/);
  assert.match(html["evidence.html"], /href="evidence\.json"/);
});

test("the install guide states that macOS and Windows are not supported", () => {
  assert.match(html["docs/install.html"], /macOS and Windows are not supported/);
});

test("play.js only looks up element ids that play.html defines", () => {
  const ids = new Set([...html["play.html"].matchAll(/\sid="([^"]+)"/g)].map((match) => match[1]));
  const used = new Set([...read("play.js").matchAll(/byId\("([^"]+)"\)/g)].map((match) => match[1]));
  for (const id of used) assert.ok(ids.has(id), `play.html is missing #${id}`);
  assert.doesNotMatch(read("play.js"), /querySelector\("\.(fine-print|result-label)"\)/, "result fields are addressed by id, not by the first matching class");
});

test("the play page has the observer view, overspend control and relay banner", () => {
  const play = html["play.html"];
  for (const id of ["observer-view", "observer-rows", "observer-leaks", "overspend", "relay-banner", "banner-simulate"]) {
    assert.match(play, new RegExp(`id="${id}"`), `play.html has #${id}`);
  }
  // Fees and timing are public; the panel must never label them hidden.
  assert.match(read("play.js"), /\["Fees, timing, settle branch, two-output shape", live \? "Visible/);
});

test("the current record is a template publish and one open, with no settlement", () => {
  assert.equal(evidence.postResetOpen.depositMicroTari, 51);
  assert.equal(evidence.postResetOpen.templateAddress, evidence.templateRecord.address);
  assert.equal(evidence.postResetOpen.note.includes("No paid calls"), true);
  assert.equal(evidence.chainRecord, undefined);
  assert.equal(evidence.liveRunRecord, undefined);
});

test("the custodial relay is described the same way on the play page and in the Sapient notes", () => {
  const phrase = "on-chain observers see neither the deposit nor the split";
  assert.ok(html["play.html"].includes(phrase));
  assert.ok(readFileSync(new URL("../../docs/sapient.md", import.meta.url), "utf8").includes(phrase));
  for (const page of PAGES) assert.doesNotMatch(html[page], /never holds (your )?funds|relay never sees/i, `${page} overstates the relay`);
});

test("any demo video the landing page embeds is in the published files", () => {
  for (const [, path] of html["index.html"].matchAll(/(?:src|poster)="(media\/[^"]+)"/g)) {
    assert.ok(existsSync(new URL(path, import.meta.url)), `${path} exists`);
  }
});

