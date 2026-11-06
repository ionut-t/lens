export const TAX_RATE = 0.2;

export function round(n: number): number {
  return Math.round(n * 100) / 100;
}

export function lineTotal(price: number, qty: number): number {
  return round(price * qty);
}

export function taxOn(amount: number): number {
  return round(amount * TAX_RATE);
}
