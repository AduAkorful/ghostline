export type DocMeta = { slug: string; label: string; group: Group; file: string; summary: string };
export type Group = "Get started" | "How it works" | "For agents" | "For sellers" | "Risk and security" | "Reference" | "Resources";
/** Sections in reading order: orientation, mechanics, task guides, risk, lookup. */
export const GROUPS: Group[] = ["Get started", "How it works", "For agents", "For sellers", "Risk and security", "Reference", "Resources"];
export const REPO = "https://github.com/AduAkorful/ghostline";
/** Public origin. Repo Markdown links to site pages with this, so the links also work on GitHub. */
export const SITE_ORIGIN = "https://ghostline-ui.vercel.app";

// The order here is the order of the sidebar and of the Previous/Next cards.
export const DOCS: DocMeta[] = [
  { slug: "overview", label: "What is Ghostline", group: "Get started", file: "overview.md", summary: "What Ghostline is, the four moves of a channel, the pieces, and what is live on mainnet." },
  { slug: "install", label: "Install", group: "Get started", file: "install.md", summary: "Build the ghostline, ghostline-mcp and ghostline-paid-api executables. Mainnet needs the source build, not the v0.4.0 release." },
  { slug: "mainnet-quickstart", label: "Mainnet quickstart", group: "Get started", file: "mainnet-quickstart.md", summary: "Run one channel on Tari Ootle mainnet with your own wallet: open, paid calls, settle, expiry refund, and the MCP agent path." },
  { slug: "architecture", label: "How channels work", group: "How it works", file: "architecture.md", summary: "Components, the payment sequence, channel identity and vouchers, and what the chain reveals." },
  { slug: "fees", label: "Fees", group: "How it works", file: "fees.md", summary: "Network fees, the 1% protocol fee, maximum settleable payout and break-even, with mainnet measurements." },
  { slug: "mcp", label: "MCP server", group: "For agents", file: "mcp.md", summary: "Connect the local ghostline-mcp server to an agent host, with every setting and tool." },
  { slug: "policy", label: "Spending policy", group: "For agents", file: "policy.md", summary: "Budget, per-call cap, vendor allowlist and human approval, and how to read the audit log." },
  { slug: "ai-agents", label: "Use with AI agents", group: "For agents", file: "ai-agents.md", summary: "Give an agent the docs as Markdown (llms.txt) and install the Ghostline skill." },
  { slug: "paywall", label: "HTTP 402 paywall", group: "For sellers", file: "paywall.md", summary: "Serve the HTTP 402 reference API or embed the paywall crate: pricing, quotes and ledgers." },
  { slug: "settle", label: "Settling and payouts", group: "For sellers", file: "settle.md", summary: "Settle before expiry, use a wallet payout destination, and refund after expiry." },
  { slug: "privacy", label: "Privacy", group: "Risk and security", file: "privacy.md", summary: "What Ghostline hides, what observers can still see, and the mainnet observer test." },
  { slug: "threat-model", label: "Threat model", group: "Risk and security", file: "threat-model.md", summary: "Who can do what, the goals and non-goals, and the main attacks." },
  { slug: "deployment", label: "Mainnet deployment", group: "Reference", file: "deployment.md", summary: "Mainnet template, fee-config and publish transaction addresses, and the protocol parameters." },
  { slug: "api", label: "API reference", group: "Reference", file: "api-reference.md", summary: "The HTTP payment protocol, reference routes, Rust crates, TypeScript package and MCP tools." },
  { slug: "cli", label: "CLI", group: "Reference", file: "cli.md", summary: "Every ghostline command and flag, generated from --help." },
  { slug: "typescript", label: "TypeScript SDK", group: "Reference", file: "typescript.md", summary: "Voucher signing, verification and the fee rules in @ghostline/sdk." },
  { slug: "glossary", label: "Glossary", group: "Reference", file: "glossary.md", summary: "Definitions of the terms used across the docs." },
  { slug: "faq", label: "FAQ", group: "Resources", file: "faq.md", summary: "Short answers on privacy, refunds, restarts, fees and audit status." },
  { slug: "troubleshooting", label: "Troubleshooting", group: "Resources", file: "troubleshooting.md", summary: "Wallet authentication, fee caps, needs_channel, refused calls and spent outputs." },
];

export const slugify = (s: string) => s.toLowerCase().replace(/<[^>]+>/g, "").replace(/[`*_]/g, "").replace(/[^a-z0-9 -]/g, "").trim().replace(/\s+/g, "-");

export type SearchEntry = { slug: string; title: string; id?: string; heading?: string; text: string };

export type TocItem = { id: string; label: string; depth: number };
/** One searchable section of the site. `href` includes the section anchor. */
export type IndexEntry = { href: string; page: string; group: string; heading?: string; text: string; weight?: number };
