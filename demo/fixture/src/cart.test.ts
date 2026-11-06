import { describe, expect, it } from "vitest";
import { itemCount, totals, type Item } from "./cart";

const basket: Item[] = [
  { name: "Keyboard", price: 50, qty: 1 },
  { name: "Mouse", price: 25, qty: 2 },
];

describe("cart", () => {
  it("counts items", () => {
    expect(itemCount(basket)).toBe(3);
  });

  describe("totals", () => {
    it("is zero for an empty cart", () => {
      expect(totals([])).toEqual({ subtotal: 0, discount: 0, tax: 0, total: 0 });
    });

    it("adds tax without a code", () => {
      expect(totals(basket)).toEqual({ subtotal: 100, discount: 0, tax: 20, total: 120 });
    });

    it("applies SAVE10 before tax", () => {
      expect(totals(basket, "SAVE10")).toEqual({
        subtotal: 100,
        discount: 10,
        tax: 20,
        total: 110,
      });
    });

    it("applies WELCOME5 as a flat amount", () => {
      expect(totals(basket, "WELCOME5").total).toBe(115);
    });
  });
});
