import { cp, mkdir, readdir, rm, writeFile } from "node:fs/promises";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const source = dirname(fileURLToPath(import.meta.url));
const output = `${source}${sep}dist`;
const configuredUrl = (process.env.GHOSTLINE_RELAY_URL || "").trim();
let relayUrl = "";

if (configuredUrl) {
  let parsed;
  try {
    parsed = new URL(configuredUrl);
  } catch {
    throw new Error("GHOSTLINE_RELAY_URL must be a valid HTTPS origin.");
  }
  if (parsed.protocol !== "https:" || parsed.username || parsed.password || parsed.pathname !== "/" || parsed.search || parsed.hash) {
    throw new Error("GHOSTLINE_RELAY_URL must be an HTTPS origin without credentials, path, query, or fragment.");
  }
  relayUrl = parsed.origin;
}

await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });
for (const entry of await readdir(source)) {
  if (["dist", "build.mjs", "README.md"].includes(entry) || entry.endsWith(".test.mjs")) continue;
  await cp(join(source, entry), join(output, entry), {
    recursive: true,
    filter(path) {
      const rel = relative(source, path);
      return !rel.endsWith(".test.mjs");
    },
  });
}
await writeFile(
  `${output}${sep}relay-config.js`,
  `window.GHOSTLINE_CONFIG = Object.freeze(${JSON.stringify({ relayUrl })});\n`,
  { mode: 0o644 },
);

console.log(`Built static UI in dist; live relay ${relayUrl ? "configured" : "not configured"}.`);
