import { describe, it, expect } from "vitest";
import { sideFor, SIDES } from "./sides.js";

const press = (code, extra = {}) => ({ code, meta: false, onSwitch: false, ...extra });

describe("which side the keyboard asks for", () => {
  it("reaches a side by number from anywhere in the panel", () => {
    expect(sideFor(press("Digit1", { meta: true }), "shortcuts")).toBe("settings");
    expect(sideFor(press("Digit2", { meta: true }), "settings")).toBe("shortcuts");
  });

  it("ignores the numbers without the command key — they are just typing", () => {
    expect(sideFor(press("Digit1"), "shortcuts")).toBe(null);
    expect(sideFor(press("Digit2"), "settings")).toBe(null);
  });

  it("walks the row with the arrows, and either arrow means the other side", () => {
    expect(sideFor(press("ArrowRight", { onSwitch: true }), "settings")).toBe("shortcuts");
    expect(sideFor(press("ArrowLeft", { onSwitch: true }), "settings")).toBe("shortcuts");
    expect(sideFor(press("ArrowRight", { onSwitch: true }), "shortcuts")).toBe("settings");
  });

  it("leaves the arrows alone away from the row: they belong to the slider", () => {
    expect(sideFor(press("ArrowRight"), "settings")).toBe(null);
    expect(sideFor(press("ArrowLeft"), "settings")).toBe(null);
  });

  it("does not take Tab: it is the only way between the controls", () => {
    expect(sideFor(press("Tab", { onSwitch: true }), "settings")).toBe(null);
    expect(sideFor(press("Tab", { shift: true }), "settings")).toBe(null);
  });

  it("settings comes first — the side you open to change something", () => {
    expect(SIDES[0]).toBe("settings");
  });
});
