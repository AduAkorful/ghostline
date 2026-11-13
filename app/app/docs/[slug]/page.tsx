import { pageMetadata } from "@/lib/metadata";
import Link from "next/link";
import { notFound } from "next/navigation";
import { DocEnhancer } from "@/components/DocEnhancer";
import { AgentSkillCard } from "@/components/AgentSkillCard";
import { DocActions } from "@/components/DocActions";
import { DocToc } from "@/components/DocToc";
import { DOCS } from "@/lib/docs-meta";
import { renderDoc } from "@/lib/docs";

export const dynamicParams = false;
export function generateStaticParams() { return DOCS.map((d) => ({ slug: d.slug })); }

export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const meta = DOCS.find((d) => d.slug === slug);
  if (!meta) return {};
  const page = pageMetadata(`${meta.label} · Ghostline docs`, renderDoc(meta).description, `/docs/${slug}/`);
  // Tell agents and crawlers where the Markdown version lives.
  return { ...page, alternates: { ...page.alternates, types: { "text/markdown": `/docs/${slug}.md` } } };
}

function Pager({ doc, direction }: { doc: (typeof DOCS)[number] | undefined; direction: "prev" | "next" }) {
  if (!doc) return <span />;
  const next = direction === "next";
  return (
    <Link href={`/docs/${doc.slug}/`} rel={direction}
      className={`group block rounded-lg border border-rule p-4 transition-colors duration-[var(--quick)] hover:border-ink/40 hover:bg-paper-2 ${next ? "text-right" : ""}`}>
      <span className="text-xs text-muted">{next ? "Next" : "Previous"}</span>
      <span className="mt-1 block font-medium">{!next && <span className="mr-1.5 text-muted transition-transform group-hover:-translate-x-0.5" aria-hidden="true">←</span>}{doc.label}{next && <span className="ml-1.5 text-muted" aria-hidden="true">→</span>}</span>
    </Link>
  );
}

export default async function DocPage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const i = DOCS.findIndex((d) => d.slug === slug);
  if (i < 0) notFound();
  const meta = DOCS[i];
  const doc = renderDoc(meta);

  return (
    <div className="grid gap-x-12 px-5 py-8 md:px-10 lg:py-12 xl:grid-cols-[minmax(0,1fr)_230px]">
      <article className="mx-auto w-full min-w-0 max-w-[820px]">
        <nav aria-label="Breadcrumb" className="text-[13px] text-muted">
          <Link href="/docs/" className="hover:text-ink">Docs</Link><span className="mx-1.5" aria-hidden="true">›</span><span>{meta.group}</span>
        </nav>
        <div className="mt-3 flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
          <h1 id={doc.titleId} className="min-w-0 text-[34px] font-semibold leading-tight tracking-tight md:text-[40px]">{doc.title}</h1>
          <DocActions slug={slug} />
        </div>
        {slug === "ai-agents" && <AgentSkillCard />}
        <DocEnhancer html={doc.html} diagram={doc.diagram} />
        <nav aria-label="Pager" className="mt-10 grid gap-3 border-t border-rule pt-8 sm:grid-cols-2">
          <Pager doc={DOCS[i - 1]} direction="prev" />
          <Pager doc={DOCS[i + 1]} direction="next" />
        </nav>
      </article>
      <DocToc items={doc.toc} />
    </div>
  );
}
