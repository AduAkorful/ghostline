"use client";
import { useId, useState } from "react";
import { fees } from "@/lib/data";
import { microTari, num } from "@/lib/format";
import { calculateFees, parseAmount, tari } from "@/lib/calculator";

function Field({ label, value, onChange, hint, error, unit = true }: { label: string; value: string; onChange: (value: string) => void; hint?: string; error?: string; unit?: boolean }) {
  const id = useId();
  return (
    <div>
      <label htmlFor={id} className="eyebrow">{label}</label>
      <div className={`mt-1.5 flex items-center rounded-sm border bg-night ${error ? "border-held" : "border-rule"} focus-within:outline-2 focus-within:outline-chain`}>
        <input id={id} inputMode="numeric" value={value} aria-invalid={!!error} aria-describedby={`${id}-h`}
          onChange={(e) => onChange(e.target.value)}
          className="w-full bg-transparent px-3 py-2.5 font-mono text-[15px] outline-none" />
        {unit && <span className="pr-3 font-mono text-sm text-muted">µT</span>}
      </div>
      <p id={`${id}-h`} className={`mt-1 text-xs ${error ? "text-held" : "text-muted"}`}>{error ?? hint}</p>
    </div>
  );
}

export function FeeCalculator() {
  const [deposit, setDeposit] = useState("100000");
  const [price, setPrice] = useState("100");
  const [calls, setCalls] = useState("500");
  const parsedDeposit = parseAmount(deposit);
  const parsedPrice = parseAmount(price);
  const parsedCalls = parseAmount(calls);
  const priceError = parsedPrice === null ? "Enter a whole number in range." : parsedPrice === 0n ? "Enter a price."
    : parsedPrice % BigInt(fees.priceStep) !== 0n ? `Prices must be multiples of ${fees.priceStep} µT.` : undefined;
  const network = BigInt(fees.open + fees.settle);
  const result = parsedDeposit === null || parsedCalls === null
    ? null : calculateFees(parsedDeposit, priceError ? 0n : (parsedPrice ?? 0n), parsedCalls, network);
  const amount = (value: bigint | undefined) => value === undefined ? "—" : microTari(value);
  const capped = result?.capped;
  return (
    <div className="grid gap-px overflow-hidden rounded-[3px] border border-rule bg-rule md:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
      <div className="space-y-5 bg-paper p-5">
        <Field label="Deposit" value={deposit} onChange={setDeposit} hint={parsedDeposit === null ? undefined : `${tari(parsedDeposit)} TARI`} error={parsedDeposit === null ? "Enter a whole number in range." : undefined} />
        <Field label="Price per call" value={price} onChange={setPrice} hint={`A multiple of ${fees.priceStep} µT`} error={priceError} />
        <Field label="Calls you expect" value={calls} onChange={setCalls} hint="Before the channel settles" error={parsedCalls === null ? "Enter a whole number in range." : undefined} unit={false} />
      </div>
      <dl className="min-w-0 bg-paper p-5 font-mono text-[14px]">
        {[
          ["Most the channel can pay out", amount(result?.cap), ""],
          ["Calls that fit", result ? num(result.capCalls) : "—", capped ? "text-held" : ""],
          ["Service payout", amount(result?.payout), ""],
          ["Protocol fee (1%)", amount(result?.protocol), ""],
          ["Change back to you", amount(result?.change), "text-ghost"],
          ["Network fees (open + settle)", microTari(network), ""],
        ].map(([k, v, c]) => (
          <div key={k} className="flex justify-between gap-4 border-b border-rule py-2.5"><dt className="font-sans text-muted">{k}</dt><dd className={`break-all text-right ${c}`}>{v}</dd></div>
        ))}
        <div className="flex justify-between gap-4 py-3"><dt className="font-sans">All-in cost per call</dt><dd className="text-lg">{amount(result?.perCall ?? undefined)}</dd></div>
        <p className="font-sans text-[13px] text-muted">
          {capped ? <span className="text-held">Only {num(result.capCalls)} calls fit this deposit. </span> : null}
          {result?.breakEven ? <>Network fees equal about {num(result.breakEven)} calls at this price. Below that, the channel costs more in fees than it carries.</> : null}
        </p>
      </dl>
    </div>
  );
}
