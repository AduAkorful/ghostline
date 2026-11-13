import RootLayout from "./layout";
import NotFound from "./not-found";
import { metadataBase, pageMetadata } from "@/lib/metadata";

export const metadata = {
  metadataBase,
  ...pageMetadata("Page not found · Ghostline", "The page you requested could not be found.", undefined, "site"),
  robots: { index: false },
};

export default function GlobalNotFound() {
  return <RootLayout><NotFound /></RootLayout>;
}
