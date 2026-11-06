import { describe, expect, it } from "vitest";
import { discountFor } from "./discounts";

describe("discounts", () => {
  it("SAVE10 takes 10% off", () => {
    expect(discountFor("SAVE10", 80)).toBe(8);
  });

  it("WELCOME5 takes a flat 5 off", () => {
    expect(discountFor("WELCOME5", 80)).toBe(5);
  });

  it("ignores unknown codes", () => {
    expect(discountFor("NOPE", 80)).toBe(0);
  });

  it("never discounts more than the amount", () => {
    expect(discountFor("WELCOME5", 3)).toBe(3);
  });
});
