import { round } from "./pricing";

type Discount = { kind: "percent"; value: number } | { kind: "flat"; value: number };

const CODES: Record<string, Discount> = {
  SAVE10: { kind: "percent", value: 10 },
  WELCOME5: { kind: "flat", value: 5 },
};

export function discountFor(code: string | undefined, amount: number): number {
  const discount = code ? CODES[code] : undefined;
  if (!discount) return 0;
  if (discount.kind === "percent") return round((amount * discount.value) / 100);
  return Math.min(discount.value, amount);
}
