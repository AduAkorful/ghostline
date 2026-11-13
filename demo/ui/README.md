# Ghostline motion demo

The Ghostline site includes a browser simulation and an optional live Esmeralda Sapient flow. The live page asks the visitor to approve a small private testnet transfer; the configured relay opens and closes the channel. The static frontend contains no wallet credentials. Its motion demo is illustrative, not a live feed or a replay of per-call data.

## The /demo page (frozen Esmeralda contest demo)

`demo/` holds the Spooky Secrets demo as entered, apart from its paths and a retiring note: `index.html` (served at `/demo/`), `play.js`, `play.css`, `sim-service.js`, `ephemeral-keys.js` and its own copy of the TypeScript SDK in `demo/vendor/ghostline.js`. It talks only to the Render relay, which runs the old build against the 7 Nov Esmeralda template (Render auto-deploy is off). The current code uses a different settle shape (three outputs, protocol fee) and 100 µT prices, so it cannot drive that template.

- Do not rebuild or replace `demo/vendor/ghostline.js`. `npm run build:browser --prefix packages/ghostline` now writes to `packages/ghostline/dist/` and never touches it.
- Do not redeploy the Render relay from `main` while the demo is up, and keep the site on `ghostline-ui.vercel.app` (the relay's CORS allowlist).
- `/play.html` redirects to `/demo/` so links shared during the contest keep working.
- Remove `demo/`, `play.html` and the relay after the contest winners are announced.

## Static hosting

The frontend is static, but live Sapient mode needs the public relay URL at build time. In the Vercel project, set **Root Directory** to `demo/ui`, **Framework Preset** to `Other`, **Build Command** to `node build.mjs`, and **Output Directory** to `dist`. Set the Production environment variable `GHOSTLINE_RELAY_URL` to the Render service's HTTPS origin, then redeploy. The build writes that public URL into `relay-config.js`; it is intentionally visible to site visitors and must not contain credentials. If it is unset, the browser simulation remains available and the live button is disabled.

The Render service must allow the deployed Vercel site's origin in `GHOSTLINE_CORS_ORIGINS`. The checked-in Blueprint already allows `https://ghostline-ui.vercel.app`.

Visitors can try the local, no-funds flow from a checkout:

```bash
bash scripts/quickstart-smoke.sh
```

The first Rust build can take several minutes. It creates temporary local keys, simulates three paid HTTP calls against a loopback API, prints a summary, then deletes its temporary state. It does not use Esmeralda or submit transactions. For live testnet use, follow the [Esmeralda quickstart](../../docs/quickstart.md) and use a wallet the visitor controls.

From `demo/ui`, start a local server (the demo page uses root-relative paths, so serve this folder as the site root):

```bash
python3 -m http.server 8000
```

Then open <http://127.0.0.1:8000/>; the demo is at <http://127.0.0.1:8000/demo/>. The landing page links to native Ghostline-styled guides at `docs/install.html`, `docs/quickstart.html`, and `docs/interfaces.html`. The guides summarize common user flows; the Markdown in the repository remains the canonical technical documentation, with full references linked from the relevant guide.

The evidence section reads sanitized Esmeralda proof records from [`evidence.json`](evidence.json), and [`evidence.html`](evidence.html) lists every record with its explorer status. On 7 Nov 2026 the channel template was published with `tari_template_lib` 0.38.0. The public indexer drops transaction bodies for about 50 epochs; `explorerAvailability` records the current publish. Fees stay public. See the [terminal demo runbook](../../docs/demo-runbook.md) for more detail.

Run the site tests from the repository root with `node --test demo/ui/demo/mock-wallet.test.mjs demo/ui/site.test.mjs`.
