import { readFileSync, writeFileSync } from "node:fs";

const read = (file) => readFileSync(new URL(file, import.meta.url), "utf8");
const server = read("../../crates/ghostline-mcp/src/lib.rs");
const quickstart = read("../../docs/quickstart.md");
const policyGuide = read("../../docs/policy.md");
const policyCode = read("../../crates/ghostline-policy/src/lib.rs") +
  read("../../crates/ghostline-policy/src/engine.rs");

const tools = Object.fromEntries(
  [...server.matchAll(/#\[tool\(\s*description\s*=\s*("(?:[^"\\]|\\.)*")\s*\)\]\s*(?:async )?fn (ghostline_\w+)/g)]
    .map(([, text, name]) => [name, JSON.parse(text)]),
);
if (!Object.keys(tools).length) throw new Error("No MCP tool descriptions found in Rust");

const examples = {
  GHOSTLINE_POLICY_BUDGET: "5000",
  GHOSTLINE_POLICY_MAX_PER_CALL: "1000",
  GHOSTLINE_POLICY_VENDOR_HEX: "<service key>",
  GHOSTLINE_POLICY_HUMAN_THRESHOLD: "1000",
  GHOSTLINE_POLICY_APPROVE_NEW_VENDOR: "1",
  GHOSTLINE_POLICY_REQUIRE_HUMAN: "1",
  GHOSTLINE_POLICY_AUDIT_LOG: "<run>/policy-audit.jsonl",
};

const descriptions = Object.fromEntries(
  [...policyGuide.matchAll(/^\| \`(GHOSTLINE_POLICY_[A-Z_]+)(?:=1)?\` \| ([^|]+) \|$/gm)]
    .map(([, env, text]) => [env, text.trim().replaceAll("\`", "")]),
);

const policy = Object.entries(examples).map(([env, example]) => {
  const text = descriptions[env];
  if (!text || !policyCode.includes(`"${env}"`) || !quickstart.includes(env)) {
    throw new Error(`Policy setting ${env} is missing from the implementation or docs`);
  }
  return { env, example, text };
});

writeFileSync(new URL("../lib/generated-tools.json", import.meta.url), JSON.stringify({ tools, policy }, null, 2) + "\n");
