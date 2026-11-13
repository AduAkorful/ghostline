# Ghostline mainnet site

The Next.js app exports HTML, JavaScript, and static assets to `out/`. It reads deployment values from `../deploy/mainnet.json`, the dated mainnet run records from `data/mainnet-proof.json`, the MCP tools from Rust, and the docs from `../docs/`. The fee calculator imports the same integer fee code used by the TypeScript SDK.

## Local build

Use Node.js 22. From the repository root:

```sh
npm ci --prefix app
npm test --prefix app
npm run build --prefix app
python3 -m http.server 3110 --directory app/out
```

The export includes `/demo/`, the original contest assets and SDK bundle, and the `/play.html` redirect. To preview the Next.js pages with hot reload, use `npm run dev --prefix app`. Stop the app's dev server before building the export.

Update the proof file only from recorded transactions and verified receipts. The run 2 settle and run 3 refund fees come from the mainnet indexer transaction summaries. To regenerate the CLI reference after a command change, run `node app/scripts/generate-cli-doc.mjs ./target/release/ghostline` from the repository root using a current CLI build.

## Vercel

For the cutover, set the existing `ghostline-ui` project’s Root Directory to `app` once the site files are available to the Git build. Enable **Include source files outside of the Root Directory in the Build Step** so Vercel can read the sibling docs, deployment, SDK, and demo files. Use the **Other** preset, Node.js 22.x, `npm ci` for install, `npm run build` for build, and `out` for output.

Keep the current production `GHOSTLINE_RELAY_URL` and `ghostline-ui.vercel.app` domain so the frozen Esmeralda relay's allowlist still matches. The build copies the frozen demo from `../demo/ui/demo/` and uses the configured public relay origin. Without that setting, the local export offers the frozen simulation.
