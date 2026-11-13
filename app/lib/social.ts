export const socialCards = {
  site: ["Every call, on your ledger.", "Two records on the chain."],
  "how-it-works": ["Lock once. Sign per call.", "Settle once."],
  privacy: ["One channel, seen twice.", "Your ledger and the chain’s view."],
  proof: ["Check the chain.", "Recorded on Tari Ootle mainnet."],
  fees: ["What a channel really costs.", "Network fees and the 1% protocol fee."],
  agents: ["Let your agent pay.", "Keep the limits yours."],
  sellers: ["Charge per call.", "Settle once."],
  docs: ["Ghostline docs.", "Install, set up, and integrate."],
} as const;

export type SocialCard = keyof typeof socialCards;
