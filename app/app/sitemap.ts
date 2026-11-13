import type { MetadataRoute } from "next";
import { DOCS, SITE_ORIGIN } from "@/lib/docs-meta";

export const dynamic = "force-static";

const PAGES = ["/", "/how-it-works/", "/privacy/", "/proof/", "/fees/", "/agents/", "/sellers/", "/docs/"];

export default function sitemap(): MetadataRoute.Sitemap {
  return [...PAGES, ...DOCS.map((d) => `/docs/${d.slug}/`)].map((path) => ({ url: `${SITE_ORIGIN}${path}` }));
}
