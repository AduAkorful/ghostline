/** Integer µT with thousands separators. Never rounds. */
export const microTari = (n: number | bigint) => `${n.toLocaleString("en-US")} µT`;
export const num = (n: number | bigint) => n.toLocaleString("en-US");
/** First 8 and last 4 characters (brief rule). */
export const shortHash = (h: string) => {
  const bare = h.replace(/^(component_|template_)/, "");
  return bare.length > 14 ? `${bare.slice(0, 8)}…${bare.slice(-4)}` : bare;
};
