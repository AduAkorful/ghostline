// Build-time only. Render and validate the same Markdown tokens.
import fs from "node:fs";
import path from "node:path";
import { Marked, type Token, type Tokens } from "marked";
import { DOCS, REPO, SITE_ORIGIN, slugify, type DocMeta, type SearchEntry } from "./docs-meta.ts";
import { shortHash } from "./format.ts";

const ROOT = path.resolve(process.cwd(), "..");
const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const docByFile = new Map(DOCS.map((doc) => [`docs/${doc.file}`, doc]));
const docByRoute = new Map(DOCS.map((doc) => [`/docs/${doc.slug}/`, doc]));
const routes = new Set(["/", "/how-it-works/", "/privacy/", "/proof/", "/fees/", "/agents/", "/sellers/", "/docs/", "/demo/"]);

type Heading = { id: string; label: string; depth: number };
type Markdown = { tokens: Tokens.Generic[]; headings: Map<Tokens.Heading, Heading>; anchors: Set<string> };
const markdown = new Map<string, Markdown>();
const lexer = new Marked({ gfm: true });

/** Text for the search index: keeps identifiers such as GHOSTLINE_POLICY_BUDGET intact and drops Markdown syntax. */
export function searchText(md: string): string {
  return md
    .replace(/^```[^\n]*\n([\s\S]*?)^```[ \t]*$/gm, "$1")
    .replace(/^[ \t]*\|?[ \t]*:?-{2,}:?[ \t]*(\|[ \t]*:?-{2,}:?[ \t]*)*\|?[ \t]*$/gm, "")
    .replace(/^[ \t]*([-*+>]|\d+\.)[ \t]+/gm, "")
    .replace(/\s*\|\s*/g, " · ")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/<[^>]+>/g, "")
    .replace(/\*\*|__/g, "")
    .replace(/^ · | · $/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function plain(md: string) {
  return md.replace(/`([^`]+)`/g, "$1").replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").replace(/<[^>]+>|[*_>]/g, "").replace(/\s+/g, " ").trim();
}

export function createSlugger() {
  const used = new Set<string>();
  return (text: string) => {
    const base = slugify(text) || "section";
    let id = base;
    for (let count = 1; used.has(id); count++) id = `${base}-${count}`;
    used.add(id);
    return id;
  };
}

function readMarkdown(file: string): Markdown {
  const cached = markdown.get(file);
  if (cached) return cached;
  const src = fs.readFileSync(path.join(ROOT, file), "utf8");
  const tokens = lexer.lexer(src);
  const headings = new Map<Tokens.Heading, Heading>();
  const anchors = new Set<string>();
  const slug = createSlugger();
  lexer.walkTokens(tokens, (token) => {
    if (token.type === "heading") {
      const heading = token as Tokens.Heading;
      const label = plain(heading.text);
      const id = slug(label);
      headings.set(heading, { id, label, depth: heading.depth });
      anchors.add(id);
    }
    if (token.type === "html") {
      for (const match of token.raw.matchAll(/\s(?:id|name)=["']([^"']+)["']/g)) anchors.add(match[1]);
    }
  });
  const result = { tokens, headings, anchors };
  markdown.set(file, result);
  return result;
}

export function rewriteHref(href: string, source: string): string {
  // Repo Markdown links to site pages with the full origin so GitHub readers get a working link; on the site it stays internal.
  if (href.startsWith(`${SITE_ORIGIN}/`)) {
    const route = href.slice(SITE_ORIGIN.length).split("#")[0];
    // Legacy pages such as /evidence.html are not app routes, so they stay ordinary external links.
    if (routes.has(route) || docByRoute.has(route)) href = href.slice(SITE_ORIGIN.length);
  }
  if (/^(https?:|mailto:)/i.test(href)) return href;
  if (/^[a-z][a-z\d+.-]*:|^\/\//i.test(href)) throw new Error(`${source}: unsupported link ${href}`);
  const [destination, fragment] = href.split("#", 2);
  const anchor = fragment === undefined ? "" : decodeURIComponent(fragment);
  const checkAnchor = (file: string) => {
    if (anchor && !readMarkdown(file).anchors.has(anchor)) throw new Error(`${source}: missing anchor #${anchor} in ${file}`);
  };
  if (!destination) {
    checkAnchor(source);
    return href;
  }
  if (destination.startsWith("/")) {
    const doc = docByRoute.get(destination);
    if (doc) checkAnchor(`docs/${doc.file}`);
    else if (!routes.has(destination) || anchor) throw new Error(`${source}: missing site target ${href}`);
    return href;
  }

  const file = path.posix.normalize(path.posix.join(path.posix.dirname(source), decodeURIComponent(destination)));
  if (file.startsWith("../") || !fs.existsSync(path.join(ROOT, file))) {
    throw new Error(`${source}: missing file ${file}`);
  }
  if (file.endsWith(".md")) checkAnchor(file);
  const doc = docByFile.get(file);
  if (doc) return `/docs/${doc.slug}/${fragment ? `#${fragment}` : ""}`;
  const type = fs.statSync(path.join(ROOT, file)).isDirectory() ? "tree" : "blob";
  return `${REPO}/${type}/main/${file.split("/").map(encodeURIComponent).join("/")}${fragment ? `#${fragment}` : ""}`;
}

export type RenderedDoc = {
  meta: DocMeta; title: string; titleId?: string; html: string; diagram: string | null;
  toc: Heading[]; search: SearchEntry[]; description: string;
};

const rendered = new Map<string, RenderedDoc>();
export function renderDoc(meta: DocMeta): RenderedDoc {
  const cached = rendered.get(meta.slug);
  if (cached) return cached;
  const file = `docs/${meta.file}`;
  const { tokens, headings } = readMarkdown(file);
  const toc: Heading[] = [];
  const titleHeading = [...headings.values()].find((heading) => heading.depth === 1);
  const title = titleHeading?.label ?? meta.label;
  let diagram: string | null = null;
  const marked = new Marked({
    gfm: true,
    renderer: {
      heading(token: Tokens.Heading) {
        const { id, label, depth } = headings.get(token)!;
        if (id === titleHeading?.id) return "";
        if (depth === 2 || depth === 3) toc.push({ id, label, depth });
        return `<h${depth} id="${esc(id)}"><a href="#${esc(id)}" class="anchor">${this.parser.parseInline(token.tokens)}</a></h${depth}>`;
      },
      link({ href, title: t, tokens }: Tokens.Link) {
        const to = rewriteHref(href, file);
        const ext = /^https?:/.test(to);
        const hash = to.match(/^https:\/\/indexer-[ab]\.ootle\.tari\.com\/(?:substates|transactions)\/((?:(?:component|template)_)?[a-f0-9]{64})(?:\/result)?$/)?.[1];
        if (hash && tokens.length === 1 && tokens[0].type === "codespan") {
          return `<span class="inline-flex items-baseline gap-1 font-mono text-[13px]"><a href="${esc(to)}" title="${hash}" target="_blank" rel="noreferrer">${shortHash(hash)}<span class="sr-only"> (opens the indexer record)</span></a><button type="button" data-copy-hash="${hash}" aria-label="Copy ${hash}" class="rounded-sm px-1 text-muted hover:text-ink">⧉</button></span>`;
        }
        return `<a href="${esc(to)}"${t ? ` title="${esc(t)}"` : ""}${ext ? ' target="_blank" rel="noreferrer"' : ""}>${this.parser.parseInline(tokens)}${ext ? '<span class="sr-only"> (opens in a new tab)</span>' : ""}</a>`;
      },
      code({ text, lang }: Tokens.Code) {
        const l = (lang ?? "").split(/\s/)[0];
        if (l === "mermaid") {
          if (diagram !== null) throw new Error(`${file}: expected a single diagram`);
          diagram = text;
          return '<div data-mermaid tabindex="0" role="region" aria-label="Payment sequence diagram"></div>';
        }
        return `<pre tabindex="0" data-lang="${esc(l || "text")}"><code>${esc(text)}</code></pre>`;
      },
      table(token: Tokens.Table) {
        const head = token.header.map((cell) => `<th scope="col">${this.parser.parseInline(cell.tokens)}</th>`).join("");
        const rows = token.rows.map((row) => `<tr>${row.map((cell) => `<td>${this.parser.parseInline(cell.tokens)}</td>`).join("")}</tr>`).join("");
        return `<div class="table-wrap" tabindex="0" role="region" aria-label="Scrollable table"><table><thead><tr>${head}</tr></thead><tbody>${rows}</tbody></table></div>`;
      },
    },
  });
  const html = marked.parser(tokens);
  const search: SearchEntry[] = [{ slug: meta.slug, title, text: "" }];
  let current = search[0];
  for (const token of tokens as Token[]) {
    if (token.type === "heading") {
      const h = headings.get(token as Tokens.Heading)!;
      if (h.id !== titleHeading?.id) {
        current = { slug: meta.slug, title, id: h.id, heading: h.label, text: "" };
        search.push(current);
      }
    } else if (token.type !== "space") {
      current.text += (current.text ? " " : "") + searchText(token.raw);
    }
  }
  const description = search[0].text.match(/^.{1,160}(?=\s|$)/)?.[0] || `${meta.label} for Ghostline on Tari Ootle.`;
  const result = { meta, title, titleId: titleHeading?.id, html, toc, search, diagram, description };
  rendered.set(meta.slug, result);
  return result;
}

export function searchIndex(): SearchEntry[] {
  return DOCS.flatMap((doc) => renderDoc(doc).search);
}

export function checkDocs() {
  for (const doc of DOCS) {
    const file = `docs/${doc.file}`;
    const { tokens } = readMarkdown(file);
    lexer.walkTokens(tokens, (token) => {
      if (token.type === "link" || token.type === "image") rewriteHref((token as Tokens.Link).href, file);
      if (token.type === "html") {
        for (const match of token.raw.matchAll(/\shref=["']([^"']+)["']/g)) rewriteHref(match[1], file);
      }
    });
    renderDoc(doc);
  }
}
