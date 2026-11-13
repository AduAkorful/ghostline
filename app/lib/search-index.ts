// Build time only. One entry per docs section plus the marketing pages, written to /search-index.json and loaded
// lazily the first time search opens (it used to be inlined into every docs page).
import { DOCS, type IndexEntry } from "./docs-meta.ts";
import { renderDoc } from "./docs.ts";

const SITE_PAGES: IndexEntry[] = [
  { href: "/how-it-works/", page: "How it works", group: "Site", text: "Open a channel, pay per call with signed vouchers, settle once, or refund at expiry. The HTTP 402 handshake and what a voucher contains." },
  { href: "/privacy/", page: "Privacy", group: "Site", text: "Two views: your private call ledger beside the public chain records. Try the simulator. What stays hidden, public or linked." },
  { href: "/proof/", page: "Mainnet proof", group: "Site", text: "Every recorded mainnet transaction with an indexer link: deployment, opens, settles and the expiry refund." },
  { href: "/fees/", page: "Fees", group: "Site", text: "Fee calculator: network fees, the 1% protocol fee, maximum settleable payout, all-in cost per call and break-even." },
  { href: "/agents/", page: "For agents", group: "Site", text: "Set up an agent: MCP server config, spending policy settings and the 13 tools." },
  { href: "/sellers/", page: "For sellers", group: "Site", text: "Charge per call with the HTTP 402 paywall: quote, serve and settle." },
];

const MAX_TEXT = 1800;

export function buildSearchIndex(): IndexEntry[] {
  const docs = DOCS.flatMap((meta) => renderDoc(meta).search.map((s): IndexEntry => ({
    href: `/docs/${s.slug}/${s.id ? `#${s.id}` : ""}`,
    page: meta.label,
    group: meta.group,
    ...(s.heading ? { heading: s.heading } : {}),
    text: s.text.length > MAX_TEXT ? `${s.text.slice(0, MAX_TEXT)}…` : s.text,
  })));
  return [...SITE_PAGES, ...docs];
}
