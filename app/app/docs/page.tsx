import { pageMetadata } from "@/lib/metadata";
import Link from "next/link";
import { OpenSearchButton } from "@/components/OpenSearchButton";
import { DOCS, GROUPS } from "@/lib/docs-meta";

export const metadata = pageMetadata("Docs · Ghostline", "Install Ghostline and find the mainnet quickstart, MCP setup, paywall integration, and references.", "/docs/");

const PATHS = [
  { t: "I run an agent that pays for APIs", d: "Set up a wallet, connect the MCP server, set a budget.", links: [["Mainnet quickstart", "mainnet-quickstart"], ["MCP server", "mcp"], ["Spending policy", "policy"]] },
  { t: "I sell an API", d: "Serve a 402 quote, verify vouchers, settle before expiry.", links: [["HTTP 402 paywall", "paywall"], ["Settling and payouts", "settle"], ["Fees", "fees"]] },
  { t: "I use a coding agent", d: "Give it the docs as Markdown and install the Ghostline skill.", links: [["Use with AI agents", "ai-agents"], ["MCP server", "mcp"], ["Glossary", "glossary"]] },
  { t: "I want to understand it first", d: "What it is, how a channel works, and what stays private.", links: [["What is Ghostline", "overview"], ["How channels work", "architecture"], ["Privacy", "privacy"]] },
];

export default function DocsHome() {
  return (
    <div className="px-5 py-10 md:px-10 lg:py-14">
      <div className="mx-auto max-w-[980px]">
        <h1 className="voice text-5xl md:text-6xl">Ghostline docs</h1>
        <p className="mt-4 max-w-2xl text-[17px] text-muted">Set up Ghostline on Tari Ootle mainnet, integrate the paywall, or see what a channel reveals.</p>
        <OpenSearchButton />
        <div className="mt-10 grid gap-4 md:grid-cols-2">
          {PATHS.map((p) => (
            <div key={p.t} className="rounded-lg border border-rule bg-night p-5">
              <h2 className="text-lg font-semibold">{p.t}</h2>
              <p className="mt-1 text-sm text-muted">{p.d}</p>
              <ul className="mt-4 space-y-1.5 text-sm">{p.links.map(([l, s]) => <li key={s}><Link className="text-chain underline-offset-4 hover:underline" href={`/docs/${s}/`}>{l} →</Link></li>)}</ul>
            </div>
          ))}
        </div>
        <h2 className="eyebrow mt-14 border-b border-rule pb-2">All pages</h2>
        <div className="mt-5 grid gap-x-8 gap-y-7 sm:grid-cols-2 lg:grid-cols-3">
          {GROUPS.map((g) => (
            <div key={g}>
              <h3 className="font-semibold">{g}</h3>
              <ul className="mt-2 space-y-1 text-sm">
                {DOCS.filter((d) => d.group === g).map((d) => (
                  <li key={d.slug}><Link className="text-muted hover:text-ink" href={`/docs/${d.slug}/`}>{d.label}</Link></li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
