import { describe, expect, it } from "vitest";
import { formatPrice, receiptTotal } from "./format";

describe("format", () => {
  it("formats whole prices", () => {
    expect(formatPrice(12)).toBe("£12.00");
  });

  it("pads pennies", () => {
    expect(formatPrice(3.5)).toBe("£3.50");
  });

  it("prints the receipt total", () => {
    const items = [{ name: "Headphones", price: 100, qty: 1 }];
    expect(receiptTotal(items, "SAVE10")).toBe("Total: £110.00");
  });
});
