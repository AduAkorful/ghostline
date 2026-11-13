export type Call = { n: number; price: number; status: "paid" | "held"; total: number };
export type State = { calls: Call[]; settled: boolean };

export const EMPTY: State = { calls: [], settled: false };

export function pay(state: State, price: number, budget: number): State {
  if (state.settled) return state;
  const accepted = state.calls.at(-1)?.total ?? 0;
  const over = price > budget - accepted;
  const call: Call = {
    n: state.calls.length + 1,
    price,
    status: over ? "held" : "paid",
    total: over ? accepted : accepted + price,
  };
  return { ...state, calls: [...state.calls, call] };
}
