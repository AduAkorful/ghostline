# Ghostline contest video capture plan

This is a recording script, not a video asset. Target length is 80 seconds, with a hard maximum of 90 seconds. Captions are burned in, because many viewers watch muted; a short voiceover is optional.

## Before recording

- Record only after the observer view, overspend refusal and relay banner are deployed to <https://ghostline-ui.vercel.app>, and `/healthz` on the relay reports `walletReady: true`.
- Use a Sapient wallet on Esmeralda with fresh faucet funds, a clean Chrome profile with no other extensions visible, a 1920×1080 window, browser zoom 110%, and a dark OS theme.
- Never show seed words, passwords, the `secrets/` folder, voucher bodies, deposit masks or relay environment variables.
- Make a fresh live run the same day. Note its open and settlement transaction IDs; the ones on screen must be those, and they go into `demo/ui/evidence.json` and the Verified runs table in [`sapient.md`](sapient.md).

## Shot list

| Time | Screen | Action | Caption (8 words or fewer) |
|---|---|---|---|
| 0:00–0:05 | Landing page hero | Slow scroll | Ghostline: private pay-per-call for AI agents |
| 0:05–0:18 | Play page, simulation | Click "Run all 5 simulated calls" | 5 paid calls. Zero transactions. |
| 0:18–0:24 | Same page, right panel | The labelled recorded run and the observer view appear | Simulation, no wallet needed |
| 0:24–0:36 | Live section | "Connect Sapient and open channel", approve the 51 µT transfer in Sapient | Real wallet: Sapient on Esmeralda |
| 0:36–0:46 | Service panel | Five live calls tick up to 50 µT | Vouchers signed in your browser |
| 0:46–0:52 | Overspend button | Click "Try to overspend"; the refused row appears | Overspend? Refused. |
| 0:52–1:02 | Settle, then the observer view | Click "Settle live channel", then "Load this settlement's public fields" | One settlement. Amounts hidden. |
| 1:02–1:14 | Explorer, the new settlement | Show the commitments and the absent revealed amounts | Observers see commitments, not amounts |
| 1:14–1:20 | Observer view, "What still leaks" line | Hold | Fees and timing stay visible |
| 1:20–1:26 | End card | Static | github.com/AduAkorful/ghostline · ghostline-ui.vercel.app |

The overspend signs a voucher for the whole 51 µT deposit. That is 1 µT past the 50 µT spend limit, because the template keeps 1 µT for the fixed two-output settlement. The service answers "voucher exceeds channel spend limit (51 > 50)". Show that message as it appears; do not describe it as an on-chain rejection.

## Export

- MP4: H.264 High, 1920×1080, 30 fps, CRF 20, AAC 128 kbps or silent, `-movflags +faststart`, at most 15 MB: `demo/ui/media/ghostline-demo.mp4`.
- Poster: 1280×720 frame from 0:52 showing the observer view: `demo/ui/media/ghostline-demo-poster.jpg`.
- Captions: `demo/ui/media/ghostline-demo.vtt`.
- GIF for the README and forum: 0:05–0:24 plus 0:46–1:02, at most 20 s, 960 px wide, 12 fps, at most 8 MB: `demo/ui/media/ghostline-demo.gif`.

  ```sh
  ffmpeg -i ghostline-demo.mp4 -vf "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse" ghostline-demo.gif
  ```

- Upload an unlisted YouTube copy for the forum and Reddit.

## Embedding

The static build copies `demo/ui/media/` as is. Once the files exist:

- Landing page, first child of the `#judge-path` section:

  ```html
  <video controls muted playsinline preload="metadata" poster="media/ghostline-demo-poster.jpg"><source src="media/ghostline-demo.mp4" type="video/mp4"><track kind="captions" src="media/ghostline-demo.vtt" srclang="en" default></video>
  ```

- Play page: a "Watch the 80-second run" link under the demo heading.
- README: the GIF directly under the title, linking to the video.
- `demo/ui/site.test.mjs` fails if the landing page references a `media/` file that is missing.
