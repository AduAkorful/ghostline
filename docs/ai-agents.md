# Use with AI agents

Give a coding agent or chat assistant the Ghostline docs in a form it reads well, and install the Ghostline skill so it follows the safety rules when it helps you. None of this lets an agent spend: payments still go through the [MCP server](mcp.md) and your [spending policy](policy.md).

## Read the docs as Markdown

Every docs page is available as plain Markdown, and the whole set is one file.

| Resource | URL |
|---|---|
| Docs index | `https://ghostline-ui.vercel.app/llms.txt` |
| Full documentation in one file | `https://ghostline-ui.vercel.app/llms-full.txt` |
| One page as Markdown | `https://ghostline-ui.vercel.app/docs/<page>.md`, for example `/docs/policy.md` |
| The Ghostline skill | `https://ghostline-ui.vercel.app/skill.md` |

Each docs page also has a **Copy page** menu at the top. It copies the page as Markdown, opens the `.md` version, or opens the page in Claude or ChatGPT.

To use the docs in a chat assistant that has no skills, paste this and attach nothing else:

```text
Read https://ghostline-ui.vercel.app/llms.txt, then read the pages you need as Markdown. Help me set up Ghostline on Tari Ootle mainnet. Do not ask me for secrets, and do not open, settle or refund a channel unless I ask for that action.
```

## Install the skill

The skill is one `SKILL.md` file. It tells an agent what Ghostline is, which MCP tools exist, the money rules, and the safety rules: quote before submitting, never handle secrets in chat, never change the policy to get a payment through, and never claim more privacy than the chain provides.

For Claude Code, install it for your user:

```bash
mkdir -p ~/.claude/skills/ghostline && curl -fsSL https://ghostline-ui.vercel.app/skill.md -o ~/.claude/skills/ghostline/SKILL.md
```

For a single project, use `.claude/skills/ghostline/SKILL.md` inside the project instead. For any other agent that loads `SKILL.md` files, put the file in that agent's skills folder. Otherwise copy the file and paste it into the agent's instructions.

Read the file before you install it.

## What the skill does not do

- It does not give an agent a wallet or a key. Opening, settling and refunding stay behind the submit flags you set in the MCP server's environment.
- It does not replace the policy. The policy runs outside the agent and refuses calls over the budget; the skill only tells the agent to report a refusal and stop.
- It does not stay current by itself. It names tools, settings and fees as of the mainnet record on 13 Nov 2026; the linked pages are the source of truth.

## Connect payments

To let an agent pay for API calls, set up the [MCP server](mcp.md) and the [mainnet quickstart](mainnet-quickstart.md#agent-path-mcp).
