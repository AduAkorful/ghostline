import { ImageResponse } from "next/og";
import { socialCards, type SocialCard } from "@/lib/social";

export const dynamic = "force-static";
export const dynamicParams = false;

export function generateStaticParams() {
  return Object.keys(socialCards).map((key) => ({ card: `${key}.png` }));
}

export async function GET(_request: Request, { params }: { params: Promise<{ card: string }> }) {
  const { card } = await params;
  const [title, subtitle] = socialCards[card.slice(0, -4) as SocialCard];
  return new ImageResponse(
    <div style={{ display: "flex", flexDirection: "column", background: "#0b0c11", color: "#ece7dc", width: "100%", height: "100%", padding: "75px 85px" }}>
      <div style={{ display: "flex", fontSize: 30, color: "#7fe7df" }}>Ghostline · Tari Ootle mainnet</div>
      <div style={{ display: "flex", marginTop: 58, fontSize: 68 }}>{title}</div>
      <div style={{ display: "flex", marginTop: 4, fontSize: 48, color: "#c4b2ff" }}>{subtitle}</div>
      <div style={{ display: "flex", flexDirection: "column", marginTop: 42, gap: 14 }}>
        {[1, 2, 3].map((row) => <div key={row} style={{ width: 460, borderTop: "1px solid #c4b2ff", opacity: 0.35 }} />)}
        <div style={{ width: 460, borderTop: "2px solid #7fe7df" }} />
      </div>
    </div>,
    { width: 1200, height: 630 },
  );
}
