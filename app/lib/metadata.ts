import type { Metadata } from "next";
import { SITE_ORIGIN } from "./docs-meta";
import { socialCards, type SocialCard } from "./social";

export const metadataBase = new URL(SITE_ORIGIN);

export function pageMetadata(title: string, description: string, path?: string, card?: SocialCard): Metadata {
  const route = path?.split("/")[1] || "site";
  const key = card ?? (Object.hasOwn(socialCards, route) ? route as SocialCard : "site");
  const image = `/social/${key}.png`;
  return {
    title,
    description,
    ...(path ? { alternates: { canonical: path } } : {}),
    openGraph: {
      title,
      description,
      siteName: "Ghostline",
      type: "website",
      ...(path ? { url: path } : {}),
      images: [{ url: image, width: 1200, height: 630, alt: `Ghostline · ${socialCards[key][0]}` }],
    },
    twitter: { card: "summary_large_image", title, description, images: [image] },
  };
}
