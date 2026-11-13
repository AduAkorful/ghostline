// Pure search over the site index (no DOM, no Node APIs), shared by the dialog and the tests.
// - identifiers are split, so "policy budget" finds GHOSTLINE_POLICY_BUDGET and "payout destination" finds payout-destination
// - every word must match somewhere (headings weigh most), as a whole word, a prefix, a fragment or one typo away
import type { IndexEntry } from "./docs-meta.ts";

export type Prepared = IndexEntry & { l: string; h: string; p: string; g: string; t: string; hw: string[]; pw: string[]; tw: string[] };
export type Result = { entry: IndexEntry; score: number };

const SEPARATORS = /[\s_\-./:()[\]{}<>=,;"'`*|\\#?!@+~^&$%]+/g;
export const normalize = (s: string) => s.toLowerCase().replace(SEPARATORS, " ").trim();
const STOP = new Set(["a", "an", "the", "to", "of", "and", "or", "in", "on", "for", "is", "do", "i", "how", "can", "my", "with", "it", "what", "does", "after", "before", "from", "are", "be", "you", "your", "this", "that"]);

export function tokensOf(query: string): string[] {
  const all = normalize(query).split(" ").filter(Boolean);
  const meaningful = all.filter((t) => !STOP.has(t));
  return [...new Set(meaningful.length ? meaningful : all)];
}

export function prepare(entries: IndexEntry[]): Prepared[] {
  return entries.map((e) => {
    const h = normalize(e.heading ?? ""), p = normalize(e.page), g = normalize(e.group), t = normalize(e.text);
    return { ...e, l: e.text.toLowerCase(), h, p, g, t, hw: h.split(" ").filter(Boolean), pw: p.split(" "), tw: [...new Set(t.split(" ").filter(Boolean))] };
  });
}

/** True when `a` and `b` differ by at most one insertion, deletion, substitution or adjacent swap. */
export function withinOneEdit(a: string, b: string): boolean {
  if (a === b) return true;
  const d = a.length - b.length;
  if (Math.abs(d) > 1) return false;
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  if (d === 0) {
    if (a.slice(i + 1) === b.slice(i + 1)) return true;
    return a[i] === b[i + 1] && a[i + 1] === b[i] && a.slice(i + 2) === b.slice(i + 2);
  }
  return d > 0 ? a.slice(i + 1) === b.slice(i) : a.slice(i) === b.slice(i + 1);
}

/** Match strength of one query word in one field: whole word 1, prefix .7, fragment .35, typo .45, none 0. */
function strength(token: string, field: string, words: string[]): number {
  if (!field) return 0;
  const padded = ` ${field} `;
  if (padded.includes(` ${token} `)) return 1;
  if (padded.includes(` ${token}`)) return 0.7;
  if (token.length >= 3 && field.includes(token)) return 0.35;
  if (token.length >= 5) for (const w of words) if (w[0] === token[0] && withinOneEdit(token, w)) return 0.45;
  return 0;
}

function occurrences(token: string, text: string, cap: number): number {
  let n = 0, from = 0;
  while (n < cap) {
    const i = text.indexOf(token, from);
    if (i < 0) break;
    n++;
    from = i + token.length;
  }
  return n;
}

export function search(index: Prepared[], query: string, limit = 12, perPage = 3): Result[] {
  const tokens = tokensOf(query);
  if (!tokens.length) return [];
  const phrase = tokens.join(" ");
  const scored: Result[] = [];
  for (const e of index) {
    let score = 0, matchedAll = true;
    for (const tok of tokens) {
      const inHeading = strength(tok, e.h, e.hw), inPage = strength(tok, e.p, e.pw), inGroup = strength(tok, e.g, [e.g]);
      const inText = strength(tok, e.t, e.tw);
      if (!inHeading && !inPage && !inGroup && !inText) { matchedAll = false; break; }
      score += inHeading * 10 + inPage * 5 + inGroup * 1 + inText * (2 + Math.min(3, occurrences(tok, e.t, 6) * 0.4));
    }
    if (!matchedAll) continue;
    if (tokens.length > 1) {
      if (e.h.includes(phrase)) score += 14;
      if (e.p.includes(phrase)) score += 8;
      if (e.t.includes(phrase)) score += 4;
    }
    // "policy budget" is a strong hit on GHOSTLINE_POLICY_BUDGET, which people copy straight from a config file.
    if (tokens.length > 1 && (e.l.includes(tokens.join("_")) || e.l.includes(tokens.join("-")))) score += 12;
    if (e.h.startsWith(phrase)) score += 5;
    // A query that names a page ("mcp", "threat") lands on that page before a section elsewhere that mentions it.
    if (!e.heading && tokens.every((t) => strength(t, e.p, e.pw) >= 0.7)) score += 14;
    scored.push({ entry: e, score: score * (e.weight ?? 1) });
  }
  scored.sort((a, b) => b.score - a.score);
  const seen = new Map<string, number>(), out: Result[] = [];
  for (const r of scored) {
    const page = r.entry.href.split("#")[0], n = seen.get(page) ?? 0;
    if (n >= perPage) continue;
    seen.set(page, n + 1);
    out.push(r);
    if (out.length >= limit) break;
  }
  return out;
}

export type Part = { text: string; hit: boolean };

const escapeRegExp = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Splits `text` into plain and highlighted parts for the query's words. */
export function highlight(text: string, query: string): Part[] {
  const tokens = tokensOf(query).filter((t) => t.length > 1);
  if (!tokens.length || !text) return [{ text, hit: false }];
  const re = new RegExp(`(${tokens.map(escapeRegExp).join("|")})`, "gi");
  return text.split(re).filter(Boolean).map((part, i) => ({ text: part, hit: i % 2 === 1 }));
}

/** A short excerpt around the first match, with the match highlighted. */
export function snippet(text: string, query: string, width = 150): Part[] {
  const tokens = tokensOf(query).filter((t) => t.length > 1);
  const lower = text.toLowerCase();
  let at = -1;
  for (const t of tokens) {
    const i = lower.indexOf(t);
    if (i >= 0 && (at < 0 || i < at)) at = i;
  }
  let start = Math.max(0, at - 50);
  if (start > 0) { const space = text.indexOf(" ", start); if (space >= 0 && space < at) start = space + 1; }
  let end = Math.min(text.length, start + width);
  if (end < text.length) { const space = text.lastIndexOf(" ", end); if (space > start + 40) end = space; }
  const parts = highlight(text.slice(start, end), query);
  if (start > 0) parts.unshift({ text: "…", hit: false });
  if (end < text.length) parts.push({ text: "…", hit: false });
  return parts;
}
