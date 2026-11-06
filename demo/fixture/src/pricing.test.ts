import { describe, expect, it } from "vitest";
import { lineTotal, round, taxOn } from "./pricing";

describe("pricing", () => {
  it("rounds to two decimals", () => {
    expect(round(1.236)).toBe(1.24);
  });

  it("multiplies price by quantity", () => {
    expect(lineTotal(25, 2)).toBe(50);
  });

  it("returns 0 for zero quantity", () => {
    expect(lineTotal(25, 0)).toBe(0);
  });

  it("charges 20% tax", () => {
    expect(taxOn(100)).toBe(20);
  });
});
