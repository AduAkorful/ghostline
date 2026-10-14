# Secrets (not committed)

This directory is gitignored except this file. Put local key material under `secrets/esmeralda/` (or another subdirectory). Do not commit it.

Wallet-built stealth outputs are encrypted to the wallet view key. Store that claim secret as `GHOSTLINE_CLAIM_SECRET` locally; do not put it in tracked docs.
