import { discountFor } from "./discounts";
import { lineTotal, round, taxOn } from "./pricing";

export type Item = { name: string; price: number; qty: number };
export type Totals = { subtotal: number; discount: number; tax: number; total: number };

export function itemCount(items: Item[]): number {
  return items.reduce((n, item) => n + item.qty, 0);
}

export function totals(items: Item[], code?: string): Totals {
  const subtotal = round(items.reduce((sum, i) => sum + lineTotal(i.price, i.qty), 0));
  const tax = taxOn(subtotal);
  const discount = discountFor(code, subtotal + tax);
  const total = round(subtotal + tax - discount);
  return { subtotal, discount, tax, total };
}
