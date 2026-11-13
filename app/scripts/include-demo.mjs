import { cp, readFile, writeFile } from "node:fs/promises";

const legacy = new URL("../../demo/ui/", import.meta.url);
const output = new URL("../out/", import.meta.url);
const files = [
  "demo/", "docs/", "play.html", "evidence.html", "evidence.json", "evidence-view.css",
  "evidence-view.js", "ghostline-mark.svg", "styles.css",
];
for (const file of files) {
  await cp(new URL(file, legacy), new URL(file, output), {
    recursive: true,
    filter: (path) => !path.endsWith(".test.mjs"),
  });
}

await cp(new URL("demo/play.css", legacy), new URL("play.css", output));

const configuredUrl = process.env.GHOSTLINE_RELAY_URL?.trim();
const relay = new URL("relay-config.js", output);
if (configuredUrl) {
  const url = new URL(configuredUrl);
  if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
    throw new Error("GHOSTLINE_RELAY_URL must be an HTTPS origin without credentials, path, query, or fragment.");
  }
  await writeFile(relay, `window.GHOSTLINE_CONFIG = Object.freeze(${JSON.stringify({ relayUrl: url.origin })});\n`);
} else {
  await cp(new URL("relay-config.js", legacy), relay);
}

// The frozen page still links to the original landing anchors through /index.html.
const landing = new URL("index.html", output);
const html = await readFile(landing, "utf8");
for (const id of ["story", "evidence", "privacy", "start"]) {
  if (!html.includes(`id="${id}"`)) throw new Error(`Missing contest demo link target #${id}`);
}
