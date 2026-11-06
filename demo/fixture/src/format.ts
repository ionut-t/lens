import { totals, type Item } from "./cart";

export function formatPrice(amount: number): string {
  return `£${amount.toFixed(2)}`;
}

export function receiptTotal(items: Item[], code?: string): string {
  return `Total: ${formatPrice(totals(items, code).total)}`;
}
