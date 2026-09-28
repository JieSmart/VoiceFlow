import { describe, expect, it } from "vitest";
import { resolvedMicrophoneId } from "./audioDevices";

describe("resolvedMicrophoneId", () => {
  const devices = [
    { id: "Headset Mic", name: "Headset Mic" },
    { id: "USB Mic", name: "USB Mic" },
  ];

  it("keeps a saved device name", () => {
    expect(resolvedMicrophoneId("USB Mic", devices)).toBe("USB Mic");
  });

  it("migrates a legacy enumeration index to the device name", () => {
    expect(resolvedMicrophoneId("1", devices)).toBe("USB Mic");
  });

  it("leaves an unknown name unchanged", () => {
    expect(resolvedMicrophoneId("Missing Mic", devices)).toBe("Missing Mic");
  });
});
