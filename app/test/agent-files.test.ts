import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { DOCS, GROUPS, SITE_ORIGIN } from "../lib/docs-meta.ts";
import { llmsFullTxt, llmsTxt, pageMarkdown, skillMarkdown } from "../lib/agent-files.ts";

const repo = path.resolve(process.cwd(), "..");
const nonCode = (md: string) => md.split(/^```[\s\S]*?^```[ \t]*$/m).join("\n");

test("llms.txt follows the format: title, summary, then a link list per section", () => {
  const txt = llmsTxt();
  assert.match(txt, /^# Ghostline\n\n> /);
  for (const group of GROUPS) assert.ok(txt.includes(`\n## ${group}\n`), group);
  for (const doc of DOCS) assert.ok(txt.includes(`- [${doc.label}](${SITE_ORIGIN}/docs/${doc.slug}.md): ${doc.summary}`), doc.slug);
  const headings = txt.match(/^## .+$/gm)!;
  assert.equal(new Set(headings).size, headings.length, "duplicate section headings");
  for (const [, url] of txt.matchAll(/\]\(([^)]+)\)/g)) assert.ok(url.startsWith(`${SITE_ORIGIN}/`), url);
});

test("every docs page is served as Markdown with absolute links and a pointer to the index", () => {
  for (const doc of DOCS) {
    const md = pageMarkdown(doc);
    assert.match(md, new RegExp(`^# .+\\n\\n> Page: ${SITE_ORIGIN}/docs/${doc.slug}/ · Docs index: ${SITE_ORIGIN}/llms.txt`), doc.slug);
    for (const [, href] of nonCode(md).matchAll(/\]\(([^)\s]+)/g)) {
      assert.ok(href.startsWith("#") || /^https?:\/\//.test(href), `${doc.slug}: relative link ${href}`);
    }
  }
  assert.ok(pageMarkdown(DOCS.find((d) => d.slug === "mcp")!).includes(`](${SITE_ORIGIN}/docs/policy.md`));
});

test("code fences are copied untouched", () => {
  const src = fs.readFileSync(path.join(repo, "docs/ai-agents.md"), "utf8");
  const fence = src.match(/^```bash\n([\s\S]*?)^```/m)![0];
  assert.ok(pageMarkdown(DOCS.find((d) => d.slug === "ai-agents")!).includes(fence));
});

test("llms-full.txt contains every page once, in registry order", () => {
  const full = llmsFullTxt();
  let at = -1;
  for (const doc of DOCS) {
    const marker = `> Page: ${SITE_ORIGIN}/docs/${doc.slug}/ `;
    assert.equal(full.split(marker).length - 1, 1, doc.slug);
    assert.ok(full.indexOf(marker) > at, `${doc.slug} out of order`);
    at = full.indexOf(marker);
  }
});

test("the skill has front matter and names only tools, settings and pages that exist", () => {
  const skill = skillMarkdown();
  const front = skill.match(/^---\nname: ([a-z-]+)\ndescription: (.+)\n---\n/);
  assert.ok(front, "front matter");
  assert.equal(front![1], "ghostline");
  assert.ok(front![2].length > 60 && front![2].length < 500);
  const tools = Object.keys(JSON.parse(fs.readFileSync(path.join(process.cwd(), "lib/generated-tools.json"), "utf8")).tools);
  for (const name of new Set(skill.match(/ghostline_[a-z_]+/g))) assert.ok(tools.includes(name), `unknown tool ${name}`);
  const docs = DOCS.filter((d) => d.file !== "cli.md").map((d) => fs.readFileSync(path.join(repo, "docs", d.file), "utf8")).join("\n");
  for (const env of new Set(skill.match(/GHOSTLINE_[A-Z_]+/g))) assert.ok(docs.includes(env), `${env} is not documented`);
  for (const [, slug] of skill.matchAll(/\/docs\/([a-z-]+)\.md/g)) assert.ok(DOCS.some((d) => d.slug === slug), `no docs page ${slug}`);
  assert.ok(!/anonymous(?! or untraceable)/.test(skill.replace("Do not call Ghostline anonymous or untraceable", "")), "privacy overclaim");
});
