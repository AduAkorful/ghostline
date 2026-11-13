// Build time only. Everything an AI agent can read instead of HTML: /llms.txt, /llms-full.txt, /docs/<page>.md and /skill.md.
import fs from "node:fs";
import path from "node:path";
import { DOCS, GROUPS, SITE_ORIGIN, type DocMeta } from "./docs-meta.ts";
import { rewriteHref } from "./docs.ts";

const ROOT = path.resolve(process.cwd(), "..");
export const SKILL_SOURCE = "skills/ghostline/SKILL.md";
export const mdUrl = (slug: string) => `${SITE_ORIGIN}/docs/${slug}.md`;

/** A link target an agent can fetch from anywhere: absolute, and Markdown for docs pages. */
function agentHref(href: string, source: string): string {
  const to = rewriteHref(href, source);
  if (to.startsWith("#") || /^(https?:|mailto:)/i.test(to)) return to;
  const doc = to.match(/^\/docs\/([a-z0-9-]+)\/(#.*)?$/);
  return doc ? `${mdUrl(doc[1])}${doc[2] ?? ""}` : `${SITE_ORIGIN}${to}`;
}

/** One docs page as Markdown: links rewritten to absolute URLs, plus a header pointing back at the page and the index. */
export function pageMarkdown(meta: DocMeta): string {
  const file = `docs/${meta.file}`;
  const src = fs.readFileSync(path.join(ROOT, file), "utf8");
  // Links inside code fences are examples, not links, so only the prose between fences is rewritten.
  const body = src.split(/(^```[\s\S]*?^```[ \t]*$)/m).map((part, i) => i % 2 ? part : part.replace(
    /(\]\()([^)\s]+)((?:\s+"[^"]*")?\))/g, (_m, open: string, href: string, close: string) => `${open}${agentHref(href, file)}${close}`,
  )).join("");
  const note = `> Page: ${SITE_ORIGIN}/docs/${meta.slug}/ · Docs index: ${SITE_ORIGIN}/llms.txt`;
  return body.replace(/^(# .+)$/m, `$1\n\n${note}`);
}

export function llmsTxt(): string {
  const lines = [
    "# Ghostline",
    "",
    "> Private pay-per-call channels for AI agents on Tari Ootle mainnet. An agent locks funds once, signs a cumulative voucher for each paid HTTP call off-chain, and the service settles once on chain. A spending policy outside the agent limits what it can sign. Unaudited: keep deposits small.",
    "",
    "Every documentation page below is Markdown. Fetch a page with the `.md` URL, or all pages at once from llms-full.txt. Amounts are integer micro-TARI (µT); one TARI is 1,000,000 µT.",
    "",
  ];
  for (const group of GROUPS) {
    lines.push(`## ${group}`, "");
    for (const meta of DOCS.filter((d) => d.group === group)) {
      lines.push(`- [${meta.label}](${mdUrl(meta.slug)}): ${meta.summary}`);
    }
    lines.push("");
  }
  lines.push(
    "## Agent resources",
    "",
    `- [Ghostline skill](${SITE_ORIGIN}/skill.md): a SKILL.md that teaches an agent the tools, money rules and safety rules`,
    `- [Full documentation](${SITE_ORIGIN}/llms-full.txt): every docs page in one file`,
    "",
    "## Site",
    "",
    `- [Mainnet proof](${SITE_ORIGIN}/proof/): every recorded mainnet transaction with an indexer link`,
    `- [Fee calculator](${SITE_ORIGIN}/fees/): maximum settleable payout, all-in cost per call and break-even`,
    `- [Privacy: two views](${SITE_ORIGIN}/privacy/): a simulation of the private ledger beside the public chain records`,
    "",
  );
  return lines.join("\n");
}

export function llmsFullTxt(): string {
  const head = [
    "# Ghostline documentation",
    "",
    `> The complete Ghostline docs in one file. Index: ${SITE_ORIGIN}/llms.txt. Each page starts with its own title and its web address.`,
    "",
  ].join("\n");
  return [head, ...DOCS.map(pageMarkdown)].join("\n\n---\n\n") + "\n";
}

export function skillMarkdown(): string {
  return fs.readFileSync(path.join(ROOT, SKILL_SOURCE), "utf8");
}
