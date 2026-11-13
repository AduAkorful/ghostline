import { metadataBase, pageMetadata } from "@/lib/metadata";
import { IBM_Plex_Mono, IBM_Plex_Sans, Instrument_Serif } from "next/font/google";
import { Header } from "@/components/Header";
import { Footer } from "@/components/Footer";
import "./globals.css";

const instrument = Instrument_Serif({ subsets: ["latin"], weight: "400", style: ["normal", "italic"], variable: "--font-instrument" });
const plexSans = IBM_Plex_Sans({ subsets: ["latin"], weight: ["400", "500", "600"], variable: "--font-plex-sans" });
const plexMono = IBM_Plex_Mono({ subsets: ["latin"], weight: ["400", "500"], variable: "--font-plex-mono" });

export const metadata = {
  metadataBase,
  ...pageMetadata("Ghostline: private pay-per-call for AI agents", "Lock funds once, pay API calls with off-chain vouchers, and settle once on Tari Ootle mainnet.", "/"),
};

export const viewport = { colorScheme: "dark", themeColor: "#0b0c11" } as const;

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${instrument.variable} ${plexSans.variable} ${plexMono.variable}`}>
      <body className="min-h-screen flex flex-col">
        <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 bg-ink text-on-ink px-3 py-2">Skip to content</a>
        <Header />
        <main id="main" tabIndex={-1} className="flex-1">{children}</main>
        <Footer />
      </body>
    </html>
  );
}
