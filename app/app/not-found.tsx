import Link from "next/link";

export default function NotFound() {
  return (
    <section className="ledger-bg">
      <div className="mx-auto max-w-[1200px] px-4 py-28 md:px-6">
        <p className="eyebrow text-held">404 · held</p>
        <h1 className="voice mt-3 text-5xl md:text-7xl">This line isn&rsquo;t <em className="text-ghost">on the ledger.</em></h1>
        <p className="mt-4 max-w-md text-muted">The page you asked for doesn&rsquo;t exist. Nothing was charged.</p>
        <div className="mt-8 flex flex-wrap gap-3">
          <Link href="/" className="rounded-sm bg-ink px-5 py-3 font-semibold text-on-ink">Back to the start</Link>
          <Link href="/docs/" className="rounded-sm border border-rule px-5 py-3 hover:border-ink">Search the docs</Link>
        </div>
      </div>
    </section>
  );
}
