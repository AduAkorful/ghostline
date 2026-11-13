# Spending policy

Set policy in the process that signs vouchers. Give the agent access to the MCP tools and keep the signing secret, channel files, and policy settings under operator control. The on-chain channel enforces the settleable limit even if the signing key is used outside the policy process.

## Set the limits

| Setting | Meaning |
|---|---|
| `GHOSTLINE_POLICY_BUDGET` | Maximum cumulative spend for the channel in µT. An amount equal to the budget is allowed. |
| `GHOSTLINE_POLICY_MAX_PER_CALL` | Maximum price of one call in µT. Set this with the budget. |
| `GHOSTLINE_POLICY_VENDOR_HEX` | Service authorization public key in hex. The signing path uses it as the allowed vendor. |
| `GHOSTLINE_POLICY_HUMAN_THRESHOLD` | Ask for consent on calls at or above this positive price in µT. |
| `GHOSTLINE_POLICY_APPROVE_NEW_VENDOR=1` | Ask on the first payment to a vendor. |
| `GHOSTLINE_POLICY_REQUIRE_HUMAN=1` | Ask on every payment. |
| `GHOSTLINE_POLICY_AUDIT_LOG` | Optional private JSONL file for decisions. A failed audit write prevents signing. |

A human approval cannot raise the budget or the per-call cap. The CLI asks in its terminal. MCP asks through elicitation, so choose a host that shows the prompt to an operator. If approval is unavailable or refused, the payment stops. The Rust policy API also supports a rate limit through `PolicyEngine::with_rate_limit`.

## Read the audit log

Each row records `unix_ms`, `approved`, `vendor_hex`, `price`, `already_owed`, and `reason`. Amounts are integer µT. On startup the engine uses approved rows to restore first-vendor history. Keep the file private because it contains vendor and spending information. An audit row records a signing decision. Use the service ledger to confirm a call was accepted.

In mainnet run 5 the budget was 5,000 µT and the per-call cap was 1,000 µT. Five calls at 1,000 µT were allowed. The sixth was refused with `cumulative 6000 exceeds policy budget 5000`.

See [MCP setup](mcp.md) and the [threat model](threat-model.md).
