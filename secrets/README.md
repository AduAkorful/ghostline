# Secrets (not committed)

This directory is gitignored except this file. Put local key material under `secrets/esmeralda/` (or another subdirectory). Do not commit it.

Wallet-built stealth outputs are encrypted to the wallet `view_only_key` (derived branch, index 0), not the Ghostline payer key and not the account spend key. Store that as `GHOSTLINE_CLAIM_SECRET` locally; do not put it in tracked docs.
