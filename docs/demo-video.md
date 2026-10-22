# Ghostline contest video capture plan

This is a recording script, not a video asset. Target length: 90–120 seconds, 1080p, captions or a short voiceover. Use the current v0.4.0 release and one newly verified Esmeralda run. Keep the terminal and explorer visible together; never show seed words, wallet credentials, secret key files, deposit masks, or voucher bodies.

## Capture sequence

| Time | Show | Narration |
|---|---|---|
| 0:00–0:10 | Landing page and browser demo CTA | “Ghostline is an agent payment rail on Tari Ootle: many signed off-chain vouchers, one channel close.” |
| 0:10–0:25 | Browser simulation: make five quote-and-pay clicks | “This zero-install page runs the real TypeScript voucher signing and verification locally. These clicks do not submit a transaction.” |
| 0:25–0:40 | Browser evidence panel and open the separate transaction link | “The link is a separate finalized Esmeralda run. It is evidence of the tested transaction fields, not a result of the simulation.” |
| 0:40–1:00 | Terminal: `ghostline demo --show-chain-evidence` | “The prebuilt CLI runs the HTTP 402 quote, signs cumulative vouchers, verifies paid responses, and exits without a wallet or chain transaction.” |
| 1:00–1:25 | Show a fresh verified Esmeralda transaction and its two output commitments | “In a real channel run, the chain confirms the close. The tested fields hide the deposit and exact output amounts; fees, timing, branch, metadata, and commitments remain visible.” |
| 1:25–1:40 | Service ledger and privacy limits | “The service sees calls, prices, vouchers, and totals. Its ledger is not chain proof of the call count.” |
| 1:40–1:55 | End card: repo, release, MIT, testnet-only | “Ghostline is unaudited and testnet-only. Funding and timing can still link activity.” |

## Evidence to capture

- CLI version, source commit, template hash, and date.
- Five successful browser simulation calls and the explicit simulation disclosure.
- A fresh channel-open and finalized settlement from a separately run Esmeralda flow, with matching explorer links.
- The exact public fields that were inspected, without implying that fees or all transaction metadata are hidden.
- A visible notice that the browser simulation and historical proof are separate.

Do not splice the old 1,000-call run into a newer transaction proof. Do not imply a live Sapient path exists until the full funding, open, settlement, and recovery lifecycle is implemented and tested. A public EVM transaction can be shown only as a clearly labelled illustration, not as a matched experiment.
