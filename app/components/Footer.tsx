import Link from "next/link";

export function Footer() {
  return (
    <footer className="border-t border-rule">
      <div className="mx-auto grid max-w-[1200px] gap-8 px-4 py-12 md:grid-cols-4 md:px-6">
        <div className="md:col-span-2">
          <p className="voice text-2xl italic">Ghostline</p>
          <p className="mt-2 max-w-sm text-sm text-muted">Private pay-per-call channels for AI agents on Tari's Ootle. MIT licensed. Unaudited: start with small deposits.</p>
        </div>
        <ul className="space-y-2 text-sm text-muted">
          <li><Link className="hover:text-ink" href="/agents/">For agents</Link></li>
          <li><Link className="hover:text-ink" href="/sellers/">For sellers</Link></li>
          <li><Link className="hover:text-ink" href="/docs/">Docs</Link></li>
          <li><Link className="hover:text-ink" href="/proof/">Mainnet proof</Link></li>
          <li><Link className="hover:text-ink" href="/docs/ai-agents/">Use with AI agents</Link></li>
          <li><a className="hover:text-ink" href="/llms.txt">llms.txt</a></li>
        </ul>
        <ul className="space-y-2 text-sm text-muted">
          <li><a className="hover:text-ink" href="https://github.com/AduAkorful/ghostline">Source on GitHub ↗</a></li>
          <li><a className="hover:text-ink" href="/demo/">Esmeralda contest demo ↗</a></li>
          <li><Link className="hover:text-ink" href="/privacy/">What stays public</Link></li>
        </ul>
      </div>
      <p className="mx-auto max-w-[1200px] px-4 pb-8 text-xs text-muted md:px-6">Mainnet evidence is dated. Simulated channels are labelled.</p>
    </footer>
  );
}
