import { describe, expect, it } from "vitest";
import { conversionOptions, presets, settingsError } from "../src/ConversionPanel";

const custom = { dpi: "300", maxWidthMm: "160", jpegQuality: "90" };

describe("backend-aligned conversion settings", () => {
  it("uses lossless original settings and the backend high and compact defaults", () => {
    expect(conversionOptions("original", custom, ["a"])).toEqual({ imageIds: ["a"], quality: "original", dpi: 300, maxWidthMm: 160, jpegQuality: 100 });
    expect(presets.high).toEqual({ dpi: 300, maxWidthMm: 160, jpegQuality: 92 });
    expect(presets.compact).toEqual({ dpi: 150, maxWidthMm: 160, jpegQuality: 75 });
    expect(settingsError("original", { dpi: "", maxWidthMm: "", jpegQuality: "" })).toBeNull();
  });

  it.each(["72", "1200"])("accepts DPI boundary %s", (dpi) => {
    expect(settingsError("custom", { ...custom, dpi })).toBeNull();
  });

  it.each(["", "71", "1201", "NaN", "150.5"])("rejects invalid DPI %s", (dpi) => {
    expect(settingsError("custom", { ...custom, dpi })).toBe("DPI 需为 72–1200 的整数");
  });

  it.each(["10", "1000", "120.5"])("accepts reference layout width %s", (maxWidthMm) => {
    expect(settingsError("custom", { ...custom, maxWidthMm })).toBeNull();
  });

  it.each(["", "9", "1001", "Infinity"])("rejects reference layout width %s", (maxWidthMm) => {
    expect(settingsError("custom", { ...custom, maxWidthMm })).toBe("参考宽度需为 10–1000 mm");
  });

  it.each(["", "0", "101", "85.5"])("rejects JPEG quality %s", (jpegQuality) => {
    expect(settingsError("custom", { ...custom, jpegQuality })).toBe("JPEG 质量需为 1–100 的整数");
  });
});
