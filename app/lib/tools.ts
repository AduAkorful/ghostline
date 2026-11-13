import generated from "./generated-tools.json";

const groups = {
  Pay: ["ghostline_call_paid", "ghostline_sign_voucher"],
  Channel: [
    "ghostline_open_channel", "ghostline_close_channel", "ghostline_channel_session",
    "ghostline_channel_status", "ghostline_channel_list", "ghostline_channel_calls", "ghostline_channel_file",
  ],
  Inspect: ["ghostline_channel_id", "ghostline_verify_voucher", "ghostline_inspect_tx", "ghostline_unblind_tx"],
} as const;

const descriptions: Record<string, string> = generated.tools;

export const MCP_TOOLS = Object.entries(groups).flatMap(([group, names]) => names.map((name) => {
  const text = descriptions[name];
  if (!text) throw new Error(`Missing MCP tool description for ${name}`);
  return { name, group, text };
}));
if (MCP_TOOLS.length !== Object.keys(descriptions).length) {
  throw new Error("Update the MCP tool groups to match the server");
}

export const POLICY = generated.policy;
