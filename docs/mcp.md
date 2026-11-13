# MCP server

`ghostline-mcp` runs locally over stdio. Use the same build as the CLI. Prepare the channel record and the appropriate wallet with the [mainnet quickstart](mainnet-quickstart.md#agent-path-mcp).

## Connect your host

Put this shape in Claude Desktop's `claude_desktop_config.json` or the project's `.mcp.json` for Claude Code. Replace the paths and the public service key. The MCP host must be able to start the executable and read the files.

```json
{
  "mcpServers": {
    "ghostline": {
      "command": "/absolute/path/to/ghostline-mcp",
      "env": {
        "GHOSTLINE_CHANNEL_FILE": "/private/run/channel.json",
        "GHOSTLINE_WALLET_COOKIE": "/private/payer-wallet/mainnet/.cookie",
        "GHOSTLINE_POLICY_BUDGET": "5000",
        "GHOSTLINE_POLICY_MAX_PER_CALL": "1000",
        "GHOSTLINE_POLICY_VENDOR_HEX": "<service-public-key>"
      }
    }
  }
}
```

Supply the signing secret through the local server process environment. Give the payer and service servers separate wallet cookies. Keep submit flags unset until you have reviewed the relevant quote. Use a host with elicitation support for human [approval rules](policy.md).

## Process settings

| Setting | Role |
|---|---|
| `GHOSTLINE_CHANNEL_FILE` | Channel record used by open, close, session creation and file status. |
| `GHOSTLINE_CHANNEL_DIR` | Channel records for list and accepted-call exports. |
| `GHOSTLINE_WALLET_COOKIE` | Cookie for the local wallet daemon performing the operation. |
| `GHOSTLINE_PAYER_SECRET` | Payer signing secret for paid calls and voucher signing. |
| `GHOSTLINE_CLAIM_SECRET` | Claim secret for opening when the channel record does not supply its own claim key. |
| `GHOSTLINE_SESSION` | Explicit payer session path. Paid calls can also use the channel record's session. |
| `GHOSTLINE_SESSION_DIR` | Directory for resolving paid-call sessions by quoted channel id. |
| `GHOSTLINE_OPEN_MAX_FEE` | Reviewed opening fee ceiling in µT. |
| `GHOSTLINE_OPEN_SUBMIT` | Set to `1` to allow opening on-chain. |
| `GHOSTLINE_OPEN_TIMEOUT_SECS` | Wallet open result timeout. |
| `GHOSTLINE_LEDGER` | Optional service ledger for session status. |
| `GHOSTLINE_EXPIRY_EPOCH`, `GHOSTLINE_INDEXER_URL` | Optional status settings when inspecting a session. |
| `GHOSTLINE_SEAL_KEY_ID_FILE` | Wallet signer reference for the service on settle or the payer on refund. |
| `GHOSTLINE_CLOSE_DRY_RUN` | Set to `1` to quote the close without broadcasting. |
| `GHOSTLINE_CLOSE_SUBMIT` | Set to `1` to allow live close. |
| `GHOSTLINE_CLOSE_MAX_FEE` | Fee cap in µT, covered by the revealed fee balance. |
| `GHOSTLINE_CLOSE_TIMEOUT_SECS` | Wallet close result timeout. |
| `GHOSTLINE_SETTLE_MARGIN` | Settlement window in epochs before expiry. |
| `GHOSTLINE_SETTLE_FORCE` | Set to `1` to settle early while settlement remains eligible. |
| `GHOSTLINE_POLICY_UNSAFE` | Explicit policy bypass. Leave unset for agent use. |

See [Spending policy](policy.md#set-the-limits) for the policy and audit settings.

## Tools

The host discovers each tool's argument schema over MCP. The examples below show the main input and the result to look for.

| Tool | Input | Result |
|---|---|---|
| `ghostline_call_paid` | `{"url":"http://127.0.0.1:8787/fortune"}` | HTTP status and body, with `payment_required` and `needs_channel` for follow-up. |
| `ghostline_sign_voucher` | Public channel parameters and a price | Hex-encoded voucher from the configured session. |
| `ghostline_open_channel` | `{}` | Open quote or submitted open, according to the server settings. |
| `ghostline_close_channel` | `{}` | Wait, settle, refund, or spent output, with dry-run or submission details when applicable. |
| `ghostline_channel_session` | `{}` | Creates or reuses the file's payer session. |
| `ghostline_channel_status` | `{}` for configured defaults | Off-chain totals and available observer/status fields. |
| `ghostline_channel_list` | `{}` | One row per channel file in the configured directory. |
| `ghostline_channel_calls` | `{}` | Accepted-call rows from the directory's ledgers. |
| `ghostline_channel_file` | `{}` | Totals, expiry window and available output presence for the configured record. |
| `ghostline_channel_id` | Public open parameters | Channel id as hex. |
| `ghostline_verify_voucher` | Channel parameters and voucher hex | `{"ok":true}` if verification succeeds. |
| `ghostline_inspect_tx` | Sealed transaction hex | Public stealth-output fields. |
| `ghostline_unblind_tx` | Sealed transaction hex | Value and mask for matching outputs. Keep results private. |

## Walk through run 5

On 13 Nov 2026 the payer server quoted `ghostline_open_channel {}` and submitted after the operator enabled `GHOSTLINE_OPEN_SUBMIT=1`. Five calls at 1,000 µT succeeded. The sixth exceeded the 5,000 µT policy budget. The service server used its own wallet cookie and signer with `ghostline_close_channel {}` to settle. The finalized settlement paid 28,808 µT in network fees.

Follow the [mainnet agent walkthrough](mainnet-quickstart.md#agent-path-mcp) for the exact sequence and view the open and settle records under [Mainnet proof](https://ghostline-ui.vercel.app/proof/).
