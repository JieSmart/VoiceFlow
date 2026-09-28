import { describe, expect, it } from "vitest";
import {
  acceleratorFromEvent,
  formatWakeHint,
  hotkeyConflicts,
  mergeHotkeys,
  normalizeAccelerator,
} from "./hotkeys";

describe("hotkeys", () => {
  it("normalizes accelerator aliases", () => {
    expect(normalizeAccelerator("control+shift+space")).toBe("Ctrl+Shift+Space");
    expect(normalizeAccelerator("Ctrl+Alt+g")).toBe("Ctrl+Alt+G");
  });

  it("builds an accelerator from a keyboard event", () => {
    const event = {
      key: "v",
      code: "KeyV",
      ctrlKey: true,
      shiftKey: true,
      altKey: false,
      metaKey: false,
    } as KeyboardEvent;
    expect(acceleratorFromEvent(event)).toBe("Ctrl+Shift+V");
  });

  it("reports missing modifiers and duplicates", () => {
    const errors = hotkeyConflicts({
      wake: "Space",
      panel: "Ctrl+Shift+V",
      ai: "Ctrl+Shift+V",
      mute: "Escape",
      mode: "toggle",
      holdMs: 220,
    });
    expect(errors.some((item) => item.includes("修饰键"))).toBe(true);
    expect(errors.some((item) => item.includes("冲突"))).toBe(true);
    expect(errors.some((item) => item.includes("Enter 或 Escape"))).toBe(true);
  });

  it("fills default hotkeys and clamps hold duration", () => {
    expect(mergeHotkeys(undefined).wake).toBe("Ctrl+Shift+Space");
    expect(mergeHotkeys(undefined).ai).toBe("Ctrl+Alt+G");
    expect(mergeHotkeys({ holdMs: 12 } as never).holdMs).toBe(100);
    expect(mergeHotkeys({ holdMs: 900 } as never).holdMs).toBe(500);
  });

  it("formats the wake accelerator for bubble copy", () => {
    expect(formatWakeHint("Ctrl+Shift+Space")).toBe("Ctrl + Shift + Space");
    expect(formatWakeHint("Ctrl+Alt+Q")).toBe("Ctrl + Alt + Q");
  });
});
