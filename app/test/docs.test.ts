import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { checkDocs, createSlugger, renderDoc, rewriteHref } from "../lib/docs.ts";
import { DOCS, REPO, SITE_ORIGIN } from "../lib/docs-meta.ts";

test("slugs preserve published anchors and give duplicate headings unique IDs", () => {
  const slug = createSlugger();
  assert.equal(slug("1. Start the wallet with cookie login"), "1-start-the-wallet-with-cookie-login");
  assert.equal(slug("Wallet auth"), "wallet-auth");
  assert.equal(slug("Wallet auth"), "wallet-auth-1");
  assert.equal(slug("Wallet auth-1"), "wallet-auth-1-1");
  assert.equal(slug("?!"), "section");
});

test("rewrites Markdown aliases and repository links, preserving anchors", () => {
  assert.equal(rewriteHref("mainnet-quickstart.md#agent-path-mcp", "docs/install.md"), "/docs/mainnet-quickstart/#agent-path-mcp");
  assert.equal(rewriteHref("./api-reference.md#http-payment-protocol", "docs/paywall.md"), "/docs/api/#http-payment-protocol");
  assert.equal(rewriteHref("#build-from-source", "docs/install.md"), "#build-from-source");
  assert.equal(rewriteHref("../README.md#development", "docs/install.md"), `${REPO}/blob/main/README.md#development`);
  assert.equal(rewriteHref("https://example.com/docs.md", "docs/install.md"), "https://example.com/docs.md");
});

test("fails on missing files, routes and fragments", () => {
  assert.throws(() => rewriteHref("missing.md", "docs/install.md"), /missing file/);
  assert.throws(() => rewriteHref("mainnet-quickstart.md#missing", "docs/install.md"), /missing anchor/);
  assert.throws(() => rewriteHref("#missing", "docs/install.md"), /missing anchor/);
  assert.throws(() => rewriteHref("/docs/missing/", "docs/install.md"), /missing site target/);
  assert.doesNotThrow(checkDocs);
});

test("search and rendered headings use the same anchors", () => {
  for (const meta of DOCS) {
    const doc = renderDoc(meta);
    for (const section of doc.search) {
      if (section.id) assert.ok(doc.html.includes(`id="${section.id}"`), `${meta.slug}: ${section.id}`);
    }
  }
});

test("site URLs in repo Markdown stay internal on the site and external elsewhere", () => {
  assert.equal(rewriteHref(`${SITE_ORIGIN}/proof/`, "docs/mcp.md"), "/proof/");
  assert.equal(rewriteHref(`${SITE_ORIGIN}/docs/policy/#set-the-limits`, "docs/mcp.md"), "/docs/policy/#set-the-limits");
  // A legacy page that is not an app route is an ordinary external link, never a build failure.
  assert.equal(rewriteHref(`${SITE_ORIGIN}/evidence.html`, "docs/privacy.md"), `${SITE_ORIGIN}/evidence.html`);
  assert.throws(() => rewriteHref(`${SITE_ORIGIN}/docs/policy/#nope`, "docs/mcp.md"), /missing anchor/);
});

test("the table of contents lists h2 and h3 headings with their depth", () => {
  const toc = renderDoc(DOCS.find((d) => d.slug === "mainnet-quickstart")!).toc;
  assert.ok(toc.length > 5 && toc.every((t) => t.depth === 2 || t.depth === 3));
});

test("published docs do not point readers into the repository", () => {
  const repoLink = new RegExp(`href="${REPO.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/(blob|tree)/`);
  // Paths and phrases that only make sense to someone looking at the source tree. Fenced code is exempt: build commands must name paths.
  const inRepo = /(?<![\w./~-])(crates|contracts|middleware|packages|scripts|demo|deploy|plans|skills|prototype)\/[A-Za-z]|this repository|the repository|your checkout|the checkout|current checkout|repository root|\bREADME\b/;
  for (const meta of DOCS) {
    const doc = renderDoc(meta);
    assert.ok(!repoLink.test(doc.html), `${meta.slug}: links to a file in the repository`);
    const prose = fs.readFileSync(path.join(process.cwd(), "..", "docs", meta.file), "utf8").replace(/^```[\s\S]*?^```[ \t]*$/gm, "");
    const hit = prose.match(inRepo);
    assert.equal(hit, null, `${meta.slug}: mentions the repository (${hit?.[0]})`);
  }
});
