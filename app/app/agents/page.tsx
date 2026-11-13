import { pageMetadata } from "@/lib/metadata";
import Link from "next/link";
import { CodeBlock } from "@/components/CodeBlock";
import { MCP_TOOLS, POLICY } from "@/lib/tools";

export const metadata = pageMetadata("For agent builders · Ghostline", "Connect a local MCP server, set a spending policy, and let your agent pay for API calls.", "/agents/");

const CONFIG = `{
  "mcpServers": {
    "ghostline": {
      "command": "ghostline-mcp",
      "env": {
        "GHOSTLINE_CHANNEL_FILE": "/home/you/ghostline/channel.json",
        "GHOSTLINE_WALLET_COOKIE": "/home/you/wallet/.cookie",
        "GHOSTLINE_POLICY_BUDGET": "5000",
        "GHOSTLINE_POLICY_MAX_PER_CALL": "1000",
        "GHOSTLINE_POLICY_VENDOR_HEX": "<service public key>",
        "GHOSTLINE_POLICY_AUDIT_LOG": "/home/you/ghostline/audit.jsonl",
        "GHOSTLINE_OPEN_MAX_FEE": "20000"
      }
    }
  }
}`;

const STEPS = [
  { t: "Install Ghostline", d: "Build ghostline, ghostline-mcp and ghostline-paid-api from the repository.", href: "/docs/install/" },
  { t: "Run your wallet with cookie login", d: "tari_ootle_walletd 0.53.3 on 127.0.0.1 with --cookie-auth true. Never --authentication none.", href: "/docs/mainnet-quickstart/#1-start-the-wallet-with-cookie-login" },
  { t: "Create the channel record", d: "ghostline channel init with the service's terms and a Ghostline claim key, so no wallet view key is exported.", href: "/docs/mainnet-quickstart/" },
  { t: "Connect the MCP server", d: "Add the config below to your MCP host. Secrets stay in the environment, never in tool arguments.", href: "/docs/mcp/" },
  { t: "Open, then let the agent pay", d: "ghostline_open_channel quotes first; it broadcasts only after you set GHOSTLINE_OPEN_SUBMIT=1.", href: "/docs/mcp/" },
];

export default function Agents() {
  const groups = ["Pay", "Channel", "Inspect"] as const;
  return (
    <>
      <section className="ledger-bg">
        <div className="mx-auto max-w-[1200px] px-4 pb-16 pt-14 md:px-6 md:pt-20">
          <p className="eyebrow">For agent builders</p>
          <h1 className="voice mt-4 max-w-4xl text-5xl leading-[.95] md:text-[76px]">Let your agent pay. <em className="text-ghost">Keep the limits yours.</em></h1>
          <p className="mt-6 max-w-2xl text-[17px] text-muted">Ghostline runs as a local MCP server next to your agent. The agent asks to pay; the policy engine, which the agent can&rsquo;t edit, decides.</p>
          <ol className="mt-12 border-t border-rule">
            {STEPS.map((s, i) => (
              <li key={s.t} className="grid gap-1 border-b border-rule py-5 md:grid-cols-[48px_280px_1fr_auto] md:items-baseline md:gap-6">
                <span className="font-mono text-sm text-ghost">0{i + 1}</span>
                <h2 className="text-lg font-semibold">{s.t}</h2>
                <p className="text-muted">{s.d}</p>
                <Link href={s.href} aria-label={`Guide: ${s.t}`} className="text-sm text-chain underline underline-offset-4">Guide</Link>
              </li>
            ))}
          </ol>
        </div>
      </section>

      <section className="border-t border-rule bg-paper">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.3fr] md:px-6">
          <div>
            <p className="eyebrow">MCP config</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">One block of config.</h2>
            <p className="mt-4 text-muted">Claude Desktop reads it from <span className="font-mono text-ink">claude_desktop_config.json</span>; Claude Code reads the same shape from a project&rsquo;s <span className="font-mono text-ink">.mcp.json</span>. Any MCP host that runs stdio servers works.</p>
            <div role="note" className="mt-6 rounded-[3px] border border-held/50 bg-held/5 px-4 py-3 text-sm">
              <strong className="text-held">Keep the payer secret out of this file.</strong> Set <span className="font-mono">GHOSTLINE_PAYER_SECRET</span> in the environment that starts your host, and give each server only its own wallet&rsquo;s cookie.
            </div>
          </div>
          <CodeBlock code={CONFIG} lang="json" title="claude_desktop_config.json" />
        </div>
      </section>

      <section className="border-t border-rule">
        <div className="mx-auto grid max-w-[1200px] gap-10 px-4 py-20 md:grid-cols-[1fr_1.3fr] md:px-6">
          <div>
            <p className="eyebrow">Spending policy</p>
            <h2 className="voice mt-3 text-4xl md:text-5xl">The leash, <em>line by line.</em></h2>
            <p className="mt-4 text-muted">Each payment goes through policy before signing. When configured, the audit log records decisions and refusals. On mainnet, the sixth call of run 5 was refused with <span className="font-mono text-[13px] text-held">cumulative 6000 exceeds policy budget 5000</span>.</p>
          </div>
          <dl className="border-t border-rule">
            {POLICY.map((p) => (
              <div key={p.env} className="grid gap-1 border-b border-rule py-3.5">
                <dt className="flex flex-wrap items-baseline justify-between gap-2 font-mono text-[13px]"><span className="text-ink">{p.env}</span><span className="text-muted">{p.example}</span></dt>
                <dd className="text-muted">{p.text}</dd>
              </div>
            ))}
          </dl>
        </div>
      </section>

      <section className="border-t border-rule bg-paper">
        <div className="mx-auto max-w-[1200px] px-4 py-20 md:px-6">
          <p className="eyebrow">What the agent gets</p>
          <h2 className="voice mt-3 text-4xl md:text-5xl">Thirteen tools. <em>No secrets as arguments.</em></h2>
          <div className="mt-10 grid gap-8 md:grid-cols-3">
            {groups.map((g) => (
              <div key={g}>
                <h3 className="eyebrow border-b border-rule pb-2">{g}</h3>
                <ul>
                  {MCP_TOOLS.filter((t) => t.group === g).map((t) => (
                    <li key={t.name} className="border-b border-rule py-3">
                      <p className={`font-mono text-[13px] ${t.group === "Pay" ? "text-ghost" : "text-ink"}`}>{t.name}</p>
                      <p className="mt-1 text-sm text-muted">{t.text}</p>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </div>
      </section>
    </>
  );
}
