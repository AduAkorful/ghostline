# Ghostline contest demo video

Capture a single real Esmeralda run after the fixed-shape template is published. Target length: 2:30. Keep the terminal and explorer side by side; use a fresh one-shot channel and never show key files, wallet RPC credentials, masks, or voucher bodies.

## Capture sequence and narration

| Time | Show | Narration |
|---|---|---|
| 0:00–0:15 | Title card, then Ghostline landing page | “Ghostline is a private pay-per-call rail for AI agents on Tari Ootle: the service sees each request, while the chain sees a channel and one close.” |
| 0:15–0:35 | Installed CLI, `ghostline --help`, and the payer/service key setup with secrets hidden | “The payer and service use fresh keys for this one-settlement channel. Policy limits spend and can require a human approval.” |
| 0:35–1:10 | Open a pre-funded channel, then the 402 quote; make 20 paid `/fortune` requests | “The agent checks the quote's channel and recipient, signs cumulative vouchers, and receives 20 responses. Those calls and prices remain in the service's records.” |
| 1:10–1:35 | Service ledger shows 20 accepted calls and the cumulative total; no voucher or key values | “The service accepted twenty calls. The ledger is local evidence for the service's accounting; the chain does not prove the call count.” |
| 1:35–2:05 | Settle once; wait for Finalized Commit; open that transaction in Tari Explorer | “The latest voucher closes the channel in one settlement. The transaction exposes fees, timing, branch, metadata, and commitments, while its claim-free fields do not show the deposit or exact output amounts.” |
| 2:05–2:20 | Show the two output commitments/shape and the fixed-shape note | “This version always emits two outputs and leaves one micro-Tari as payer change, so output count no longer reveals whether the deposit was fully spent.” |
| 2:20–2:30 | End card with GitHub, evidence page, and testnet warning | “Ghostline is unaudited and testnet-only. The service still sees the calls; funding and timing may still link activity.” |

## Evidence shown in the video

- Fresh channel-open transaction id and explorer URL.
- Twenty successful paid HTTP responses and the service ledger's call count/accepted total.
- Settlement transaction id, indexer `Finalized Commit`, and explorer URL.
- Screenshot or split-screen comparison with a public transfer example, labelled as different transaction designs rather than a matched same-network experiment.
- The exact binary version, source commit, template address/hash, and run date in the end card or video description.

Do not splice the old 1,000-call run into the new settlement proof. It predates the amount-masked template and does not prove a 20-call fixed-shape settlement. Do not claim that the settlement proves call count or hides fees, timing, branch, funding links, or transaction metadata.
