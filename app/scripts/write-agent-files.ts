import fs from "node:fs";
import path from "node:path";
import { DOCS } from "../lib/docs-meta.ts";
import { llmsFullTxt, llmsTxt, pageMarkdown, skillMarkdown } from "../lib/agent-files.ts";

const pub = path.resolve(import.meta.dirname, "..", "public");
fs.mkdirSync(path.join(pub, "docs"), { recursive: true });
const keep = new Set(DOCS.map((d) => `${d.slug}.md`));
for (const file of fs.readdirSync(path.join(pub, "docs"))) {
  if (file.endsWith(".md") && !keep.has(file)) fs.rmSync(path.join(pub, "docs", file));
}
for (const meta of DOCS) fs.writeFileSync(path.join(pub, "docs", `${meta.slug}.md`), pageMarkdown(meta));
fs.writeFileSync(path.join(pub, "llms.txt"), llmsTxt());
fs.writeFileSync(path.join(pub, "llms-full.txt"), llmsFullTxt());
fs.writeFileSync(path.join(pub, "skill.md"), skillMarkdown());
console.log(`Wrote llms.txt, llms-full.txt, skill.md and ${DOCS.length} Markdown pages.`);
