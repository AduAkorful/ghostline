import type { NextConfig } from "next";
import { fileURLToPath } from "node:url";

const config: NextConfig = {
  output: "export",
  experimental: { globalNotFound: true },
  trailingSlash: true,
  images: { unoptimized: true },
  transpilePackages: ["@ghostline/sdk"],
  turbopack: { root: fileURLToPath(new URL("..", import.meta.url)) },
};
export default config;
